use sdl2::pixels::Color;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::dom::node::{Node, Element};
use crate::render::font::FontCache;
use crate::render::image::ImageCache;

use super::paint::{fill_rect, fill_rect_alpha};
use super::state::{LayoutState, MARGIN_RIGHT, BLOCK_MARGIN};

/// Default cell padding used when no CSS padding is set on the cell.
const DEFAULT_CELL_PAD: i32 = 6;

/// Lay out a `<table>` element using a uniform column grid.
pub fn layout_table(
    ls:       &mut LayoutState,
    canvas:   &mut Canvas<Window>,
    tc:       &TextureCreator<WindowContext>,
    fonts:    &mut FontCache,
    images:   &mut ImageCache,
    base_url: &str,
    table:    &Element,
    max_w:    i32,
) {
    if ls.cursor_x > ls.margin_left {
        ls.newline(table.style.font_size, table.style.line_height_mul);
    }
    ls.cursor_y += BLOCK_MARGIN;

    let table_x       = ls.margin_left + ls.indent;
    let table_start_y = ls.cursor_y;
    let table_w       = (max_w - table_x - MARGIN_RIGHT).max(60);

    let rows = collect_rows(table);
    if rows.is_empty() {
        ls.cursor_y += BLOCK_MARGIN;
        return;
    }

    let col_count = rows.iter()
        .map(|r| r.iter().filter(|n| is_cell(n)).count())
        .max()
        .unwrap_or(1)
        .max(1);

    let col_w = table_w / col_count as i32;

    for row in &rows {
        let cells: Vec<&Node> = row.iter().copied().filter(|n| is_cell(n)).collect();
        if cells.is_empty() { continue; }

        let row_start_y = ls.cursor_y;

        // Pass 1 — measure each cell height
        let mut cell_heights = vec![0i32; cells.len()];
        for (ci, cell_node) in cells.iter().enumerate() {
            if let Node::Element(cell) = cell_node {
                let cx   = table_x + ci as i32 * col_w;
                // Use cell's CSS padding if set, otherwise fall back to default
                let pad_top  = if cell.style.padding.top  > 0 { cell.style.padding.top  } else { DEFAULT_CELL_PAD };
                let pad_left = if cell.style.padding.left > 0 { cell.style.padding.left } else { DEFAULT_CELL_PAD };
                let pad_bot  = if cell.style.padding.bottom > 0 { cell.style.padding.bottom } else { DEFAULT_CELL_PAD };
                let pad_rgt  = if cell.style.padding.right  > 0 { cell.style.padding.right  } else { DEFAULT_CELL_PAD };
                let mut sub = sub_state(ls, cx + pad_left, row_start_y + pad_top, cell.style.font_size);
                for child in &cell.children {
                    sub.layout_node(canvas, tc, fonts, images, base_url, child, cx + col_w - pad_rgt);
                }
                cell_heights[ci] = sub.cursor_y + sub.line_height + pad_bot - row_start_y;
            }
        }

        let row_h = cell_heights.iter().copied().max().unwrap_or(24).max(24);

        for (ci, cell_node) in cells.iter().enumerate() {
            if let Node::Element(cell) = cell_node {
                let cx = table_x + ci as i32 * col_w;

                if let Some(bg) = cell.style.bg_color {
                    fill_rect(canvas, Color::RGB(bg[0], bg[1], bg[2]),
                              cx, row_start_y, col_w, row_h,
                              ls.ctx.scroll_y, ls.ctx.viewport_height);
                }

                if cell.style.borders.top.width > 0 {
                    let bc   = cell.style.borders.top.color;
                    let bc_c = Color::RGB(bc[0], bc[1], bc[2]);
                    fill_rect_alpha(canvas, bc_c, 255,
                        cx, row_start_y, col_w, 1,
                        ls.rounding_clip.as_ref(),
                        ls.ctx.scroll_y, ls.ctx.viewport_height);
                    fill_rect_alpha(canvas, bc_c, 255,
                        cx, row_start_y, 1, row_h,
                        ls.rounding_clip.as_ref(),
                        ls.ctx.scroll_y, ls.ctx.viewport_height);
                }

                let pad_top  = if cell.style.padding.top    > 0 { cell.style.padding.top    } else { DEFAULT_CELL_PAD };
                let pad_left = if cell.style.padding.left   > 0 { cell.style.padding.left   } else { DEFAULT_CELL_PAD };
                let pad_rgt  = if cell.style.padding.right  > 0 { cell.style.padding.right  } else { DEFAULT_CELL_PAD };
                let mut sub = sub_state(ls, cx + pad_left, row_start_y + pad_top, cell.style.font_size);
                for child in &cell.children {
                    sub.layout_node(canvas, tc, fonts, images, base_url, child, cx + col_w - pad_rgt);
                }
                // Merge clickable areas back to the parent state
                ls.link_areas.extend(sub.link_areas);
                ls.button_areas.extend(sub.button_areas);
                ls.details_areas.extend(sub.details_areas);
                ls.audio_areas.extend(sub.audio_areas);
                ls.event_areas.extend(sub.event_areas);
                // Merge input areas back, fixing up indices relative to parent's count
                let base_idx = ls.input_count;
                for mut ia in sub.input_areas {
                    ia.index += base_idx;
                    ls.input_areas.push(ia);
                }
                ls.input_count += sub.input_count;
                // Merge any new input values captured by the sub-state
                if sub.input_values.len() > ls.input_values.len() {
                    ls.input_values.resize(base_idx + sub.input_values.len(), String::new());
                    for (i, v) in sub.input_values.into_iter().enumerate() {
                        if !v.is_empty() { ls.input_values[base_idx + i] = v; }
                    }
                }
            }
        }

        ls.cursor_y += row_h;
    }

    // Collapsed border closing lines — only needed when cells have borders.
    // Draw the right edge and bottom edge to close the grid.
    let table_h = ls.cursor_y - table_start_y;
    let cells_have_borders = rows.iter().flatten().any(|n| {
        matches!(n, Node::Element(e) if is_cell(n) && e.style.borders.top.width > 0)
    });
    if cells_have_borders && table_h > 0 && table_w > 0 {
        let grid_color = first_cell_border_color(&rows);
        let gc = Color::RGB(grid_color[0], grid_color[1], grid_color[2]);
        fill_rect_alpha(canvas, gc, 255,
            table_x + table_w - 1, table_start_y, 1, table_h,
            ls.rounding_clip.as_ref(),
            ls.ctx.scroll_y, ls.ctx.viewport_height);
        fill_rect_alpha(canvas, gc, 255,
            table_x, ls.cursor_y - 1, table_w, 1,
            ls.rounding_clip.as_ref(),
            ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    if table.style.borders.top.width > 0 {
        let bc = table.style.borders.top.color;
        let bc_c = Color::RGB(bc[0], bc[1], bc[2]);
        fill_rect_alpha(canvas, bc_c, 255, table_x, table_start_y, table_w, 1,
            ls.rounding_clip.as_ref(),
            ls.ctx.scroll_y, ls.ctx.viewport_height);
        fill_rect_alpha(canvas, bc_c, 255, table_x, table_start_y, 1, table_h,
            ls.rounding_clip.as_ref(),
            ls.ctx.scroll_y, ls.ctx.viewport_height);
        fill_rect_alpha(canvas, bc_c, 255, table_x + table_w - 1, table_start_y, 1, table_h,
            ls.rounding_clip.as_ref(),
            ls.ctx.scroll_y, ls.ctx.viewport_height);
        fill_rect_alpha(canvas, bc_c, 255, table_x, table_start_y + table_h - 1, table_w, 1,
            ls.rounding_clip.as_ref(),
            ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    ls.cursor_y   += BLOCK_MARGIN;
    ls.cursor_x    = ls.margin_left;
    ls.line_height = 16;
}

/// Create a child LayoutState that shares the same RenderCtx.
fn sub_state<'ctx>(
    parent: &LayoutState<'ctx>,
    cx: i32,
    cy: i32,
    font_size: u16,
) -> LayoutState<'ctx> {
    LayoutState {
        ctx:           parent.ctx,
        cursor_x:      cx,
        cursor_y:      cy,
        line_height:   font_size as i32,
        indent:        0,
        margin_left:   cx,   // cell content starts at cx
        boxes:         Vec::new(),
        link_areas:    Vec::new(),
        input_areas:   Vec::new(),
        button_areas:  Vec::new(),
        details_areas: Vec::new(),
        audio_areas:   Vec::new(),
        event_areas:   Vec::new(),
        audio_playback: std::collections::HashMap::new(),
        audio_count:   0,
        input_count:   0,
        input_values:  parent.input_values.clone(),
        focused_input: parent.focused_input,
        form_action:   parent.form_action.clone(),
        ol_stack:      Vec::new(),
        content_height: 0,
        rounding_clip:  parent.rounding_clip.clone(),
        positioned_ancestors: parent.positioned_ancestors.clone(),
        in_absolute_pass: parent.in_absolute_pass,
    }
}

/// Collect all `<tr>` rows inside a table, reaching through thead/tbody/tfoot.
fn collect_rows(table: &Element) -> Vec<Vec<&Node>> {
    let mut rows = Vec::new();
    for child in &table.children {
        match child {
            Node::Element(e) if e.tag == "tr" => {
                rows.push(e.children.iter().collect());
            }
            Node::Element(e) if matches!(e.tag.as_str(), "thead"|"tbody"|"tfoot") => {
                for sub in &e.children {
                    if let Node::Element(tr) = sub {
                        if tr.tag == "tr" {
                            rows.push(tr.children.iter().collect());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    rows
}

fn is_cell(node: &Node) -> bool {
    matches!(node, Node::Element(e) if matches!(e.tag.as_str(), "td" | "th"))
}

/// Return the border colour of the first cell found in the row list, or a
/// neutral grey fallback used for the closing grid lines.
fn first_cell_border_color(rows: &[Vec<&Node>]) -> [u8; 3] {
    for row in rows {
        for node in row {
            if let Node::Element(e) = node {
                if is_cell(node) {
                    return e.style.borders.top.color;
                }
            }
        }
    }
    [200, 200, 200]
}
