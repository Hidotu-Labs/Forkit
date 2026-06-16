/// Minimal flex layout engine.
///
/// Supports:
///   - flex-direction: row | column | row-reverse | column-reverse
///   - flex-wrap: nowrap | wrap
///   - justify-content: flex-start | flex-end | center | space-between | space-around | space-evenly
///   - align-items: stretch | flex-start | flex-end | center
///   - flex-grow / flex-shrink (per-item)
///   - gap (uniform)
///   - padding on the container
///
/// Does NOT support: align-self, order, multi-line wrap with align-content.

use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::dom::node::{Display, Element, Node, FlexDirection, FlexWrap, JustifyContent, AlignItems};
use crate::render::font::FontCache;
use crate::render::image::ImageCache;

use super::block::{
    layout_element, measure_block_children,
    paint_block_bg_gradient, paint_block_bg_image, paint_block_border,
    resolve_size,
    measure_block_content_width, measure_inline_block_children,
};
use super::paint::{fill_rounded_rect, draw_rounded_rect, rgba_color};
use super::state::{LayoutState, LayoutBox, BLOCK_MARGIN, LINE_SPACING, MARGIN_RIGHT};

/// Lay out a flex container element.
pub fn layout_flex(
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
    let font_size = s.font_size;

    // ── Container box geometry ──────────────────────────────────────────────
    let vw = ls.ctx.viewport_width;
    let vh = ls.ctx.viewport_height;
    let avail_w = (max_w - ls.margin_left - MARGIN_RIGHT).max(1);
    let avail_h = vh;

    let resolved_width  = resolve_size(s.size.width,  s.size.width_raw.as_deref(),  avail_w, vw, vh, font_size);
    let resolved_height = resolve_size(s.size.height, s.size.height_raw.as_deref(), avail_h, vw, vh, font_size);

    let contain_left  = ls.margin_left + ls.indent;
    let contain_right = max_w - MARGIN_RIGHT;
    let contain_w     = (contain_right - contain_left).max(0);

    let box_w = if let Some(rw) = resolved_width {
        rw + s.padding.left + s.padding.right
    } else {
        (contain_w - s.margin.left - s.margin.right).max(0)
    };

    let remaining_for_auto = (contain_w - box_w - s.margin.left - s.margin.right).max(0);
    let ml = if s.margin_auto_left || s.margin_auto_right {
        match (s.margin_auto_left, s.margin_auto_right) {
            (true,  true)  => s.margin.left + remaining_for_auto / 2,
            (false, true)  => s.margin.left,
            (true,  false) => s.margin.left + remaining_for_auto,
            (false, false) => s.margin.left,
        }
    } else {
        s.margin.left
    };

    // Begin block: advance past any pending inline content
    if ls.cursor_x > ls.margin_left + ls.indent {
        ls.cursor_y += ls.line_height + LINE_SPACING;
    }
    ls.cursor_y += BLOCK_MARGIN + s.margin.top;
    ls.line_height = font_size as i32;

    let container_x = contain_left + ml;
    // Honour explicit widths even when they exceed the containing-block width.
    let container_w = if resolved_width.is_some() {
        box_w.max(0)
    } else {
        box_w.min(contain_right - container_x - s.margin.right).max(0)
    };
    let start_y = ls.cursor_y;

    let pad_l = s.padding.left;
    let pad_r = s.padding.right;
    let pad_t = s.padding.top;
    let pad_b = s.padding.bottom;

    // Inner content area
    let inner_x = container_x + pad_l;
    let inner_w = (container_w - pad_l - pad_r).max(1);

    // ── Collect visible flex items ──────────────────────────────────────────
    let items: Vec<&Node> = el.children.iter().filter(|n| {
        match n {
            Node::Element(e) => {
                if e.style.position == crate::dom::node::Position::Absolute { false }
                else if e.style.position == crate::dom::node::Position::Fixed { false }
                else if e.style.display == Display::Hidden { false }
                else { true }
            }
            Node::Text(t) => {
                !t.text.trim().is_empty()
            }
        }
    }).collect();

    if items.is_empty() {
        // Empty container — paint background and return
        let box_h = resolved_height.map(|h| h + pad_t + pad_b).unwrap_or(pad_t + pad_b);
        paint_flex_bg(ls, canvas, tc, images, base_url, s, container_x, start_y, container_w, box_h.max(font_size as i32));
        paint_block_border(ls, canvas, s, container_x, start_y, container_w, box_h.max(font_size as i32));
        ls.cursor_y = start_y + box_h.max(font_size as i32) + BLOCK_MARGIN + s.margin.bottom;
        ls.cursor_x = ls.margin_left + ls.indent;
        ls.line_height = 16;
        return;
    }

    let is_row    = s.flex_direction.is_row();
    let is_reverse = matches!(s.flex_direction, FlexDirection::RowReverse | FlexDirection::ColumnReverse);
    let gap       = s.gap;

    // ── Measure each item's natural size ────────────────────────────────────
    // We do a dry-run measurement to get each item's natural height/width.
    let n = items.len();

    // For row layout: measure each item's natural content width (using flex-basis or auto)
    // then distribute remaining space by flex-grow.
    struct ItemGeom {
        base_size:    i32,   // main-axis "hypothetical main size"
        cross_size:   i32,   // cross-axis natural size
        grow:         f32,
        shrink:       f32,
    }

    let mut measure_item_h = |node: &Node, item_w: i32| -> i32 {
        let is = node.style();
        // Temporarily save and restore layout cursor state for measurement
        let old_cx = ls.cursor_x;
        let old_cy = ls.cursor_y;
        let old_ml = ls.margin_left;
        let old_indent = ls.indent;
        let old_lh = ls.line_height;

        let h = match node {
            Node::Element(item) => measure_block_children(ls, fonts, item, inner_x + item_w, &item.style),
            Node::Text(t) => {
                let (tw, th) = crate::render::layout::paint::measure_text(fonts, &t.text, &t.style);
                th
            }
        };

        ls.cursor_x    = old_cx;
        ls.cursor_y    = old_cy;
        ls.margin_left = old_ml;
        ls.indent      = old_indent;
        ls.line_height = old_lh;

        h.max(is.font_size as i32)
    };

    let geoms: Vec<ItemGeom> = if is_row {
        // Step 1: determine each item's hypothetical main size (flex-basis or intrinsic).
        // flex-basis: auto → use the item's intrinsic content width.
        // flex-basis: <length> or explicit width → use that.
        let base_content_widths: Vec<i32> = items.iter().map(|item| {
            let is = item.style();
            if let Some(basis) = is.flex_basis {
                basis
            } else if let Some(w) = is.size.width {
                w
            } else if let Some(raw) = is.size.width_raw.as_deref() {
                // Resolve percentage widths (e.g. width:100%) against the inner
                // flex container width so they participate correctly in wrapping.
                resolve_size(None, Some(raw), inner_w, vw, vh, is.font_size)
                    .unwrap_or(0)
            } else if let Node::Element(e) = item {
                if e.style.display == crate::dom::node::Display::Flex {
                    measure_flex_natural_width(fonts, e)
                } else {
                    let has_block_children = e.children.iter().any(|c| {
                        matches!(c, crate::dom::node::Node::Element(e) if e.style.display_block)
                    });
                    if has_block_children {
                        measure_block_content_width(fonts, &e.children, is.font_size)
                    } else {
                        measure_inline_block_children(fonts, &e.children, is.font_size)
                    }
                }
            } else if let Node::Text(t) = item {
                crate::render::layout::paint::measure_text(fonts, &t.text, is).0
            } else { 0 }
        }).collect();

        // Step 2: compute the total hypothetical size including padding/margin/gaps
        let total_hypo: i32 = base_content_widths.iter().zip(items.iter())
            .map(|(cw, item)| {
                let is = item.style();
                cw + is.padding.left + is.padding.right + is.margin.left + is.margin.right
            })
            .sum::<i32>()
            + gap * (n as i32 - 1);

        // Step 3: distribute free space via flex-grow / shrink
        let free = inner_w - total_hypo;
        let total_grow:   f32 = items.iter().map(|i| i.style().flex_grow).sum();
        let total_shrink: f32 = items.iter().map(|i| i.style().flex_shrink).sum();

        items.iter().enumerate().map(|(idx, item)| {
            let is = item.style();
            let item_margin_h = is.padding.left + is.padding.right + is.margin.left + is.margin.right;
            let mut content_w = base_content_widths[idx];

            if free > 0 && total_grow > 0.0 && is.flex_grow > 0.0 {
                content_w += (free as f32 * is.flex_grow / total_grow) as i32;
            } else if free < 0 && total_shrink > 0.0 && is.flex_shrink > 0.0 {
                content_w += (free as f32 * is.flex_shrink / total_shrink) as i32;
            }
            content_w = content_w.max(0);

            let main = content_w + item_margin_h;
            // Measure the cross-axis (height) for this row item.
            // measure_block_children already includes padding.top and padding.bottom
            // in its return value (it subtracts padding.top from start_y so the caller
            // gets the full visual height including both vertical paddings).
            // We must NOT add padding.top/bottom again — only add margin.top/bottom.
            let cross = if let Node::Element(e) = item {
                let old_cx = ls.cursor_x;
                let old_cy = ls.cursor_y;
                let old_ml = ls.margin_left;
                let old_indent = ls.indent;
                let old_lh = ls.line_height;
                ls.cursor_x    = inner_x + is.padding.left;
                ls.cursor_y    = 0;
                ls.margin_left = inner_x + is.padding.left;
                ls.indent      = 0;
                ls.line_height = is.font_size as i32;
                // max_w must be set so that the available width for text inside
                // the item equals content_w.  measure_block_children computes:
                //   avail = max_w - ls.margin_left - MARGIN_RIGHT
                // so: max_w = ls.margin_left + content_w + MARGIN_RIGHT
                //           = (inner_x + padding.left) + content_w + MARGIN_RIGHT
                let item_max_w = ls.margin_left + content_w.max(1) + MARGIN_RIGHT;
                let h = measure_block_children(ls, fonts, e, item_max_w, is);
                ls.cursor_x    = old_cx;
                ls.cursor_y    = old_cy;
                ls.margin_left = old_ml;
                ls.indent      = old_indent;
                ls.line_height = old_lh;
                // For form controls (input, button, select, textarea), use their
                // actual rendered height rather than text-content measurement.
                let form_h = form_control_height(e, is);
                h.max(form_h).max(is.font_size as i32 + is.padding.top + is.padding.bottom)
            } else if let Node::Text(t) = item {
                crate::render::layout::paint::measure_text(fonts, &t.text, is).1
            } else { 0 };
            // measure_block_children return already contains padding.top + content + padding.bottom.
            // Only add the item's outer margins for the full cross extent.
            let cross = cross + is.margin.top + is.margin.bottom;
            ItemGeom { base_size: main, cross_size: cross, grow: is.flex_grow, shrink: is.flex_shrink }
        }).collect()
    } else {
        items.iter().map(|item| {
            let is = item.style();
            let item_outer_w = inner_w;
            let item_inner_w = (item_outer_w
                - is.padding.left - is.padding.right
                - is.margin.left  - is.margin.right).max(1);

            let natural_h = if let Node::Element(e) = item {
                if e.style.display == crate::dom::node::Display::Flex {
                    measure_flex_height(ls, fonts, e, item_inner_w)
                } else {
                    let old_cx = ls.cursor_x;
                    let old_cy = ls.cursor_y;
                    let old_ml = ls.margin_left;
                    let old_indent = ls.indent;
                    let old_lh = ls.line_height;
                    ls.cursor_x    = inner_x + is.padding.left;
                    ls.cursor_y    = 0;
                    ls.margin_left = inner_x + is.padding.left;
                    ls.indent      = 0;
                    ls.line_height = is.font_size as i32;
                    let h = measure_block_children(ls, fonts, e, inner_x + item_inner_w, is);
                    ls.cursor_x    = old_cx;
                    ls.cursor_y    = old_cy;
                    ls.margin_left = old_ml;
                    ls.indent      = old_indent;
                    ls.line_height = old_lh;
                    h.max(is.font_size as i32 + is.padding.top + is.padding.bottom)
                }
            } else if let Node::Text(t) = item {
                crate::render::layout::paint::measure_text(fonts, &t.text, is).1
            } else { 0 };
            let base_size = natural_h + is.margin.top + is.margin.bottom;
            ItemGeom { base_size, cross_size: inner_w, grow: is.flex_grow, shrink: is.flex_shrink }
        }).collect()
    };

    // ── Cross-axis size of the container ────────────────────────────────────
    let max_cross = geoms.iter().map(|g| g.cross_size).max().unwrap_or(0);

    // ── Wrap: split items into lines when flex-wrap is active ────────────────
    // We support flex-wrap: wrap for row containers.  Items are placed on a
    // line until adding the next item would overflow inner_w; that item starts
    // a new line instead.  Items whose resolved main-axis size equals inner_w
    // (e.g. width: 100%) are always placed on their own line.
    let do_wrap = is_row && s.flex_wrap == crate::dom::node::FlexWrap::Wrap;

    // lines[i] = list of indices into `geoms`/`items` that belong to line i.
    let lines: Vec<Vec<usize>> = if do_wrap {
        let mut lines: Vec<Vec<usize>> = Vec::new();
        let mut current_line: Vec<usize> = Vec::new();
        let mut line_used: i32 = 0;
        for idx in 0..n {
            let g = &geoms[idx];
            let item_size = g.base_size;
            let gap_add   = if current_line.is_empty() { 0 } else { gap };
            // If this item fills the full width, or adding it would overflow → new line.
            if !current_line.is_empty()
                && (item_size >= inner_w || line_used + gap_add + item_size > inner_w)
            {
                lines.push(current_line);
                current_line = Vec::new();
                line_used    = 0;
            }
            line_used += if current_line.is_empty() { 0 } else { gap } + item_size;
            current_line.push(idx);
        }
        if !current_line.is_empty() { lines.push(current_line); }
        lines
    } else {
        vec![(0..n).collect()]
    };

    // Height of each line = max cross_size of items in that line.
    let line_heights: Vec<i32> = lines.iter().map(|line| {
        line.iter().map(|&idx| geoms[idx].cross_size).max().unwrap_or(0)
    }).collect();
    let row_gap = s.row_gap.max(s.gap); // use row_gap if set, else gap

    let container_h = if let Some(h) = resolved_height {
        h + pad_t + pad_b
    } else if is_row {
        if do_wrap {
            let total_h: i32 = line_heights.iter().sum::<i32>()
                + row_gap * (line_heights.len() as i32 - 1).max(0);
            total_h + pad_t + pad_b
        } else {
            max_cross + pad_t + pad_b
        }
    } else {
        let total_main: i32 = geoms.iter().map(|g| g.base_size).sum::<i32>()
            + gap * (n as i32 - 1);
        total_main + pad_t + pad_b
    };

    // ── Paint container background ──────────────────────────────────────────
    paint_flex_bg(ls, canvas, tc, images, base_url, s, container_x, start_y, container_w, container_h);
    paint_block_border(ls, canvas, s, container_x, start_y, container_w, container_h);

    // ── Compute positions along main axis (justify-content) ─────────────────
    let total_main: i32 = if is_row {
        geoms.iter().map(|g| g.base_size).sum::<i32>() + gap * (n as i32 - 1)
    } else {
        geoms.iter().map(|g| g.base_size).sum::<i32>() + gap * (n as i32 - 1)
    };

    let inner_main = if is_row { inner_w } else {
        let h = container_h - pad_t - pad_b;
        h.max(total_main)
    };

    let free_main = (inner_main - total_main).max(0);

    let (initial_offset, spacing_between) = match s.justify_content {
        JustifyContent::FlexStart   => (0, 0),
        JustifyContent::FlexEnd     => (free_main, 0),
        JustifyContent::Center      => (free_main / 2, 0),
        JustifyContent::SpaceBetween => {
            if n > 1 { (0, free_main / (n as i32 - 1)) } else { (0, 0) }
        }
        JustifyContent::SpaceAround => {
            let sp = free_main / n as i32;
            (sp / 2, sp)
        }
        JustifyContent::SpaceEvenly => {
            let sp = free_main / (n as i32 + 1);
            (sp, sp)
        }
    };

    // ── Lay out children ────────────────────────────────────────────────────
    let saved_margin_left = ls.margin_left;
    let saved_indent      = ls.indent;

    if do_wrap {
        // ── Multi-line wrapped row layout ────────────────────────────────
        let mut line_y_offset: i32 = 0;
        for (line_idx, line) in lines.iter().enumerate() {
            let line_h = line_heights[line_idx];
            let line_n = line.len();
            let line_total: i32 = line.iter().map(|&i| geoms[i].base_size).sum::<i32>()
                + gap * (line_n as i32 - 1).max(0);
            let line_free = (inner_w - line_total).max(0);
            let (line_off, line_sp) = match s.justify_content {
                JustifyContent::FlexStart   => (0, 0),
                JustifyContent::FlexEnd     => (line_free, 0),
                JustifyContent::Center      => (line_free / 2, 0),
                JustifyContent::SpaceBetween => {
                    if line_n > 1 { (0, line_free / (line_n as i32 - 1)) } else { (0, 0) }
                }
                JustifyContent::SpaceAround => {
                    let sp = if line_n > 0 { line_free / line_n as i32 } else { 0 };
                    (sp / 2, sp)
                }
                JustifyContent::SpaceEvenly => {
                    let sp = line_free / (line_n as i32 + 1);
                    (sp, sp)
                }
            };
            let mut main_cursor = line_off;
            for (slot, &idx) in line.iter().enumerate() {
                let item = items[idx];
                let geom = &geoms[idx];
                let is   = item.style();
                let item_main  = geom.base_size;
                let item_cross = geom.cross_size;
                let cross_offset = match s.align_items {
                    AlignItems::FlexStart | AlignItems::Baseline => 0,
                    AlignItems::FlexEnd   => (line_h - item_cross).max(0),
                    AlignItems::Center    => (line_h - item_cross).max(0) / 2,
                    AlignItems::Stretch   => 0,
                };
                let physical_main = if is_reverse { inner_w - main_cursor - item_main } else { main_cursor };
                let child_x  = inner_x + physical_main + is.margin.left;
                let child_y  = start_y + pad_t + line_y_offset + cross_offset + is.margin.top;
                let child_mw = child_x + item_main - is.margin.left - is.margin.right;
                let (old_cx, old_cy, old_ml, old_ind, old_lh) =
                    (ls.cursor_x, ls.cursor_y, ls.margin_left, ls.indent, ls.line_height);
                ls.cursor_x    = child_x - is.margin.left;
                ls.cursor_y    = child_y - is.margin.top;
                ls.margin_left = child_x - is.margin.left;
                ls.indent      = 0;
                ls.line_height = is.font_size as i32;
                ls.layout_node(canvas, tc, fonts, images, base_url, item, child_mw.max(child_x + 1));
                ls.cursor_x    = old_cx;
                ls.cursor_y    = old_cy;
                ls.margin_left = old_ml;
                ls.indent      = old_ind;
                ls.line_height = old_lh;
                main_cursor += item_main + gap;
                if slot < line_n - 1 { main_cursor += line_sp; }
            }
            line_y_offset += line_h + row_gap;
        }
    } else {
        // ── Single-line (no-wrap) layout — original path ─────────────────
        let cross_inner = if is_row { container_h - pad_t - pad_b } else { inner_w };
        let mut main_cursor = initial_offset;

        for (slot, idx) in (0..n).enumerate() {
            let item = items[idx];
            let geom = &geoms[idx];
            let is = item.style();

            let item_main  = geom.base_size;
            let item_cross = geom.cross_size;

            let cross_offset = match s.align_items {
                AlignItems::FlexStart | AlignItems::Baseline => 0,
                AlignItems::FlexEnd   => (cross_inner - item_cross).max(0),
                AlignItems::Center    => (cross_inner - item_cross).max(0) / 2,
                AlignItems::Stretch   => 0,
            };
            let physical_main = if is_reverse { inner_main - main_cursor - item_main } else { main_cursor };
            let (child_x, child_y, child_max_w) = if is_row {
                let x  = inner_x + physical_main + is.margin.left;
                let y  = start_y + pad_t + cross_offset + is.margin.top;
                let mw = x + item_main - is.margin.left - is.margin.right;
                (x, y, mw)
            } else {
                let x  = inner_x + cross_offset + is.margin.left;
                let y  = start_y + pad_t + physical_main + is.margin.top;
                let mw = inner_x + inner_w - is.margin.right;
                (x, y, mw)
            };

            let old_cx     = ls.cursor_x;
            let old_cy     = ls.cursor_y;
            let old_ml     = ls.margin_left;
            let old_indent = ls.indent;
            let old_lh     = ls.line_height;
            ls.cursor_x    = child_x - is.margin.left;
            ls.cursor_y    = child_y - is.margin.top;
            ls.margin_left = child_x - is.margin.left;
            ls.indent      = 0;
            ls.line_height = is.font_size as i32;
            ls.layout_node(canvas, tc, fonts, images, base_url, item, child_max_w.max(child_x + 1));
            ls.cursor_x    = old_cx;
            ls.cursor_y    = old_cy;
            ls.margin_left = old_ml;
            ls.indent      = old_indent;
            ls.line_height = old_lh;

            main_cursor += item_main + gap;
            if slot < n - 1 { main_cursor += spacing_between; }
        }
    }

    ls.margin_left = saved_margin_left;
    ls.indent      = saved_indent;

    let is_positioned = s.position != crate::dom::node::Position::Static;
    if is_positioned {
        ls.positioned_ancestors.push(LayoutBox {
            x: container_x, y: start_y, w: container_w, h: container_h,
        });
    }

    // Phase 2: Absolute
    for child in &el.children {
        if let Node::Element(e) = child {
            if e.style.position == crate::dom::node::Position::Absolute {
                ls.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
            }
        }
    }

    if is_positioned {
        ls.positioned_ancestors.pop();
    }

    // Push the container layout box
    ls.boxes.push(LayoutBox {
        x: container_x, y: start_y, w: container_w, h: container_h,
    });

    // Advance the outer cursor past this block
    ls.cursor_y    = start_y + container_h + BLOCK_MARGIN + s.margin.bottom;
    ls.cursor_x    = ls.margin_left + ls.indent;
    ls.line_height = 16;
}

// ── Helper: measure the rendered height of a flex container without painting ─

/// Returns the natural (shrink-to-fit) content width of a flex container,
/// correctly accounting for gap between items and each item's own padding/margin.
/// Used when a flex item is itself a flex container and has no explicit width.
fn measure_flex_natural_width(fonts: &mut FontCache, el: &Element) -> i32 {
    let s = &el.style;
    let is_row = s.flex_direction.is_row();
    let gap    = s.gap;

    let items: Vec<&Node> = el.children.iter().filter(|n| {
        match n {
            Node::Element(e) => e.style.display != crate::dom::node::Display::Hidden,
            Node::Text(_) => true,
        }
    }).collect();

    if items.is_empty() {
        return 0;
    }

    let n = items.len();

    // Measure each item's natural content width (recurse for nested flex).
    let item_widths: Vec<i32> = items.iter().map(|item| {
        let is = item.style();
        let content_w = if let Some(basis) = is.flex_basis {
            basis
        } else if let Some(w) = is.size.width {
            w
        } else if let Node::Element(e) = item {
            if e.style.display == crate::dom::node::Display::Flex {
                measure_flex_natural_width(fonts, e)
            } else {
                let has_block = e.children.iter().any(|c| {
                    matches!(c, crate::dom::node::Node::Element(e) if e.style.display_block)
                });
                if has_block {
                    measure_block_content_width(fonts, &e.children, is.font_size)
                } else {
                    measure_inline_block_children(fonts, &e.children, is.font_size)
                }
            }
        } else if let Node::Text(t) = item {
            crate::render::layout::paint::measure_text(fonts, &t.text, is).0
        } else { 0 };
        content_w + is.padding.left + is.padding.right + is.margin.left + is.margin.right
    }).collect();

    if is_row {
        // Row: natural width = sum of all item widths + gaps
        item_widths.iter().sum::<i32>() + gap * (n as i32 - 1)
            + s.padding.left + s.padding.right
    } else {
        // Column: natural width = widest item
        item_widths.iter().copied().max().unwrap_or(0)
            + s.padding.left + s.padding.right
    }
}

/// Returns the height of flex container `el` including its own padding.top and
/// padding.bottom but NOT its margin (matching the same contract as
/// `measure_block_children`). The caller adds margin separately.
fn measure_flex_height(
    ls:    &mut LayoutState,
    fonts: &mut FontCache,
    el:    &Element,
    avail_w: i32,
) -> i32 {
    let s = &el.style;
    let is_row = s.flex_direction.is_row();
    let gap    = s.gap;

    let items: Vec<&Node> = el.children.iter().filter(|n| {
        match n {
            Node::Element(e) => e.style.display != crate::dom::node::Display::Hidden,
            Node::Text(_) => true,
        }
    }).collect();

    if items.is_empty() {
        return s.font_size as i32;
    }

    let n = items.len();

    if is_row {
        // Row flex: height = max cross-size of all items.
        // For a quick measurement we give each item an equal share of avail_w.
        let per_item_w = ((avail_w - gap * (n as i32 - 1)) / n as i32).max(1);
        let max_h = items.iter().map(|item| {
            let is = item.style();
            let item_inner_w = (per_item_w
                - is.padding.left - is.padding.right
                - is.margin.left  - is.margin.right).max(1);

            let h = if let Node::Element(e) = item {
                if e.style.display == crate::dom::node::Display::Flex {
                    measure_flex_height(ls, fonts, e, item_inner_w)
                } else {
                    let old_cx = ls.cursor_x; let old_cy = ls.cursor_y;
                    let old_ml = ls.margin_left; let old_indent = ls.indent; let old_lh = ls.line_height;
                    ls.cursor_x = is.padding.left; ls.cursor_y = 0;
                    ls.margin_left = is.padding.left; ls.indent = 0;
                    ls.line_height = is.font_size as i32;
                    let h = measure_block_children(ls, fonts, e, item_inner_w, is);
                    ls.cursor_x = old_cx; ls.cursor_y = old_cy;
                    ls.margin_left = old_ml; ls.indent = old_indent; ls.line_height = old_lh;
                    h.max(is.font_size as i32 + is.padding.top + is.padding.bottom)
                }
            } else if let Node::Text(t) = item {
                crate::render::layout::paint::measure_text(fonts, &t.text, is).1
            } else { 0 };
            h + is.margin.top + is.margin.bottom
        }).max().unwrap_or(s.font_size as i32);
        // Add the container's own vertical padding to match measure_block_children semantics.
        max_h + s.padding.top + s.padding.bottom
    } else {
        // Column flex: height = sum of item heights + gaps.
        let total: i32 = items.iter().map(|item| {
            let is = item.style();
            let item_inner_w = (avail_w
                - is.padding.left - is.padding.right
                - is.margin.left  - is.margin.right).max(1);

            let h = if let Node::Element(e) = item {
                if e.style.display == crate::dom::node::Display::Flex {
                    measure_flex_height(ls, fonts, e, item_inner_w)
                } else {
                    let old_cx = ls.cursor_x; let old_cy = ls.cursor_y;
                    let old_ml = ls.margin_left; let old_indent = ls.indent; let old_lh = ls.line_height;
                    ls.cursor_x = is.padding.left; ls.cursor_y = 0;
                    ls.margin_left = is.padding.left; ls.indent = 0;
                    ls.line_height = is.font_size as i32;
                    let h = measure_block_children(ls, fonts, e, item_inner_w, is);
                    ls.cursor_x = old_cx; ls.cursor_y = old_cy;
                    ls.margin_left = old_ml; ls.indent = old_indent; ls.line_height = old_lh;
                    h.max(is.font_size as i32 + is.padding.top + is.padding.bottom)
                }
            } else if let Node::Text(t) = item {
                crate::render::layout::paint::measure_text(fonts, &t.text, is).1
            } else { 0 };
            h + is.margin.top + is.margin.bottom
        }).sum::<i32>() + gap * (n as i32 - 1);
        // Add the container's own vertical padding to match measure_block_children semantics.
        total + s.padding.top + s.padding.bottom
    }
}

// ── Helper: paint the flex container background ──────────────────────────────

fn paint_flex_bg(
    ls:       &mut LayoutState,
    canvas:   &mut Canvas<Window>,
    tc:       &TextureCreator<WindowContext>,
    images:   &mut ImageCache,
    base_url: &str,
    s:        &crate::dom::node::Style,
    x: i32, y: i32, w: i32, h: i32,
) {
    if let Some(bg) = s.bg_color {
        let alpha = s.bg_alpha;
        fill_rounded_rect(canvas, rgba_color(bg, alpha), alpha,
                          x, y, w, h, s.border_radius,
                          ls.ctx.scroll_y, ls.ctx.viewport_height);
    }
    if s.bg_gradient.is_some() {
        paint_block_bg_gradient(ls, canvas, s, x, y, w, h);
    }
    if s.bg_image_url.is_some() {
        paint_block_bg_image(ls, canvas, tc, images, base_url, s, x, y, w, h);
    }
}

/// Return the actual rendered height for a form control element, matching the
/// same logic used in `paint_form_control` so the flex cross-axis measurement
/// is accurate for inputs, buttons, textareas, etc.
fn form_control_height(el: &crate::dom::node::Element, s: &crate::dom::node::Style) -> i32 {
    let tag = el.tag.as_str();
    let input_type = crate::dom::parser::get_attr(&el.attrs_raw, "type")
        .unwrap_or("text")
        .to_ascii_lowercase();

    if matches!(tag, "input" | "button" | "select" | "textarea") || tag == "button" {
        match (tag, input_type.as_str()) {
            (_, "hidden") => 0,
            (_, "checkbox") | (_, "radio") => 16,
            (_, "range") => s.size.height.unwrap_or(20).max(20),
            (_, "color") => 28,
            ("textarea", _) => s.size.height.unwrap_or(80),
            _ => s.size.height.unwrap_or(28),
        }
    } else {
        0
    }
}
