use sdl2::pixels::Color;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::image::ImageRWops;

use crate::dom::node::{Element, Node, Style, ListStyleType, Display, Visibility, BgSize, BgRepeat};
use crate::dom::css::{parse_length_ctx, LengthContext};
use crate::render::font::FontCache;
use crate::render::image::ImageCache;

use super::paint::{
    paint_text, measure_text, fill_rect_alpha, fill_rounded_rect,
    draw_rounded_rect, rgba_color, paint_box_shadow,
};
use super::state::{LayoutState, LayoutBox, InputArea, InputKind, ButtonArea, ButtonAction, MARGIN_RIGHT, BLOCK_MARGIN, LINE_SPACING};
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

    // visibility:hidden — element occupies layout space but is not painted.
    // We redirect rendering to a no-op sink so that all layout cursor
    // advances happen normally while nothing is drawn to the screen.
    if el.style.visibility == Visibility::Hidden {
        // Use a headless layout pass that advances cursors without painting.
        layout_element_invisible(ls, fonts, el, max_w);
        return;
    }

    let tag = el.tag.as_str();
    let s   = &el.style;

    // Re-resolve font-size for viewport-relative units (vw, vh, calc) now that
    // the real viewport dimensions are available in ls.ctx.
    // All downstream code in this function uses `font_size` instead of `s.font_size`.
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

    // ── Structural root containers ─────────────────────────────────────────
    if matches!(tag, "#document" | "html" | "body") {
        // Background spans the full viewport width and at least the full viewport height.
        let bg_h = ls.ctx.viewport_height.max(32_000);
        if s.bg_color.is_some() {
            paint_block_bg(ls, canvas, s, 0, 0, max_w, bg_h);
        }
        if s.bg_image_url.is_some() {
            paint_block_bg_image(ls, canvas, tc, images, base_url, s,
                                 0, 0, max_w, ls.ctx.viewport_height);
        }

        // Only html/body carry real layout margins; #document is a virtual root.
        if tag == "#document" {
            for child in &el.children {
                ls.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
            }
            return;
        }

        // Resolve body margin + padding into page offsets.
        // padding is added to margin to form the content inset.
        let pad_l = s.padding.left;
        let pad_t = s.padding.top;
        let pad_r = s.padding.right;
        let mar_l = s.margin.left;
        let mar_t = s.margin.top;
        let mar_r = s.margin.right;

        // Resolve viewport-relative widths for body/html width constraints.
        let vw = ls.ctx.viewport_width;
        let vh = ls.ctx.viewport_height;
        let body_avail = max_w;
        let resolved_w    = resolve_size(s.size.width,     s.size.width_raw.as_deref(),     body_avail, vw, vh, font_size);
        let resolved_maxw = resolve_size(s.size.max_width, s.size.max_width_raw.as_deref(), body_avail, vw, vh, font_size);

        // Determine body content width and left offset.
        // If body has an explicit width or max-width AND margin:auto, center it.
        let (body_left, body_content_w) = {
            // Start with full width minus explicit margins/padding
            let total_side = mar_l + pad_l + mar_r + pad_r;
            let mut content_w = (max_w - total_side).max(1);

            // Apply explicit width constraint
            if let Some(w) = resolved_w    { content_w = content_w.min(w); }
            if let Some(w) = resolved_maxw { content_w = content_w.min(w); }

            let left = if s.margin_auto_left || s.margin_auto_right {
                // margin: auto — center the content area
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

        // body_max_w is the absolute right edge for child layout
        // = left content edge + content width (excludes right padding/margin, which
        //   belong to the body box itself, not to children)
        let body_max_w = (body_left + body_content_w).min(max_w);

        for child in &el.children {
            ls.layout_node(canvas, tc, fonts, images, base_url, child, body_max_w);
        }
        return;
    }

    // ── Void / replaced elements ───────────────────────────────────────────
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
    // Horizontal percent_base = available content width inside this element's parent.
    let avail_w = (max_w - ls.margin_left - MARGIN_RIGHT).max(1);
    let avail_h = vh;

    let resolved_width      = resolve_size(s.size.width,      s.size.width_raw.as_deref(),      avail_w, vw, vh, font_size);
    let resolved_max_width  = resolve_size(s.size.max_width,  s.size.max_width_raw.as_deref(),  avail_w, vw, vh, font_size);
    let resolved_min_width  = resolve_size(s.size.min_width,  s.size.min_width_raw.as_deref(),  avail_w, vw, vh, font_size);
    let resolved_height     = resolve_size(s.size.height,     s.size.height_raw.as_deref(),     avail_h, vw, vh, font_size);
    let resolved_max_height = resolve_size(s.size.max_height, s.size.max_height_raw.as_deref(), avail_h, vw, vh, font_size);
    let resolved_min_height = resolve_size(s.size.min_height, s.size.min_height_raw.as_deref(), avail_h, vw, vh, font_size);

    // ── Resolve horizontal box geometry (block_x, block_w) ────────────────
    //
    // We work entirely in absolute pixel coordinates:
    //   block_x  = left edge of the border box
    //   block_w  = width of the border box (border + padding + content)
    //
    // The containing block's content area is [ls.margin_left + ls.indent .. max_w - MARGIN_RIGHT].
    let contain_left  = ls.margin_left + ls.indent;
    let contain_right = max_w - MARGIN_RIGHT;
    let contain_w     = (contain_right - contain_left).max(0);

    // Step 1: determine the content/border-box width.
    // Start with the explicit width (CSS `width`); fall back to full available.
    let mut box_w = resolved_width.unwrap_or(
        (contain_w - s.margin.left - s.margin.right).max(0)
    );
    // Clamp by max-width / min-width
    if let Some(mw) = resolved_max_width { box_w = box_w.min(mw); }
    if let Some(mn) = resolved_min_width { box_w = box_w.max(mn); }
    box_w = box_w.max(0);

    // Step 2: determine the left margin (resolving `auto`).
    let ml = if s.margin_auto_left || s.margin_auto_right {
        let remaining = (contain_w - box_w - s.margin.left - s.margin.right).max(0);
        match (s.margin_auto_left, s.margin_auto_right) {
            (true,  true)  => s.margin.left + remaining / 2,  // center
            (false, true)  => s.margin.left,                   // flush left
            (true,  false) => s.margin.left + remaining,       // flush right
            (false, false) => s.margin.left,
        }
    } else {
        s.margin.left
    };

    // Step 3: absolute positions
    let block_x = contain_left + ml;
    let block_w = box_w.min(contain_right - block_x - s.margin.right).max(0);

    // effective_max_w is no longer needed — all consumers use block_x/block_w directly.

    let is_block = s.display_block;

    // ── Inline-block: render background+border as a pill/box, apply padding ─
    // Elements with `display: inline-block` sit in the inline flow but paint
    // their own background, border, and padding — the key requirement for the
    // `border-radius: 999px` pill shape.
    if s.display == crate::dom::node::Display::InlineBlock {
        let pad_l = s.padding.left;
        let pad_r = s.padding.right;
        let pad_t = s.padding.top;
        let pad_b = s.padding.bottom;

        // ── Step 1: measure content width & natural text height via dry run ──
        // We walk children with font metrics only — no SDL painting — so that
        // ls.line_height is not inflated before we compute ib_h.
        let content_w = measure_inline_block_children(fonts, &el.children, font_size);
        let content_h = font_size as i32;   // natural single-line text height

        // ── Step 2: compute box dimensions from measurement ──────────────────
        let ib_x = ls.cursor_x;
        let ib_y = ls.cursor_y;
        let ib_w = (pad_l + content_w + pad_r).max(0);
        let ib_h = (pad_t + content_h + pad_b).max(font_size as i32);

        let radii = s.border_radius;

        // ── Step 3: paint background & border behind the text ────────────────
        if let Some(bg) = s.bg_color {
            let alpha = s.bg_alpha;
            fill_rounded_rect(canvas, rgba_color(bg, alpha), alpha,
                              ib_x, ib_y, ib_w, ib_h, radii,
                              ls.ctx.scroll_y, ls.ctx.viewport_height);
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

        // ── Step 4: single paint pass for children ───────────────────────────
        ls.cursor_x = ib_x + pad_l;
        for child in &el.children {
            ls.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
        }

        // ── Step 5: advance cursor past the box ──────────────────────────────
        ls.cursor_x = ib_x + ib_w;
        if ib_h > ls.line_height {
            ls.line_height = ib_h;
        }

        return;
    }

    // ── Block open ─────────────────────────────────────────────────────────
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

        // Box shadow (behind background)
        if let Some(ref shadow) = s.box_shadow {
            let mut block_h = measure_block_children(ls, fonts, el, block_x + block_w, s);
            if let Some(h)  = resolved_height     { block_h = h; }
            if let Some(mn) = resolved_min_height { block_h = block_h.max(mn); }
            if let Some(mx) = resolved_max_height { block_h = block_h.min(mx); }
            paint_box_shadow(canvas, shadow, block_x, start_y, block_w, block_h,
                             ls.ctx.scroll_y, ls.ctx.viewport_height);
        }

        // Background
        if s.bg_color.is_some() {
            let mut block_h = measure_block_children(ls, fonts, el, block_x + block_w, s);
            if let Some(h)  = resolved_height     { block_h = h; }
            if let Some(mn) = resolved_min_height { block_h = block_h.max(mn); }
            if let Some(mx) = resolved_max_height { block_h = block_h.min(mx); }
            paint_block_bg(ls, canvas, s, block_x, start_y, block_w, block_h);
        }
        if s.bg_image_url.is_some() {
            let mut block_h = measure_block_children(ls, fonts, el, block_x + block_w, s);
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

    // Children must not paint beyond the right padding edge of this block.
    // cursor_x is already at block_x + padding.left; the right edge is
    // block_x + block_w - padding.right.
    let children_max_w = if is_block {
        (block_x + block_w - s.padding.right).max(ls.cursor_x + 1)
    } else {
        max_w
    };

    let link_start_y = ls.cursor_y;
    let link_start_x = ls.cursor_x;

    // ── overflow:hidden / scroll / auto clipping ──────────────────────────
    // When overflow restricts content AND a height constraint applies, use
    // SDL2's clip rect to prevent child content from painting outside the box.
    // overflow:scroll and overflow:auto additionally paint a visual scrollbar.
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
    if needs_clip {
        let clip_h = resolved_height
            .or(resolved_max_height)
            .unwrap_or(0)
            .max(0);
        let ry = start_y - ls.ctx.scroll_y;
        if clip_h > 0 && block_w > 0 {
            canvas.set_clip_rect(sdl2::rect::Rect::new(
                block_x,
                ry,
                block_w as u32,
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
        let lh = (ls.line_height).max(font_size as i32);
        ls.link_areas.push(super::state::LinkArea {
            x: link_start_x, y: link_start_y,
            w: lw, h: lh,
            href: s.href.clone().unwrap_or_default(),
        });
    }

    // ── Block close ────────────────────────────────────────────────────────
    if is_block {
        ls.cursor_y += s.padding.bottom;

        // Only count line_height if there's an unfinished inline run on the
        // current line (cursor_x advanced past the block's own content left
        // edge = block_x + padding.left).
        let content_left = block_x + s.padding.left;
        let pending_lh = if ls.cursor_x > content_left { ls.line_height } else { 0 };
        let end_y = ls.cursor_y + pending_lh;
        let mut block_h = (end_y - start_y).max(0);

        // Apply height / min-height / max-height
        if let Some(h) = resolved_height     { block_h = h; }
        if let Some(mn) = resolved_min_height { block_h = block_h.max(mn); }
        if let Some(mx) = resolved_max_height {
            if block_h > mx {
                block_h = mx;
                // For overflow:hidden/scroll/auto, snap cursor_y so children
                // beyond max-height don't push subsequent siblings down.
                if overflow_clips {
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

        // ── overflow:scroll / auto — paint a static scrollbar ─────────────
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

// ---------------------------------------------------------------------------
// Block helpers
// ---------------------------------------------------------------------------

/// Paint a Chrome-style thin scrollbar on the right edge of a scroll/auto box.
///
/// - Track: full box height, 6 px wide, light grey  
/// - Thumb: proportional to visible/content ratio, rounded, darker grey  
/// - Horizontal indicator: small arrow chevrons at bottom-left edge  
fn paint_scrollbar(
    ls:      &mut LayoutState,
    canvas:  &mut Canvas<Window>,
    fonts:   &mut FontCache,
    el:      &Element,
    box_x:   i32,
    box_y:   i32,
    box_w:   i32,
    box_h:   i32,
    s:       &Style,
) {
    const TRACK_W: i32 = 8;  // total scrollbar column width
    const THUMB_W: i32 = 6;  // visible thumb/track width (2 px inset on left)
    const MIN_THUMB_H: i32 = 18;
    const TRACK_COLOR:  [u8; 3] = [240, 240, 240];
    const THUMB_COLOR:  [u8; 3] = [180, 180, 180];
    const BORDER_COLOR: [u8; 3] = [210, 210, 210];

    // Measure full content height to compute thumb ratio
    let content_h = measure_block_children(ls, fonts, el, box_x + box_w, s)
        .max(box_h);

    let track_x = box_x + box_w - TRACK_W;
    let track_y = box_y;

    // ── Track ──────────────────────────────────────────────────────────────
    fill_rect_alpha(canvas, rgba_color(TRACK_COLOR, 255), 255,
        track_x, track_y, THUMB_W, box_h,
        ls.ctx.scroll_y, ls.ctx.viewport_height);
    // Left border line
    fill_rect_alpha(canvas, rgba_color(BORDER_COLOR, 255), 255,
        track_x, track_y, 1, box_h,
        ls.ctx.scroll_y, ls.ctx.viewport_height);

    // ── Thumb ──────────────────────────────────────────────────────────────
    let thumb_h = ((box_h as f32 / content_h as f32) * box_h as f32) as i32;
    let thumb_h = thumb_h.max(MIN_THUMB_H).min(box_h);
    // Thumb is at the top (scroll position 0 — static render)
    let thumb_y = track_y;
    let thumb_x = track_x + 1; // 1 px inset from border
    fill_rect_alpha(canvas, rgba_color(THUMB_COLOR, 255), 255,
        thumb_x, thumb_y, THUMB_W - 1, thumb_h,
        ls.ctx.scroll_y, ls.ctx.viewport_height);

    // ── Horizontal scrollbar hint at the bottom ────────────────────────────
    // A thin horizontal track along the bottom edge mirrors Chrome's behaviour
    // of showing both scrollbars when overflow:scroll is set.
    const HTRACK_H: i32 = 8;
    let htrack_y = box_y + box_h - HTRACK_H;
    let htrack_w = box_w - TRACK_W; // stop before the vertical scrollbar
    fill_rect_alpha(canvas, rgba_color(TRACK_COLOR, 255), 255,
        box_x, htrack_y, htrack_w, HTRACK_H - 1,
        ls.ctx.scroll_y, ls.ctx.viewport_height);
    // Top border of horizontal track
    fill_rect_alpha(canvas, rgba_color(BORDER_COLOR, 255), 255,
        box_x, htrack_y, htrack_w, 1,
        ls.ctx.scroll_y, ls.ctx.viewport_height);
    // Horizontal thumb (proportional to content width vs box width — use 60% as static estimate)
    let hthumb_w = ((htrack_w as f32 * 0.6) as i32).max(MIN_THUMB_H).min(htrack_w);
    fill_rect_alpha(canvas, rgba_color(THUMB_COLOR, 255), 255,
        box_x, htrack_y + 1, hthumb_w, HTRACK_H - 2,
        ls.ctx.scroll_y, ls.ctx.viewport_height);
}

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

/// Dry-run: measure the total text width of inline-block children using font
/// metrics only — no SDL painting, no cursor mutation.
fn measure_inline_block_children(fonts: &mut FontCache, children: &[Node], _font_size: u16) -> i32 {
    let mut total = 0i32;
    for child in children {
        match child {
            Node::Text(t) => {
                let (w, _) = measure_text(fonts, t.text.trim(), &t.style);
                total += w;
            }
            Node::Element(el) => {
                if el.style.display == Display::Hidden { continue; }
                total += measure_inline_block_children(fonts, &el.children, el.style.font_size);
            }
        }
    }
    // Clamp to at least the font size so an empty inline-block has visible height
    total.max(0)
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
    measure_children_recursive(&el.children, fonts, max_w, &mut cx, &mut cy, &mut lh, ls.indent, ls.margin_left, ls.ctx.viewport_width, ls.ctx.viewport_height);
    cy += s.padding.bottom;
    // Mirror the block-close logic: if cx is back at the left margin the last
    // child was a block that already advanced cy, so there's no pending inline
    // content and we must not add lh (which would create a phantom gap).
    let pending_lh = if cx > ls.margin_left + ls.indent { lh } else { 0 };
    let end_y = cy + pending_lh;
    (end_y - start_y).max(0)
}

fn measure_children_recursive(
    children:    &[Node],
    fonts:       &mut FontCache,
    max_w:       i32,
    cx:          &mut i32,
    cy:          &mut i32,
    lh:          &mut i32,
    indent:      i32,
    margin_left: i32,
    viewport_w:  i32,
    viewport_h:  i32,
) {
    for child in children {
        match child {
            Node::Text(t) => {
                // Re-resolve viewport-relative font-size (vw/vh/calc) so that
                // height measurement matches what will actually be painted.
                let font_size = if let Some(raw) = &t.style.font_size_raw {
                    let ctx = crate::dom::css::LengthContext {
                        base_font_size: t.style.font_size,
                        percent_base:   16,
                        viewport_width:  viewport_w,
                        viewport_height: viewport_h,
                    };
                    crate::dom::css::parse_length_ctx(raw, &ctx)
                        .map(|n| n.clamp(8, 96) as u16)
                        .unwrap_or(t.style.font_size)
                } else {
                    t.style.font_size
                };
                let words: Vec<&str> = t.text.split_whitespace().collect();
                if words.is_empty() { continue; }
                let mut line = String::new();
                for word in &words {
                    let test = if line.is_empty() { word.to_string() }
                               else { format!("{} {}", line, word) };
                    let (tw, _) = fonts.get(font_size, t.style.bold, t.style.italic)
                        .and_then(|f| f.size_of(&test).ok())
                        .map(|(w, h)| (w as i32, h as i32))
                        .unwrap_or((test.len() as i32 * 8, font_size as i32));
                    if tw > max_w - *cx && !line.is_empty() {
                        let line_h = (font_size as f32 * t.style.line_height_mul) as i32;
                        *cy += (*lh).max(line_h) + LINE_SPACING;
                        *cx  = margin_left + indent;
                        *lh  = font_size as i32;
                        line = word.to_string();
                    } else {
                        line = test;
                    }
                }
                if !line.is_empty() {
                    let (_, th) = fonts.get(font_size, t.style.bold, t.style.italic)
                        .and_then(|f| f.size_of(&line).ok())
                        .map(|(w, h)| (w as i32, h as i32))
                        .unwrap_or((0, font_size as i32));
                    if th > *lh { *lh = th; }
                }
            }
            Node::Element(child_el) => {
                if child_el.style.display == Display::Hidden { continue; }
                let child_tag = child_el.tag.as_str();
                // Re-resolve font-size for the element itself too
                let child_font_size = if let Some(raw) = &child_el.style.font_size_raw {
                    let ctx = crate::dom::css::LengthContext {
                        base_font_size: child_el.style.font_size,
                        percent_base:   16,
                        viewport_width:  viewport_w,
                        viewport_height: viewport_h,
                    };
                    crate::dom::css::parse_length_ctx(raw, &ctx)
                        .map(|n| n.clamp(8, 96) as u16)
                        .unwrap_or(child_el.style.font_size)
                } else {
                    child_el.style.font_size
                };
                if child_el.style.display_block {
                    if *cx > margin_left + indent { *cy += *lh + LINE_SPACING; }
                    *cy += BLOCK_MARGIN + child_el.style.margin.top;
                    *cx  = margin_left + indent + child_el.style.margin.left;
                    *lh  = child_font_size as i32;
                    if child_tag.len() == 2 && child_tag.starts_with('h')
                        && child_tag.as_bytes()[1].is_ascii_digit()
                    {
                        *cy += child_font_size as i32 / 2;
                    }
                    *cy += child_el.style.padding.top;
                    *cx += child_el.style.padding.left;
                    let saved = indent;
                    let new_indent = *cx - margin_left;
                    measure_children_recursive(&child_el.children, fonts, max_w, cx, cy, lh, new_indent, margin_left, viewport_w, viewport_h);
                    *cy += child_el.style.padding.bottom;
                    if *cx > margin_left { *cy += *lh + LINE_SPACING; }
                    *cy += BLOCK_MARGIN + child_el.style.margin.bottom;
                    *cx  = margin_left + saved;
                    *lh  = 16;
                } else {
                    measure_children_recursive(&child_el.children, fonts, max_w, cx, cy, lh, indent, margin_left, viewport_w, viewport_h);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// visibility:hidden layout pass
//
// Advances the layout cursor exactly as the normal pass would, but does not
// paint anything.  This preserves the space that the element would occupy so
// that surrounding content is pushed down / across normally.
// ---------------------------------------------------------------------------

/// Advance cursor for a text node without painting it (for visibility:hidden).
pub fn advance_text_invisible(
    ls:    &mut LayoutState,
    fonts: &mut FontCache,
    text:  &str,
    s:     &crate::dom::node::Style,
) {
    if text.trim().is_empty() { return; }
    let (tw, th) = fonts.get(s.font_size, s.bold, s.italic)
        .and_then(|f| f.size_of(text.trim()).ok())
        .map(|(w, h)| (w as i32, h as i32))
        .unwrap_or((text.len() as i32 * 8, s.font_size as i32));
    ls.cursor_x += tw;
    if th > ls.line_height { ls.line_height = th; }
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

    // br — advance to next line
    if tag == "br" {
        ls.newline(s.font_size, s.line_height_mul);
        return;
    }

    // For block elements: open block, measure children, close block
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

        // Recurse into children (also invisible)
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
        // Inline — measure text width and advance cursor_x
        for child in &el.children {
            layout_node_invisible(ls, fonts, child, max_w);
        }
    }
}

fn layout_node_invisible(
    ls:    &mut LayoutState,
    fonts: &mut FontCache,
    node:  &Node,
    max_w: i32,
) {
    match node {
        Node::Text(t) => {
            // Measure text width and advance cursor as the inline pass would.
            let s = &t.style;
            let text = &t.text;
            if text.trim().is_empty() { return; }
            // Approximate: use font measurement for the whole text block
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
            // Available width accounts for the image's own horizontal box model.
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

    // Total horizontal footprint of the image including its box model.
    let total_w = ml + pl + dw + pr + mr;

    // Wrap to the next line if the image (with its spacing) won't fit.
    if ls.cursor_x + total_w > max_w && ls.cursor_x > ls.margin_left {
        ls.cursor_y += ls.line_height + LINE_SPACING;
        ls.cursor_x  = ls.margin_left + ls.indent;
        ls.line_height = 0;
    }

    // ── Paint positions ───────────────────────────────────────────────────
    // cursor_y is the TOP of the current line. For inline elements we must NOT
    // mutate cursor_y directly — only line_height is updated. The top margin
    // and padding offset the image downward relative to the line baseline,
    // and the bottom margin/padding inflate line_height so the next line starts
    // far enough below.
    let box_x = ls.cursor_x + ml;            // left edge of padding box
    let img_x = box_x + pl;                  // left edge of image content
    let box_y = ls.cursor_y + mt;            // top edge of padding box (scroll-adjusted later)
    let img_y = box_y + pt;                  // top edge of image content

    // Paint background behind the padding box (if any).
    if let Some(bg) = style.bg_color {
        let box_w = (pl + dw + pr).max(0) as u32;
        let box_h = (pt + dh + pb).max(0) as u32;
        let screen_y = box_y - ls.ctx.scroll_y;
        if box_w > 0 && box_h > 0 && screen_y + box_h as i32 > 0 && screen_y < ls.ctx.viewport_height {
            canvas.set_draw_color(Color::RGBA(bg[0], bg[1], bg[2], style.bg_alpha));
            let _ = canvas.fill_rect(sdl2::rect::Rect::new(box_x, screen_y, box_w, box_h));
        }
    }

    // Paint the image texture.
    let screen_y = img_y - ls.ctx.scroll_y;
    if screen_y + dh > 0 && screen_y < ls.ctx.viewport_height {
        if let Ok(tex) = tc.create_texture_from_surface(&surface) {
            let _ = canvas.copy(&tex, None, sdl2::rect::Rect::new(img_x, screen_y, dw as u32, dh as u32));
        }
    }

    // Advance cursor horizontally past the full box model footprint.
    ls.cursor_x += total_w;

    // The line-height contribution is the full vertical footprint of the box
    // model so the next line starts below the bottom margin.
    let full_h = mt + pt + dh + pb + mb;
    if full_h > ls.line_height { ls.line_height = full_h; }
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
        // Combine bg_alpha (from rgba()) with opacity (from CSS opacity property).
        // Both are already baked into bg_alpha during the cascade, so use it
        // directly with SDL2 blend mode rather than pre-compositing against white.
        // Pre-compositing against white would make opacity:0.5 on a dark colour
        // look washed-out grey instead of the correct semi-transparent dark.
        let alpha = style.bg_alpha;
        let color = rgba_color(bg, alpha);
        if radii != [0, 0, 0, 0] {
            fill_rounded_rect(canvas, color, alpha,
                              x, y, w, h, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);
        } else {
            fill_rect_alpha(canvas, color, alpha,
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
    let alpha = style.opacity;  // baked effective opacity (255 = fully opaque)

    let has_any = b.top.width > 0 || b.bottom.width > 0
               || b.left.width > 0 || b.right.width > 0;
    if !has_any { return; }

    if radii != [0, 0, 0, 0] {
        let outline = if b.top.width > 0 { b.top.color } else { b.left.color };
        draw_rounded_rect(canvas, rgba_color(outline, alpha), alpha,
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
