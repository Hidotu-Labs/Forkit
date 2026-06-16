/// CSS Grid layout engine.
///
/// Supports:
///   - grid-template-columns / grid-template-rows  (px, fr, %, auto, repeat(), minmax())
///   - grid-auto-flow: row | column
///   - grid-auto-rows / grid-auto-columns
///   - column-gap / row-gap (and the `gap` shorthand)
///   - justify-content / align-content  (space-between, space-around, space-evenly, center, end)
///   - align-items / justify-items
///   - per-item placement: grid-column, grid-row, grid-column-start/end, grid-row-end, grid-area
///   - span N  (negative line values)
///   - padding on the container

use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::dom::node::{
    Display, Element, Node, GridTrackSize, GridAutoFlow,
    AlignItems, JustifyContent, JustifyItems, AlignContent,
};
use crate::render::font::FontCache;
use crate::render::image::ImageCache;

use super::block::{
    layout_element, measure_block_children, resolve_size,
    paint_block_bg_gradient, paint_block_bg_image, paint_block_border,
    measure_block_content_width, measure_inline_block_children,
};
use super::paint::{fill_rounded_rect, draw_rounded_rect, rgba_color};
use super::state::{LayoutState, LayoutBox, BLOCK_MARGIN, LINE_SPACING, MARGIN_RIGHT};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Lay out a `display: grid` container element.
pub fn layout_grid(
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

    // Begin block
    if ls.cursor_x > ls.margin_left + ls.indent {
        ls.cursor_y += ls.line_height + LINE_SPACING;
    }
    ls.cursor_y += BLOCK_MARGIN + s.margin.top;
    ls.line_height = font_size as i32;

    let container_x = contain_left + ml;
    let container_w = box_w.min(contain_right - container_x - s.margin.right).max(0);
    let start_y = ls.cursor_y;

    let pad_l = s.padding.left;
    let pad_r = s.padding.right;
    let pad_t = s.padding.top;
    let pad_b = s.padding.bottom;

    let inner_x = container_x + pad_l;
    let inner_w = (container_w - pad_l - pad_r).max(1);

    // ── Collect visible grid items ──────────────────────────────────────────
    let items: Vec<&Node> = el.children.iter().filter(|n| match n {
        Node::Element(e) => {
            e.style.position != crate::dom::node::Position::Absolute
                && e.style.display != Display::Hidden
        }
        Node::Text(t) => !t.text.trim().is_empty(),
    }).collect();

    if items.is_empty() {
        let box_h = resolved_height.map(|h| h + pad_t + pad_b)
            .unwrap_or(pad_t + pad_b)
            .max(font_size as i32);
        paint_grid_bg(ls, canvas, tc, images, base_url, s, container_x, start_y, container_w, box_h);
        paint_block_border(ls, canvas, s, container_x, start_y, container_w, box_h);
        ls.cursor_y    = start_y + box_h + BLOCK_MARGIN + s.margin.bottom;
        ls.cursor_x    = ls.margin_left + ls.indent;
        ls.line_height = 16;
        return;
    }

    let n = items.len();
    let col_gap = s.column_gap;
    let row_gap = s.row_gap;

    // ── Resolve explicit column / row tracks ────────────────────────────────
    let num_explicit_cols = s.grid_template_columns.len();

    // Determine the number of columns to use:
    // - From grid-template-columns if set, or
    // - From grid-auto-flow: column (treat as 1 column), or
    // - Default: sqrt(n) rounded up, or 1 if no items.
    let num_cols: usize = if num_explicit_cols > 0 {
        num_explicit_cols
    } else if s.grid_auto_flow.is_column() {
        1
    } else {
        // Heuristic: use 1 col, let items stack (most common case when no template).
        // A better heuristic could compute ceil(sqrt(n)), but erring on the side
        // of a single column is safer for arbitrary content.
        let sqrt_n = (n as f64).sqrt().ceil() as usize;
        sqrt_n.max(1)
    };

    // Resolve column widths from track sizes.
    let col_widths = resolve_tracks(
        &s.grid_template_columns,
        &s.grid_auto_columns,
        inner_w,
        col_gap,
        num_cols,
        vw, vh, font_size,
    );

    // ── Place items into grid cells ─────────────────────────────────────────
    // Grid placement algorithm (simplified — no overlap detection):
    // 1. Items with explicit placement are placed first.
    // 2. Remaining items fill in row order (or column order for grid-auto-flow: column).

    struct PlacedItem {
        item_idx:  usize,
        col_start: usize, // 0-based
        col_span:  usize,
        row_start: usize, // 0-based
        row_span:  usize,
    }

    let is_col_flow = s.grid_auto_flow.is_column();

    let mut placed: Vec<PlacedItem> = Vec::with_capacity(n);
    // Track occupied cells: (row, col) -> true
    let mut occupied: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

    // Separate items with and without explicit placement.
    let mut explicit_items: Vec<(usize, &Node)> = Vec::new();
    let mut auto_items: Vec<(usize, &Node)>     = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        let is = item.style();
        let has_explicit = is.grid_column_start != 0 || is.grid_row_start != 0
            || is.grid_column_end != 0 || is.grid_row_end != 0;
        if has_explicit {
            explicit_items.push((idx, item));
        } else {
            auto_items.push((idx, item));
        }
    }

    // Place explicit items first.
    for (idx, item) in &explicit_items {
        let is = item.style();
        let (col_start, col_span) = resolve_line_pair(
            is.grid_column_start, is.grid_column_end, num_cols, 1,
        );
        let (row_start, row_span) = resolve_line_pair(
            is.grid_row_start, is.grid_row_end, usize::MAX, 1,
        );
        // Mark cells occupied.
        for r in row_start..row_start+row_span {
            for c in col_start..col_start+col_span {
                occupied.insert((r, c));
            }
        }
        placed.push(PlacedItem { item_idx: *idx, col_start, col_span, row_start, row_span });
    }

    // Auto-place remaining items.
    let mut auto_cursor_row = 0usize;
    let mut auto_cursor_col = 0usize;
    for (idx, item) in &auto_items {
        let is = item.style();
        let col_span = if is.grid_column_end < 0 { (-is.grid_column_end) as usize } else { 1 };
        let row_span = if is.grid_row_end < 0 { (-is.grid_row_end) as usize } else { 1 };
        let col_span = col_span.max(1).min(num_cols);
        let row_span = row_span.max(1);

        // Find the next free position.
        loop {
            if auto_cursor_col + col_span > num_cols {
                auto_cursor_col = 0;
                auto_cursor_row += 1;
            }
            let fits = (auto_cursor_row..auto_cursor_row + row_span)
                .all(|r| (auto_cursor_col..auto_cursor_col + col_span)
                    .all(|c| !occupied.contains(&(r, c))));
            if fits { break; }
            if is_col_flow {
                auto_cursor_row += 1;
            } else {
                auto_cursor_col += 1;
                if auto_cursor_col + col_span > num_cols {
                    auto_cursor_col = 0;
                    auto_cursor_row += 1;
                }
            }
        }

        for r in auto_cursor_row..auto_cursor_row + row_span {
            for c in auto_cursor_col..auto_cursor_col + col_span {
                occupied.insert((r, c));
            }
        }

        placed.push(PlacedItem {
            item_idx:  *idx,
            col_start: auto_cursor_col,
            col_span,
            row_start: auto_cursor_row,
            row_span,
        });

        if is_col_flow {
            auto_cursor_row += row_span;
        } else {
            auto_cursor_col += col_span;
            if auto_cursor_col >= num_cols {
                auto_cursor_col = 0;
                auto_cursor_row += 1;
            }
        }
    }

    let num_rows = placed.iter().map(|p| p.row_start + p.row_span).max().unwrap_or(1);

    // ── Measure row heights ──────────────────────────────────────────────────
    let mut row_heights: Vec<i32> = vec![0; num_rows];

    for p in &placed {
        if p.row_span != 1 { continue; } // skip spanning items for now
        let item = items[p.item_idx];
        let is = item.style();

        // Width available to this item.
        let item_w = col_span_width(&col_widths, p.col_start, p.col_span, col_gap);
        let item_content_w = (item_w - is.padding.left - is.padding.right
                               - is.margin.left - is.margin.right).max(1);

        let nat_h = measure_item_height(ls, fonts, item, inner_x, item_content_w, is);
        let full_h = nat_h + is.margin.top + is.margin.bottom;
        if full_h > row_heights[p.row_start] {
            row_heights[p.row_start] = full_h;
        }
    }

    // Resolve explicit row tracks.
    if !s.grid_template_rows.is_empty() {
        let explicit_row_heights = resolve_tracks(
            &s.grid_template_rows,
            &s.grid_auto_rows,
            // Use total available height as percent-base.
            vh,
            row_gap,
            num_rows,
            vw, vh, font_size,
        );
        for (i, &h) in explicit_row_heights.iter().enumerate() {
            if i < row_heights.len() && h > 0 {
                row_heights[i] = h;
            }
        }
    }

    // Apply grid-auto-rows to implicit rows.
    for h in &mut row_heights {
        if *h == 0 {
            if let Some(auto_h) = resolve_track_size(&s.grid_auto_rows, vh, vw, vh, font_size) {
                if auto_h > 0 { *h = auto_h; }
            }
        }
        // Ensure minimum of font_size.
        if *h < font_size as i32 { *h = font_size as i32; }
    }

    // Total content height.
    let total_content_h: i32 = row_heights.iter().sum::<i32>()
        + row_gap * (num_rows as i32 - 1).max(0);

    let container_h = if let Some(h) = resolved_height {
        h + pad_t + pad_b
    } else {
        total_content_h + pad_t + pad_b
    };

    // ── Paint container background ──────────────────────────────────────────
    paint_grid_bg(ls, canvas, tc, images, base_url, s, container_x, start_y, container_w, container_h);
    paint_block_border(ls, canvas, s, container_x, start_y, container_w, container_h);

    // ── Compute column x-offsets ────────────────────────────────────────────
    // apply justify-content free space distribution across columns.
    let total_col_w: i32 = col_widths.iter().sum::<i32>()
        + col_gap * (num_cols as i32 - 1).max(0);
    let free_x = (inner_w - total_col_w).max(0);

    let (col_offset_x, col_spacing) = distribute_free_space(free_x, num_cols, s.justify_content);
    let col_offsets = build_offsets(&col_widths, col_gap + col_spacing, col_offset_x);

    // apply align-content free space across rows.
    let free_y = (container_h - pad_t - pad_b - total_content_h).max(0);
    let (row_offset_y, row_spacing) = distribute_free_space_content(free_y, num_rows, s.align_content);
    let row_offsets = build_offsets(&row_heights, row_gap + row_spacing, row_offset_y);

    // ── Lay out children into their cells ───────────────────────────────────
    let saved_margin_left = ls.margin_left;
    let saved_indent      = ls.indent;

    for p in &placed {
        let item = items[p.item_idx];
        let is = item.style();

        let cell_x = inner_x + col_offsets[p.col_start];
        let cell_y = start_y + pad_t + row_offsets[p.row_start];
        let cell_w = col_span_width(&col_widths, p.col_start, p.col_span, col_gap);
        let cell_h = row_span_height(&row_heights, p.row_start, p.row_span, row_gap);

        // Align item within cell.
        let (item_x, item_w) = align_item_inline(
            cell_x, cell_w, is, s.justify_items,
        );
        let (item_y, _item_h) = align_item_block(
            cell_y, cell_h, is, s.align_items,
        );

        let child_max_w = item_x + item_w - is.margin.right;

        let old_cx     = ls.cursor_x;
        let old_cy     = ls.cursor_y;
        let old_ml     = ls.margin_left;
        let old_indent = ls.indent;
        let old_lh     = ls.line_height;

        ls.cursor_x    = item_x - is.margin.left;
        ls.cursor_y    = item_y - is.margin.top;
        ls.margin_left = item_x - is.margin.left;
        ls.indent      = 0;
        ls.line_height = is.font_size as i32;

        ls.layout_node(canvas, tc, fonts, images, base_url, item, child_max_w.max(item_x + 1));

        ls.cursor_x    = old_cx;
        ls.cursor_y    = old_cy;
        ls.margin_left = old_ml;
        ls.indent      = old_indent;
        ls.line_height = old_lh;
    }

    ls.margin_left = saved_margin_left;
    ls.indent      = saved_indent;

    // Positioned children (absolute).
    let is_positioned = s.position != crate::dom::node::Position::Static;
    if is_positioned {
        ls.positioned_ancestors.push(LayoutBox {
            x: container_x, y: start_y, w: container_w, h: container_h,
        });
    }
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

    ls.boxes.push(LayoutBox {
        x: container_x, y: start_y, w: container_w, h: container_h,
    });

    ls.cursor_y    = start_y + container_h + BLOCK_MARGIN + s.margin.bottom;
    ls.cursor_x    = ls.margin_left + ls.indent;
    ls.line_height = 16;
}

// ---------------------------------------------------------------------------
// Track resolution
// ---------------------------------------------------------------------------

/// Resolve a list of `GridTrackSize` values to concrete pixel widths / heights.
///
/// `available` is the total space available for the tracks (including gaps).
/// `auto_track` is used for implicitly created tracks.
fn resolve_tracks(
    template:   &[GridTrackSize],
    auto_track: &GridTrackSize,
    available:  i32,
    gap:        i32,
    count:      usize,
    vw: i32, vh: i32, base: u16,
) -> Vec<i32> {
    if count == 0 { return Vec::new(); }

    // Build a list of count track sizes, padding with auto_track if needed.
    let sizes: Vec<&GridTrackSize> = (0..count)
        .map(|i| template.get(i).unwrap_or(auto_track))
        .collect();

    let total_gap = gap * (count as i32 - 1).max(0);
    let avail_for_tracks = (available - total_gap).max(0);

    // First pass: resolve non-fr tracks.
    let mut resolved: Vec<Option<i32>> = vec![None; count];
    let mut total_fixed: i32 = 0;
    let mut total_fr: f32    = 0.0;

    for (i, size) in sizes.iter().enumerate() {
        match size {
            GridTrackSize::Fr(fr) => {
                total_fr += fr;
            }
            GridTrackSize::Minmax(min, max) => {
                // Use min value for fixed sizing pass.
                let v = resolve_track_size(min, avail_for_tracks, vw, vh, base)
                    .unwrap_or(0);
                resolved[i] = Some(v);
                total_fixed += v;
            }
            _ => {
                let v = resolve_track_size(size, avail_for_tracks, vw, vh, base)
                    .unwrap_or(0);
                resolved[i] = Some(v);
                total_fixed += v;
            }
        }
    }

    // Second pass: distribute remaining space to fr tracks.
    let remaining = (avail_for_tracks - total_fixed).max(0);
    for (i, size) in sizes.iter().enumerate() {
        if let GridTrackSize::Fr(fr) = size {
            let px = if total_fr > 0.0 {
                (remaining as f32 * fr / total_fr) as i32
            } else { 0 };
            resolved[i] = Some(px.max(0));
        }
    }

    resolved.into_iter().map(|v| v.unwrap_or(0)).collect()
}

/// Resolve a single `GridTrackSize` to a pixel value.
/// `percent_base` is the containing dimension (width for columns, height for rows).
fn resolve_track_size(
    size: &GridTrackSize,
    percent_base: i32,
    vw: i32, vh: i32, base: u16,
) -> Option<i32> {
    match size {
        GridTrackSize::Px(px)      => Some(*px),
        GridTrackSize::Fr(_)       => None, // handled separately
        GridTrackSize::Percent(pct) => Some((pct / 100.0 * percent_base as f32) as i32),
        GridTrackSize::Auto        => None,
        GridTrackSize::MinContent  => None,
        GridTrackSize::MaxContent  => None,
        GridTrackSize::Minmax(min, _max) => resolve_track_size(min, percent_base, vw, vh, base),
    }
}

// ---------------------------------------------------------------------------
// Placement helpers
// ---------------------------------------------------------------------------

/// Convert CSS 1-based line numbers (or span) to 0-based (start, span) pair.
///
/// * `start_line` – CSS grid-column-start / grid-row-start  
///   Positive = explicit 1-based line.  Negative = span count.  0 = auto.
/// * `end_line` – CSS grid-column-end / grid-row-end  
///   Positive = explicit 1-based line (exclusive).  Negative = span count.  0 = auto.
fn resolve_line_pair(start_line: i32, end_line: i32, max_tracks: usize, default_span: usize) -> (usize, usize) {
    let start = if start_line > 0 {
        (start_line as usize - 1).min(max_tracks.saturating_sub(1))
    } else { 0 };

    let span = if end_line > 0 {
        // end is exclusive 1-based line
        let end = (end_line as usize - 1).min(max_tracks);
        end.saturating_sub(start).max(1)
    } else if end_line < 0 {
        // negative = span count
        (-end_line) as usize
    } else {
        // auto end
        if start_line < 0 { (-start_line) as usize } else { default_span }
    };

    let span = span.min(max_tracks.saturating_sub(start)).max(1);
    (start, span)
}

/// Compute the pixel width of a column span (including internal gaps).
fn col_span_width(col_widths: &[i32], start: usize, span: usize, gap: i32) -> i32 {
    let end = (start + span).min(col_widths.len());
    if end <= start { return 0; }
    let w: i32 = col_widths[start..end].iter().sum();
    let g = gap * (end - start - 1).max(0) as i32;
    w + g
}

/// Compute the pixel height of a row span (including internal gaps).
fn row_span_height(row_heights: &[i32], start: usize, span: usize, gap: i32) -> i32 {
    let end = (start + span).min(row_heights.len());
    if end <= start { return 0; }
    let h: i32 = row_heights[start..end].iter().sum();
    let g = gap * (end - start - 1).max(0) as i32;
    h + g
}

/// Build cumulative offsets from track sizes and gap.
fn build_offsets(sizes: &[i32], step_gap: i32, initial_offset: i32) -> Vec<i32> {
    let mut offsets = Vec::with_capacity(sizes.len());
    let mut cursor = initial_offset;
    for &size in sizes {
        offsets.push(cursor);
        cursor += size + step_gap;
    }
    offsets
}

// ---------------------------------------------------------------------------
// Alignment helpers
// ---------------------------------------------------------------------------

/// Distribute free space across tracks using `justify-content` semantics.
/// Returns `(initial_offset, extra_gap_per_slot)`.
fn distribute_free_space(free: i32, n: usize, jc: JustifyContent) -> (i32, i32) {
    if n == 0 || free <= 0 { return (0, 0); }
    match jc {
        JustifyContent::FlexStart   => (0, 0),
        JustifyContent::FlexEnd     => (free, 0),
        JustifyContent::Center      => (free / 2, 0),
        JustifyContent::SpaceBetween => {
            if n > 1 { (0, free / (n as i32 - 1)) } else { (0, 0) }
        }
        JustifyContent::SpaceAround => {
            let sp = free / n as i32;
            (sp / 2, sp)
        }
        JustifyContent::SpaceEvenly => {
            let sp = free / (n as i32 + 1);
            (sp, sp)
        }
    }
}

/// Like `distribute_free_space` but for `align-content`.
fn distribute_free_space_content(free: i32, n: usize, ac: AlignContent) -> (i32, i32) {
    if n == 0 || free <= 0 { return (0, 0); }
    match ac {
        AlignContent::Start   | AlignContent::Stretch => (0, 0),
        AlignContent::End     => (free, 0),
        AlignContent::Center  => (free / 2, 0),
        AlignContent::SpaceBetween => {
            if n > 1 { (0, free / (n as i32 - 1)) } else { (0, 0) }
        }
        AlignContent::SpaceAround => {
            let sp = free / n as i32;
            (sp / 2, sp)
        }
        AlignContent::SpaceEvenly => {
            let sp = free / (n as i32 + 1);
            (sp, sp)
        }
    }
}

/// Compute the inline (x) position and width of a grid item inside its cell,
/// applying `justify-items` and the item's own margin.
fn align_item_inline(
    cell_x: i32,
    cell_w: i32,
    is:     &crate::dom::node::Style,
    ji:     JustifyItems,
) -> (i32, i32) {
    let outer_w = cell_w - is.margin.left - is.margin.right;
    let item_w = if let Some(w) = is.size.width { w } else { outer_w };
    let item_w = item_w.min(outer_w).max(0);

    let x_offset = match ji {
        JustifyItems::Stretch | JustifyItems::Start => 0,
        JustifyItems::End    => outer_w - item_w,
        JustifyItems::Center => (outer_w - item_w) / 2,
    };

    let x = cell_x + is.margin.left + x_offset;
    let w = if ji == JustifyItems::Stretch { outer_w } else { item_w };
    (x, w.max(0))
}

/// Compute the block (y) position of a grid item inside its cell,
/// applying `align-items` and the item's own margin.
fn align_item_block(
    cell_y: i32,
    cell_h: i32,
    is:     &crate::dom::node::Style,
    ai:     AlignItems,
) -> (i32, i32) {
    let outer_h = (cell_h - is.margin.top - is.margin.bottom).max(0);
    let item_h = if let Some(h) = is.size.height { h } else { outer_h };
    let item_h = item_h.min(outer_h).max(0);

    let y_offset = match ai {
        AlignItems::Stretch | AlignItems::FlexStart | AlignItems::Baseline => 0,
        AlignItems::FlexEnd  => outer_h - item_h,
        AlignItems::Center   => (outer_h - item_h) / 2,
    };

    let y = cell_y + is.margin.top + y_offset;
    (y, item_h)
}

// ---------------------------------------------------------------------------
// Measurement helpers
// ---------------------------------------------------------------------------

/// Measure the natural height of a single grid item without painting.
fn measure_item_height(
    ls:     &mut LayoutState,
    fonts:  &mut FontCache,
    item:   &Node,
    inner_x: i32,
    content_w: i32,
    is:     &crate::dom::node::Style,
) -> i32 {
    match item {
        Node::Element(e) => {
            let old_cx = ls.cursor_x; let old_cy = ls.cursor_y;
            let old_ml = ls.margin_left; let old_indent = ls.indent; let old_lh = ls.line_height;
            ls.cursor_x    = inner_x + is.padding.left;
            ls.cursor_y    = 0;
            ls.margin_left = inner_x + is.padding.left;
            ls.indent      = 0;
            ls.line_height = is.font_size as i32;
            let h = measure_block_children(ls, fonts, e, inner_x + content_w.max(1), is);
            ls.cursor_x    = old_cx; ls.cursor_y    = old_cy;
            ls.margin_left = old_ml; ls.indent      = old_indent; ls.line_height = old_lh;
            h.max(is.font_size as i32 + is.padding.top + is.padding.bottom)
        }
        Node::Text(t) => {
            crate::render::layout::paint::measure_text(fonts, &t.text, &t.style).1
        }
    }
}

// ---------------------------------------------------------------------------
// Background painter
// ---------------------------------------------------------------------------

fn paint_grid_bg(
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
