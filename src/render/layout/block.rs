use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::pixels::Color;
use crate::dom::node::Element;
use crate::render::font::FontCache;
use crate::render::image::ImageCache;
use crate::dom::parser::get_attr;
use crate::dom::css::{self};
use super::LayoutState;

pub fn layout_element(
    state:    &mut LayoutState,
    canvas:   &mut Canvas<Window>,
    tc:       &TextureCreator<WindowContext>,
    fonts:    &mut FontCache,
    images:   &mut ImageCache,
    base_url: &str,
    el:       &Element,
    max_w:    i32,
) {
    let tag = el.tag.to_lowercase();
    if matches!(tag.as_str(), "head" | "style" | "title" | "meta" | "link") {
        return;
    }
    let is_block = matches!(tag.as_str(), "div" | "p" | "h1" | "h2" | "h3" | "ul" | "li" | "body" | "html" | "header" | "footer" | "section");

    let old_link = state.active_link.clone();
    let old_color = state.current_color;
    let old_bg = state.current_bg_color;
    let old_font_size = state.current_font_size;
    let old_bold = state.current_bold;
    let old_italic = state.current_italic;
    let old_line_height = state.line_height;
    let old_transform = state.current_text_transform;
    let old_opacity = state.current_opacity;
    let old_border_radius = state.current_border_radius;

    // Tag-specific defaults for "exact look as chrome"
    match tag.as_str() {
        "h1" => {
            state.current_font_size = 22;
            state.current_bold = true;
        },
        "h2" => {
            state.current_font_size = 17;
            state.current_bold = true;
        },
        "b" | "strong" => {
            state.current_bold = true;
        },
        "i" | "em" => {
            state.current_italic = true;
        },
        _ => {
            state.current_font_size = 14;
        }
    }

    // 1. Apply global styles from <style> tags
    let mut matching_rules = Vec::new();
    for sheet in &state.stylesheets {
        for rule in &sheet.rules {
            if rule.selector == tag || rule.selector == "*" {
                matching_rules.push(rule.clone());
            }
        }
    }

    for rule in matching_rules {
        for (prop, val) in &rule.properties {
            apply_style_prop(state, prop, val);
        }
    }

    // 2. Apply inline styles (overwrites global)
    if let Some(style_raw) = get_attr(&el.attrs_raw, "style") {
        for part in style_raw.split(';') {
            let mut kv = part.split(':');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                apply_style_prop(state, k.trim(), v.trim());
            }
        }
    }

    state.line_height = (state.current_font_size as f32 * 1.2) as i32;
    let is_header = matches!(tag.as_str(), "h1" | "h2" | "h3");

    if tag == "a" && !el.href.is_empty() {
        state.active_link = Some(el.href.clone());
    }

    if tag == "br" {
        state.cursor_y += state.line_height;
        state.cursor_x = 8;
        return;
    }

    if is_header {
        state.cursor_y += state.line_height / 2;
    }

    let initial_paint = state.paint;
    let _pre_layout_y = state.cursor_y;

    if is_block && state.cursor_x > 8 {
        state.cursor_y += state.line_height;
        state.cursor_x = 8;
    }

    let inner_start_y = state.cursor_y;
    let inner_start_x = state.cursor_x;
    let inner_line_height = state.line_height;

    if is_block && state.current_bg_color.is_some() {
        // Pass 1: Measure height (paint = false)
        state.paint = false;
        for child in &el.children {
            state.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
        }
        let mut end_y = state.cursor_y;
        if state.cursor_x > 8 {
            end_y += state.line_height;
        }
        
        let mut bg_y = inner_start_y;
        let mut bg_h = end_y - inner_start_y;
        
        // Body and HTML backgrounds should fill the whole viewport
        if tag == "body" || tag == "html" {
            bg_h = bg_h.max(state.ctx.viewport_height + state.ctx.scroll_y);
        } else if is_header {
            bg_y -= 4;
            bg_h += 8;
        }
        
        // Draw background
        if state.paint == false && initial_paint {
            if let Some(bg) = state.current_bg_color {
                let alpha = (bg[3] as f32 * state.current_opacity) as u8;
                let (rect_x, rect_w) = if tag == "body" || tag == "html" {
                    (0, max_w)
                } else {
                    (8, max_w - 8)
                };
                let rect = sdl2::rect::Rect::new(rect_x, bg_y - state.ctx.scroll_y, rect_w as u32, bg_h as u32);
                fill_rounded_rect(canvas, rect, state.current_border_radius, Color::RGBA(bg[0], bg[1], bg[2], alpha));
            }
        }

        // Pass 2: Paint (paint = initial_paint)
        state.paint = initial_paint;
        state.cursor_y = inner_start_y;
        state.cursor_x = inner_start_x;
        state.line_height = inner_line_height;
        
        for child in &el.children {
            state.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
        }
    } else {
        // Single pass for regular elements
        for child in &el.children {
            state.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
        }
    }

    if is_block {
        if state.cursor_x > 8 {
            state.cursor_y += state.line_height;
        }
        state.cursor_x = 8;
        if is_header {
            state.cursor_y += state.line_height / 2;
        }
    }
    
    state.active_link = old_link;
    state.current_color = old_color;
    state.current_bg_color = old_bg;
    state.current_font_size = old_font_size;
    state.current_bold = old_bold;
    state.current_italic = old_italic;
    state.line_height = old_line_height;
    state.current_text_transform = old_transform;
    state.current_opacity = old_opacity;
    state.current_border_radius = old_border_radius;
}

fn apply_style_prop(state: &mut LayoutState, prop: &str, val: &str) {
    match prop {
        "color" => {
            if let Some(css_color) = css::color::CssColor::parse(val) {
                let (r, g, b, a) = css_color.to_rgba8();
                state.current_color = [r, g, b, a];
            }
        }
        "background-color" | "background" => {
            if let Some(css_color) = css::color::CssColor::parse(val) {
                state.current_bg_color = Some(css_color.to_rgba8().into());
            }
        }
        "font-size" => {
            if let Some(v) = val.strip_suffix("px").and_then(|v| v.parse::<u16>().ok()) {
                state.current_font_size = v;
            }
        }
        "font-weight" => {
            state.current_bold = matches!(val, "bold" | "700" | "800" | "900");
        }
        "border-radius" => {
            if let Some(v) = val.strip_suffix("px").and_then(|v| v.parse::<i32>().ok()) {
                state.current_border_radius = v;
            }
        }
        "font-style" => {
            state.current_italic = val.eq_ignore_ascii_case("italic");
        }
        "opacity" => {
            if let Some(v) = val.parse::<f32>().ok() {
                state.current_opacity = (state.current_opacity * v).clamp(0.0, 1.0);
            }
        }
        "text-transform" => {
            state.current_text_transform = match val {
                "uppercase"  => crate::render::layout::state::TextTransform::Uppercase,
                "lowercase"  => crate::render::layout::state::TextTransform::Lowercase,
                "capitalize" => crate::render::layout::state::TextTransform::Capitalize,
                _ => crate::render::layout::state::TextTransform::None,
            };
        }
        _ => {}
    }
}

fn fill_rounded_rect(canvas: &mut Canvas<Window>, rect: sdl2::rect::Rect, radius: i32, color: Color) {
    if radius <= 0 {
        canvas.set_draw_color(color);
        let _ = canvas.fill_rect(rect);
        return;
    }

    let r = radius.min(rect.width() as i32 / 2).min(rect.height() as i32 / 2);
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    canvas.set_draw_color(color);

    // Central body
    let center = sdl2::rect::Rect::new(rect.x() + r, rect.y(), (rect.width() as i32 - 2 * r) as u32, rect.height());
    let _ = canvas.fill_rect(center);

    // Side bars
    let left = sdl2::rect::Rect::new(rect.x(), rect.y() + r, r as u32, (rect.height() as i32 - 2 * r) as u32);
    let _ = canvas.fill_rect(left);
    let right = sdl2::rect::Rect::new(rect.x() + rect.width() as i32 - r, rect.y() + r, r as u32, (rect.height() as i32 - 2 * r) as u32);
    let _ = canvas.fill_rect(right);

    // Corner quadrants
    draw_corner(canvas, rect.x() + r, rect.y() + r, r, -1, -1); // Top-left
    draw_corner(canvas, rect.x() + rect.width() as i32 - r - 1, rect.y() + r, r, 1, -1); // Top-right
    draw_corner(canvas, rect.x() + r, rect.y() + rect.height() as i32 - r - 1, r, -1, 1); // Bottom-left
    draw_corner(canvas, rect.x() + rect.width() as i32 - r - 1, rect.y() + rect.height() as i32 - r - 1, r, 1, 1); // Bottom-right
}

fn draw_corner(canvas: &mut Canvas<Window>, cx: i32, cy: i32, r: i32, dx: i32, dy: i32) {
    for y in 0..r {
        for x in 0..r {
            if x * x + y * y <= r * r {
                let px = cx + x * dx;
                let py = cy + y * dy;
                let _ = canvas.draw_point(sdl2::rect::Point::new(px, py));
            }
        }
    }
}
