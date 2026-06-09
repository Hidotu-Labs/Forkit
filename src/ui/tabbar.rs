use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::render::font::FontCache;

/// Height of the tab strip in pixels.
pub const TAB_BAR_HEIGHT: i32 = 28;

const TAB_MIN_W:  i32 = 80;
const TAB_MAX_W:  i32 = 180;   // cap so tabs don't sprawl across the whole bar
const TAB_PAD:    i32 = 8;
const CLOSE_W:    i32 = 20;    // hit-test width for the × button
const FONT_SIZE:  u16 = 13;
const NEW_BTN_W:  i32 = 32;

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
    pub fn draw(
        canvas:  &mut Canvas<Window>,
        tc:      &TextureCreator<WindowContext>,
        fonts:   &mut FontCache,
        win_w:   i32,
        titles:  &[String],
        active:  usize,
    ) {
        // Strip background
        canvas.set_draw_color(Color::RGB(210, 210, 215));
        let _ = canvas.fill_rect(Rect::new(0, 0, win_w as u32, TAB_BAR_HEIGHT as u32));

        let n     = titles.len().max(1);
        let tab_w = Self::tab_width(n, win_w);

        for (i, title) in titles.iter().enumerate() {
            let tx        = i as i32 * tab_w;
            let is_active = i == active;

            // Tab background
            let bg = if is_active { Color::RGB(245, 245, 248) } else { Color::RGB(220, 220, 225) };
            canvas.set_draw_color(bg);
            let _ = canvas.fill_rect(Rect::new(tx, 0, tab_w as u32, TAB_BAR_HEIGHT as u32));

            // Right divider (skip for active tab and its right neighbour)
            if !is_active {
                canvas.set_draw_color(Color::RGB(185, 185, 190));
                let _ = canvas.fill_rect(Rect::new(tx + tab_w - 1, 3, 1, (TAB_BAR_HEIGHT - 6) as u32));
            }

            // Active-tab bottom connector (hides the bottom border under that tab)
            if is_active {
                canvas.set_draw_color(Color::RGB(245, 245, 248));
                let _ = canvas.fill_rect(Rect::new(tx, TAB_BAR_HEIGHT - 1, tab_w as u32, 1));
            }

            // Close button "×"
            let close_x = tx + tab_w - TAB_PAD - CLOSE_W;
            let close_y = (TAB_BAR_HEIGHT - CLOSE_W) / 2;
            draw_close_btn(canvas, fonts, tc, close_x, close_y, CLOSE_W);

            // Title text — clipped to the space left of the close button
            let text_max_w = (tab_w - TAB_PAD * 2 - CLOSE_W - 4).max(0);
            if text_max_w > 0 {
                let text_col = if is_active { Color::RGB(20, 20, 20) } else { Color::RGB(70, 70, 70) };
                draw_tab_text(canvas, fonts, tc, title, tx + TAB_PAD, 0, text_max_w, TAB_BAR_HEIGHT, text_col);
            }
        }

        // "+" new tab button
        let new_x = titles.len() as i32 * tab_w;
        if new_x + NEW_BTN_W <= win_w {
            canvas.set_draw_color(Color::RGB(195, 195, 200));
            let _ = canvas.fill_rect(Rect::new(new_x + 3, 5, (NEW_BTN_W - 6) as u32, (TAB_BAR_HEIGHT - 10) as u32));
            draw_tab_text(canvas, fonts, tc, "+", new_x, 0, NEW_BTN_W, TAB_BAR_HEIGHT, Color::RGB(40, 40, 40));
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

fn draw_close_btn(
    canvas: &mut Canvas<Window>,
    fonts:  &mut FontCache,
    tc:     &TextureCreator<WindowContext>,
    x: i32, y: i32, size: i32,
) {
    // Subtle rounded background so it's visually obvious
    canvas.set_draw_color(Color::RGBA(150, 150, 150, 60));
    let _ = canvas.fill_rect(Rect::new(x + 2, y + 2, (size - 4) as u32, (size - 4) as u32));

    draw_tab_text(canvas, fonts, tc, "×", x, y, size, size, Color::RGB(90, 90, 90));
}
