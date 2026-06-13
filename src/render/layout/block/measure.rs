use crate::dom::node::{Node, Element, Display};
use crate::render::layout::state::{LayoutState, LINE_SPACING, BLOCK_MARGIN};
use crate::render::font::FontCache;
use crate::render::layout::paint::measure_text;

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

pub fn measure_inline_block_children(fonts: &mut FontCache, children: &[Node], _font_size: u16) -> i32 {
    let mut total = 0i32;
    for child in children {
        match child {
            Node::Text(t) => {
                let text = if t.style.white_space_pre {
                    t.text.as_str()
                } else {
                    t.text.trim()
                };
                let (w, _) = measure_text(fonts, text, &t.style);
                total += w;
            }
            Node::Element(el) => {
                if el.style.display == Display::Hidden { continue; }
                total += measure_inline_block_children(fonts, &el.children, el.style.font_size);
            }
        }
    }
    total.max(0)
}

/// Measure the shrink-to-fit width of block children (e.g. `li` items inside
/// an `inline-block` `ul`/`ol`).  Returns the widest line across all children.
pub fn measure_block_content_width(fonts: &mut FontCache, children: &[Node], font_size: u16) -> i32 {
    let mut max_w = 0i32;
    for child in children {
        match child {
            Node::Text(t) => {
                let text = t.text.trim();
                if text.is_empty() { continue; }
                let (w, _) = measure_text(fonts, text, &t.style);
                if w > max_w { max_w = w; }
            }
            Node::Element(el) => {
                if el.style.display == Display::Hidden { continue; }
                let child_fs = el.style.font_size;
                let pad_h = el.style.padding.left + el.style.padding.right;
                let child_w = measure_block_content_width(fonts, &el.children, child_fs) + pad_h;
                if child_w > max_w { max_w = child_w; }
            }
        }
    }
    max_w
}

pub fn measure_block_children(
    ls:    &LayoutState,
    fonts: &mut FontCache,
    el:    &Element,
    max_w: i32,
    s:     &crate::dom::node::Style,
) -> i32 {
    let mut cy  = ls.cursor_y;
    let mut cx  = ls.cursor_x;
    let mut lh  = s.font_size as i32;
    let start_y = cy - s.padding.top;
    measure_children_recursive(&el.children, fonts, max_w, &mut cx, &mut cy, &mut lh, ls.indent, ls.margin_left, ls.ctx.viewport_width, ls.ctx.viewport_height);
    cy += s.padding.bottom;
    let pending_lh = if cx > ls.margin_left + ls.indent { lh } else { 0 };
    let end_y = cy + pending_lh;
    (end_y - start_y).max(0)
}

pub fn measure_children_recursive(
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
                let text = &t.text;
                let normalized_ws: String;
                let text = if !t.style.white_space_pre {
                    let mut n = String::with_capacity(text.len());
                    let mut last_was_ws = false;
                    for c in text.chars() {
                        if c.is_whitespace() {
                            if !last_was_ws {
                                n.push(' ');
                                last_was_ws = true;
                            }
                        } else {
                            n.push(c);
                            last_was_ws = false;
                        }
                    }
                    normalized_ws = n;
                    &normalized_ws as &str
                } else {
                    text
                };

                if !t.style.white_space_pre && text.chars().all(|c| c.is_whitespace()) && !text.is_empty() {
                    let (sw, _) = fonts.get(font_size, t.style.bold, t.style.italic)
                        .and_then(|f| f.size_of(" ").ok())
                        .map(|(w, h)| (w as i32, h as i32))
                        .unwrap_or((8, font_size as i32));
                    if *cx + sw > max_w && *cx > margin_left + indent {
                        let line_h = (font_size as f32 * t.style.line_height_mul) as i32;
                        *cy += (*lh).max(line_h) + LINE_SPACING;
                        *cx = margin_left + indent;
                    }
                    *cx += sw;
                    continue;
                }

                let mut line = String::new();
                let mut started = false;
                for word in text.split(' ') {
                    let test = if !started {
                        started = true;
                        word.to_string()
                    } else {
                        format!("{} {}", line, word)
                    };
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

                    if child_tag == "details" {
                        let is_open = crate::dom::parser::get_attr(&child_el.attrs_raw, "open").is_some();
                        *cy += child_el.style.padding.top;
                        *cx += child_el.style.padding.left;
                        let saved = indent;
                        let new_indent = *cx - margin_left;
                        
                        let mut summary_found = false;
                        for child in &child_el.children {
                            if let Node::Element(cel) = child {
                                if cel.tag == "summary" {
                                    summary_found = true;
                                    let inner_indent = indent + child_font_size as i32;
                                    measure_children_recursive(std::slice::from_ref(child), fonts, max_w, cx, cy, lh, inner_indent, margin_left, viewport_w, viewport_h);
                                    continue;
                                }
                            }
                            if is_open {
                                measure_children_recursive(std::slice::from_ref(child), fonts, max_w, cx, cy, lh, new_indent, margin_left, viewport_w, viewport_h);
                            }
                        }
                        if !summary_found {
                             *cx += child_font_size as i32;
                             *lh = child_font_size as i32;
                        }

                        *cy += child_el.style.padding.bottom;
                        if *cx > margin_left { *cy += *lh + LINE_SPACING; }
                        *cy += BLOCK_MARGIN + child_el.style.margin.bottom;
                        *cx  = margin_left + saved;
                        *lh  = 16;
                        continue;
                    }

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
