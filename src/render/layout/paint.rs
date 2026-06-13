use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::dom::node::{Style, FontFamilyHint};
use crate::render::font::{FontCache, FontFamily};

use crate::render::layout::state::{RoundedClip};

/// Build an `RGBA` colour from separate RGB and alpha components.
pub fn rgba_color(rgb: [u8; 3], alpha: u8) -> Color {
    Color::RGBA(rgb[0], rgb[1], rgb[2], alpha)
}

fn hint_to_family(hint: FontFamilyHint) -> FontFamily {
    match hint {
        FontFamilyHint::Monospace => FontFamily::Monospace,
        FontFamilyHint::Serif     => FontFamily::Serif,
        FontFamilyHint::SansSerif => FontFamily::SansSerif,
    }
}

/// Render `text` at canvas position `(x, y)` applying the scroll offset.
/// Returns `(rendered_width, rendered_height)` in pixels.
pub fn paint_text(
    canvas:     &mut Canvas<Window>,
    tc:         &TextureCreator<WindowContext>,
    fonts:      &mut FontCache,
    text:       &str,
    style:      &Style,
    x: i32, y: i32,
    clip:       Option<&RoundedClip>,
    scroll_y:   i32,
    viewport_h: i32,
) -> (i32, i32) {
    if text.is_empty() { return (0, 0); }
    let family = hint_to_family(style.font_family);
    let font = match fonts.get_family(style.font_size, style.bold, style.italic, family) {
        Some(f) => f,
        None    => return (0, 0),
    };

    // Measure first for background and lines
    let (sw, sh) = font.size_of(text).map(|(w, h)| (w as i32, h as i32)).unwrap_or((0, 0));

    // 1. Background highlight
    if let Some(bg) = style.bg_color {
        if style.border_radius != [0, 0, 0, 0] {
            // Note: fill_rounded_rect currently doesn't support outer clipping,
            // but for highlights inside a clipped container, it's usually small enough.
            fill_rounded_rect(canvas, rgba_color(bg, style.bg_alpha), style.bg_alpha,
                              x, y, sw, sh, style.border_radius,
                              scroll_y, viewport_h);
        } else {
            fill_rect_alpha(canvas, rgba_color(bg, style.bg_alpha), style.bg_alpha,
                            x, y, sw, sh, clip, scroll_y, viewport_h);
        }
    }

    // 2. The Text itself
    let alpha = style.color_alpha;
    let color = rgba_color(style.color, alpha);
    if alpha < 255 {
        canvas.set_blend_mode(BlendMode::Blend);
    }
    if let Ok(surface) = font.render(text).blended(color) {
        let ry = y - scroll_y;
        if ry + sh > 0 && ry < viewport_h {
            if let Ok(mut tex) = tc.create_texture_from_surface(&surface) {
                if alpha < 255 {
                    let _ = tex.set_blend_mode(BlendMode::Blend);
                    let _ = tex.set_alpha_mod(alpha);
                }
                let _ = canvas.copy(&tex, None, Rect::new(x, ry, sw as u32, sh as u32));
            }
        }
    }
    if alpha < 255 {
        canvas.set_blend_mode(BlendMode::None);
    }

    // 3. Decorations (underline / strikethrough)
    let dc = rgba_color(style.color, alpha);
    if style.underline {
        fill_rect_alpha(canvas, dc, alpha, x, y + sh - 2, sw, 1, clip, scroll_y, viewport_h);
    }
    if style.strikethrough {
        fill_rect_alpha(canvas, dc, alpha, x, y + sh / 2, sw, 1, clip, scroll_y, viewport_h);
    }

    (sw, sh)
}

/// Measure `text` without rendering.  Returns `(width, height)`.
pub fn measure_text(fonts: &mut FontCache, text: &str, style: &Style) -> (i32, i32) {
    let family = hint_to_family(style.font_family);
    let font = match fonts.get_family(style.font_size, style.bold, style.italic, family) {
        Some(f) => f,
        None    => return (0, 0),
    };
    font.size_of(text).map(|(w, h)| (w as i32, h as i32)).unwrap_or((0, 0))
}

// ---------------------------------------------------------------------------
// Box shadow
// ---------------------------------------------------------------------------

/// Paint a simple box shadow behind a rectangle.
/// We draw the shadow as a blurred-looking offset rectangle with alpha blending.
pub fn paint_box_shadow(
    canvas:     &mut Canvas<Window>,
    shadow:     &crate::dom::node::BoxShadow,
    x: i32, y: i32, w: i32, h: i32,
    scroll_y:   i32,
    viewport_h: i32,
) {
    let blur = shadow.blur.max(0);
    // Simulate blur with a few semi-transparent passes expanding outward
    let passes = (blur / 2 + 1).min(6);
    for i in 0..passes {
        let expand = i;
        let base_alpha = (shadow.alpha as i32 / passes).max(1).min(255) as u8;
        fill_rect_alpha(
            canvas,
            rgba_color(shadow.color, base_alpha),
            base_alpha,
            x + shadow.offset_x - expand,
            y + shadow.offset_y - expand,
            w + expand * 2,
            h + expand * 2,
            None,
            scroll_y,
            viewport_h,
        );
    }
}

fn isqrt(n: i64) -> i64 {
    if n <= 0 { return 0; }
    let mut x = (n as f64).sqrt() as i64;
    while x * x > n              { x -= 1; }
    while (x + 1) * (x + 1) <= n { x += 1; }
    x
}

pub fn fill_rect(
    canvas:     &mut Canvas<Window>,
    color:      Color,
    x: i32, y: i32, w: i32, h: i32,
    scroll_y:   i32,
    viewport_h: i32,
) {
    fill_rect_alpha(canvas, color, 255, x, y, w, h, None, scroll_y, viewport_h);
}

pub fn fill_rect_alpha(
    canvas:     &mut Canvas<Window>,
    color:      Color,
    alpha:      u8,
    x: i32, y: i32, w: i32, h: i32,
    clip:       Option<&RoundedClip>,
    scroll_y:   i32,
    viewport_h: i32,
) {
    let ry = y - scroll_y;
    if ry + h <= 0 || ry >= viewport_h || w <= 0 || h <= 0 { return; }

    let c = Color::RGBA(color.r, color.g, color.b, alpha);
    if alpha < 255 {
        canvas.set_blend_mode(BlendMode::Blend);
    }
    canvas.set_draw_color(c);

    match clip {
        Some(rc) => {
            // Rounded clipping: paint row by row
            for row in 0..h {
                let abs_y = y + row;
                let row_ry = abs_y - scroll_y;
                if row_ry < 0 { continue; }
                if row_ry >= viewport_h { break; }

                // Intersect the row with the rounded clip's horizontal range
                let (cl, cr) = get_rounded_rect_row_range(abs_y - rc.y, rc.h, &rc.radii);
                let safe_lx = rc.x + cl;
                let safe_rx = rc.x + rc.w - cr;

                let draw_lx = x.max(safe_lx);
                let draw_rx = (x + w).min(safe_rx);
                let draw_w  = draw_rx - draw_lx;

                if draw_w > 0 {
                    let _ = canvas.fill_rect(Rect::new(draw_lx, row_ry, draw_w as u32, 1));
                }
            }
        }
        None => {
            let _ = canvas.fill_rect(Rect::new(x, ry, w as u32, h as u32));
        }
    }

    if alpha < 255 {
        canvas.set_blend_mode(BlendMode::None);
    }
}

fn get_rounded_rect_row_range(row: i32, h: i32, r: &[u16; 4]) -> (i32, i32) {
    if row < 0 || row >= h { return (h, h); } // fully clipped

    let left_clip = if row < r[0] as i32 {
        let dy = r[0] as i32 - row - 1;
        let r0 = r[0] as i64;
        r[0] as i32 - isqrt(r0 * r0 - (dy as i64) * (dy as i64)) as i32
    } else if row >= h - r[3] as i32 {
        let dy = row - (h - r[3] as i32);
        let r3 = r[3] as i64;
        r[3] as i32 - isqrt(r3 * r3 - (dy as i64) * (dy as i64)) as i32
    } else {
        0
    };

    let right_clip = if row < r[1] as i32 {
        let dy = r[1] as i32 - row - 1;
        let r1 = r[1] as i64;
        r[1] as i32 - isqrt(r1 * r1 - (dy as i64) * (dy as i64)) as i32
    } else if row >= h - r[2] as i32 {
        let dy = row - (h - r[2] as i32);
        let r2 = r[2] as i64;
        r[2] as i32 - isqrt(r2 * r2 - (dy as i64) * (dy as i64)) as i32
    } else {
        0
    };

    (left_clip, right_clip)
}

pub fn draw_rect(
    canvas:     &mut Canvas<Window>,
    color:      Color,
    x: i32, y: i32, w: i32, h: i32,
    scroll_y:   i32,
    viewport_h: i32,
) {
    let ry = y - scroll_y;
    if ry + h > 0 && ry < viewport_h && w > 0 && h > 0 {
        canvas.set_draw_color(color);
        let _ = canvas.draw_rect(Rect::new(x, ry, w as u32, h as u32));
    }
}

/// Fill a rounded rectangle using horizontal scan-lines. No SDL2_gfx required.
///
/// `radii` order: [top-left, top-right, bottom-right, bottom-left] in pixels.
/// Falls through to plain `fill_rect_alpha` when all radii are zero.
pub fn fill_rounded_rect(
    canvas:     &mut Canvas<Window>,
    color:      Color,
    alpha:      u8,
    x: i32, y: i32, w: i32, h: i32,
    radii:      [u16; 4],
    scroll_y:   i32,
    viewport_h: i32,
) {
    if w <= 0 || h <= 0 { return; }

    // Fast path — no rounding needed
    if radii == [0, 0, 0, 0] {
        fill_rect_alpha(canvas, color, alpha, x, y, w, h, None, scroll_y, viewport_h);
        return;
    }

    // Clamp each radius so it never exceeds half the smaller dimension
    let max_r = ((w.min(h)) / 2).max(0) as u16;
    let r = [
        radii[0].min(max_r),   // top-left
        radii[1].min(max_r),   // top-right
        radii[2].min(max_r),   // bottom-right
        radii[3].min(max_r),   // bottom-left
    ];

    // We paint row by row.  For each row decide the left and right x extents
    // based on which corner region it falls in.
    for row in 0..h {
        let abs_y = y + row;
        let row_ry = abs_y - scroll_y;
        if row_ry >= viewport_h { break; }
        if row_ry < 0 { continue; }

        let (cl, cr) = get_rounded_rect_row_range(row, h, &r);
        let lx   = x + cl;
        let rw   = w - cl - cr;
        if rw <= 0 { continue; }

        fill_rect_alpha(canvas, color, alpha, lx, abs_y, rw, 1, None, scroll_y, viewport_h);
    }
}

/// Draw the outline of a rounded rectangle, 1 px thick on each edge/arc.
///
/// Straight edges are drawn between the two corner arc endpoints; corner arcs
/// are drawn band-by-band (one pixel per band).
pub fn draw_rounded_rect(
    canvas:     &mut Canvas<Window>,
    color:      Color,
    alpha:      u8,
    x: i32, y: i32, w: i32, h: i32,
    radii:      [u16; 4],
    scroll_y:   i32,
    viewport_h: i32,
) {
    if w <= 0 || h <= 0 { return; }

    if radii == [0, 0, 0, 0] {
        // Plain outline: top, bottom, left, right
        fill_rect_alpha(canvas, color, alpha, x,         y,         w, 1, None, scroll_y, viewport_h);
        fill_rect_alpha(canvas, color, alpha, x,         y + h - 1, w, 1, None, scroll_y, viewport_h);
        fill_rect_alpha(canvas, color, alpha, x,         y,         1, h, None, scroll_y, viewport_h);
        fill_rect_alpha(canvas, color, alpha, x + w - 1, y,         1, h, None, scroll_y, viewport_h);
        return;
    }

    let max_r = ((w.min(h)) / 2).max(0) as u16;
    let r = [
        radii[0].min(max_r),
        radii[1].min(max_r),
        radii[2].min(max_r),
        radii[3].min(max_r),
    ];

    // Top edge between top-left and top-right arc endpoints
    let top_left_x  = x + r[0] as i32;
    let top_right_x = x + w - r[1] as i32;
    if top_right_x > top_left_x {
        fill_rect_alpha(canvas, color, alpha,
            top_left_x, y, top_right_x - top_left_x, 1, None, scroll_y, viewport_h);
    }

    // Bottom edge
    let bot_left_x  = x + r[3] as i32;
    let bot_right_x = x + w - r[2] as i32;
    if bot_right_x > bot_left_x {
        fill_rect_alpha(canvas, color, alpha,
            bot_left_x, y + h - 1, bot_right_x - bot_left_x, 1, None, scroll_y, viewport_h);
    }

    // Left edge
    let left_top_y    = y + r[0] as i32;
    let left_bottom_y = y + h - r[3] as i32;
    if left_bottom_y > left_top_y {
        fill_rect_alpha(canvas, color, alpha,
            x, left_top_y, 1, left_bottom_y - left_top_y, None, scroll_y, viewport_h);
    }

    // Right edge
    let right_top_y    = y + r[1] as i32;
    let right_bottom_y = y + h - r[2] as i32;
    if right_bottom_y > right_top_y {
        fill_rect_alpha(canvas, color, alpha,
            x + w - 1, right_top_y, 1, right_bottom_y - right_top_y, None, scroll_y, viewport_h);
    }

    // Corner arcs — draw the outermost pixel ring of each quarter circle
    // Top-left (r[0])
    let r0 = r[0] as i64;
    for row in 0..r[0] as i32 {
        let dy = r[0] as i32 - row - 1;
        let outer = isqrt(r0 * r0 - (dy as i64) * (dy as i64));
        let px = x + r[0] as i32 - outer as i32;
        fill_rect_alpha(canvas, color, alpha, px, y + row, 1, 1, None, scroll_y, viewport_h);
    }
    // Top-right (r[1])
    let r1 = r[1] as i64;
    for row in 0..r[1] as i32 {
        let dy = r[1] as i32 - row - 1;
        let outer = isqrt(r1 * r1 - (dy as i64) * (dy as i64));
        let px = x + w - r[1] as i32 + outer as i32 - 1;
        fill_rect_alpha(canvas, color, alpha, px, y + row, 1, 1, None, scroll_y, viewport_h);
    }
    // Bottom-right (r[2])
    let r2 = r[2] as i64;
    for row in 0..r[2] as i32 {
        let dy = row;
        let outer = isqrt(r2 * r2 - (dy as i64) * (dy as i64));
        let px = x + w - r[2] as i32 + outer as i32 - 1;
        fill_rect_alpha(canvas, color, alpha, px, y + h - r[2] as i32 + row, 1, 1, None, scroll_y, viewport_h);
    }
    // Bottom-left (r[3])
    let r3 = r[3] as i64;
    for row in 0..r[3] as i32 {
        let dy = row;
        let outer = isqrt(r3 * r3 - (dy as i64) * (dy as i64));
        let px = x + r[3] as i32 - outer as i32;
        fill_rect_alpha(canvas, color, alpha, px, y + h - r[3] as i32 + row, 1, 1, None, scroll_y, viewport_h);
    }
}

pub fn paint_disclosure_triangle(
    canvas:     &mut Canvas<Window>,
    color:      Color,
    x:          i32,
    y:          i32,
    size:       i32,
    is_open:    bool,
    scroll_y:   i32,
    viewport_h: i32,
) {
    let ry = y - scroll_y;
    if ry + size < 0 || ry > viewport_h { return; }

    canvas.set_draw_color(color);
    if is_open {
        // Pointing down
        for i in 0..size {
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(x + i, ry + i),
                sdl2::rect::Point::new(x + size * 2 - i, ry + i),
            );
        }
    } else {
        // Pointing right
        for i in 0..size {
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(x + i, ry + i),
                sdl2::rect::Point::new(x + i, ry + size * 2 - i),
            );
        }
    }
}
