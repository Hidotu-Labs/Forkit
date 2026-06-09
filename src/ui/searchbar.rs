use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::render::font::FontCache;

/// Height of the address-bar chrome in pixels.
pub const BAR_HEIGHT: i32 = 36;

const BAR_PAD:   i32 = 5;
const BTN_W:     i32 = 32;
const BTN_H:     i32 = BAR_HEIGHT - BAR_PAD * 2;
const FONT_SIZE: u16 = 15;

/// Regions within the bar — used for hit-testing clicks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarRegion { Back, Forward, Input, None }

pub struct SearchBar {
    pub url:     String,
    pub focused: bool,
    /// Pending URL to navigate to (set on Enter, drained by the event loop).
    pub pending: Option<String>,
}

impl SearchBar {
    pub fn new(initial_url: &str) -> Self {
        SearchBar { url: initial_url.to_owned(), focused: false, pending: None }
    }

    // -----------------------------------------------------------------------
    // Input handlers
    // -----------------------------------------------------------------------

    /// Classify a click and update focus state.
    /// Returns which region was clicked.
    pub fn on_click(&mut self, mx: i32, my: i32, win_w: i32) -> BarRegion {
        if my < 0 || my >= BAR_HEIGHT { self.focused = false; return BarRegion::None; }
        let region = self.region_at(mx, win_w);
        self.focused = region == BarRegion::Input;
        region
    }

    pub fn on_text_input(&mut self, text: &str) {
        if self.focused { self.url.push_str(text); }
    }

    pub fn on_backspace(&mut self) {
        if self.focused {
            let mut chars = self.url.chars();
            chars.next_back();
            self.url = chars.as_str().to_string();
        }
    }

    pub fn on_enter(&mut self) {
        if self.focused && !self.url.is_empty() {
            let url = normalise_url(&self.url);
            self.pending = Some(url.clone());
            self.url     = url;
            self.focused = false;
        }
    }

    // -----------------------------------------------------------------------
    // Rendering
    // -----------------------------------------------------------------------

    pub fn draw(
        &self,
        canvas:      &mut Canvas<Window>,
        tc:          &TextureCreator<WindowContext>,
        fonts:       &mut FontCache,
        win_w:       i32,
        can_back:    bool,
        can_forward: bool,
    ) {
        // Chrome background
        canvas.set_draw_color(Color::RGB(235, 235, 235));
        let _ = canvas.fill_rect(Rect::new(0, 0, win_w as u32, BAR_HEIGHT as u32));

        // Bottom separator
        canvas.set_draw_color(Color::RGB(190, 190, 190));
        let _ = canvas.fill_rect(Rect::new(0, BAR_HEIGHT - 1, win_w as u32, 1));

        // ---- Back button ----
        let back_x = BAR_PAD;
        draw_nav_button(canvas, fonts, tc, "←", back_x, BAR_PAD, BTN_W, BTN_H, can_back);

        // ---- Forward button ----
        let fwd_x = BAR_PAD * 2 + BTN_W;
        draw_nav_button(canvas, fonts, tc, "→", fwd_x, BAR_PAD, BTN_W, BTN_H, can_forward);

        // ---- URL input box ----
        let input_x = BAR_PAD * 3 + BTN_W * 2;
        let input_w = (win_w - input_x - BAR_PAD) as u32;
        let input_h = BTN_H as u32;

        let fill = if self.focused { Color::WHITE } else { Color::RGB(248, 248, 248) };
        canvas.set_draw_color(fill);
        let _ = canvas.fill_rect(Rect::new(input_x, BAR_PAD, input_w, input_h));

        let border_col = if self.focused { Color::RGB(100, 149, 237) } else { Color::RGB(200, 200, 200) };
        canvas.set_draw_color(border_col);
        let _ = canvas.draw_rect(Rect::new(input_x, BAR_PAD, input_w, input_h));

        // URL text
        let display = if self.url.is_empty() { "Enter URL or search…" } else { &self.url };
        let style = crate::dom::node::Style {
            font_size: FONT_SIZE,
            color: if self.url.is_empty() { [160, 160, 160] } else { [20, 20, 20] },
            ..Default::default()
        };

        if let Some(font) = fonts.get(style.font_size, false, false) {
            let c = Color::RGB(style.color[0], style.color[1], style.color[2]);
            if let Ok(surf) = font.render(display).blended(c) {
                if let Ok(tex) = tc.create_texture_from_surface(&surf) {
                    let (sw, sh) = (surf.width(), surf.height());
                    let ty = BAR_PAD + (BTN_H - sh as i32) / 2;
                    let max_text_w = (input_w as i32 - 12).max(0) as u32;
                    let draw_w = sw.min(max_text_w);
                    let _ = canvas.copy(
                        &tex,
                        Some(Rect::new(0, 0, draw_w, sh)),
                        Rect::new(input_x + 6, ty, draw_w, sh),
                    );
                }
            }

            // Cursor
            if self.focused {
                if let Ok((tw, _)) = font.size_of(display) {
                    let cx = (input_x + 6 + tw as i32 + 1).min(input_x + input_w as i32 - 4);
                    canvas.set_draw_color(Color::RGB(40, 40, 40));
                    let _ = canvas.fill_rect(Rect::new(cx, BAR_PAD + 3, 2, (BTN_H - 6) as u32));
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Hit-testing
    // -----------------------------------------------------------------------

    pub fn region_at(&self, mx: i32, win_w: i32) -> BarRegion {
        let back_x  = BAR_PAD;
        let fwd_x   = BAR_PAD * 2 + BTN_W;
        let input_x = BAR_PAD * 3 + BTN_W * 2;
        let input_w = win_w - input_x - BAR_PAD;

        if mx >= back_x  && mx < back_x  + BTN_W            { BarRegion::Back    }
        else if mx >= fwd_x   && mx < fwd_x   + BTN_W       { BarRegion::Forward }
        else if mx >= input_x && mx < input_x + input_w      { BarRegion::Input   }
        else                                                  { BarRegion::None    }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn normalise_url(raw: &str) -> String {
    if raw.starts_with("http://")
        || raw.starts_with("https://")
        || raw.starts_with("file://")
    {
        raw.to_owned()
    } else {
        // Default to HTTPS; net::fetch_with_auto_https will fall back to HTTP
        format!("https://{}", raw)
    }
}

fn draw_nav_button(
    canvas: &mut Canvas<Window>,
    fonts:  &mut FontCache,
    tc:     &TextureCreator<WindowContext>,
    label:  &str,
    x: i32, y: i32, w: i32, h: i32,
    enabled: bool,
) {
    let bg = if enabled { Color::RGB(220, 220, 220) } else { Color::RGB(235, 235, 235) };
    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));

    canvas.set_draw_color(Color::RGB(190, 190, 190));
    let _ = canvas.draw_rect(Rect::new(x, y, w as u32, h as u32));

    let text_col = if enabled { Color::RGB(30, 30, 30) } else { Color::RGB(170, 170, 170) };
    if let Some(font) = fonts.get(16, false, false) {
        if let Ok(surf) = font.render(label).blended(text_col) {
            if let Ok(tex) = tc.create_texture_from_surface(&surf) {
                let (sw, sh) = (surf.width(), surf.height());
                let tx = x + (w - sw as i32) / 2;
                let ty = y + (h - sh as i32) / 2;
                let _ = canvas.copy(&tex, None, Rect::new(tx, ty, sw, sh));
            }
        }
    }
}
