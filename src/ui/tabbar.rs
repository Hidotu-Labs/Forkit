use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::image::ImageRWops;

use crate::render::font::FontCache;
use crate::render::image::sniff_image_type;

/// Height of the tab strip in pixels.
pub const TAB_BAR_HEIGHT: i32 = 28;

const TAB_MIN_W:  i32 = 80;
const TAB_MAX_W:  i32 = 180;
const TAB_PAD:    i32 = 8;
const CLOSE_W:    i32 = 20;
const FONT_SIZE:  u16 = 13;
const NEW_BTN_W:  i32 = 32;
const FAVICON_SZ: i32 = 16;  // favicon rendered size in pixels

// Spinner: 8 arc segments, ~100 ms each
const SPINNER_STEPS: usize = 8;
const SPINNER_MS: u128 = 100;

/// Which region of the tab bar was clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabBarRegion {
    /// Click on tab `i` (but not its close button).
    Tab(usize),
    /// Click the close button on tab `i`.
    Close(usize),
    /// Click the "new tab" `+` button.
    NewTab,
    /// Click elsewhere (ignored).
    None,
}

pub struct TabBar;

impl TabBar {
    /// Compute the rendered width of a single tab given `n` total tabs and
    /// the available window width.
    fn tab_width(n: usize, win_w: i32) -> i32 {
        let avail = (win_w - NEW_BTN_W).max(TAB_MIN_W * n as i32);
        let w = avail / n as i32;
        w.clamp(TAB_MIN_W, TAB_MAX_W)
    }

    /// Hit-test a click at `(mx, my)` (window-absolute coords).
    pub fn region_at(mx: i32, my: i32, win_w: i32, titles: &[String], _active: usize) -> TabBarRegion {
        if my < 0 || my >= TAB_BAR_HEIGHT { return TabBarRegion::None; }

        let n     = titles.len().max(1);
        let tab_w = Self::tab_width(n, win_w);

        for i in 0..titles.len() {
            let tx = i as i32 * tab_w;
            if mx >= tx && mx < tx + tab_w {
                // Close button occupies the right portion of the tab
                let close_x = tx + tab_w - TAB_PAD - CLOSE_W;
                if mx >= close_x && mx < close_x + CLOSE_W {
                    return TabBarRegion::Close(i);
                }
                return TabBarRegion::Tab(i);
            }
        }

        // "+" new tab button sits right after the last tab
        let new_btn_x = titles.len() as i32 * tab_w;
        if mx >= new_btn_x && mx < new_btn_x + NEW_BTN_W {
            return TabBarRegion::NewTab;
        }

        TabBarRegion::None
    }

    /// Draw the tab strip.
    ///
    /// `favicons` is a parallel slice to `titles`: each entry is the raw image
    /// bytes for that tab's favicon, or `None` if not yet loaded / unavailable.
    /// `loading_states` is a parallel slice indicating whether each tab is
    /// currently loading (shows a spinner instead of the favicon).
    pub fn draw(
        canvas:         &mut Canvas<Window>,
        tc:             &TextureCreator<WindowContext>,
        fonts:          &mut FontCache,
        win_w:          i32,
        titles:         &[String],
        active:         usize,
        favicons:       &[Option<Vec<u8>>],
        loading_states: &[bool],
    ) {
        // Current time in ms — used to derive the spinner frame index.
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let spinner_frame = ((now_ms / SPINNER_MS) as usize) % SPINNER_STEPS;

        // Strip background
        canvas.set_draw_color(Color::RGB(210, 210, 215));
        let _ = canvas.fill_rect(Rect::new(0, 0, win_w as u32, TAB_BAR_HEIGHT as u32));

        let n     = titles.len().max(1);
        let tab_w = Self::tab_width(n, win_w);

        for (i, title) in titles.iter().enumerate() {
            let tx        = i as i32 * tab_w;
            let is_active = i == active;
            let is_loading = loading_states.get(i).copied().unwrap_or(false);

            // Tab background
            let bg = if is_active { Color::RGB(245, 245, 248) } else { Color::RGB(220, 220, 225) };
            canvas.set_draw_color(bg);
            let _ = canvas.fill_rect(Rect::new(tx, 0, tab_w as u32, TAB_BAR_HEIGHT as u32));

            // Right divider
            if !is_active {
                canvas.set_draw_color(Color::RGB(185, 185, 190));
                let _ = canvas.fill_rect(Rect::new(tx + tab_w - 1, 3, 1, (TAB_BAR_HEIGHT - 6) as u32));
            }

            // Active-tab bottom connector
            if is_active {
                canvas.set_draw_color(Color::RGB(245, 245, 248));
                let _ = canvas.fill_rect(Rect::new(tx, TAB_BAR_HEIGHT - 1, tab_w as u32, 1));
            }

            // Close button "×"
            let close_x = tx + tab_w - TAB_PAD - CLOSE_W;
            let close_y = (TAB_BAR_HEIGHT - CLOSE_W) / 2;
            draw_close_btn(canvas, fonts, tc, close_x, close_y, CLOSE_W);

            // Favicon / spinner — drawn left of the title
            let icon_drawn_w = if is_loading {
                // Animated spinner drawn with primitives — no font glyph needed
                let cx = tx + TAB_PAD + FAVICON_SZ / 2;
                let cy = TAB_BAR_HEIGHT / 2;
                draw_spinner(canvas, cx, cy, FAVICON_SZ / 2 - 1, spinner_frame);
                FAVICON_SZ
            } else {
                let favicon_bytes = favicons.get(i).and_then(|v| v.as_deref());
                if let Some(bytes) = favicon_bytes {
                    draw_favicon(canvas, tc, bytes, tx + TAB_PAD, (TAB_BAR_HEIGHT - FAVICON_SZ) / 2)
                } else {
                    0
                }
            };
            let icon_gap = if icon_drawn_w > 0 { icon_drawn_w + 4 } else { 0 };

            // Title text — "Loading..." while fetching, otherwise the real title
            let display_title = if is_loading {
                "Loading...".to_owned()
            } else {
                title.clone()
            };
            let text_x     = tx + TAB_PAD + icon_gap;
            let text_max_w = (close_x - text_x - 4).max(0);
            if text_max_w > 0 {
                let text_col = if is_active { Color::RGB(20, 20, 20) } else { Color::RGB(70, 70, 70) };
                draw_tab_text(canvas, fonts, tc, &display_title, text_x, 0, text_max_w, TAB_BAR_HEIGHT, text_col);
            }
        }

        // "+" new tab button — drawn with primitives, no background
        let new_x = titles.len() as i32 * tab_w;
        if new_x + NEW_BTN_W <= win_w {
            draw_plus_btn(canvas, new_x, 0, NEW_BTN_W, TAB_BAR_HEIGHT);
        }

        // Bottom border of the entire tab bar
        canvas.set_draw_color(Color::RGB(180, 180, 185));
        let _ = canvas.fill_rect(Rect::new(0, TAB_BAR_HEIGHT - 1, win_w as u32, 1));
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn draw_tab_text(
    canvas:  &mut Canvas<Window>,
    fonts:   &mut FontCache,
    tc:      &TextureCreator<WindowContext>,
    text:    &str,
    x: i32, y: i32, max_w: i32, height: i32,
    color: Color,
) {
    if let Some(font) = fonts.get(FONT_SIZE, false, false) {
        if let Ok(surf) = font.render(text).blended(color) {
            if let Ok(tex) = tc.create_texture_from_surface(&surf) {
                let sw     = surf.width() as i32;
                let sh     = surf.height() as i32;
                let ty     = y + (height - sh) / 2;
                let draw_w = sw.min(max_w).max(0) as u32;
                if draw_w > 0 {
                    let _ = canvas.copy(
                        &tex,
                        Some(Rect::new(0, 0, draw_w, sh as u32)),
                        Rect::new(x, ty, draw_w, sh as u32),
                    );
                }
            }
        }
    }
}

/// Draw a primitive "+" centred in the new-tab button cell.
fn draw_plus_btn(canvas: &mut Canvas<Window>, bx: i32, by: i32, bw: i32, bh: i32) {
    let cx = bx + bw / 2;
    let cy = by + bh / 2;
    let half = bw / 5;          // arm half-length
    let thick = 2.max(bh / 8);  // arm thickness

    canvas.set_draw_color(Color::RGB(60, 60, 60));

    // Horizontal arm
    let _ = canvas.fill_rect(Rect::new(cx - half, cy - thick / 2, (half * 2) as u32, thick as u32));
    // Vertical arm
    let _ = canvas.fill_rect(Rect::new(cx - thick / 2, cy - half, thick as u32, (half * 2) as u32));
}

fn draw_close_btn(
    canvas: &mut Canvas<Window>,
    _fonts: &mut FontCache,
    _tc:    &TextureCreator<WindowContext>,
    x: i32, y: i32, size: i32,
) {
    let pad = size / 4;
    let x1 = x + pad;
    let y1 = y + pad;
    let x2 = x + size - pad;
    let y2 = y + size - pad;

    canvas.set_draw_color(Color::RGB(90, 90, 90));

    for d in -1..=1_i32 {
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(x1 + d, y1),
            sdl2::rect::Point::new(x2 + d, y2),
        );
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(x1, y1 + d),
            sdl2::rect::Point::new(x2, y2 + d),
        );
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(x2 + d, y1),
            sdl2::rect::Point::new(x1 + d, y2),
        );
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(x2, y1 + d),
            sdl2::rect::Point::new(x1, y2 + d),
        );
    }
}

/// Draw a circular arc spinner at `(cx, cy)` with the given radius.
/// `frame` in 0..SPINNER_STEPS controls which arc segment is highlighted.
/// The spinner is drawn as a ring of dots; the highlighted dot is bright blue,
/// the others are light grey — giving an orbiting-dot animation.
fn draw_spinner(canvas: &mut Canvas<Window>, cx: i32, cy: i32, r: i32, frame: usize) {
    let dots = SPINNER_STEPS;
    for i in 0..dots {
        // Angle: start at top (−π/2), go clockwise
        let angle = (i as f64 / dots as f64) * std::f64::consts::TAU
            - std::f64::consts::FRAC_PI_2;
        let px = cx + (r as f64 * angle.cos()).round() as i32;
        let py = cy + (r as f64 * angle.sin()).round() as i32;

        // The "head" dot and the two behind it are bright; rest fade out
        let dist = (dots + frame - i) % dots; // how far behind the head
        let color = match dist {
            0 => Color::RGB(60, 120, 220),  // head — bright blue
            1 => Color::RGB(90, 150, 230),
            2 => Color::RGB(140, 180, 235),
            3 => Color::RGB(185, 205, 240),
            _ => Color::RGB(210, 215, 225), // tail — light grey
        };

        // Draw a 2×2 dot
        canvas.set_draw_color(color);
        let _ = canvas.fill_rect(Rect::new(px - 1, py - 1, 2, 2));
    }
}

/// Decode and draw a favicon from raw bytes at `(x, y)`, scaled to FAVICON_SZ×FAVICON_SZ.
/// Returns the width drawn (FAVICON_SZ) on success, 0 on failure.
fn draw_favicon(
    canvas: &mut Canvas<Window>,
    tc:     &TextureCreator<WindowContext>,
    bytes:  &[u8],
    x: i32, y: i32,
) -> i32 {
    let fmt = sniff_image_type(bytes);
    let rwops = match sdl2::rwops::RWops::from_bytes(bytes) {
        Ok(r)  => r,
        Err(_) => return 0,
    };
    let surface = match rwops.load_typed(fmt) {
        Ok(s)  => s,
        Err(_) => return 0,
    };
    let tex = match tc.create_texture_from_surface(&surface) {
        Ok(t)  => t,
        Err(_) => return 0,
    };
    let dst = Rect::new(x, y, FAVICON_SZ as u32, FAVICON_SZ as u32);
    let _ = canvas.copy(&tex, None, dst);
    FAVICON_SZ
}
