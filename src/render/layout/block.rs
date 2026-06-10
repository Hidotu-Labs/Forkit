use sdl2::pixels::Color;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::image::ImageRWops;

use crate::dom::node::{Element, Node, Style, ListStyleType, Display, BgSize, BgRepeat};
use crate::dom::css::{parse_length_ctx, LengthContext};
use crate::render::font::FontCache;
use crate::render::image::ImageCache;

use super::paint::{
    paint_text, measure_text, fill_rect_alpha, fill_rounded_rect,
    draw_rounded_rect, rgba_color, paint_box_shadow,
};
use super::state::{LayoutState, LayoutBox, InputArea, InputKind, ButtonArea, ButtonAction, MARGIN_LEFT, MARGIN_RIGHT, BLOCK_MARGIN, LINE_SPACING};
use super::table;

// ---------------------------------------------------------------------------
// Sizing helpers
// ---------------------------------------------------------------------------

/// Resolve one sizing dimension at layout time.
///
/// * `pre_resolved` — value already resolved at cascade time (absolute px/em).
/// * `raw`          — raw CSS string for `%`/`vw`/`vh` units; `None` for
///                    pre-resolved values.
/// * `percent_base` — the reference dimension for `%` (containing-block width
///                    for horizontal props, height for vertical).
/// * `viewport_w/h` — real window dimensions in px.
/// * `font_size`    — element's computed font-size for `em` units.
fn resolve_size(
    pre_resolved: Option<i32>,
    raw:          Option<&str>,
    percent_base: i32,
    viewport_w:   i32,
    viewport_h:   i32,
    font_size:    u16,
) -> Option<i32> {
    if let Some(r) = raw {
        let ctx = LengthContext {
            base_font_size:  font_size,
            percent_base,
            viewport_width:  viewport_w,
            viewport_height: viewport_h,
        };
        parse_length_ctx(r, &ctx).filter(|&n| n > 0)
    } else {
        pre_resolved
    }
}

/// Main dispatch for a single element node.
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

    let tag = el.tag.as_str();
    let s   = &el.style;

    // ── Structural root containers ─────────────────────────────────────────
    if matches!(tag, "#document" | "html" | "body") {
        // Background spans the full viewport width and at least the full viewport height.
        // We use viewport_height (not 32_000) so cover/contain sizing works correctly.
        let bg_h = ls.ctx.viewport_height.max(32_000);
        if s.bg_color.is_some() {
            paint_block_bg(ls, canvas, s, 0, 0, max_w, bg_h);
        }
        if s.bg_image_url.is_some() {
            paint_block_bg_image(ls, canvas, tc, images, base_url, s,
                                 0, 0, max_w, ls.ctx.viewport_height);
        }
        let ml = s.margin.left  + s.padding.left;
        let mt = s.margin.top   + s.padding.top;
        let mr = s.margin.right + s.padding.right;
        if ml > 0 { ls.margin_left = ml; ls.cursor_x = ml; ls.indent = 0; }
        if mt > 0 { ls.cursor_y = mt; }
        let right_edge = if mr > 0 { mr } else { ls.margin_left };
        let body_max_w = (max_w - right_edge).max(ls.margin_left + 1);
        let body_max_w = if let Some(w) = s.size.width {
            let body_w = w.min(body_max_w - ls.margin_left);
            let left   = ((max_w - body_w) / 2).max(0);
            ls.margin_left = left; ls.cursor_x = left; ls.indent = 0;
            left + body_w
        } else { body_max_w };
        for child in &el.children {
            ls.layout_node(canvas, tc, fonts, images, base_url, child, body_max_w);
        }
        return;
    }

    // ── Void / replaced elements ───────────────────────────────────────────
    if tag == "br" {
        ls.newline(s.font_size, s.line_height_mul);
        return;
    }

    if tag == "img" {
        let src = crate::dom::parser::get_attr(&el.attrs_raw, "src").unwrap_or("");
        if !src.is_empty() {
            paint_image(ls, canvas, tc, images, base_url, src,
                        s.size.width, s.size.height, max_w);
        }
        return;
    }

    if tag == "hr" {
        if ls.cursor_x > ls.margin_left + ls.indent {
            ls.newline(s.font_size, s.line_height_mul);
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
        ls.line_height = s.font_size as i32;
        return;
    }

    // ── Media placeholders ─────────────────────────────────────────────────
    if matches!(tag, "video" | "audio" | "canvas") {
        let dw = s.size.width.unwrap_or(if tag == "audio" { 300 } else { 320 });
        let dh = s.size.height.unwrap_or(if tag == "audio" { 36 } else { 180 });
        paint_media_placeholder(ls, canvas, tc, fonts, tag, dw, dh, s, max_w);
        return;
    }

    // ── Form containers ────────────────────────────────────────────────────
    if tag == "form" {
        let action = crate::dom::parser::get_attr(&el.attrs_raw, "action")
            .unwrap_or("")
            .to_owned();
        let saved_action = std::mem::replace(&mut ls.form_action, action);
        // Fall through to normal block layout for children
        let is_block = s.display_block;
        if is_block {
            if ls.cursor_x > ls.margin_left + ls.indent {
                ls.cursor_y += ls.line_height + LINE_SPACING;
            }
            ls.cursor_y += BLOCK_MARGIN + s.margin.top;
            ls.cursor_x  = ls.margin_left + ls.indent + s.margin.left;
            ls.line_height = s.font_size as i32;
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

    // ── Form controls ──────────────────────────────────────────────────────
    if matches!(tag, "input" | "button" | "select" | "textarea") {
        paint_form_control(ls, canvas, tc, fonts, el, s, max_w);
        return;
    }

    if matches!(tag, "progress" | "meter") {
        paint_progress(ls, canvas, s, max_w);
        return;
    }

    // ── <details> ─────────────────────────────────────────────────────────
    if tag == "details" {
        let saved = ls.indent;
        open_block(ls, s);
        for child in &el.children {
            ls.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
        }
        ls.indent = saved;
        close_block(ls);
        return;
    }

    // ── Tables ─────────────────────────────────────────────────────────────
    if tag == "table" {
        table::layout_table(ls, canvas, tc, fonts, images, base_url, el, max_w);
        return;
    }
    if matches!(tag, "tr" | "td" | "th" | "thead" | "tbody" | "tfoot") {
        return;
    }

    // ── <ol> counter ──────────────────────────────────────────────────────
    if tag == "ol" { ls.ol_stack.push(0); }

    // ── Resolve sizing with real viewport / containing-block dimensions ────
    let vw = ls.ctx.viewport_width;
    let vh = ls.ctx.viewport_height;
    // Horizontal: percent_base = available width (max_w minus left margin).
    let avail_w = (max_w - ls.margin_left - MARGIN_RIGHT).max(1);
    let avail_h = vh; // vertical percent resolves against viewport height

    let resolved_width      = resolve_size(s.size.width,      s.size.width_raw.as_deref(),      avail_w, vw, vh, s.font_size);
    let resolved_max_width  = resolve_size(s.size.max_width,  s.size.max_width_raw.as_deref(),  avail_w, vw, vh, s.font_size);
    let resolved_min_width  = resolve_size(s.size.min_width,  s.size.min_width_raw.as_deref(),  avail_w, vw, vh, s.font_size);
    let resolved_height     = resolve_size(s.size.height,     s.size.height_raw.as_deref(),     avail_h, vw, vh, s.font_size);
    let resolved_max_height = resolve_size(s.size.max_height, s.size.max_height_raw.as_deref(), avail_h, vw, vh, s.font_size);
    let resolved_min_height = resolve_size(s.size.min_height, s.size.min_height_raw.as_deref(), avail_h, vw, vh, s.font_size);

    // ── Width clamping ─────────────────────────────────────────────────────
    let effective_max_w = {
        let mut w = max_w;
        // max-width caps the box — stored value is a content width, so add
        // the left origin (margin_left) to convert to an absolute right edge.
        if let Some(mw) = resolved_max_width { w = w.min(ls.margin_left + mw); }
        // explicit width works the same way
        if let Some(fw) = resolved_width     { w = w.min(ls.margin_left + fw); }
        // min-width expands the box when the natural width would be narrower
        if let Some(mn) = resolved_min_width { w = w.max(ls.margin_left + mn); }
        w
    };

    let is_block = s.display_block;

    // ── Block open ─────────────────────────────────────────────────────────
    let start_y;
    if is_block {
        if ls.cursor_x > ls.margin_left + ls.indent {
            ls.cursor_y += ls.line_height + LINE_SPACING;
        }
        ls.cursor_y   += BLOCK_MARGIN + s.margin.top;
        ls.cursor_x    = ls.margin_left + ls.indent + s.margin.left;
        ls.line_height = s.font_size as i32;

        start_y = ls.cursor_y;
        ls.cursor_y += s.padding.top;
        ls.cursor_x += s.padding.left;

        // block_x must include ls.indent so nested blocks align with their
        // cursor position (the same way cursor_x is set above).
        let block_x = ls.margin_left + ls.indent + s.margin.left;

        let block_w = (effective_max_w - block_x - MARGIN_RIGHT - s.margin.right).max(0);

        // Box shadow (behind background)
        if let Some(ref shadow) = s.box_shadow {
            let mut block_h = measure_block_children(ls, fonts, el, effective_max_w, s);
            if let Some(h)  = resolved_height     { block_h = h; }
            if let Some(mn) = resolved_min_height { block_h = block_h.max(mn); }
            if let Some(mx) = resolved_max_height { block_h = block_h.min(mx); }
            paint_box_shadow(canvas, shadow, block_x, start_y, block_w, block_h,
                             ls.ctx.scroll_y, ls.ctx.viewport_height);
        }

        // Background
        if s.bg_color.is_some() {
            let mut block_h = measure_block_children(ls, fonts, el, effective_max_w, s);
            if let Some(h)  = resolved_height     { block_h = h; }
            if let Some(mn) = resolved_min_height { block_h = block_h.max(mn); }
            if let Some(mx) = resolved_max_height { block_h = block_h.min(mx); }
            paint_block_bg(ls, canvas, s, block_x, start_y, block_w, block_h);
        }
        if s.bg_image_url.is_some() {
            let mut block_h = measure_block_children(ls, fonts, el, effective_max_w, s);
            if let Some(h)  = resolved_height     { block_h = h; }
            if let Some(mn) = resolved_min_height { block_h = block_h.max(mn); }
            if let Some(mx) = resolved_max_height { block_h = block_h.min(mx); }
            paint_block_bg_image(ls, canvas, tc, images, base_url, s,
                                 block_x, start_y, block_w, block_h);
        }
    } else {
        start_y = ls.cursor_y;
    }

    // ── <li> bullet ───────────────────────────────────────────────────────
    if tag == "li" {
        paint_bullet(ls, canvas, tc, fonts, s);
    }

    // ── Children ──────────────────────────────────────────────────────────
    let saved_indent = ls.indent;
    if is_block { ls.indent = ls.cursor_x - ls.margin_left; }

    // Subtract this block's right padding and margin so children don't
    // overflow into the right gutter of their parent container.
    let children_max_w = if is_block {
        (effective_max_w - s.padding.right - s.margin.right).max(ls.cursor_x + 1)
    } else {
        effective_max_w
    };

    let link_start_y = ls.cursor_y;
    let link_start_x = ls.cursor_x;

    // ── overflow:hidden clipping ───────────────────────────────────────────
    // When overflow is hidden AND a height constraint applies, use SDL2's
    // clip rect to prevent child content from painting outside the box.
    let needs_clip = is_block
        && s.overflow == crate::dom::node::Overflow::Hidden
        && (resolved_max_height.is_some() || resolved_height.is_some());
    let saved_clip = canvas.clip_rect();
    if needs_clip {
        let clip_h = resolved_height
            .or(resolved_max_height)
            .unwrap_or(0)
            .max(0);
        let block_x_clip = ls.margin_left + ls.indent + s.margin.left;
        let block_w_clip = (effective_max_w - block_x_clip - MARGIN_RIGHT - s.margin.right).max(0);
        let ry = start_y - ls.ctx.scroll_y;
        if clip_h > 0 && block_w_clip > 0 {
            canvas.set_clip_rect(sdl2::rect::Rect::new(
                block_x_clip,
                ry,
                block_w_clip as u32,
                clip_h as u32,
            ));
        }
    }

    for child in &el.children {
        ls.layout_node(canvas, tc, fonts, images, base_url, child, children_max_w);
    }

    // Restore clip rect
    if needs_clip {
        canvas.set_clip_rect(saved_clip);
    }
    ls.indent = saved_indent;

    if s.href.is_some() && !is_block {
        let lw = (ls.cursor_x - link_start_x).max(0);
        let lh = (ls.line_height).max(s.font_size as i32);
        ls.link_areas.push(super::state::LinkArea {
            x: link_start_x, y: link_start_y,
            w: lw, h: lh,
            href: s.href.clone().unwrap_or_default(),
        });
    }

    // ── Block close ────────────────────────────────────────────────────────
    if is_block {
        ls.cursor_y += s.padding.bottom;

        // Use saved_indent (the indent value when this block was opened) so that
        // the border/box positions match the background that was already painted.
        let block_x = ls.margin_left + saved_indent + s.margin.left;
        let block_w = (effective_max_w - block_x - MARGIN_RIGHT - s.margin.right).max(0);
        let end_y   = ls.cursor_y + ls.line_height;
        let mut block_h = (end_y - start_y).max(0);

        // Apply height / min-height / max-height
        if let Some(h) = resolved_height     { block_h = h; }
        if let Some(mn) = resolved_min_height { block_h = block_h.max(mn); }
        if let Some(mx) = resolved_max_height {
            if block_h > mx {
                block_h = mx;
                // For overflow:hidden, snap cursor_y so children beyond max-height
                // are not laid out further (content was already painted, so this
                // prevents the element from pushing subsequent siblings down).
                if s.overflow == crate::dom::node::Overflow::Hidden {
                    ls.cursor_y = start_y + block_h - s.padding.bottom;
                }
            }
        }

        // When an explicit height was set, advance cursor_y to match it
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

        if ls.cursor_x > ls.margin_left + saved_indent {
            ls.cursor_y += ls.line_height + LINE_SPACING;
        }
        ls.cursor_y   += BLOCK_MARGIN + s.margin.bottom;
        ls.cursor_x    = ls.margin_left + saved_indent;
        ls.line_height = 16;
    }

    if tag == "ol" { ls.ol_stack.pop(); }
}

// ---------------------------------------------------------------------------
// Block helpers
// ---------------------------------------------------------------------------

fn open_block(ls: &mut LayoutState, s: &Style) {
    if ls.cursor_x > ls.margin_left + ls.indent {
        ls.cursor_y += ls.line_height + LINE_SPACING;
    }
    ls.cursor_y   += BLOCK_MARGIN;
    ls.cursor_x    = ls.margin_left + ls.indent;
    ls.line_height = s.font_size as i32;
}

fn close_block(ls: &mut LayoutState) {
    if ls.cursor_x > ls.margin_left { ls.cursor_y += ls.line_height + LINE_SPACING; }
    ls.cursor_y   += BLOCK_MARGIN;
    ls.cursor_x    = ls.margin_left;
    ls.line_height = 16;
}

fn paint_bullet(
    ls:     &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc:     &TextureCreator<WindowContext>,
    fonts:  &mut FontCache,
    s:      &Style,
) {
    let bstyle = Style { font_size: s.font_size, color: s.color, ..Default::default() };
    let bullet = if let Some(count) = ls.ol_stack.last_mut() {
        *count += 1;
        format!("{}. ", count)
    } else {
        match s.list_style_type {
            ListStyleType::Circle => "○ ".to_string(),
            ListStyleType::Square => "▪ ".to_string(),
            ListStyleType::None   => return,
            _                     => "• ".to_string(),
        }
    };
    let (bw, _) = measure_text(fonts, &bullet, &bstyle);
    let bx = (ls.cursor_x - bw).max(ls.margin_left);
    paint_text(canvas, tc, fonts, &bullet, &bstyle, bx, ls.cursor_y,
               ls.ctx.scroll_y, ls.ctx.viewport_height);
}

/// Dry-run: measure how tall a block element's children will be.
fn measure_block_children(
    ls:    &LayoutState,
    fonts: &mut FontCache,
    el:    &Element,
    max_w: i32,
    s:     &Style,
) -> i32 {
    let mut cy  = ls.cursor_y;
    let mut cx  = ls.cursor_x;
    let mut lh  = s.font_size as i32;
    let start_y = cy - s.padding.top;
    measure_children_recursive(&el.children, fonts, max_w, &mut cx, &mut cy, &mut lh, ls.indent);
    cy += s.padding.bottom;
    let end_y = cy + lh;
    (end_y - start_y).max(0)
}

fn measure_children_recursive(
    children: &[Node],
    fonts:    &mut FontCache,
    max_w:    i32,
    cx:       &mut i32,
    cy:       &mut i32,
    lh:       &mut i32,
    indent:   i32,
) {
    for child in children {
        match child {
            Node::Text(t) => {
                let words: Vec<&str> = t.text.split_whitespace().collect();
                if words.is_empty() { continue; }
                let mut line = String::new();
                for word in &words {
                    let test = if line.is_empty() { word.to_string() }
                               else { format!("{} {}", line, word) };
                    let (tw, _) = fonts.get(t.style.font_size, t.style.bold, t.style.italic)
                        .and_then(|f| f.size_of(&test).ok())
                        .map(|(w, h)| (w as i32, h as i32))
                        .unwrap_or((test.len() as i32 * 8, t.style.font_size as i32));
                    if tw > max_w - *cx && !line.is_empty() {
                        let line_h = (t.style.font_size as f32 * t.style.line_height_mul) as i32;
                        *cy += (*lh).max(line_h) + LINE_SPACING;
                        *cx  = MARGIN_LEFT + indent;
                        *lh  = t.style.font_size as i32;
                        line = word.to_string();
                    } else {
                        line = test;
                    }
                }
                if !line.is_empty() {
                    let (_, th) = fonts.get(t.style.font_size, t.style.bold, t.style.italic)
                        .and_then(|f| f.size_of(&line).ok())
                        .map(|(w, h)| (w as i32, h as i32))
                        .unwrap_or((0, t.style.font_size as i32));
                    if th > *lh { *lh = th; }
                }
            }
            Node::Element(child_el) => {
                if child_el.style.display == Display::Hidden { continue; }
                let child_tag = child_el.tag.as_str();
                if child_el.style.display_block {
                    if *cx > MARGIN_LEFT + indent { *cy += *lh + LINE_SPACING; }
                    *cy += BLOCK_MARGIN + child_el.style.margin.top;
                    *cx  = MARGIN_LEFT + indent + child_el.style.margin.left;
                    *lh  = child_el.style.font_size as i32;
                    if child_tag.len() == 2 && child_tag.starts_with('h')
                        && child_tag.as_bytes()[1].is_ascii_digit()
                    {
                        *cy += child_el.style.font_size as i32 / 2;
                    }
                    *cy += child_el.style.padding.top;
                    *cx += child_el.style.padding.left;
                    let saved = indent;
                    let new_indent = *cx - MARGIN_LEFT;
                    measure_children_recursive(&child_el.children, fonts, max_w, cx, cy, lh, new_indent);
                    *cy += child_el.style.padding.bottom;
                    if *cx > MARGIN_LEFT { *cy += *lh + LINE_SPACING; }
                    *cy += BLOCK_MARGIN + child_el.style.margin.bottom;
                    *cx  = MARGIN_LEFT + saved;
                    *lh  = 16;
                } else {
                    measure_children_recursive(&child_el.children, fonts, max_w, cx, cy, lh, indent);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Image
// ---------------------------------------------------------------------------

fn paint_image(
    ls:          &mut LayoutState,
    canvas:      &mut Canvas<Window>,
    tc:          &TextureCreator<WindowContext>,
    images:      &mut ImageCache,
    base_url:    &str,
    src:         &str,
    width_hint:  Option<i32>,
    height_hint: Option<i32>,
    max_w:       i32,
) {
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
            let avail = (max_w - ls.cursor_x).max(1);
            if nat_w > avail {
                let h = if nat_w > 0 { avail * nat_h / nat_w } else { nat_h };
                (avail, h)
            } else {
                (nat_w, nat_h)
            }
        }
    };
    if dw <= 0 || dh <= 0 { return; }
    if ls.cursor_x + dw > max_w && ls.cursor_x > ls.margin_left {
        ls.cursor_y += ls.line_height + LINE_SPACING;
        ls.cursor_x  = ls.margin_left + ls.indent;
        ls.line_height = 0;
    }
    let rx = ls.cursor_x;
    let ry = ls.cursor_y - ls.ctx.scroll_y;
    if ry + dh > 0 && ry < ls.ctx.viewport_height {
        if let Ok(tex) = tc.create_texture_from_surface(&surface) {
            let _ = canvas.copy(&tex, None, sdl2::rect::Rect::new(rx, ry, dw as u32, dh as u32));
        }
    }
    ls.cursor_x += dw + 4;
    if dh > ls.line_height { ls.line_height = dh; }
}

// ---------------------------------------------------------------------------
// Media / form / progress placeholders
// ---------------------------------------------------------------------------

fn paint_media_placeholder(
    ls:     &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc:     &TextureCreator<WindowContext>,
    fonts:  &mut FontCache,
    kind:   &str,
    dw:     i32,
    dh:     i32,
    s:      &Style,
    max_w:  i32,
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
                    x, y, dw, dh, ls.ctx.scroll_y, ls.ctx.viewport_height);

    // Label
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
                   ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    ls.cursor_y += dh + BLOCK_MARGIN;
    ls.cursor_x  = ls.margin_left;
    ls.line_height = 16;
    let _ = max_w;
}

fn paint_form_control(
    ls:     &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc:     &TextureCreator<WindowContext>,
    fonts:  &mut FontCache,
    el:     &Element,
    s:      &Style,
    max_w:  i32,
) {
    let tag = el.tag.as_str();
    let input_type = crate::dom::parser::get_attr(&el.attrs_raw, "type")
        .unwrap_or("text")
        .to_ascii_lowercase();

    if input_type == "hidden" { return; }

    // Determine the input kind so hit-testing knows what to focus
    let kind = match tag {
        "textarea" => InputKind::TextArea,
        "input" => match input_type.as_str() {
            "password" => InputKind::Password,
            "text" | "email" | "search" | "tel" | "url" | "number" => InputKind::Text,
            _ => InputKind::Other,
        },
        _ => InputKind::Other,
    };

    // For submit/button inputs, show the value or type as text.
    // Attribute values are raw HTML so they may contain entities (e.g. &#350;)
    // — decode them before display.
    let label: String = if tag == "button" {
        // render children as text inline — let normal layout handle it
        // Just draw the box around the children content area
        String::new()
    } else {
        crate::dom::parser::get_attr(&el.attrs_raw, "value")
            .map(|v| crate::dom::parser::decode_entities(v))
            .unwrap_or_else(|| match input_type.as_str() {
                "submit"  => "Submit".to_owned(),
                "reset"   => "Reset".to_owned(),
                "checkbox"| "radio" => String::new(),
                _         => crate::dom::parser::get_attr(&el.attrs_raw, "placeholder")
                                .map(|p| crate::dom::parser::decode_entities(p))
                                .unwrap_or_default(),
            })
    };

    let ctrl_w = s.size.width.unwrap_or(if tag == "textarea" { 300 } else { 200 });
    let ctrl_h = s.size.height.unwrap_or(if tag == "textarea" { 80 } else { 28 });

    // Checkbox / radio — small box
    if matches!(input_type.as_str(), "checkbox" | "radio") {
        let bx = ls.cursor_x;
        let by = ls.cursor_y;
        let sz = 16i32;
        fill_rect_alpha(canvas, Color::RGB(255, 255, 255), 255,
                        bx, by, sz, sz, ls.ctx.scroll_y, ls.ctx.viewport_height);
        // border
        let bc = Color::RGB(150, 150, 150);
        // top / bottom / left / right 1px
        for (rx2, ry2, rw, rh) in [
            (bx, by, sz, 1), (bx, by + sz - 1, sz, 1),
            (bx, by, 1, sz), (bx + sz - 1, by, 1, sz),
        ] {
            fill_rect_alpha(canvas, bc, 255, rx2, ry2, rw, rh,
                            ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
        ls.cursor_x   += sz + 6;
        if sz > ls.line_height { ls.line_height = sz; }
        return;
    }

    if ls.cursor_x > ls.margin_left { ls.cursor_y += ls.line_height + LINE_SPACING; }
    ls.cursor_y += BLOCK_MARGIN;

    let x  = ls.cursor_x;
    let y  = ls.cursor_y;

    // Assign index and register this control so clicks can focus it
    let input_index = if matches!(kind, InputKind::Text | InputKind::Password | InputKind::TextArea) {
        let idx = ls.input_count;
        ls.input_count += 1;
        // Retrieve live value and focused flag from the existing input_areas if available
        // (filled in by the caller via Tab's input_values / focused_input)
        ls.input_areas.push(InputArea {
            x, y, w: ctrl_w, h: ctrl_h,
            index: idx,
            kind: kind.clone(),
        });
        Some(idx)
    } else {
        None
    };

    // Pull live value out of the extra context stored in LayoutState
    // (We use the scratch fields added for this purpose below)
    let live_value: Option<String> = input_index.and_then(|idx| {
        ls.input_values.get(idx).map(|v| v.clone())
    });
    let is_focused = input_index.map(|idx| ls.focused_input == Some(idx)).unwrap_or(false);

    let bg = s.bg_color.unwrap_or([255, 255, 255]);
    let radii = s.border_radius;

    // Background fill
    if radii != [0, 0, 0, 0] {
        fill_rounded_rect(canvas, Color::RGB(bg[0], bg[1], bg[2]), 255,
                          x, y, ctrl_w, ctrl_h, radii,
                          ls.ctx.scroll_y, ls.ctx.viewport_height);
    } else {
        fill_rect_alpha(canvas, Color::RGB(bg[0], bg[1], bg[2]), 255,
                        x, y, ctrl_w, ctrl_h, ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    // Border — highlight blue when focused
    let border_color = if is_focused { Color::RGB(66, 133, 244) } else { Color::RGB(180, 180, 180) };
    let border_width = if is_focused { 2i32 } else { 1i32 };

    if radii != [0, 0, 0, 0] {
        draw_rounded_rect(canvas, border_color, 255,
                          x, y, ctrl_w, ctrl_h, radii,
                          ls.ctx.scroll_y, ls.ctx.viewport_height);
    } else {
        for bw in 0..border_width {
            let bx = x - bw; let by2 = y - bw;
            let bw2 = ctrl_w + bw * 2; let bh2 = ctrl_h + bw * 2;
            fill_rect_alpha(canvas, border_color, 255, bx,       by2,            bw2, 1, ls.ctx.scroll_y, ls.ctx.viewport_height);
            fill_rect_alpha(canvas, border_color, 255, bx,       by2 + bh2 - 1, bw2, 1, ls.ctx.scroll_y, ls.ctx.viewport_height);
            fill_rect_alpha(canvas, border_color, 255, bx,       by2,            1, bh2, ls.ctx.scroll_y, ls.ctx.viewport_height);
            fill_rect_alpha(canvas, border_color, 255, bx + bw2 - 1, by2,        1, bh2, ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
    }

    // Text inside the control
    let display_text: String = if let Some(ref v) = live_value {
        // For password fields, mask the text
        if kind == InputKind::Password {
            "•".repeat(v.chars().count())
        } else {
            v.clone()
        }
    } else {
        label.clone()
    };

    // Determine text colour: grey for placeholder, normal for live value
    let is_placeholder = live_value.as_ref().map(|v| v.is_empty()).unwrap_or(true) && !label.is_empty() && live_value.is_none();
    let text_color = if is_placeholder { [160, 160, 160] } else { [30, 30, 30] };

    if !display_text.is_empty() {
        let text_style = Style {
            font_size: s.font_size,
            color: text_color,
            ..Default::default()
        };
        paint_text(canvas, tc, fonts, &display_text, &text_style,
                   x + s.padding.left.max(6),
                   y + (ctrl_h - s.font_size as i32) / 2,
                   ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    // Blinking cursor when focused (always show it; blink can be added later)
    if is_focused {
        let cursor_text = live_value.as_deref().unwrap_or("");
        let cursor_style = Style { font_size: s.font_size, ..Default::default() };
        let display_before_cursor = if kind == InputKind::Password {
            "•".repeat(cursor_text.chars().count())
        } else {
            cursor_text.to_owned()
        };
        let (cx_off, _) = measure_text(fonts, &display_before_cursor, &cursor_style);
        let cx = x + s.padding.left.max(6) + cx_off;
        let cy_top    = y + (ctrl_h - s.font_size as i32) / 2;
        let cy_bottom = cy_top + s.font_size as i32;
        fill_rect_alpha(canvas, Color::RGB(30, 30, 30), 255,
                        cx, cy_top, 1, (cy_bottom - cy_top).max(2),
                        ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    // For button: render children inside
    if tag == "button" && !el.children.is_empty() {
        let saved_x = ls.cursor_x;
        let saved_y = ls.cursor_y;
        let saved_ml = ls.margin_left;
        ls.cursor_x  = x + s.padding.left.max(8);
        ls.cursor_y  = y + s.padding.top.max(4);
        ls.margin_left = ls.cursor_x;
        for child in &el.children {
            // Simple: just paint text children directly
            if let Node::Text(t) = child {
                let ts = Style { font_size: s.font_size, color: s.color, bold: s.bold, ..Default::default() };
                paint_text(canvas, tc, fonts, &t.text, &ts,
                           ls.cursor_x, ls.cursor_y,
                           ls.ctx.scroll_y, ls.ctx.viewport_height);
            }
        }
        ls.cursor_x   = saved_x;
        ls.cursor_y   = saved_y;
        ls.margin_left = saved_ml;
    }

    // Register a ButtonArea so clicks on this control can be detected
    let btn_action = if tag == "button" || input_type == "submit" {
        ButtonAction::Submit(ls.form_action.clone())
    } else if input_type == "reset" {
        ButtonAction::Reset
    } else {
        ButtonAction::None
    };
    if btn_action != ButtonAction::None {
        ls.button_areas.push(ButtonArea { x, y, w: ctrl_w, h: ctrl_h, action: btn_action });
    }

    ls.cursor_y   += ctrl_h + BLOCK_MARGIN;
    ls.cursor_x    = ls.margin_left;
    ls.line_height = 16;
    let _ = max_w;
}

fn paint_progress(
    ls:     &mut LayoutState,
    canvas: &mut Canvas<Window>,
    s:      &Style,
    _max_w: i32,
) {
    if ls.cursor_x > ls.margin_left { ls.cursor_y += ls.line_height + LINE_SPACING; }
    ls.cursor_y += BLOCK_MARGIN;

    let x = ls.cursor_x;
    let y = ls.cursor_y;
    let w = s.size.width.unwrap_or(200);
    let h = s.size.height.unwrap_or(16);
    let radii = s.border_radius;
    let bg = s.bg_color.unwrap_or([220, 220, 220]);

    if radii != [0, 0, 0, 0] {
        fill_rounded_rect(canvas, Color::RGB(bg[0], bg[1], bg[2]), 255,
                          x, y, w, h, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);
        // Fill bar at 40% for visual
        let fill_w = (w * 2 / 5).max(0);
        if fill_w > 0 {
            fill_rounded_rect(canvas, Color::RGB(66, 133, 244), 255,
                              x, y, fill_w, h, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
        draw_rounded_rect(canvas, Color::RGB(160, 160, 160), 255,
                          x, y, w, h, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);
    } else {
        fill_rect_alpha(canvas, Color::RGB(bg[0], bg[1], bg[2]), 255,
                        x, y, w, h, ls.ctx.scroll_y, ls.ctx.viewport_height);
        let fill_w = (w * 2 / 5).max(0);
        if fill_w > 0 {
            fill_rect_alpha(canvas, Color::RGB(66, 133, 244), 255,
                            x, y, fill_w, h, ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
    }

    ls.cursor_y   += h + BLOCK_MARGIN;
    ls.cursor_x    = ls.margin_left;
    ls.line_height = 16;
}

// ---------------------------------------------------------------------------
// Block background and border
// ---------------------------------------------------------------------------

/// Paint a CSS `background-image` inside the given box.
/// Respects `background-size` (auto/cover/contain) and `background-repeat`.
fn paint_block_bg_image(
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

    // ── Resolve tile size based on background-size ────────────────────────
    let (tile_w, tile_h) = match style.bg_size {
        BgSize::Cover => {
            // Scale so the image covers the box entirely (crop allowed)
            let scale_x = w as f32 / nat_w as f32;
            let scale_y = h as f32 / nat_h as f32;
            let scale   = scale_x.max(scale_y);
            ((nat_w as f32 * scale) as i32, (nat_h as f32 * scale) as i32)
        }
        BgSize::Contain => {
            // Scale so the image fits entirely inside the box (letterbox allowed)
            let scale_x = w as f32 / nat_w as f32;
            let scale_y = h as f32 / nat_h as f32;
            let scale   = scale_x.min(scale_y);
            ((nat_w as f32 * scale) as i32, (nat_h as f32 * scale) as i32)
        }
        BgSize::Auto => (nat_w, nat_h),
    };
    if tile_w <= 0 || tile_h <= 0 { return; }

    // ── Resolve position — sentinels 5000=50%, 10000=100% ────────────────
    let resolve_pos = |sentinel: i32, box_dim: i32, tile_dim: i32| -> i32 {
        match sentinel {
            5000  => (box_dim - tile_dim) / 2,   // center
            10000 => (box_dim - tile_dim).max(0), // end / right / bottom
            n     => n,
        }
    };
    let off_x = resolve_pos(style.bg_position.x, w, tile_w);
    let off_y = resolve_pos(style.bg_position.y, h, tile_h);

    // ── Paint tiles ────────────────────────────────────────────────────────
    // Determine tiling ranges
    let (start_tx, step_x, end_tx) = match style.bg_repeat {
        BgRepeat::Repeat | BgRepeat::RepeatX => {
            // Start far enough left that the first tile's right edge is ≥ x
            let start = if tile_w > 0 {
                off_x - ((off_x.abs() / tile_w + 1) * tile_w)
            } else { off_x };
            (start, tile_w, w)
        }
        _ => (off_x, w + 1, off_x + 1), // single tile, loop runs once
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
                // Clip the tile to the box
                let dst_x  = abs_x.max(x);
                let dst_y  = abs_y.max(y);
                let dst_x2 = (abs_x + tile_w).min(x + w);
                let dst_y2 = (abs_y + tile_h).min(y + h);
                let dst_w  = dst_x2 - dst_x;
                let dst_h  = dst_y2 - dst_y;
                if dst_w > 0 && dst_h > 0 {
                    // Corresponding source rect (in natural image coords)
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

fn paint_block_bg(
    ls:    &LayoutState,
    canvas: &mut Canvas<Window>,
    style: &Style,
    x: i32, y: i32, w: i32, h: i32,
) {
    let radii = style.border_radius;
    if let Some(bg) = style.bg_color {
        let alpha = style.bg_alpha;
        // Pre-composite the background colour against opaque white so that
        // semi-transparent backgrounds (rgba / hsla) look correct regardless
        // of whether the SDL2 renderer's blend mode works as expected.
        // Formula: out = alpha * src + (1 - alpha) * white
        let a = alpha as u32;
        let pre = [
            ((a * bg[0] as u32 + (255 - a) * 255) / 255) as u8,
            ((a * bg[1] as u32 + (255 - a) * 255) / 255) as u8,
            ((a * bg[2] as u32 + (255 - a) * 255) / 255) as u8,
        ];
        if radii != [0, 0, 0, 0] {
            fill_rounded_rect(canvas, rgba_color(pre, 255), 255,
                              x, y, w, h, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);
        } else {
            fill_rect_alpha(canvas, rgba_color(pre, 255), 255,
                            x, y, w, h, ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
    }
}

fn paint_block_border(
    ls:    &LayoutState,
    canvas: &mut Canvas<Window>,
    style: &Style,
    x: i32, y: i32, w: i32, h: i32,
) {
    let radii = style.border_radius;
    let b     = &style.borders;
    let alpha = 255u8;

    let has_any = b.top.width > 0 || b.bottom.width > 0
               || b.left.width > 0 || b.right.width > 0;
    if !has_any { return; }

    if radii != [0, 0, 0, 0] {
        let outline = if b.top.width > 0 { b.top.color } else { b.left.color };
        draw_rounded_rect(canvas, rgba_color(outline, 255), alpha,
                          x, y, w, h, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);
    } else {
        let bw_t = b.top.width    as i32;
        let bw_r = b.right.width  as i32;
        let bw_b = b.bottom.width as i32;
        let bw_l = b.left.width   as i32;

        let draw = |canvas: &mut Canvas<Window>, brd: &crate::dom::node::Border,
                    rx: i32, ry: i32, rw: i32, rh: i32| {
            if brd.width > 0 {
                fill_rect_alpha(canvas, rgba_color(brd.color, 255), alpha,
                                rx, ry, rw, rh, ls.ctx.scroll_y, ls.ctx.viewport_height);
            }
        };

        draw(canvas, &b.top,    x,         y,         w,           bw_t.max(1));
        draw(canvas, &b.bottom, x,         y + h - 1, w,           bw_b.max(1));
        draw(canvas, &b.left,   x,         y,         bw_l.max(1), h);
        draw(canvas, &b.right,  x + w - 1, y,         bw_r.max(1), h);

        if bw_l >= 3 {
            fill_rect_alpha(canvas, rgba_color(b.left.color, 255), alpha,
                            x, y, bw_l, h, ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
    }
}
