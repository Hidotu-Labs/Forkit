use sdl2::pixels::Color;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::dom::node::{Element, Node, Display, Visibility};
use crate::render::font::FontCache;
use crate::render::image::ImageCache;

use super::paint::{
    rgba_color, paint_box_shadow, paint_disclosure_triangle,
    fill_rounded_rect, draw_rounded_rect,
};
use super::state::{
    LayoutState, LayoutBox, DetailsArea, MARGIN_RIGHT, BLOCK_MARGIN, LINE_SPACING,
    RoundedClip,
};
use super::{table, flex};

pub mod utils;
pub mod measure;
pub mod paint;
pub mod images;
pub mod forms;

pub use utils::{resolve_size, resolve_pos, open_block, close_block};
pub use measure::{measure_block_children, measure_inline_block_children, measure_block_content_width, advance_text_invisible};
pub use paint::{paint_block_bg, paint_block_bg_gradient, paint_block_bg_image, paint_block_border, paint_scrollbar, paint_bullet};
pub use images::{paint_image, paint_media_placeholder, paint_audio_player};
pub use forms::{paint_form_control, paint_progress};/// Main dispatch for a single element node.
pub fn layout_element(
    ls:       &mut LayoutState,
    canvas:   &mut Canvas<Window>,
    tc:       &TextureCreator<WindowContext>,
    fonts:    &mut FontCache,
    images:   &mut ImageCache,
    base_url: &str,
    el:       &Element,
    max_w:    i32,
) {
    if el.style.display == Display::Hidden { return; }

    if el.style.visibility == Visibility::Hidden {
        layout_element_invisible(ls, fonts, el, max_w);
        return;
    }

    if el.style.position == crate::dom::node::Position::Absolute && !ls.in_absolute_pass {
        layout_absolute_element(ls, canvas, tc, fonts, images, base_url, el, max_w);
        return;
    }

    // position: fixed — rendered relative to the viewport (like absolute but
    // anchored to viewport coords rather than any positioned ancestor).
    if el.style.position == crate::dom::node::Position::Fixed && !ls.in_absolute_pass {
        layout_fixed_element(ls, canvas, tc, fonts, images, base_url, el, max_w);
        return;
    }

    let tag = el.tag.as_str();
    let s   = &el.style;

    let font_size: u16 = if let Some(raw) = &s.font_size_raw {
        let ctx = crate::dom::css::LengthContext {
            base_font_size:  s.font_size,
            percent_base:    16,
            viewport_width:  ls.ctx.viewport_width,
            viewport_height: ls.ctx.viewport_height,
        };
        crate::dom::css::parse_length_ctx(raw, &ctx)
            .map(|n| n.clamp(8, 96) as u16)
            .unwrap_or(s.font_size)
    } else {
        s.font_size
    };

    if matches!(tag, "#document" | "html" | "body") {
        if s.bg_color.is_some() {
            // For body/html: cover the full scrollable content area so the
            // background colour is visible beyond the initial viewport.
            let bg_h = ls.content_height.max(ls.ctx.viewport_height * 4);
            paint_block_bg(ls, canvas, s, 0, 0, max_w, bg_h);
        }
        if s.bg_gradient.is_some() {
            let bg_h = ls.content_height.max(ls.ctx.viewport_height * 4);
            paint_block_bg_gradient(ls, canvas, s, 0, 0, max_w, bg_h);
        }
        if s.bg_image_url.is_some() {
            if s.bg_attachment_fixed {
                // background-attachment: fixed — image is painted relative to the
                // viewport, not the document. Re-draw it covering the current
                // scroll window so it appears stationary as the user scrolls.
                let scroll_y   = ls.ctx.scroll_y;
                let viewport_h = ls.ctx.viewport_height;
                paint_block_bg_image(ls, canvas, tc, images, base_url, s,
                                     0, scroll_y, max_w, viewport_h);
            } else {
                // Normal attachment: cover the full content area.
                let bg_h = ls.content_height.max(ls.ctx.viewport_height * 4);
                paint_block_bg_image(ls, canvas, tc, images, base_url, s,
                                     0, 0, max_w, bg_h);
            }
        }

        if tag == "#document" {
            for child in &el.children {
                ls.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
            }
            return;
        }

        // If the body / html element is a flex container, let the full flex
        // layout engine handle child placement (align-items: center etc.).
        if s.display == crate::dom::node::Display::Flex {
            // Background already painted above (full height). Reset cursor to
            // the body's content origin before handing off to the flex engine.
            ls.margin_left = s.padding.left;
            ls.cursor_x    = s.padding.left;
            ls.cursor_y    = (s.margin.top + s.padding.top).max(0);
            ls.indent      = 0;
            flex::layout_flex(ls, canvas, tc, fonts, images, base_url, el, max_w);
            return;
        }

        let pad_l = s.padding.left;
        let pad_t = s.padding.top;
        let pad_r = s.padding.right;
        let mar_l = s.margin.left;
        let mar_t = s.margin.top;
        let mar_r = s.margin.right;

        let vw = ls.ctx.viewport_width;
        let vh = ls.ctx.viewport_height;
        let body_avail = max_w;
        let resolved_w    = resolve_size(s.size.width,     s.size.width_raw.as_deref(),     body_avail, vw, vh, font_size);
        let resolved_maxw = resolve_size(s.size.max_width, s.size.max_width_raw.as_deref(), body_avail, vw, vh, font_size);

        let (body_left, body_content_w) = {
            let total_side = mar_l + pad_l + mar_r + pad_r;
            let mut content_w = (max_w - total_side).max(1);

            if let Some(w) = resolved_w    { content_w = content_w.min(w); }
            if let Some(w) = resolved_maxw { content_w = content_w.min(w); }

            let left = if s.margin_auto_left || s.margin_auto_right {
                let remaining = (max_w - content_w - pad_l - pad_r - mar_l - mar_r).max(0);
                match (s.margin_auto_left, s.margin_auto_right) {
                    (true,  true)  => mar_l + pad_l + remaining / 2,
                    (false, true)  => mar_l + pad_l,
                    (true,  false) => mar_l + pad_l + remaining,
                    (false, false) => mar_l + pad_l,
                }
            } else {
                mar_l + pad_l
            };
            (left, content_w)
        };

        ls.margin_left = body_left;
        ls.cursor_x    = body_left;
        ls.indent      = 0;
        ls.cursor_y    = (mar_t + pad_t).max(0);

        let body_max_w = (body_left + body_content_w).min(max_w);

        for child in &el.children {
            ls.layout_node(canvas, tc, fonts, images, base_url, child, body_max_w);
        }
        return;
    }

    if tag == "br" {
        ls.newline(font_size, s.line_height_mul);
        return;
    }

    if tag == "img" {
        let src = crate::dom::parser::get_attr(&el.attrs_raw, "src").unwrap_or("");
        if !src.is_empty() {
            paint_image(ls, canvas, tc, images, base_url, src,
                        s.size.width, s.size.height, max_w, s);
        }
        return;
    }

    if tag == "svg" {
        // Inline SVG: the builder stored the raw SVG markup as a single Text child.
        // Extract it, pre-process via the existing SVG pipeline, and render as a texture.
        let svg_markup: Option<String> = el.children.iter().find_map(|child| {
            if let crate::dom::node::Node::Text(t) = child {
                let trimmed = t.text.trim();
                if trimmed.starts_with('<') { Some(trimmed.to_owned()) } else { None }
            } else {
                None
            }
        });

        if let Some(markup) = svg_markup {
            // Break to its own line (block element).
            if ls.cursor_x > ls.margin_left + ls.indent {
                ls.cursor_y += ls.line_height + LINE_SPACING;
                ls.cursor_x  = ls.margin_left + ls.indent;
            }
            ls.cursor_y += BLOCK_MARGIN;

            use sdl2::image::ImageRWops;
            let (processed, svg_alpha) =
                crate::render::image::preprocess_svg(markup.as_bytes());

            let rwops = sdl2::rwops::RWops::from_bytes(&processed).ok();
            let surface = rwops.and_then(|r| r.load_typed("SVG").ok());

            if let Some(surface) = surface {
                let nat_w = surface.width() as i32;
                let nat_h = surface.height() as i32;

                // Resolve display dimensions: explicit CSS size, then viewBox natural size,
                // then cap to available width.
                let avail_w = (max_w - ls.cursor_x).max(1);
                let (dw, dh) = match (s.size.width, s.size.height) {
                    (Some(w), Some(h)) => (w, h),
                    (Some(w), None)    => {
                        let h = if nat_w > 0 { w * nat_h / nat_w } else { nat_h };
                        (w, h)
                    }
                    (None, Some(h))    => {
                        let w = if nat_h > 0 { h * nat_w / nat_h } else { nat_w };
                        (w, h)
                    }
                    (None, None) => {
                        if nat_w > avail_w {
                            let h = if nat_w > 0 { avail_w * nat_h / nat_w } else { nat_h };
                            (avail_w, h)
                        } else {
                            (nat_w, nat_h)
                        }
                    }
                };

                if dw > 0 && dh > 0 {
                    let x       = ls.cursor_x;
                    let y       = ls.cursor_y;
                    let ry      = y - ls.ctx.scroll_y;

                    if ry + dh > 0 && ry < ls.ctx.viewport_height {
                        if let Ok(mut tex) = tc.create_texture_from_surface(&surface) {
                            let _ = tex.set_blend_mode(sdl2::render::BlendMode::Blend);
                            let _ = tex.set_alpha_mod(svg_alpha);
                            let _ = canvas.copy(
                                &tex,
                                None,
                                sdl2::rect::Rect::new(x, ry, dw as u32, dh as u32),
                            );
                        }
                    }

                    ls.cursor_y   += dh + BLOCK_MARGIN;
                    ls.cursor_x    = ls.margin_left;
                    ls.line_height = 16;
                }
            }
        }
        return;
    }

    if tag == "hr" {
        if ls.cursor_x > ls.margin_left + ls.indent {
            ls.newline(font_size, s.line_height_mul);
        }
        ls.cursor_y += BLOCK_MARGIN;
        let ry = ls.cursor_y - ls.ctx.scroll_y;
        if ry >= 0 && ry < ls.ctx.viewport_height {
            canvas.set_draw_color(Color::RGB(180, 180, 180));
            let _ = canvas.fill_rect(sdl2::rect::Rect::new(
                ls.margin_left, ry,
                (max_w - ls.margin_left - MARGIN_RIGHT).max(0) as u32, 2,
            ));
        }
        ls.cursor_y   += 2 + BLOCK_MARGIN;
        ls.line_height = font_size as i32;
        return;
    }

    if matches!(tag, "video" | "audio" | "canvas" | "iframe") {
        let dw = s.size.width.unwrap_or(if tag == "audio" { 300 } else { 320 });
        let dh = s.size.height.unwrap_or(if tag == "audio" { 54 } else { 180 });

        if tag == "audio" {
            // Resolve the audio src — from `src` attribute or first `<source>` child.
            let src = crate::dom::parser::get_attr(&el.attrs_raw, "src")
                .map(|s| s.to_owned())
                .or_else(|| {
                    el.children.iter().find_map(|c| {
                        if let crate::dom::node::Node::Element(child) = c {
                            if child.tag == "source" {
                                return crate::dom::parser::get_attr(&child.attrs_raw, "src")
                                    .map(|s| s.to_owned());
                            }
                        }
                        None
                    })
                })
                .unwrap_or_default();

            // Resolve relative src to absolute URL.
            let resolved_src = if src.is_empty() {
                src.clone()
            } else {
                crate::net::resolve_url(&src, base_url)
            };

            paint_audio_player(
                ls, canvas, tc, fonts,
                &resolved_src,
                dw, dh, s,
            );
            return;
        }

        paint_media_placeholder(ls, canvas, tc, fonts, tag, dw, dh, s, max_w);
        return;
    }

    if tag == "form" {
        let action = crate::dom::parser::get_attr(&el.attrs_raw, "action")
            .unwrap_or("")
            .to_owned();
        let saved_action = std::mem::replace(&mut ls.form_action, action);
        let is_block = s.display_block;
        if is_block {
            if ls.cursor_x > ls.margin_left + ls.indent {
                ls.cursor_y += ls.line_height + LINE_SPACING;
            }
            ls.cursor_y += BLOCK_MARGIN + s.margin.top;
            ls.cursor_x  = ls.margin_left + ls.indent + s.margin.left;
            ls.line_height = font_size as i32;
        }
        for child in &el.children {
            ls.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
        }
        if is_block {
            if ls.cursor_x > ls.margin_left {
                ls.cursor_y += ls.line_height + LINE_SPACING;
            }
            ls.cursor_y += BLOCK_MARGIN + s.margin.bottom;
            ls.cursor_x  = ls.margin_left;
            ls.line_height = 16;
        }
        ls.form_action = saved_action;
        return;
    }

    if matches!(tag, "input" | "button" | "select" | "textarea") {
        paint_form_control(ls, canvas, tc, fonts, el, s, max_w);
        return;
    }

    if tag == "details" {
        let is_open = crate::dom::parser::get_attr(&el.attrs_raw, "open").is_some();
        let saved = ls.indent;
        open_block(ls, s);
        
        let mut summary_found = false;
        for child in &el.children {
            if let Node::Element(child_el) = child {
                if child_el.tag == "summary" {
                    summary_found = true;
                    let sy_top = ls.cursor_y;
                    let sx_left = ls.cursor_x;
                    
                    let triangle_spacing = s.font_size as i32;
                    let saved_indent = ls.indent;
                    ls.indent += triangle_spacing;
                    
                    ls.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
                    
                    paint_disclosure_triangle(canvas, rgba_color(s.color, 255), 
                        sx_left, sy_top + (s.font_size as i32 / 2), 
                        s.font_size as i32 / 4, is_open, 
                        ls.ctx.scroll_y, ls.ctx.viewport_height);

                    ls.details_areas.push(DetailsArea {
                        x: sx_left,
                        y: sy_top,
                        w: max_w - sx_left,
                        h: (ls.cursor_y + ls.line_height - sy_top).max(s.font_size as i32),
                        element_ptr: el as *const Element as usize,
                    });

                    ls.indent = saved_indent;
                    continue;
                }
            }
            if is_open {
                ls.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
            }
        }
        
        if !summary_found {
             let sy_top = ls.cursor_y;
             let sx_left = ls.cursor_x;
             
             let triangle_spacing = s.font_size as i32;
             paint_disclosure_triangle(canvas, rgba_color(s.color, 255), 
                        sx_left, sy_top + (s.font_size as i32 / 3), 
                        s.font_size as i32 / 4, is_open, 
                        ls.ctx.scroll_y, ls.ctx.viewport_height);
             
             let saved_indent = ls.indent;
             ls.indent += triangle_spacing;
             
             let dummy_summary = crate::dom::node::TextNode {
                 text: "Details".to_string(),
                 style: s.clone(),
             };
             ls.layout_node(canvas, tc, fonts, images, base_url, &Node::Text(dummy_summary), max_w);
             
             ls.details_areas.push(DetailsArea {
                 x: sx_left,
                 y: sy_top,
                 w: (max_w - sx_left).max(20),
                 h: (ls.cursor_y + ls.line_height - sy_top).max(s.font_size as i32),
                 element_ptr: el as *const Element as usize,
             });
             ls.indent = saved_indent;
        }

        ls.indent = saved;
        close_block(ls);
        return;
    }

    if tag == "table" {
        table::layout_table(ls, canvas, tc, fonts, images, base_url, el, max_w);
        return;
    }
    if matches!(tag, "tr" | "td" | "th" | "thead" | "tbody" | "tfoot") {
        return;
    }

    if tag == "ol" { ls.ol_stack.push(0); }

    let vw = ls.ctx.viewport_width;
    let vh = ls.ctx.viewport_height;
    let avail_w = (max_w - ls.margin_left - MARGIN_RIGHT).max(1);
    let avail_h = vh;

    let resolved_width      = resolve_size(s.size.width,      s.size.width_raw.as_deref(),      avail_w, vw, vh, font_size);
    let resolved_max_width  = resolve_size(s.size.max_width,  s.size.max_width_raw.as_deref(),  avail_w, vw, vh, font_size);
    let resolved_height     = resolve_size(s.size.height,     s.size.height_raw.as_deref(),     avail_h, vw, vh, font_size);
    let resolved_max_height = resolve_size(s.size.max_height, s.size.max_height_raw.as_deref(), avail_h, vw, vh, font_size);
    let resolved_min_height = resolve_size(s.size.min_height, s.size.min_height_raw.as_deref(), avail_h, vw, vh, font_size);

    let contain_left  = ls.margin_left + ls.indent;
    let contain_right = max_w - MARGIN_RIGHT;
    let contain_w     = (contain_right - contain_left).max(0);

    let mut box_w = if let Some(rw) = resolved_width {
        // CSS default is content-box: the resolved width is the content area.
        // Add padding so the visual box is wide enough and children get rw px.
        rw + s.padding.left + s.padding.right
    } else {
        (contain_w - s.margin.left - s.margin.right).max(0)
    };
    if let Some(mw) = resolved_max_width { box_w = box_w.min(mw + s.padding.left + s.padding.right); }
    
    let remaining = (contain_w - box_w - s.margin.left - s.margin.right).max(0);
    let ml = if s.margin_auto_left || s.margin_auto_right {
        match (s.margin_auto_left, s.margin_auto_right) {
            (true,  true)  => s.margin.left + remaining / 2,
            (false, true)  => s.margin.left,
            (true,  false) => s.margin.left + remaining,
            (false, false) => s.margin.left,
        }
    } else {
        s.margin.left
    };

    let block_x = contain_left + ml;
    // When an explicit width was requested, honour it even if it exceeds the
    // current containing-block width (e.g. a 1000px container inside a smaller
    // flex parent).  Only clamp to contain_right when no explicit width is set.
    let block_w = if resolved_width.is_some() {
        box_w.max(0)
    } else {
        box_w.min(contain_right - block_x - s.margin.right).max(0)
    };

    if s.display == Display::Flex {
        flex::layout_flex(ls, canvas, tc, fonts, images, base_url, el, max_w);
        return;
    }

    if s.display == Display::Grid {
        super::grid::layout_grid(ls, canvas, tc, fonts, images, base_url, el, max_w);
        return;
    }

    if s.display == Display::InlineBlock {
        let pad_l = s.padding.left;
        let pad_r = s.padding.right;
        let pad_t = s.padding.top;
        let pad_b = s.padding.bottom;

        let ib_x = ls.cursor_x;
        let ib_y = ls.cursor_y;

        if matches!(tag, "progress" | "meter") {
            paint_progress(ls, canvas, el, s, max_w);
            return;
        }

        // Measure the content area first so we can paint the background
        // before rendering children.  For shrink-to-fit (no explicit width),
        // measure the widest text line across all block/inline children.
        let content_w = if let Some(rw) = resolved_width {
            rw
        } else {
            // Check if children contain any block-level elements.
            let has_block_children = el.children.iter().any(|c| {
                matches!(c, crate::dom::node::Node::Element(e) if e.style.display_block)
            });
            if has_block_children {
                // Shrink-to-fit: widest rendered line among block children.
                measure_block_content_width(fonts, &el.children, font_size)
            } else {
                // Pure inline content: sum of all inline widths.
                measure_inline_block_children(fonts, &el.children, font_size)
            }
        };

        // Measure height by doing a dry-run layout.
        // Temporarily advance cursor_x to the content start of the inline-block so
        // that measure_block_children's pending_lh check (cx > margin_left + indent)
        // is always satisfied — otherwise the first inline-block on a fresh line
        // (where cursor_x == margin_left + indent) would measure a zero-height body.
        let content_h = {
            let saved_cx = ls.cursor_x;
            ls.cursor_x = ib_x + pad_l;
            let raw_h = measure_block_children(ls, fonts, el,
                ib_x + pad_l + content_w + pad_r, s);
            ls.cursor_x = saved_cx;
            raw_h.max(font_size as i32)
        };

        let ib_w = (pad_l + content_w + pad_r).max(0);
        let ib_h = (pad_t + content_h + pad_b).max(font_size as i32);

        let radii = s.border_radius;

        if let Some(bg) = s.bg_color {
            let alpha = s.bg_alpha;
            fill_rounded_rect(canvas, rgba_color(bg, alpha), alpha,
                               ib_x, ib_y, ib_w, ib_h, radii,
                               ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
        if s.bg_gradient.is_some() {
            paint_block_bg_gradient(ls, canvas, s, ib_x, ib_y, ib_w, ib_h);
        }
        if s.bg_image_url.is_some() {
            paint_block_bg_image(ls, canvas, tc, images, base_url, s,
                                 ib_x, ib_y, ib_w, ib_h);
        }
        let b = &s.borders;
        let has_border = b.top.width > 0 || b.bottom.width > 0
                      || b.left.width > 0 || b.right.width > 0;
        if has_border {
            let outline = if b.top.width > 0 { b.top.color } else { b.left.color };
            draw_rounded_rect(canvas, rgba_color(outline, 255), 255,
                               ib_x, ib_y, ib_w, ib_h, radii,
                               ls.ctx.scroll_y, ls.ctx.viewport_height);
        }

        // Save layout state, render children into the inline-block's coordinate
        // space, then restore so subsequent inline content continues on the same line.
        let saved_margin_left = ls.margin_left;
        let saved_indent      = ls.indent;

        ls.cursor_x    = ib_x + pad_l;
        ls.cursor_y    = ib_y + pad_t;
        ls.margin_left = ib_x + pad_l;
        ls.indent      = 0;

        let children_max_w = (ib_x + pad_l + content_w).max(ls.cursor_x + 1);
        
        let is_positioned = s.position != crate::dom::node::Position::Static;
        if is_positioned {
            ls.positioned_ancestors.push(LayoutBox { x: ib_x, y: ib_y, w: ib_w, h: ib_h });
        }

        // Phase 1: Normal flow
        for child in &el.children {
            if let Node::Element(e) = child {
                if e.style.position == crate::dom::node::Position::Absolute { continue; }
                if e.style.position == crate::dom::node::Position::Fixed    { continue; }
            }
            ls.layout_node(canvas, tc, fonts, images, base_url, child, children_max_w);
        }

        // Phase 2: Absolute
        for child in &el.children {
            if let Node::Element(e) = child {
                if e.style.position == crate::dom::node::Position::Absolute {
                    ls.layout_node(canvas, tc, fonts, images, base_url, child, children_max_w);
                }
            }
        }

        if is_positioned {
            ls.positioned_ancestors.pop();
        }

        ls.margin_left = saved_margin_left;
        ls.indent      = saved_indent;
        ls.cursor_x    = ib_x + ib_w + s.margin.right;
        ls.cursor_y    = ib_y;   // restore — inline-block sits on the current line
        if ib_h > ls.line_height {
            ls.line_height = ib_h;
        }

        if tag == "ol" { ls.ol_stack.pop(); }
        return;
    }

    let is_block = s.display_block;
    let start_y;
    if is_block {
        if ls.cursor_x > ls.margin_left + ls.indent {
            ls.cursor_y += ls.line_height + LINE_SPACING;
        }
        ls.cursor_y   += BLOCK_MARGIN + s.margin.top;

        ls.cursor_x    = block_x;
        ls.line_height = font_size as i32;

        start_y = ls.cursor_y;
        ls.cursor_y += s.padding.top;
        ls.cursor_x += s.padding.left;

        // Measure once and reuse for all background / shadow paint calls
        // that need to know the element's height before children are laid out.
        let needs_pre_measure = s.box_shadow.is_some()
            || s.bg_color.is_some()
            || s.bg_gradient.is_some()
            || s.bg_image_url.is_some();
        let pre_block_h: Option<i32> = if needs_pre_measure {
            let mut h = measure_block_children(ls, fonts, el, block_x + block_w, s);
            if let Some(fh) = resolved_height     { h = fh; }
            if let Some(mn) = resolved_min_height { h = h.max(mn); }
            if let Some(mx) = resolved_max_height { h = h.min(mx); }
            Some(h)
        } else {
            None
        };

        if let Some(ref shadow) = s.box_shadow {
            let block_h = pre_block_h.unwrap_or(0);
            paint_box_shadow(canvas, shadow, block_x, start_y, block_w, block_h,
                             ls.ctx.scroll_y, ls.ctx.viewport_height);
        }

        if s.bg_color.is_some() {
            let block_h = pre_block_h.unwrap_or(0);
            paint_block_bg(ls, canvas, s, block_x, start_y, block_w, block_h);
        }
        if s.bg_gradient.is_some() {
            let block_h = pre_block_h.unwrap_or(0);
            paint_block_bg_gradient(ls, canvas, s, block_x, start_y, block_w, block_h);
        }
        if s.bg_image_url.is_some() {
            let block_h = pre_block_h.unwrap_or(0);
            paint_block_bg_image(ls, canvas, tc, images, base_url, s,
                                 block_x, start_y, block_w, block_h);
        }

        if tag == "li" {
            paint_bullet(ls, canvas, tc, fonts, s);
        }
    } else {
        start_y = ls.cursor_y;
    }

    let saved_indent = ls.indent;
    if is_block { ls.indent = ls.cursor_x - ls.margin_left; }

    let children_max_w = if is_block {
        (block_x + block_w - s.padding.right).max(ls.cursor_x + 1)
    } else {
        max_w
    };

    let link_start_y = ls.cursor_y;
    let link_start_x = ls.cursor_x;

    // For `header.major` elements, detect heading children and draw the
    // decorative flanking lines (normally rendered via CSS ::before/::after
    // pseudo-elements which Forkit doesn't support).
    let is_header_major = tag == "header"
        && el.class_name.split_ascii_whitespace().any(|c| c == "major");

    let overflow_clips = matches!(
        s.overflow,
        crate::dom::node::Overflow::Hidden
        | crate::dom::node::Overflow::Scroll
        | crate::dom::node::Overflow::Auto
    );
    let needs_clip = is_block
        && overflow_clips
        && (resolved_max_height.is_some() || resolved_height.is_some());
    let saved_clip = canvas.clip_rect();
    let clip_h = resolved_height
        .or(resolved_max_height)
        .unwrap_or(0);

    if needs_clip && clip_h > 0 && block_w > 0 {
        let ry = start_y - ls.ctx.scroll_y;
        canvas.set_clip_rect(sdl2::rect::Rect::new(
            block_x,
            ry,
            block_w as u32,
            clip_h as u32 as u32,
        ));
    }

    let saved_rounding_clip = ls.rounding_clip.clone();
    if is_block && overflow_clips && s.border_radius != [0, 0, 0, 0] {
        ls.rounding_clip = Some(RoundedClip {
            x: block_x,
            y: start_y,
            w: block_w,
            h: clip_h,
            radii: s.border_radius,
        });
    }

    for child in &el.children {
        if let Node::Element(e) = child {
            if e.style.position == crate::dom::node::Position::Absolute { continue; }
            if e.style.position == crate::dom::node::Position::Fixed    { continue; }
        }
        if is_header_major {
            if let crate::dom::node::Node::Element(child_el) = child {
                let ct = child_el.tag.as_str();
                let is_heading = ct.len() == 2
                    && ct.starts_with('h')
                    && ct.as_bytes()[1].is_ascii_digit();
                if is_heading {
                    let heading_y    = ls.cursor_y;
                    let heading_font = child_el.style.font_size;
                    let viewport_w   = ls.ctx.viewport_width;
                    let heading_color = child_el.style.color;

                    // Measure heading text width for symmetric wing placement.
                    // Use the same measurement for both sides so spacing is equal.
                    let text_w = measure_heading_text_width(
                        fonts, &child_el.children, heading_font,
                    );
                    // Half-width rounded to nearest even pixel so both sides
                    // get exactly the same gap.
                    let text_half_w = (text_w + 1) / 2;

                    ls.layout_node(canvas, tc, fonts, images, base_url, child, children_max_w);
                    paint_header_major_decoration(
                        canvas,
                        viewport_w,
                        heading_y,
                        heading_font,
                        text_half_w,
                        heading_color,
                        ls.ctx.scroll_y,
                        ls.ctx.viewport_height,
                    );
                    continue;
                }
            }
        }
        ls.layout_node(canvas, tc, fonts, images, base_url, child, children_max_w);
    }

    if needs_clip {
        canvas.set_clip_rect(saved_clip);
    }
    ls.rounding_clip = saved_rounding_clip;
    ls.indent = saved_indent;

    if s.href.is_some() && !is_block {
        let lw = (ls.cursor_x - link_start_x).max(0);
        let lh = (ls.line_height).max(font_size as i32);
        ls.link_areas.push(super::state::LinkArea {
            x: link_start_x, y: link_start_y,
            w: lw, h: lh,
            href: s.href.clone().unwrap_or_default(),
        });
    }

    if is_block {
        ls.cursor_y += s.padding.bottom;

        let content_left = block_x + s.padding.left;
        let pending_lh = if ls.cursor_x > content_left { ls.line_height } else { 0 };
        let end_y = ls.cursor_y + pending_lh;
        let mut block_h = (end_y - start_y).max(0);

        if let Some(h) = resolved_height     { block_h = h; }
        if let Some(mn) = resolved_min_height { block_h = block_h.max(mn); }
        if let Some(mx) = resolved_max_height {
            if block_h > mx {
                block_h = mx;
                if overflow_clips {
                    ls.cursor_y = start_y + block_h - s.padding.bottom;
                }
            }
        }

        if resolved_height.is_some() || resolved_min_height.is_some() {
            let target_end = start_y + block_h;
            if ls.cursor_y + ls.line_height < target_end {
                ls.cursor_y = target_end - ls.line_height;
            }
        }

        if s.href.is_some() {
            ls.link_areas.push(super::state::LinkArea {
                x: block_x, y: start_y, w: block_w, h: block_h,
                href: s.href.clone().unwrap_or_default(),
            });
        }

        paint_block_border(ls, canvas, s, block_x, start_y, block_w, block_h);
        ls.boxes.push(LayoutBox { x: block_x, y: start_y, w: block_w, h: block_h });

        let is_positioned = s.position != crate::dom::node::Position::Static;
        if is_positioned {
            ls.positioned_ancestors.push(LayoutBox { x: block_x, y: start_y, w: block_w, h: block_h });
        }

        // Phase 2: Absolute
        for child in &el.children {
            if let Node::Element(e) = child {
                if e.style.position == crate::dom::node::Position::Absolute {
                    ls.layout_node(canvas, tc, fonts, images, base_url, child, children_max_w);
                }
            }
        }

        if is_positioned {
            ls.positioned_ancestors.pop();
        }

        let needs_scrollbar = matches!(
            s.overflow,
            crate::dom::node::Overflow::Scroll | crate::dom::node::Overflow::Auto
        ) && block_h > 0 && block_w > 8;
        if needs_scrollbar {
            paint_scrollbar(ls, canvas, fonts, el, block_x, start_y, block_w, block_h, s);
        }

        if ls.cursor_x > ls.margin_left + saved_indent {
            ls.cursor_y += ls.line_height + LINE_SPACING;
        }
        ls.cursor_y   += BLOCK_MARGIN + s.margin.bottom;
        ls.cursor_x    = ls.margin_left + saved_indent;
        ls.line_height = 16;
    }

    if tag == "ol" { ls.ol_stack.pop(); }
}

fn layout_element_invisible(
    ls:    &mut LayoutState,
    fonts: &mut FontCache,
    el:    &Element,
    max_w: i32,
) {
    let s   = &el.style;
    let tag = el.tag.as_str();

    if s.display == Display::Hidden { return; }

    if tag == "br" {
        ls.newline(s.font_size, s.line_height_mul);
        return;
    }

    if s.display_block {
        if ls.cursor_x > ls.margin_left + ls.indent {
            ls.cursor_y += ls.line_height + LINE_SPACING;
        }
        ls.cursor_y   += BLOCK_MARGIN + s.margin.top;
        ls.cursor_x    = ls.margin_left + ls.indent + s.margin.left;
        ls.line_height = s.font_size as i32;

        let start_y = ls.cursor_y;
        ls.cursor_y += s.padding.top;
        ls.cursor_x += s.padding.left;

        let saved_indent = ls.indent;
        ls.indent = ls.cursor_x - ls.margin_left;

        for child in &el.children {
            layout_node_invisible(ls, fonts, child, max_w);
        }
        ls.indent = saved_indent;

        ls.cursor_y += s.padding.bottom;
        let end_y   = ls.cursor_y + ls.line_height;
        let block_h = (end_y - start_y).max(0);

        if ls.cursor_x > ls.margin_left + saved_indent {
            ls.cursor_y += ls.line_height + LINE_SPACING;
        }
        ls.cursor_y   += BLOCK_MARGIN + s.margin.bottom;
        ls.cursor_x    = ls.margin_left + saved_indent;
        ls.line_height = 16;
        let _ = block_h;
    } else {
        for child in &el.children {
            layout_node_invisible(ls, fonts, child, max_w);
        }
    }
}

pub fn layout_node_invisible(
    ls:    &mut LayoutState,
    fonts: &mut FontCache,
    node:  &Node,
    max_w: i32,
) {
    match node {
        Node::Text(t) => {
            let s = &t.style;
            let text = &t.text;
            if text.trim().is_empty() { return; }
            let (tw, th) = fonts.get(s.font_size, s.bold, s.italic)
                .and_then(|f| f.size_of(text.trim()).ok())
                .map(|(w, h)| (w as i32, h as i32))
                .unwrap_or((text.len() as i32 * 8, s.font_size as i32));
            ls.cursor_x += tw;
            if th > ls.line_height { ls.line_height = th; }
        }
        Node::Element(el) => {
            layout_element_invisible(ls, fonts, el, max_w);
        }
    }
}

/// Measure the total pixel width of all text nodes inside a heading element.
fn measure_heading_text_width(
    fonts:     &mut FontCache,
    children:  &[crate::dom::node::Node],
    font_size: u16,
) -> i32 {
    use crate::dom::node::Node;
    use crate::render::layout::paint::measure_text;
    let mut total = 0i32;
    for child in children {
        match child {
            Node::Text(t) => {
                let text = t.text.trim();
                if !text.is_empty() {
                    let (w, _) = measure_text(fonts, text, &t.style);
                    total += w;
                }
            }
            Node::Element(e) => {
                let fs = if e.style.font_size > 0 { e.style.font_size } else { font_size };
                total += measure_heading_text_width(fonts, &e.children, fs);
            }
        }
    }
    total
}

/// Draw the decorative flanking lines for `header.major > h*` elements.
///
/// The lines are derived from `header-major-on-light.svg` (three staggered
/// horizontal lines). On dark backgrounds the site uses `header-major-on-dark.svg`
/// which has lighter lines — we approximate this by using the heading's own
/// color with 20% opacity, which automatically adapts to light/dark contexts.
///
/// Both wings use exactly `text_half_w + em_gap` from the viewport centre so
/// the spacing is perfectly symmetric regardless of font metric rounding.
fn paint_header_major_decoration(
    canvas:      &mut Canvas<Window>,
    viewport_w:  i32,
    heading_y:   i32,
    font_size:   u16,
    text_half_w: i32,
    heading_color: [u8; 3],
    scroll_y:    i32,
    viewport_h:  i32,
) {
    // Derive line colour from the heading's own text colour at 20% opacity.
    // On light backgrounds (dark text) this gives a subtle dark line.
    // On dark backgrounds (light/white text) this gives a subtle light line.
    let [r, g, b] = heading_color;
    let line_color = sdl2::pixels::Color::RGBA(r, g, b, 51); // 51 ≈ 0.2 * 255

    let deco_w = 150i32;
    // Gap = 1em from the text edge, minimum 16px
    let em_gap = (font_size as i32).max(16);
    let deco_h = 20i32;

    // Vertical centre: mid-cap-height of the heading
    let cy = heading_y + (font_size as i32 / 2);

    // Symmetric anchor: both wings are the same distance from viewport centre
    let center_x   = viewport_w / 2;
    let half_offset = text_half_w + em_gap; // same value used for both sides

    // Three staggered lines (x_start, x_end, y_offset_from_deco_top):
    let line_defs: &[(i32, i32, i32)] = &[
        (0,  150,  1),
        (20, 150, 10),
        (40, 150, 19),
    ];

    // ── Left wing: right-aligned, right edge at center_x - half_offset ──
    let left_right = center_x - half_offset;
    let left_left  = left_right - deco_w;
    for &(x0, x1, dy) in line_defs {
        let ax = left_left + x0;
        let bx = left_left + x1;
        let ay = cy - deco_h / 2 + dy;
        let ry = ay - scroll_y;
        if ry >= 0 && ry < viewport_h && ax < bx {
            canvas.set_draw_color(line_color);
            let _ = canvas.draw_line((ax, ry), (bx, ry));
        }
    }

    // ── Right wing: left-aligned, left edge at center_x + half_offset,
    //    mirrored so the widest line is outermost (away from text) ────────
    let right_left = center_x + half_offset;
    for &(x0, x1, dy) in line_defs {
        // Mirror: near edge = deco_w - x1, far edge = deco_w - x0
        let ax = right_left + (deco_w - x1);
        let bx = right_left + (deco_w - x0);
        let ay = cy - deco_h / 2 + dy;
        let ry = ay - scroll_y;
        if ry >= 0 && ry < viewport_h && ax < bx {
            canvas.set_draw_color(line_color);
            let _ = canvas.draw_line((ax, ry), (bx, ry));
        }
    }
}

pub fn layout_absolute_element(
    ls:       &mut LayoutState,
    canvas:   &mut Canvas<Window>,
    tc:       &TextureCreator<WindowContext>,
    fonts:    &mut FontCache,
    images:   &mut ImageCache,
    base_url: &str,
    el:       &Element,
    max_w:    i32,
) {
    let s = &el.style;
    let vw = ls.ctx.viewport_width;
    let vh = ls.ctx.viewport_height;
    
    // 1. Determine containing block dimensions
    let (ctx_x, ctx_y, ctx_w, ctx_h) = if let Some(parent) = ls.positioned_ancestors.last() {
        (parent.x, parent.y, parent.w, parent.h)
    } else {
        (0, 0, vw, ls.content_height.max(vh))
    };

    let font_size = s.font_size; 

    // 2. Measure size to determine total dimensions of the absolute box
    let box_w = resolve_size(s.size.width, s.size.width_raw.as_deref(), ctx_w, vw, vh, font_size);
    let box_h = resolve_size(s.size.height, s.size.height_raw.as_deref(), ctx_h, vw, vh, font_size);

    let content_w = box_w.unwrap_or_else(|| {
        measure_block_content_width(fonts, &el.children, font_size).min(ctx_w)
    });
    
    let content_h = box_h.unwrap_or_else(|| {
        let saved_x = ls.cursor_x;
        let saved_y = ls.cursor_y;
        let saved_ml = ls.margin_left;
        let saved_ind = ls.indent;
        let saved_lh = ls.line_height;
        
        ls.cursor_x = 0;
        ls.cursor_y = 0;
        ls.margin_left = 0;
        ls.indent = 0;
        ls.line_height = font_size as i32;
        
        let h = measure_block_children(ls, fonts, el, content_w, s);
        
        ls.cursor_x = saved_x;
        ls.cursor_y = saved_y;
        ls.margin_left = saved_ml;
        ls.indent = saved_ind;
        ls.line_height = saved_lh;
        h
    });

    let border_w = (s.borders.left.width + s.borders.right.width) as i32;
    let border_h = (s.borders.top.width + s.borders.bottom.width) as i32;
    let total_w = content_w + s.padding.left + s.padding.right + border_w;
    let total_h = content_h + s.padding.top + s.padding.bottom + border_h;

    // 3. Determine top-left position relative to the document
    let x = if let Some(l) = resolve_pos(s.left, s.left_raw.as_deref(), ctx_w, vw, vh, font_size) {
        ctx_x + l
    } else if let Some(r) = resolve_pos(s.right, s.right_raw.as_deref(), ctx_w, vw, vh, font_size) {
        ctx_x + ctx_w - r - total_w
    } else {
        ctx_x
    };

    let y = if let Some(t) = resolve_pos(s.top, s.top_raw.as_deref(), ctx_h, vw, vh, font_size) {
        ctx_y + t
    } else if let Some(b) = resolve_pos(s.bottom, s.bottom_raw.as_deref(), ctx_h, vw, vh, font_size) {
        ctx_y + ctx_h - b - total_h
    } else {
        ctx_y
    };

    // 4. Delegate INNER layout and painting to layout_element with the recursive flag set.
    // This allows the element to behave like a normal block/flex container at the new coordinate.
    let saved_x = ls.cursor_x;
    let saved_y = ls.cursor_y;
    let saved_ml = ls.margin_left;
    let saved_ind = ls.indent;
    let saved_lh = ls.line_height;

    // Position the cursor so that the inner layout (which adds mar.top/left and BLOCK_MARGIN)
    // lands exactly at our calculated (x, y).
    let is_block = s.display_block || s.display == crate::dom::node::Display::Flex || s.display == crate::dom::node::Display::Grid;
    if is_block {
        ls.cursor_x = x - s.margin.left;
        ls.cursor_y = y - s.margin.top - crate::render::layout::state::BLOCK_MARGIN;
    } else {
        ls.cursor_x = x;
        ls.cursor_y = y;
    }
    ls.margin_left = ls.cursor_x;
    ls.indent      = 0;

    // max_w for the inner pass should be the absolute box's right edge.
    let inner_max_w = x + total_w + s.margin.right;
    
    ls.in_absolute_pass = true;
    layout_element(ls, canvas, tc, fonts, images, base_url, el, inner_max_w);
    ls.in_absolute_pass = false;

    ls.cursor_x = saved_x;
    ls.cursor_y = saved_y;
    ls.margin_left = saved_ml;
    ls.indent = saved_ind;
    ls.line_height = saved_lh;
}

/// Lay out a `position: fixed` element.
///
/// Fixed elements are positioned relative to the viewport (not the document),
/// so top/left/right/bottom are resolved against the current viewport dimensions.
/// The element is rendered at `scroll_y + top` so it appears at a fixed position
/// on screen regardless of scroll position.
pub fn layout_fixed_element(
    ls:       &mut LayoutState,
    canvas:   &mut Canvas<Window>,
    tc:       &TextureCreator<WindowContext>,
    fonts:    &mut FontCache,
    images:   &mut ImageCache,
    base_url: &str,
    el:       &Element,
    max_w:    i32,
) {
    let s = &el.style;
    let vw = ls.ctx.viewport_width;
    let vh = ls.ctx.viewport_height;

    // Containing block for fixed = viewport
    let ctx_x = 0;
    let ctx_y = ls.ctx.scroll_y;   // document-space top of the viewport
    let ctx_w = vw;
    let ctx_h = vh;

    let font_size = s.font_size;

    // Measure size
    let box_w = resolve_size(s.size.width, s.size.width_raw.as_deref(), ctx_w, vw, vh, font_size);
    let box_h = resolve_size(s.size.height, s.size.height_raw.as_deref(), ctx_h, vw, vh, font_size);

    let content_w = box_w.unwrap_or_else(|| {
        measure_block_content_width(fonts, &el.children, font_size).min(ctx_w)
    });

    let content_h = box_h.unwrap_or_else(|| {
        let saved_x = ls.cursor_x; let saved_y = ls.cursor_y;
        let saved_ml = ls.margin_left; let saved_ind = ls.indent; let saved_lh = ls.line_height;
        ls.cursor_x = 0; ls.cursor_y = 0;
        ls.margin_left = 0; ls.indent = 0; ls.line_height = font_size as i32;
        let h = measure_block_children(ls, fonts, el, content_w, s);
        ls.cursor_x = saved_x; ls.cursor_y = saved_y;
        ls.margin_left = saved_ml; ls.indent = saved_ind; ls.line_height = saved_lh;
        h
    });

    let border_w = (s.borders.left.width + s.borders.right.width) as i32;
    let border_h = (s.borders.top.width + s.borders.bottom.width) as i32;
    let total_w = content_w + s.padding.left + s.padding.right + border_w;
    let total_h = content_h + s.padding.top + s.padding.bottom + border_h;

    // Resolve position relative to viewport, then convert to document space
    let x = if let Some(l) = resolve_pos(s.left, s.left_raw.as_deref(), ctx_w, vw, vh, font_size) {
        ctx_x + l
    } else if let Some(r) = resolve_pos(s.right, s.right_raw.as_deref(), ctx_w, vw, vh, font_size) {
        ctx_x + ctx_w - r - total_w
    } else {
        ctx_x
    };

    let y = if let Some(t) = resolve_pos(s.top, s.top_raw.as_deref(), ctx_h, vw, vh, font_size) {
        ctx_y + t
    } else if let Some(b) = resolve_pos(s.bottom, s.bottom_raw.as_deref(), ctx_h, vw, vh, font_size) {
        ctx_y + ctx_h - b - total_h
    } else {
        ctx_y
    };

    let saved_x  = ls.cursor_x;  let saved_y  = ls.cursor_y;
    let saved_ml = ls.margin_left; let saved_ind = ls.indent; let saved_lh = ls.line_height;

    let is_block = s.display_block
        || s.display == crate::dom::node::Display::Flex
        || s.display == crate::dom::node::Display::Grid;
    if is_block {
        ls.cursor_x = x - s.margin.left;
        ls.cursor_y = y - s.margin.top - crate::render::layout::state::BLOCK_MARGIN;
    } else {
        ls.cursor_x = x;
        ls.cursor_y = y;
    }
    ls.margin_left = ls.cursor_x;
    ls.indent      = 0;

    let inner_max_w = x + total_w + s.margin.right;

    ls.in_absolute_pass = true;
    layout_element(ls, canvas, tc, fonts, images, base_url, el, inner_max_w);
    ls.in_absolute_pass = false;

    ls.cursor_x    = saved_x;
    ls.cursor_y    = saved_y;
    ls.margin_left = saved_ml;
    ls.indent      = saved_ind;
    ls.line_height = saved_lh;
}
