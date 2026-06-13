use crate::render::layout::state::{LayoutState, LINE_SPACING, BLOCK_MARGIN};
use crate::dom::node::Style;
use crate::dom::css::{LengthContext, parse_length_ctx};

pub fn resolve_size(
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

pub fn open_block(ls: &mut LayoutState, s: &Style) {
    if ls.cursor_x > ls.margin_left + ls.indent {
        ls.cursor_y += ls.line_height + LINE_SPACING;
    }
    ls.cursor_y   += BLOCK_MARGIN;
    ls.cursor_x    = ls.margin_left + ls.indent;
    ls.line_height = s.font_size as i32;
}

pub fn close_block(ls: &mut LayoutState) {
    if ls.cursor_x > ls.margin_left { ls.cursor_y += ls.line_height + LINE_SPACING; }
    ls.cursor_y   += BLOCK_MARGIN;
    ls.cursor_x    = ls.margin_left;
    ls.line_height = 16;
}
