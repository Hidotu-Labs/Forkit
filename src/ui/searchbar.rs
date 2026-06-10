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
        draw_nav_button(canvas, fonts, tc, true, back_x, BAR_PAD, BTN_W, BTN_H, can_back);

        // ---- Forward button ----
        let fwd_x = BAR_PAD * 2 + BTN_W;
        draw_nav_button(canvas, fonts, tc, false, fwd_x, BAR_PAD, BTN_W, BTN_H, can_forward);

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
    canvas:   &mut Canvas<Window>,
    _fonts:   &mut FontCache,
    _tc:      &TextureCreator<WindowContext>,
    is_back:  bool,
    x: i32, y: i32, w: i32, h: i32,
    enabled: bool,
) {
    // Background
    let bg = if enabled { Color::RGB(220, 220, 220) } else { Color::RGB(235, 235, 235) };
    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));

    // Border
    canvas.set_draw_color(Color::RGB(190, 190, 190));
    let _ = canvas.draw_rect(Rect::new(x, y, w as u32, h as u32));

    // Draw arrow icon using SDL2 primitives
    let arrow_col = if enabled { Color::RGB(40, 40, 40) } else { Color::RGB(185, 185, 185) };
    draw_arrow(canvas, is_back, x, y, w, h, arrow_col);
}

/// Draw a left (back) or right (forward) arrow icon centred inside a button cell.
///
/// Strategy: define three triangle vertices explicitly, then fill using
/// horizontal scan lines — no ambiguous interpolation direction.
fn draw_arrow(
    canvas:  &mut Canvas<Window>,
    is_back: bool,
    bx: i32, by: i32, bw: i32, bh: i32,
    color: Color,
) {
    let cy = by + bh / 2;

    // Arrow proportions
    let tri_half_h: i32 = bh / 4;      // half-height of the arrowhead
    let tri_depth:  i32 = bw / 4;      // horizontal width of the arrowhead
    let stem_len:   i32 = bw / 5;      // length of the shaft
    let stem_h:     i32 = 2.max(bh / 8);
    let total_w:    i32 = tri_depth + stem_len;

    // Centre the entire arrow (head + shaft) horizontally in the button.
    // For ← the layout is:  [tip] --tri_depth--> [base] --stem_len--> [shaft_end]
    // For → the layout is:  [shaft_end] --stem_len--> [base] --tri_depth--> [tip]
    let arrow_left = bx + (bw - total_w) / 2;   // leftmost pixel of the whole arrow

    canvas.set_draw_color(color);

    if is_back {
        // ← : tip at left-center, two base corners at right top/bottom
        //     tip  = (arrow_left, cy)
        //     base = (arrow_left + tri_depth, cy ± tri_half_h)
        let tip_x  = arrow_left;
        let base_x = arrow_left + tri_depth;

        for dy in -tri_half_h..=tri_half_h {
            // At dy=±tri_half_h the row left edge is at base_x (zero width from tip side)
            // At dy=0 the row left edge is at tip_x (full tri_depth width)
            // Linear interp: left = base_x - (1 - |dy|/tri_half_h) * tri_depth
            let t = (tri_half_h - dy.abs()) as f32 / tri_half_h as f32; // 1.0 at center, 0.0 at edges
            let row_left = base_x - (t * tri_depth as f32).round() as i32;
            let w = (base_x - row_left + 1).max(1) as u32;
            let _ = canvas.fill_rect(Rect::new(row_left, cy + dy, w, 1));
        }
        // Shaft to the right of the triangle base
        let _ = canvas.fill_rect(Rect::new(base_x, cy - stem_h / 2, stem_len as u32, stem_h as u32));
    } else {
        // → : tip at right-center, two base corners at left top/bottom
        //     tip  = (arrow_left + total_w, cy)
        //     base = (arrow_left + stem_len, cy ± tri_half_h)
        let tip_x  = arrow_left + total_w;
        let base_x = arrow_left + stem_len;

        for dy in -tri_half_h..=tri_half_h {
            let t = (tri_half_h - dy.abs()) as f32 / tri_half_h as f32;
            let row_right = base_x + (t * tri_depth as f32).round() as i32;
            let w = (row_right - base_x + 1).max(1) as u32;
            let _ = canvas.fill_rect(Rect::new(base_x, cy + dy, w, 1));
        }
        // Shaft to the left of the triangle base
        let _ = canvas.fill_rect(Rect::new(arrow_left, cy - stem_h / 2, stem_len as u32, stem_h as u32));
    }
}
