use sdl2::pixels::Color;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::image::ImageRWops;

use crate::dom::node::{Style, ListStyleType, BgSize, BgRepeat, Border, BorderStyle};
use crate::render::font::FontCache;
use crate::render::image::ImageCache;
use crate::render::layout::paint::{
    paint_text, measure_text, fill_rect_alpha, fill_rounded_rect,
    draw_rounded_rect, rgba_color, get_rounded_rect_row_range_pub,
};
use crate::render::layout::state::{LayoutState, LINE_SPACING, BLOCK_MARGIN};
use crate::render::layout::block::measure::measure_block_children;pub fn paint_scrollbar(
    ls:      &mut LayoutState,
    canvas:  &mut Canvas<Window>,
    fonts:   &mut FontCache,
    el:      &crate::dom::node::Element,
    box_x:   i32,
    box_y:   i32,
    box_w:   i32,
    box_h:   i32,
    s:       &Style,
) {
    const TRACK_W: i32 = 8;
    const THUMB_W: i32 = 6;
    const MIN_THUMB_H: i32 = 18;
    const TRACK_COLOR:  [u8; 3] = [240, 240, 240];
    const THUMB_COLOR:  [u8; 3] = [180, 180, 180];
    const BORDER_COLOR: [u8; 3] = [210, 210, 210];

    let content_h = measure_block_children(ls, fonts, el, box_x + box_w, s)
        .max(box_h);

    let track_x = box_x + box_w - TRACK_W;
    let track_y = box_y;

    fill_rect_alpha(canvas, rgba_color(TRACK_COLOR, 255), 255,
        track_x, track_y, THUMB_W, box_h,
        ls.rounding_clip.as_ref(),
        ls.ctx.scroll_y, ls.ctx.viewport_height);
    fill_rect_alpha(canvas, rgba_color(BORDER_COLOR, 255), 255,
        track_x, track_y, 1, box_h,
        ls.rounding_clip.as_ref(),
        ls.ctx.scroll_y, ls.ctx.viewport_height);

    let thumb_h = ((box_h as f32 / content_h as f32) * box_h as f32) as i32;
    let thumb_h = thumb_h.max(MIN_THUMB_H).min(box_h);
    let thumb_y = track_y;
    let thumb_x = track_x + 1;
    fill_rect_alpha(canvas, rgba_color(THUMB_COLOR, 255), 255,
        thumb_x, thumb_y, THUMB_W - 1, thumb_h,
        ls.rounding_clip.as_ref(),
        ls.ctx.scroll_y, ls.ctx.viewport_height);

    const HTRACK_H: i32 = 8;
    let htrack_y = box_y + box_h - HTRACK_H;
    let htrack_w = box_w - TRACK_W;
    fill_rect_alpha(canvas, rgba_color(TRACK_COLOR, 255), 255,
        box_x, htrack_y, htrack_w, HTRACK_H - 1,
        ls.rounding_clip.as_ref(),
        ls.ctx.scroll_y, ls.ctx.viewport_height);
    fill_rect_alpha(canvas, rgba_color(BORDER_COLOR, 255), 255,
        box_x, htrack_y, htrack_w, 1,
        ls.rounding_clip.as_ref(),
        ls.ctx.scroll_y, ls.ctx.viewport_height);
    let hthumb_w = ((htrack_w as f32 * 0.6) as i32).max(MIN_THUMB_H).min(htrack_w);
    fill_rect_alpha(canvas, rgba_color(THUMB_COLOR, 255), 255,
        box_x, htrack_y + 1, hthumb_w, HTRACK_H - 2,
        ls.rounding_clip.as_ref(),
        ls.ctx.scroll_y, ls.ctx.viewport_height);
}

pub fn paint_bullet(
    ls:     &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc:     &TextureCreator<WindowContext>,
    fonts:  &mut FontCache,
    s:      &Style,
) {
    let bstyle = Style { font_size: s.font_size, color: s.color, ..Default::default() };
    let line_h = (s.font_size as f32 * s.line_height_mul) as i32;
    
    if let Some(count) = ls.ol_stack.last_mut() {
        *count += 1;
        let bullet = format!("{}.", count);
        let (bw, bh) = measure_text(fonts, &bullet, &bstyle);
        let gap = 3; // tight gap between number and text, like a browser
        let bx = ls.cursor_x - bw - gap;
        // Vertically center the number on the line
        let by = ls.cursor_y + (line_h - bh) / 2;
        paint_text(canvas, tc, fonts, &bullet, &bstyle, bx, by,
                   ls.rounding_clip.as_ref(),
                   ls.ctx.scroll_y, ls.ctx.viewport_height);
        return;
    }

    let size = (s.font_size as i32 / 4).max(4);
    // Gap between bullet and text: ~0.4em
    let gap = (s.font_size as i32 * 2 / 5).max(5);
    let bx = ls.cursor_x - size - gap;
    // Vertically center bullet on the text line
    let by = ls.cursor_y + (line_h / 2) - (size / 2);

    match s.list_style_type {
        ListStyleType::None => {}
        ListStyleType::Square => {
            fill_rect_alpha(canvas, rgba_color(s.color, 255), 255,
                            bx, by, size, size,
                            ls.rounding_clip.as_ref(),
                            ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
        ListStyleType::Circle => {
            draw_rounded_rect(canvas, rgba_color(s.color, 255), 255,
                              bx, by, size, size, [size as u16 / 2; 4],
                              ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
        _ => {
            // Disc (default)
            fill_rounded_rect(canvas, rgba_color(s.color, 255), 255,
                              bx, by, size, size, [size as u16 / 2; 4],
                              ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
    }
}

pub fn paint_block_bg_image(
    ls:       &LayoutState,
    canvas:   &mut Canvas<Window>,
    tc:       &TextureCreator<WindowContext>,
    images:   &mut ImageCache,
    base_url: &str,
    style:    &Style,
    x: i32, y: i32, w: i32, h: i32,
) {
    let url = match &style.bg_image_url {
        Some(u) => u.clone(),
        None    => return,
    };
    if w <= 0 || h <= 0 { return; }

    let bytes = match images.get_bytes(&url, base_url) {
        Some(b) => b,
        None    => return,
    };

    let fmt = crate::render::image::sniff_image_type(bytes);

    // For SVG files: pre-process to promote <style> CSS rules into inline
    // style= attributes, because nanosvg (used by SDL2_image) ignores
    // <style> blocks and renders everything with default black fill.
    let preprocessed: Vec<u8>;
    let svg_alpha: u8;
    let bytes = if fmt == "SVG" {
        let (p, a) = crate::render::image::preprocess_svg(bytes);
        preprocessed = p;
        svg_alpha = a;
        &preprocessed[..]
    } else {
        svg_alpha = 255;
        bytes
    };

    let rwops = match sdl2::rwops::RWops::from_bytes(bytes) {
        Ok(r)  => r,
        Err(_) => return,
    };
    let surface = match rwops.load_typed(fmt) {
        Ok(s)  => s,
        Err(_) => return,
    };

    let nat_w = surface.width()  as i32;
    let nat_h = surface.height() as i32;
    if nat_w <= 0 || nat_h <= 0 { return; }

    let tex = match tc.create_texture_from_surface(&surface) {
        Ok(t)  => t,
        Err(_) => return,
    };

    // Enable alpha blending so SVG transparency (and any other RGBA image)
    // composites correctly against the already-painted background instead of
    // blending against black (SDL2's default BlendMode::None behaviour).
    let mut tex = tex;
    let _ = tex.set_blend_mode(sdl2::render::BlendMode::Blend);
    let _ = tex.set_alpha_mod(svg_alpha);

    let (tile_w, tile_h) = match style.bg_size {
        BgSize::Cover => {
            let scale_x = w as f32 / nat_w as f32;
            let scale_y = h as f32 / nat_h as f32;
            let scale   = scale_x.max(scale_y);
            ((nat_w as f32 * scale) as i32, (nat_h as f32 * scale) as i32)
        }
        BgSize::Contain => {
            let scale_x = w as f32 / nat_w as f32;
            let scale_y = h as f32 / nat_h as f32;
            let scale   = scale_x.min(scale_y);
            ((nat_w as f32 * scale) as i32, (nat_h as f32 * scale) as i32)
        }
        BgSize::Explicit(ew, eh) => (ew.max(1), eh.max(1)),
        BgSize::Auto => (nat_w, nat_h),
    };
    if tile_w <= 0 || tile_h <= 0 { return; }

    let resolve_pos = |sentinel: i32, box_dim: i32, tile_dim: i32| -> i32 {
        match sentinel {
            5000  => (box_dim - tile_dim) / 2,
            10000 => (box_dim - tile_dim).max(0),
            // Percentage sentinels: values 0–10000 represent 0%–100%.
            // Detect them: any value in 0..=10000 that is a multiple of 100
            // (or the well-known 5000/10000 above) is a percentage sentinel.
            // Actually simpler: values ≥ 100 that would be unreasonable as raw
            // pixel offsets for positions. We use: if value was set via %, the
            // sentinel is (pct * 100) as i32 which is in 0..=10000.
            // Raw px values can also be in that range so we can't distinguish.
            // Instead, store a flag. For now use a practical heuristic:
            // if the sentinel is in 1..9999 and is a multiple of 100, treat as %.
            n if n > 0 && n < 10000 && n % 100 == 0 => {
                let pct = n as f32 / 10000.0;
                ((box_dim as f32 - tile_dim as f32) * pct) as i32
            }
            n => n,
        }
    };
    let off_x = resolve_pos(style.bg_position.x, w, tile_w);
    let off_y = resolve_pos(style.bg_position.y, h, tile_h);

    let (start_tx, step_x, end_tx) = match style.bg_repeat {
        BgRepeat::Repeat | BgRepeat::RepeatX => {
            let start = if tile_w > 0 {
                off_x - ((off_x.abs() / tile_w + 1) * tile_w)
            } else { off_x };
            (start, tile_w, w)
        }
        _ => (off_x, w + 1, off_x + 1),
    };
    let (start_ty, step_y, end_ty) = match style.bg_repeat {
        BgRepeat::Repeat | BgRepeat::RepeatY => {
            let start = if tile_h > 0 {
                off_y - ((off_y.abs() / tile_h + 1) * tile_h)
            } else { off_y };
            (start, tile_h, h)
        }
        _ => (off_y, h + 1, off_y + 1),
    };

    let scroll_y   = ls.ctx.scroll_y;
    let viewport_h = ls.ctx.viewport_height;

    let mut ty = start_ty;
    while ty < end_ty {
        let abs_y = y + ty;
        let ry    = abs_y - scroll_y;
        if ry < viewport_h && ry + tile_h > 0 {
            let mut tx = start_tx;
            while tx < end_tx {
                let abs_x = x + tx;
                let dst_x  = abs_x.max(x);
                let dst_y  = abs_y.max(y);
                let dst_x2 = (abs_x + tile_w).min(x + w);
                let dst_y2 = (abs_y + tile_h).min(y + h);
                let dst_w  = dst_x2 - dst_x;
                let dst_h  = dst_y2 - dst_y;
                if dst_w > 0 && dst_h > 0 {
                    let src_x  = (dst_x - abs_x) * nat_w / tile_w;
                    let src_y  = (dst_y - abs_y) * nat_h / tile_h;
                    let src_w  = dst_w * nat_w / tile_w;
                    let src_h  = dst_h * nat_h / tile_h;
                    let src = sdl2::rect::Rect::new(src_x, src_y,
                                                     src_w.max(1) as u32,
                                                     src_h.max(1) as u32);
                    let dst = sdl2::rect::Rect::new(dst_x, dst_y - scroll_y,
                                                     dst_w as u32, dst_h as u32);
                    let _ = canvas.copy(&tex, src, dst);
                }
                tx += step_x;
            }
        }
        ty += step_y;
    }
}

pub fn paint_block_bg(
    ls:    &LayoutState,
    canvas: &mut Canvas<Window>,
    style: &Style,
    x: i32, y: i32, w: i32, h: i32,
) {
    let radii = style.border_radius;
    if let Some(bg) = style.bg_color {
        let alpha = style.bg_alpha;
        let color = rgba_color(bg, alpha);
        if radii != [0, 0, 0, 0] {
            fill_rounded_rect(canvas, color, alpha,
                              x, y, w, h, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);
        } else {
            fill_rect_alpha(canvas, color, alpha,
                            x, y, w, h, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
    }
}

/// Paint a CSS `linear-gradient(…)` onto the box (x, y, w, h).
pub fn paint_block_bg_gradient(
    ls:     &LayoutState,
    canvas: &mut Canvas<Window>,
    style:  &Style,
    x: i32, y: i32, w: i32, h: i32,
) {

    let grad = match &style.bg_gradient {
        Some(g) => g,
        None    => return,
    };
    if w <= 0 || h <= 0 { return; }
    if grad.stops.len() < 2 { return; }

    let scroll_y   = ls.ctx.scroll_y;
    let viewport_h = ls.ctx.viewport_height;

    // Convert CSS angle to a direction vector.
    // CSS: 0° = to top, 90° = to right, 180° = to bottom, 270° = to left.
    let rad = (grad.angle_deg - 90.0).to_radians();
    let dx = rad.cos();  // positive → right
    let dy = rad.sin();  // positive → down

    // For each row (or column), project the centre-point onto the gradient axis
    // and interpolate the colour.  We scan scanline-by-scanline for simplicity.
    //
    // The gradient axis runs from (cx - dx*L/2, cy - dy*L/2) to
    // (cx + dx*L/2, cy + dy*L/2) where L = w*|dx| + h*|dy| (the "gradient length").

    let cx = x as f32 + w as f32 / 2.0;
    let cy = y as f32 + h as f32 / 2.0;
    let gradient_len = w as f32 * dx.abs() + h as f32 * dy.abs();
    if gradient_len == 0.0 { return; }

    // Interpolate a colour at position t ∈ [0, 1] along the gradient.
    let interpolate = |t: f32| -> ([u8; 3], u8) {
        let t = t.clamp(0.0, 1.0);
        let stops = &grad.stops;
        // Find surrounding stops
        let mut lo = 0;
        let mut hi = stops.len() - 1;
        for (i, s) in stops.iter().enumerate() {
            if s.pos.unwrap_or(i as f32 / (stops.len() - 1) as f32) <= t {
                lo = i;
            }
        }
        for (i, s) in stops.iter().enumerate().rev() {
            if s.pos.unwrap_or(i as f32 / (stops.len() - 1) as f32) >= t {
                hi = i;
            }
        }
        if lo == hi {
            return (stops[lo].color, stops[lo].alpha);
        }
        let p0 = stops[lo].pos.unwrap_or(lo as f32 / (stops.len() - 1) as f32);
        let p1 = stops[hi].pos.unwrap_or(hi as f32 / (stops.len() - 1) as f32);
        let span = p1 - p0;
        let frac = if span <= 0.0 { 0.0 } else { ((t - p0) / span).clamp(0.0, 1.0) };
        let lerp = |a: u8, b: u8| -> u8 {
            (a as f32 + (b as f32 - a as f32) * frac) as u8
        };
        let c0 = stops[lo].color;
        let c1 = stops[hi].color;
        let a0 = stops[lo].alpha;
        let a1 = stops[hi].alpha;
        ([lerp(c0[0], c1[0]), lerp(c0[1], c1[1]), lerp(c0[2], c1[2])],
         lerp(a0, a1))
    };

    // Paint scanline by scanline.
    for row in 0..h {
        let py = y + row;
        let ry = py - scroll_y;
        if ry < 0 || ry >= viewport_h { continue; }

        // Project the row's midpoint onto the gradient axis.
        let px_f = cx;
        let py_f = py as f32 + 0.5;
        let proj = (px_f - cx) * dx + (py_f - cy) * dy;
        let t    = 0.5 + proj / gradient_len;

        let (color, alpha) = interpolate(t);
        let sdl_color = sdl2::pixels::Color::RGBA(color[0], color[1], color[2], alpha);
        canvas.set_draw_color(sdl_color);
        let _ = canvas.fill_rect(sdl2::rect::Rect::new(x, ry, w as u32, 1));
    }
}

pub fn paint_block_border(
    ls:    &LayoutState,
    canvas: &mut Canvas<Window>,
    style: &Style,
    x: i32, y: i32, w: i32, h: i32,
) {
    let radii = style.border_radius;
    let b     = &style.borders;
    let alpha = style.opacity;

    let has_any = b.top.width > 0 || b.bottom.width > 0
               || b.left.width > 0 || b.right.width > 0;
    if !has_any { return; }

    if radii != [0, 0, 0, 0] {
        let bw_t = b.top.width    as i32;
        let bw_r = b.right.width  as i32;
        let bw_b = b.bottom.width as i32;
        let bw_l = b.left.width   as i32;
        let max_bw = bw_t.max(bw_r).max(bw_b).max(bw_l);

        // Clamp radii for outer and inner boxes
        let clamp_radii = |bx: i32, bw: i32, bh: i32, r: [u16; 4]| -> [u16; 4] {
            let max_r = ((bw.min(bh)) / 2).max(0) as u16;
            [r[0].min(max_r), r[1].min(max_r), r[2].min(max_r), r[3].min(max_r)]
        };

        let outer_r = clamp_radii(x, w, h, radii);

        // Inner box (inset by border widths on each side)
        let inner_x = x + bw_l;
        let inner_y = y + bw_t;
        let inner_w = w - bw_l - bw_r;
        let inner_h = h - bw_t - bw_b;
        let inner_radii = [
            (radii[0] as i32 - bw_l.min(bw_t)).max(0) as u16,
            (radii[1] as i32 - bw_r.min(bw_t)).max(0) as u16,
            (radii[2] as i32 - bw_r.min(bw_b)).max(0) as u16,
            (radii[3] as i32 - bw_l.min(bw_b)).max(0) as u16,
        ];
        let inner_r = if inner_w > 0 && inner_h > 0 {
            Some(clamp_radii(inner_x, inner_w, inner_h, inner_radii))
        } else {
            None
        };

        let scroll_y   = ls.ctx.scroll_y;
        let viewport_h = ls.ctx.viewport_height;

        // Paint the border ring row by row using the scanline approach:
        // For each row, get the outer [lx..rx] span and inner [lx..rx] span,
        // and fill only the ring pixels between them.
        // This correctly handles all border widths and radii without needing
        // to know the background color.
        for row in 0..h {
            let abs_y = y + row;
            let row_ry = abs_y - scroll_y;
            if row_ry < 0 { continue; }
            if row_ry >= viewport_h { break; }

            // Outer span
            let (ocl, ocr) = get_rounded_rect_row_range_pub(row, h, &outer_r);
            let o_lx = x + ocl;
            let o_rx = x + w - ocr;
            if o_rx <= o_lx { continue; }

            // Determine border color for this row (top/bottom portions use their color)
            let is_top_region    = row < bw_t;
            let is_bottom_region = row >= h - bw_b;
            let is_left_color    = !is_top_region && !is_bottom_region;

            // Inner span (the area NOT covered by border)
            let (i_lx, i_rx) = if let Some(ir) = inner_r {
                if inner_w > 0 && inner_h > 0 {
                    let inner_row = row - bw_t;
                    if inner_row >= 0 && inner_row < inner_h {
                        let (icl, icr) = get_rounded_rect_row_range_pub(inner_row, inner_h, &ir);
                        (inner_x + icl, inner_x + inner_w - icr)
                    } else {
                        (o_rx, o_rx) // no inner span on this row
                    }
                } else {
                    (o_rx, o_rx)
                }
            } else {
                (o_rx, o_rx) // no inner box at all — solid fill
            };

            // Left ring segment (from outer left to inner left)
            let left_w = (i_lx - o_lx).max(0);
            if left_w > 0 {
                let left_color = if is_top_region { b.top.color }
                                 else if is_bottom_region { b.bottom.color }
                                 else { b.left.color };
                fill_rect_alpha(canvas, rgba_color(left_color, alpha), alpha,
                                o_lx, abs_y, left_w, 1, None, scroll_y, viewport_h);
            }

            // Right ring segment (from inner right to outer right)
            let right_w = (o_rx - i_rx).max(0);
            if right_w > 0 {
                let right_color = if is_top_region { b.top.color }
                                  else if is_bottom_region { b.bottom.color }
                                  else { b.right.color };
                fill_rect_alpha(canvas, rgba_color(right_color, alpha), alpha,
                                i_rx, abs_y, right_w, 1, None, scroll_y, viewport_h);
            }

            // Top/bottom full-row fill (when inner span doesn't exist for this row)
            if is_top_region || is_bottom_region {
                // Fill the middle gap between left ring and right ring with top/bottom color
                let mid_lx = (o_lx + left_w).max(o_lx);
                let mid_rx = (o_rx - right_w).min(o_rx);
                let mid_w  = (mid_rx - mid_lx).max(0);
                if mid_w > 0 {
                    let mid_color = if is_top_region { b.top.color } else { b.bottom.color };
                    fill_rect_alpha(canvas, rgba_color(mid_color, alpha), alpha,
                                    mid_lx, abs_y, mid_w, 1, None, scroll_y, viewport_h);
                }
            }
        }
    } else {
        let bw_t = b.top.width    as i32;
        let bw_r = b.right.width  as i32;
        let bw_b = b.bottom.width as i32;
        let bw_l = b.left.width   as i32;

        let draw = |canvas: &mut Canvas<Window>, brd: &Border,
                    rx: i32, ry: i32, rw: i32, rh: i32| {
            draw_block_border_segment(ls, canvas, brd, alpha, rx, ry, rw, rh);
        };

        draw(canvas, &b.top,    x,              y,              w,           bw_t);
        draw(canvas, &b.bottom, x,              y + h - bw_b,   w,           bw_b);
        draw(canvas, &b.left,   x,              y,              bw_l,        h);
        draw(canvas, &b.right,  x + w - bw_r,   y,              bw_r,        h);
    }
}

fn draw_block_border_segment(
    ls:     &LayoutState,
    canvas: &mut Canvas<Window>,
    brd:    &Border,
    alpha:  u8,
    rx: i32, ry: i32, rw: i32, rh: i32
) {
    if brd.width == 0 { return; }

    let color = rgba_color(brd.color, 255);
    let scroll_y   = ls.ctx.scroll_y;
    let viewport_h = ls.ctx.viewport_height;

    match brd.style {
        BorderStyle::None => {}

        BorderStyle::Solid => {
            fill_rect_alpha(canvas, color, alpha,
                            rx, ry, rw, rh,
                            ls.rounding_clip.as_ref(), scroll_y, viewport_h);
        }

        BorderStyle::Dashed => {
            // CSS spec / browser behaviour: dash ≈ 3× border-width, gap ≈ 1× border-width.
            let w = brd.width as i32;
            let dash_len = (w * 3).max(4);
            let gap_len  = w.max(2);
            let step     = dash_len + gap_len;
            let is_horizontal = rw >= rh;
            if is_horizontal {
                let mut x = rx;
                while x < rx + rw {
                    let end = (x + dash_len).min(rx + rw);
                    fill_rect_alpha(canvas, color, alpha,
                                    x, ry, end - x, rh,
                                    ls.rounding_clip.as_ref(), scroll_y, viewport_h);
                    x += step;
                }
            } else {
                let mut y = ry;
                while y < ry + rh {
                    let end = (y + dash_len).min(ry + rh);
                    fill_rect_alpha(canvas, color, alpha,
                                    rx, y, rw, end - y,
                                    ls.rounding_clip.as_ref(), scroll_y, viewport_h);
                    y += step;
                }
            }
        }

        BorderStyle::Dotted => {
            // CSS spec: dots are circles with diameter = border-width, gap = border-width.
            let w    = brd.width as i32;
            let dot  = w.max(1);           // diameter
            let step = (dot * 2).max(2);   // dot + equal gap
            let r    = [dot as u16 / 2; 4]; // radius for fill_rounded_rect
            let is_horizontal = rw >= rh;
            if is_horizontal {
                // Centre dot vertically within the border stripe.
                let cy = ry + (rh - dot) / 2;
                let mut x = rx;
                while x < rx + rw {
                    let end = (x + dot).min(rx + rw);
                    let dw  = end - x;
                    fill_rounded_rect(canvas, color, alpha,
                                      x, cy, dw, dot, r,
                                      scroll_y, viewport_h);
                    x += step;
                }
            } else {
                let cx = rx + (rw - dot) / 2;
                let mut y = ry;
                while y < ry + rh {
                    let end = (y + dot).min(ry + rh);
                    let dh  = end - y;
                    fill_rounded_rect(canvas, color, alpha,
                                      cx, y, dot, dh, r,
                                      scroll_y, viewport_h);
                    y += step;
                }
            }
        }
    }
}
