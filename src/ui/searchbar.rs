use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::render::font::FontCache;

/// Height of the address-bar chrome in pixels.
pub const BAR_HEIGHT: i32 = 45;

pub const BAR_PAD:   i32 = 6;
const BTN_W:     i32 = 30;
const BTN_H:     i32 = 33; // Exactly 2*pill_r + 1
const FONT_SIZE: u16 = 14;

// Light chrome palette (matches tab bar)
const CHROME_BG:    (u8,u8,u8) = (245, 245, 250);
const INPUT_BG:     (u8,u8,u8) = (255, 255, 255);
const INPUT_BORDER: (u8,u8,u8) = (200, 200, 210);
const INPUT_FOCUS:  (u8,u8,u8) = (0,   120, 215);
const BTN_BG:       (u8,u8,u8) = (235, 235, 240);
const BTN_BORDER:   (u8,u8,u8) = (210, 210, 220);
const ICON_ENABLED: (u8,u8,u8) = (60,  60,  70 );
const ICON_DISABLED:(u8,u8,u8) = (180, 180, 190);
const TEXT_FG:      (u8,u8,u8) = (30,  30,  40 );
const PLACEHOLDER:  (u8,u8,u8) = (140, 140, 160);
const SEP_COL:      (u8,u8,u8) = (215, 215, 225);

/// Regions within the bar — used for hit-testing clicks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarRegion { Back, Forward, History, Input, None }

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
        let (cr, cg, cb) = CHROME_BG;
        canvas.set_draw_color(Color::RGB(cr, cg, cb));
        let _ = canvas.fill_rect(Rect::new(0, 0, win_w as u32, BAR_HEIGHT as u32));

        // Bottom separator
        let (sr, sg, sb) = SEP_COL;
        canvas.set_draw_color(Color::RGB(sr, sg, sb));
        let _ = canvas.fill_rect(Rect::new(0, BAR_HEIGHT - 1, win_w as u32, 1));

        // Back button
        let back_x = BAR_PAD;
        draw_nav_button(canvas, fonts, tc, true, back_x, BAR_PAD, BTN_W, BTN_H, can_back);

        // Forward button
        let fwd_x = BAR_PAD * 2 + BTN_W;
        draw_nav_button(canvas, fonts, tc, false, fwd_x, BAR_PAD, BTN_W, BTN_H, can_forward);

        // History button (right edge)
        let hist_x = win_w - BAR_PAD - BTN_W;
        draw_history_button(canvas, fonts, tc, hist_x, BAR_PAD, BTN_W, BTN_H);

        // Pill-shaped URL input
        let input_x = BAR_PAD * 3 + BTN_W * 2;
        let input_w = (hist_x - input_x - BAR_PAD).max(0) as u32;
        let input_h = BTN_H as u32;
        let pill_r  = BTN_H / 2;  // corner radius for a perfect pill

        // Input background (pill shape)
        let (ir, ig, ib) = INPUT_BG;
        canvas.set_draw_color(Color::RGB(ir, ig, ib));
        // Middle rect
        let _ = canvas.fill_rect(Rect::new(input_x + pill_r, BAR_PAD, input_w - pill_r as u32 * 2, input_h));
        // Rounded end caps
        draw_filled_circle(canvas, input_x + pill_r, BAR_PAD + BTN_H / 2, pill_r, Color::RGB(ir, ig, ib));
        draw_filled_circle(canvas, input_x + input_w as i32 - pill_r, BAR_PAD + BTN_H / 2, pill_r, Color::RGB(ir, ig, ib));

        // Input border (focus glow or subtle)
        let (br, bg_c, bb) = if self.focused { INPUT_FOCUS } else { INPUT_BORDER };
        let bc = Color::RGB(br, bg_c, bb);
        // Top/bottom border lines
        canvas.set_draw_color(bc);
        let _ = canvas.fill_rect(Rect::new(input_x + pill_r, BAR_PAD, input_w - pill_r as u32 * 2, 1));
        let _ = canvas.fill_rect(Rect::new(input_x + pill_r, BAR_PAD + BTN_H - 1, input_w - pill_r as u32 * 2, 1));
        // Left and right edges via arcs
        draw_circle_arc_left(canvas, input_x + pill_r, BAR_PAD + BTN_H / 2, pill_r, bc);
        draw_circle_arc_right(canvas, input_x + input_w as i32 - pill_r, BAR_PAD + BTN_H / 2, pill_r, bc);

        // URL text
        let display = if self.url.is_empty() { "Enter URL or search…" } else { &self.url };
        let (tr, tg, tb) = if self.url.is_empty() { PLACEHOLDER } else { TEXT_FG };
        if let Some(font) = fonts.get(FONT_SIZE, false, false) {
            let c = Color::RGB(tr, tg, tb);
            if let Ok(surf) = font.render(display).blended(c) {
                if let Ok(tex) = tc.create_texture_from_surface(&surf) {
                    let (sw, sh) = (surf.width(), surf.height());
                    let ty = BAR_PAD + (BTN_H - sh as i32) / 2;
                    let pad_x = pill_r + 4;
                    let max_text_w = (input_w as i32 - pad_x * 2).max(0) as u32;
                    let draw_w = sw.min(max_text_w);
                    let _ = canvas.copy(
                        &tex,
                        Some(Rect::new(0, 0, draw_w, sh)),
                        Rect::new(input_x + pad_x, ty, draw_w, sh),
                    );
                }
            }

            // Blinking cursor
            if self.focused {
                if let Ok((tw, _)) = font.size_of(display) {
                    let pad_x = pill_r + 4;
                    let cx = (input_x + pad_x + tw as i32 + 1).min(input_x + input_w as i32 - pad_x - 2);
                    let (ar, ag, ab) = INPUT_FOCUS;
                    canvas.set_draw_color(Color::RGB(ar, ag, ab));
                    let _ = canvas.fill_rect(Rect::new(cx, BAR_PAD + 4, 2, (BTN_H - 8) as u32));
                }
            }
        }
    }

    pub fn region_at(&self, mx: i32, win_w: i32) -> BarRegion {
        let back_x  = BAR_PAD;
        let fwd_x   = BAR_PAD * 2 + BTN_W;
        let input_x = BAR_PAD * 3 + BTN_W * 2;
        let hist_x  = win_w - BAR_PAD - BTN_W;   // history button at the right edge
        let input_w = hist_x - input_x - BAR_PAD;

        if mx >= back_x  && mx < back_x  + BTN_W            { BarRegion::Back    }
        else if mx >= fwd_x   && mx < fwd_x   + BTN_W       { BarRegion::Forward }
        else if mx >= hist_x  && mx < hist_x  + BTN_W       { BarRegion::History }
        else if mx >= input_x && mx < input_x + input_w     { BarRegion::Input   }
        else                                                 { BarRegion::None    }
    }
}

fn normalise_url(raw: &str) -> String {
    if raw.starts_with("http://")
        || raw.starts_with("https://")
        || raw.starts_with("file://")
        || raw.starts_with("about:")
        || raw.starts_with("forkit://")
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
    // Rounded button background
    let (br, bg_c, bb) = BTN_BG;
    canvas.set_draw_color(Color::RGB(br, bg_c, bb));
    let r = (w.min(h) / 2).min(8);
    // Fill center rect
    let _ = canvas.fill_rect(Rect::new(x + r, y, (w - r * 2) as u32, h as u32));
    let _ = canvas.fill_rect(Rect::new(x, y + r, w as u32, (h - r * 2) as u32));
    // Corner circles
    draw_filled_circle(canvas, x + r,     y + r,     r, Color::RGB(br, bg_c, bb));
    draw_filled_circle(canvas, x + w - r, y + r,     r, Color::RGB(br, bg_c, bb));
    draw_filled_circle(canvas, x + r,     y + h - r, r, Color::RGB(br, bg_c, bb));
    draw_filled_circle(canvas, x + w - r, y + h - r, r, Color::RGB(br, bg_c, bb));

    // Border
    let (bor_r, bor_g, bor_b) = BTN_BORDER;
    canvas.set_draw_color(Color::RGB(bor_r, bor_g, bor_b));
    let _ = canvas.fill_rect(Rect::new(x + r, y, (w - r * 2) as u32, 1));              // top
    let _ = canvas.fill_rect(Rect::new(x + r, y + h - 1, (w - r * 2) as u32, 1));     // bottom
    let _ = canvas.fill_rect(Rect::new(x, y + r, 1, (h - r * 2) as u32));             // left
    let _ = canvas.fill_rect(Rect::new(x + w - 1, y + r, 1, (h - r * 2) as u32));    // right

    let (ar, ag, ab) = if enabled { ICON_ENABLED } else { ICON_DISABLED };
    draw_arrow(canvas, is_back, x, y, w, h, Color::RGB(ar, ag, ab));
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
    let tri_half_h: i32 = bh / 4;
    let tri_depth:  i32 = bw / 4;
    let stem_len:   i32 = bw / 5;
    let stem_h:     i32 = 2.max(bh / 8);
    let total_w:    i32 = tri_depth + stem_len;

    // Centre the entire arrow (head + shaft) horizontally in the button.
    // For ← the layout is:  [tip] --tri_depth--> [base] --stem_len--> [shaft_end]
    // For → the layout is:  [shaft_end] --stem_len--> [base] --tri_depth--> [tip]
    let arrow_left = bx + (bw - total_w) / 2;   // leftmost pixel of the whole arrow

    canvas.set_draw_color(color);

    if is_back {
        // ← : t
        //     base = (arrow_left + tri_depth, cy ± tri_half_h)
        let _tip_x  = arrow_left;
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
        let _ = canvas.fill_rect(Rect::new(base_x, cy - stem_h / 2, stem_len as u32, stem_h as u32));
    } else {
        // → : tip at right-center, two base corners at left top/bottom
        //     tip  = (arrow_left + total_w, cy)
        //     base = (arrow_left + stem_len, cy ± tri_half_h)
        let _tip_x  = arrow_left + total_w;
        let base_x = arrow_left + stem_len;

        for dy in -tri_half_h..=tri_half_h {
            let t = (tri_half_h - dy.abs()) as f32 / tri_half_h as f32;
            let row_right = base_x + (t * tri_depth as f32).round() as i32;
            let w = (row_right - base_x + 1).max(1) as u32;
            let _ = canvas.fill_rect(Rect::new(base_x, cy + dy, w, 1));
        }
        let _ = canvas.fill_rect(Rect::new(arrow_left, cy - stem_h / 2, stem_len as u32, stem_h as u32));
    }
}

/// Draw the history clock button (⏱ icon rendered as a simple clock face).
fn draw_history_button(
    canvas: &mut Canvas<Window>,
    _fonts: &mut FontCache,
    _tc:    &TextureCreator<WindowContext>,
    x: i32, y: i32, w: i32, h: i32,
) {
    // Rounded button background (same as nav buttons)
    let (br, bg_c, bb) = BTN_BG;
    let r = (w.min(h) / 2).min(8);
    canvas.set_draw_color(Color::RGB(br, bg_c, bb));
    let _ = canvas.fill_rect(Rect::new(x + r, y, (w - r * 2) as u32, h as u32));
    let _ = canvas.fill_rect(Rect::new(x, y + r, w as u32, (h - r * 2) as u32));
    draw_filled_circle(canvas, x + r,     y + r,     r, Color::RGB(br, bg_c, bb));
    draw_filled_circle(canvas, x + w - r, y + r,     r, Color::RGB(br, bg_c, bb));
    draw_filled_circle(canvas, x + r,     y + h - r, r, Color::RGB(br, bg_c, bb));
    draw_filled_circle(canvas, x + w - r, y + h - r, r, Color::RGB(br, bg_c, bb));

    let (bor_r, bor_g, bor_b) = BTN_BORDER;
    canvas.set_draw_color(Color::RGB(bor_r, bor_g, bor_b));
    let _ = canvas.fill_rect(Rect::new(x + r, y, (w - r * 2) as u32, 1));
    let _ = canvas.fill_rect(Rect::new(x + r, y + h - 1, (w - r * 2) as u32, 1));
    let _ = canvas.fill_rect(Rect::new(x, y + r, 1, (h - r * 2) as u32));
    let _ = canvas.fill_rect(Rect::new(x + w - 1, y + r, 1, (h - r * 2) as u32));

    // Clock icon
    let cx = x + w / 2;
    let cy = y + h / 2;
    let clock_r  = (w.min(h) / 2 - 5).max(3);
    let (ir, ig, ib) = ICON_ENABLED;
    canvas.set_draw_color(Color::RGB(ir, ig, ib));
    draw_circle_outline(canvas, cx, cy, clock_r);
    // Hour hand (~10 o'clock)
    let hx = cx + (-clock_r as f32 * 0.45_f32) as i32;
    let hy = cy + (-clock_r as f32 * 0.45_f32) as i32;
    let _ = canvas.draw_line(sdl2::rect::Point::new(cx, cy), sdl2::rect::Point::new(hx, hy));
    // Minute hand (12 o'clock)
    let my2 = cy - (clock_r as f32 * 0.75_f32) as i32;
    let _ = canvas.draw_line(sdl2::rect::Point::new(cx, cy), sdl2::rect::Point::new(cx, my2));
}

/// Fill a circle using horizontal scan lines.
fn draw_filled_circle(canvas: &mut Canvas<Window>, cx: i32, cy: i32, r: i32, color: Color) {
    canvas.set_draw_color(color);
    for dy in -r..=r {
        let half_w = ((r * r - dy * dy) as f64).sqrt().round() as i32;
        let _ = canvas.fill_rect(Rect::new(cx - half_w, cy + dy, (half_w * 2 + 1) as u32, 1));
    }
}

/// Draw only the left half-circle arc.
fn draw_circle_arc_left(canvas: &mut Canvas<Window>, cx: i32, cy: i32, r: i32, color: Color) {
    canvas.set_draw_color(color);
    let mut x = r;
    let mut y = 0_i32;
    let mut err = 0_i32;
    while x >= y {
        for (dx, dy) in [(-x, y),(-x,-y),(-y, x),(-y,-x)] {
            let _ = canvas.draw_point(sdl2::rect::Point::new(cx + dx, cy + dy));
        }
        y += 1;
        err += 1 + 2 * y;
        if 2 * (err - x) + 1 > 0 { x -= 1; err += 1 - 2 * x; }
    }
}

/// Draw only the right half-circle arc.
fn draw_circle_arc_right(canvas: &mut Canvas<Window>, cx: i32, cy: i32, r: i32, color: Color) {
    canvas.set_draw_color(color);
    let mut x = r;
    let mut y = 0_i32;
    let mut err = 0_i32;
    while x >= y {
        for (dx, dy) in [( x, y),( x,-y),( y, x),( y,-x)] {
            let _ = canvas.draw_point(sdl2::rect::Point::new(cx + dx, cy + dy));
        }
        y += 1;
        err += 1 + 2 * y;
        if 2 * (err - x) + 1 > 0 { x -= 1; err += 1 - 2 * x; }
    }
}

/// Draw a circle outline using the midpoint circle algorithm.
fn draw_circle_outline(canvas: &mut Canvas<Window>, cx: i32, cy: i32, r: i32) {
    let mut x = r;
    let mut y = 0_i32;
    let mut err = 0_i32;

    while x >= y {
        let pts = [
            ( x,  y), (-x,  y), ( x, -y), (-x, -y),
            ( y,  x), (-y,  x), ( y, -x), (-y, -x),
        ];
        for (dx, dy) in pts {
            let _ = canvas.draw_point(sdl2::rect::Point::new(cx + dx, cy + dy));
        }
        y += 1;
        err += 1 + 2 * y;
        if 2 * (err - x) + 1 > 0 {
            x -= 1;
            err += 1 - 2 * x;
        }
    }
}
