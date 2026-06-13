use sdl2::pixels::Color;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::image::ImageRWops;

use crate::dom::node::{Style, ListStyleType, BgSize, BgRepeat, Border};
use crate::render::font::FontCache;
use crate::render::image::ImageCache;
use crate::render::layout::paint::{
    paint_text, measure_text, fill_rect_alpha, fill_rounded_rect,
    draw_rounded_rect, rgba_color,
};
use crate::render::layout::state::{LayoutState, LINE_SPACING, BLOCK_MARGIN};
use crate::render::layout::block::measure::measure_block_children;

pub fn paint_scrollbar(
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
    
    if let Some(count) = ls.ol_stack.last_mut() {
        *count += 1;
        let bullet = format!("{}. ", count);
        let (bw, _) = measure_text(fonts, &bullet, &bstyle);
        let bx = (ls.cursor_x - bw).max(ls.margin_left);
        paint_text(canvas, tc, fonts, &bullet, &bstyle, bx, ls.cursor_y,
                   ls.rounding_clip.as_ref(),
                   ls.ctx.scroll_y, ls.ctx.viewport_height);
        return;
    }

    match s.list_style_type {
        ListStyleType::None => {}
        ListStyleType::Square => {
            let size = (s.font_size as i32 / 4).max(4);
            let bw = size + 8; // space for bullet + gap
            let bx = (ls.cursor_x - bw + 4).max(ls.margin_left);
            let by = ls.cursor_y + (s.font_size as i32 / 2) - (size / 2);
            fill_rect_alpha(canvas, rgba_color(s.color, 255), 255,
                            bx, by, size, size,
                            ls.rounding_clip.as_ref(),
                            ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
        ListStyleType::Circle => {
            let size = (s.font_size as i32 / 4).max(4);
            let bw = size + 8;
            let bx = (ls.cursor_x - bw + 4).max(ls.margin_left);
            let by = ls.cursor_y + (s.font_size as i32 / 2) - (size / 2);
            draw_rounded_rect(canvas, rgba_color(s.color, 255), 255,
                              bx, by, size, size, [size as u16 / 2; 4],
                              ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
        _ => {
            // Disc
            let size = (s.font_size as i32 / 4).max(4);
            let bw = size + 8;
            let bx = (ls.cursor_x - bw + 4).max(ls.margin_left);
            let by = ls.cursor_y + (s.font_size as i32 / 2) - (size / 2);
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
        BgSize::Auto => (nat_w, nat_h),
    };
    if tile_w <= 0 || tile_h <= 0 { return; }

    let resolve_pos = |sentinel: i32, box_dim: i32, tile_dim: i32| -> i32 {
        match sentinel {
            5000  => (box_dim - tile_dim) / 2,
            10000 => (box_dim - tile_dim).max(0),
            n     => n,
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
        let outline = if b.top.width > 0 { b.top.color } else { b.left.color };
        draw_rounded_rect(canvas, rgba_color(outline, alpha), alpha,
                          x, y, w, h, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);

        if b.left.width > 1 {
            let bw_l = b.left.width as i32;
            fill_rounded_rect(canvas, rgba_color(b.left.color, 255), alpha,
                              x, y, bw_l, h, [radii[0], 0, 0, radii[3]],
                              ls.ctx.scroll_y, ls.ctx.viewport_height);
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
    if brd.width > 0 {
        fill_rect_alpha(canvas, rgba_color(brd.color, 255), alpha,
                        rx, ry, rw, rh, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
    }
}
