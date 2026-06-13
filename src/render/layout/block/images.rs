use sdl2::pixels::Color;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::image::ImageRWops;

use crate::dom::node::Style;
use crate::render::font::FontCache;
use crate::render::image::ImageCache;
use crate::render::layout::paint::{
    paint_text, measure_text, fill_rect_alpha,
};
use crate::render::layout::state::{LayoutState, LINE_SPACING, BLOCK_MARGIN};

pub fn paint_image(
    ls:          &mut LayoutState,
    canvas:      &mut Canvas<Window>,
    tc:          &TextureCreator<WindowContext>,
    images:      &mut ImageCache,
    base_url:    &str,
    src:         &str,
    width_hint:  Option<i32>,
    height_hint: Option<i32>,
    max_w:       i32,
    style:       &crate::dom::node::Style,
) {
    let ml = style.margin.left;
    let mr = style.margin.right;
    let mt = style.margin.top;
    let mb = style.margin.bottom;
    let pl = style.padding.left;
    let pr = style.padding.right;
    let pt = style.padding.top;
    let pb = style.padding.bottom;

    let bytes = match images.get_bytes(src, base_url) {
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
    let (nat_w, nat_h) = (surface.width() as i32, surface.height() as i32);
    let (dw, dh) = match (width_hint, height_hint) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None)    => { let h = if nat_w > 0 { w * nat_h / nat_w } else { nat_h }; (w, h) }
        (None, Some(h))    => { let w = if nat_h > 0 { h * nat_w / nat_h } else { nat_w }; (w, h) }
        (None, None) => {
            let avail = (max_w - ls.cursor_x - ml - pl - pr - mr).max(1);
            if nat_w > avail {
                let h = if nat_w > 0 { avail * nat_h / nat_w } else { nat_h };
                (avail, h)
            } else {
                (nat_w, nat_h)
            }
        }
    };
    if dw <= 0 || dh <= 0 { return; }

    let total_w = ml + pl + dw + pr + mr;

    if ls.cursor_x + total_w > max_w && ls.cursor_x > ls.margin_left {
        ls.cursor_y += ls.line_height + LINE_SPACING;
        ls.cursor_x  = ls.margin_left + ls.indent;
        ls.line_height = 0;
    }

    let box_x = ls.cursor_x + ml;
    let img_x = box_x + pl;
    let box_y = ls.cursor_y + mt;
    let img_y = box_y + pt;

    if let Some(bg) = style.bg_color {
        let box_w = (pl + dw + pr).max(0) as u32;
        let box_h = (pt + dh + pb).max(0) as u32;
        let screen_y = box_y - ls.ctx.scroll_y;
        if box_w > 0 && box_h > 0 && screen_y + box_h as i32 > 0 && screen_y < ls.ctx.viewport_height {
            canvas.set_draw_color(Color::RGBA(bg[0], bg[1], bg[2], style.bg_alpha));
            let _ = canvas.fill_rect(sdl2::rect::Rect::new(box_x, screen_y, box_w, box_h));
        }
    }

    let screen_y = img_y - ls.ctx.scroll_y;
    if screen_y + dh > 0 && screen_y < ls.ctx.viewport_height {
        if let Ok(tex) = tc.create_texture_from_surface(&surface) {
            let _ = canvas.copy(&tex, None, sdl2::rect::Rect::new(img_x, screen_y, dw as u32, dh as u32));
        }
    }

    ls.cursor_x += total_w;

    let full_h = mt + pt + dh + pb + mb;
    if full_h > ls.line_height { ls.line_height = full_h; }
}

pub fn paint_media_placeholder(
    ls:     &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc:     &TextureCreator<WindowContext>,
    fonts:  &mut FontCache,
    kind:   &str,
    dw:     i32,
    dh:     i32,
    s:      &Style,
    _max_w:  i32,
) {
    if ls.cursor_x > ls.margin_left + ls.indent {
        ls.cursor_y += ls.line_height + LINE_SPACING;
        ls.cursor_x  = ls.margin_left + ls.indent;
    }
    ls.cursor_y += BLOCK_MARGIN;

    let x  = ls.cursor_x;
    let y  = ls.cursor_y;
    let ry = y - ls.ctx.scroll_y;
    let bg = s.bg_color.unwrap_or([30, 30, 30]);

    fill_rect_alpha(canvas, Color::RGB(bg[0], bg[1], bg[2]), 255,
                    x, y, dw, dh, ls.rounding_clip.as_ref(),
                    ls.ctx.scroll_y, ls.ctx.viewport_height);

    let label = match kind {
        "video"  => "▶ video",
        "audio"  => "♪ audio",
        "canvas" => "canvas",
        _        => kind,
    };
    let label_style = Style { color: [200, 200, 200], font_size: 13, ..Default::default() };
    if ry + dh > 0 && ry < ls.ctx.viewport_height {
        let (lw, lh2) = measure_text(fonts, label, &label_style);
        let lx = x + (dw - lw) / 2;
        let ly = y + (dh - lh2) / 2;
        paint_text(canvas, tc, fonts, label, &label_style, lx, ly,
                   ls.rounding_clip.as_ref(),
                   ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    ls.cursor_y += dh + BLOCK_MARGIN;
    ls.cursor_x  = ls.margin_left;
    ls.line_height = 16;
}
