use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::image::ImageRWops;

use crate::render::font::FontCache;
use crate::render::image::sniff_image_type;

pub const TAB_BAR_HEIGHT: i32 = 36;

const TAB_MIN_W:  i32 = 90;
const TAB_MAX_W:  i32 = 200;
const TAB_PAD:    i32 = 10;
const CLOSE_W:    i32 = 18;
const FONT_SIZE:  u16 = 13;
const NEW_BTN_W:  i32 = 36;
const FAVICON_SZ: i32 = 16;

// Light chrome palette
const CHROME_BG:     (u8,u8,u8) = (240, 240, 245);
const TAB_ACTIVE_BG: (u8,u8,u8) = (255, 255, 255);
const TAB_IDLE_BG:   (u8,u8,u8) = (230, 230, 235);
const ACCENT:        (u8,u8,u8) = (0,   120, 215);  // blue accent line
const SEP_COL:       (u8,u8,u8) = (210, 210, 220);
const TEXT_ACTIVE:   (u8,u8,u8) = (30,  30,  40 );
const TEXT_IDLE:     (u8,u8,u8) = (100, 100, 120);
const CLOSE_COL:     (u8,u8,u8) = (110, 110, 130);
const PLUS_COL:      (u8,u8,u8) = (100, 100, 120);

const SPINNER_STEPS: usize = 8;
const SPINNER_MS: u128 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabBarRegion {
    Tab(usize),
    Close(usize),
    NewTab,
    None,
}

pub struct TabBar;

impl TabBar {
    fn tab_width(n: usize, win_w: i32) -> i32 {
        let avail = (win_w - NEW_BTN_W).max(TAB_MIN_W * n as i32);
        let w = avail / n as i32;
        w.clamp(TAB_MIN_W, TAB_MAX_W)
    }

    pub fn region_at(mx: i32, my: i32, win_w: i32, titles: &[String], _active: usize) -> TabBarRegion {
        if my < 0 || my >= TAB_BAR_HEIGHT { return TabBarRegion::None; }

        let n     = titles.len().max(1);
        let tab_w = Self::tab_width(n, win_w);

        for i in 0..titles.len() {
            let tx = i as i32 * tab_w;
            if mx >= tx && mx < tx + tab_w {
                let close_x = tx + tab_w - TAB_PAD - CLOSE_W;
                if mx >= close_x && mx < close_x + CLOSE_W {
                    return TabBarRegion::Close(i);
                }
                return TabBarRegion::Tab(i);
            }
        }

        let new_btn_x = titles.len() as i32 * tab_w;
        if mx >= new_btn_x && mx < new_btn_x + NEW_BTN_W {
            return TabBarRegion::NewTab;
        }

        TabBarRegion::None
    }

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
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let spinner_frame = ((now_ms / SPINNER_MS) as usize) % SPINNER_STEPS;

        // Background strip
        let (cr, cg, cb) = CHROME_BG;
        canvas.set_draw_color(Color::RGB(cr, cg, cb));
        let _ = canvas.fill_rect(Rect::new(0, 0, win_w as u32, TAB_BAR_HEIGHT as u32));

        let n     = titles.len().max(1);
        let tab_w = Self::tab_width(n, win_w);

        for (i, title) in titles.iter().enumerate() {
            let tx        = i as i32 * tab_w;
            let is_active = i == active;
            let is_loading = loading_states.get(i).copied().unwrap_or(false);

            // Tab background
            let (br, bg_c, bb) = if is_active { TAB_ACTIVE_BG } else { TAB_IDLE_BG };
            canvas.set_draw_color(Color::RGB(br, bg_c, bb));
            let _ = canvas.fill_rect(Rect::new(tx, 0, tab_w as u32, TAB_BAR_HEIGHT as u32));

            // Right-edge separator between inactive tabs
            if !is_active {
                let (sr, sg, sb) = SEP_COL;
                canvas.set_draw_color(Color::RGB(sr, sg, sb));
                let _ = canvas.fill_rect(Rect::new(tx + tab_w - 1, 6, 1, (TAB_BAR_HEIGHT - 12) as u32));
            }

            // Active tab accent underline (2px vivid blue)
            if is_active {
                let (ar, ag, ab) = ACCENT;
                canvas.set_draw_color(Color::RGB(ar, ag, ab));
                let _ = canvas.fill_rect(Rect::new(tx + 2, TAB_BAR_HEIGHT - 2, (tab_w - 4) as u32, 2));
            }

            // Close button
            let close_x = tx + tab_w - TAB_PAD - CLOSE_W;
            let close_y = (TAB_BAR_HEIGHT - CLOSE_W) / 2;
            draw_close_btn(canvas, fonts, tc, close_x, close_y, CLOSE_W);

            // Favicon / spinner
            let icon_drawn_w = if is_loading {
                let cx = tx + TAB_PAD + FAVICON_SZ / 2;
                let cy = TAB_BAR_HEIGHT / 2;
                draw_spinner(canvas, cx, cy, FAVICON_SZ / 2 - 1, spinner_frame);
                FAVICON_SZ
            } else if let Some(favicon_bytes) = favicons.get(i).and_then(|v| v.as_deref()) {
                draw_favicon(canvas, tc, favicon_bytes, tx + TAB_PAD, (TAB_BAR_HEIGHT - FAVICON_SZ) / 2)
            } else {
                0
            };

            // Title
            let icon_gap      = if icon_drawn_w > 0 { icon_drawn_w + 5 } else { 0 };
            let display_title = if is_loading { "Loading...".to_owned() } else { title.clone() };
            let text_x        = tx + TAB_PAD + icon_gap;
            let text_max_w    = (close_x - text_x - 4).max(0);
            if text_max_w > 0 {
                let (tr, tg, tb) = if is_active { TEXT_ACTIVE } else { TEXT_IDLE };
                draw_tab_text(canvas, fonts, tc, &display_title, text_x, 0, text_max_w, TAB_BAR_HEIGHT, Color::RGB(tr, tg, tb));
            }
        }

        // '+' new-tab button
        let new_x = titles.len() as i32 * tab_w;
        if new_x + NEW_BTN_W <= win_w {
            draw_plus_btn(canvas, new_x, 0, NEW_BTN_W, TAB_BAR_HEIGHT);
        }

        // Bottom border (accent line width, subtle)
        let (sr, sg, sb) = SEP_COL;
        canvas.set_draw_color(Color::RGB(sr, sg, sb));
        let _ = canvas.fill_rect(Rect::new(0, TAB_BAR_HEIGHT - 1, win_w as u32, 1));
    }
}

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
            if let Ok(tex) = tc.create_texture_from_surface::<&sdl2::surface::SurfaceRef>(&surf) {
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

fn draw_plus_btn(canvas: &mut Canvas<Window>, bx: i32, by: i32, bw: i32, bh: i32) {
    // Pill-shaped area hint (just a subtle hover-state circle fill)
    let cx    = bx + bw / 2;
    let cy    = by + bh / 2;
    let r     = (bw.min(bh) / 2 - 4).max(4);
    // Draw a faint circle background
    let (pr, pg, pb) = (220, 220, 230);
    canvas.set_draw_color(Color::RGB(pr, pg, pb));
    for dy in -r..=r {
        let half_w = ((r*r - dy*dy) as f64).sqrt() as i32;
        let _ = canvas.fill_rect(Rect::new(cx - half_w, cy + dy, (half_w * 2) as u32, 1));
    }
    // Cross arms
    let half  = r / 2;
    let thick = 2_i32;
    let (pr, pg, pb) = PLUS_COL;
    canvas.set_draw_color(Color::RGB(pr, pg, pb));
    let _ = canvas.fill_rect(Rect::new(cx - half, cy - thick / 2, (half * 2) as u32, thick as u32));
    let _ = canvas.fill_rect(Rect::new(cx - thick / 2, cy - half, thick as u32, (half * 2) as u32));
}

fn draw_close_btn(
    canvas: &mut Canvas<Window>,
    _fonts: &mut FontCache,
    _tc:    &TextureCreator<WindowContext>,
    x: i32, y: i32, size: i32,
) {
    // Subtle dark circle hover background
    let cx    = x + size / 2;
    let cy    = y + size / 2;
    let r     = size / 2 - 1;
    canvas.set_draw_color(Color::RGB(225, 225, 235));
    for dy in -r..=r {
        let half_w = ((r*r - dy*dy) as f64).sqrt() as i32;
        let _ = canvas.fill_rect(Rect::new(cx - half_w, cy + dy, (half_w * 2) as u32, 1));
    }
    // Cross
    let pad = size / 4 + 1;
    let x1  = x + pad;
    let y1  = y + pad;
    let x2  = x + size - pad;
    let y2  = y + size - pad;
    let (cr, cg, cb) = CLOSE_COL;
    canvas.set_draw_color(Color::RGB(cr, cg, cb));
    for d in 0..=1_i32 {
        let _ = canvas.draw_line(sdl2::rect::Point::new(x1 + d, y1), sdl2::rect::Point::new(x2 + d, y2));
        let _ = canvas.draw_line(sdl2::rect::Point::new(x2 + d, y1), sdl2::rect::Point::new(x1 + d, y2));
    }
}

fn draw_spinner(canvas: &mut Canvas<Window>, cx: i32, cy: i32, r: i32, frame: usize) {
    let dots = SPINNER_STEPS;
    for i in 0..dots {
        let angle = (i as f64 / dots as f64) * std::f64::consts::TAU
            - std::f64::consts::FRAC_PI_2;
        let px = cx + (r as f64 * angle.cos()).round() as i32;
        let py = cy + (r as f64 * angle.sin()).round() as i32;

        let dist = (dots + frame - i) % dots;
        let color = match dist {
            0 => Color::RGB(0, 100, 200),
            1 => Color::RGB(60, 140, 220),
            2 => Color::RGB(120, 180, 235),
            3 => Color::RGB(180, 205, 240),
            _ => Color::RGB(220, 225, 235),
        };

        canvas.set_draw_color(color);
        let _ = canvas.fill_rect(Rect::new(px - 1, py - 1, 2, 2));
    }
}

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
    let _ = canvas.copy(&tex, None, Rect::new(x, y, FAVICON_SZ as u32, FAVICON_SZ as u32));
    FAVICON_SZ
}
