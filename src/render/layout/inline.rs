use sdl2::pixels::Color;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::dom::node::{Style, TextAlign, TextTransform};
use crate::render::font::FontCache;

use super::paint::{paint_text, measure_text, fill_rect, fill_rect_alpha};
use super::state::{LayoutState, MARGIN_LEFT, MARGIN_RIGHT, LINE_SPACING};

/// Render `text` with word-wrapping (or verbatim for `white-space: pre`).
pub fn paint_wrapped(
    ls:     &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc:     &TextureCreator<WindowContext>,
    fonts:  &mut FontCache,
    text:   &str,
    style:  &Style,
    max_w:  i32,
) {
    if style.white_space_pre {
        for line in text.lines() {
            flush_line(ls, canvas, tc, fonts, line, style, max_w);
            ls.newline(style.font_size, style.line_height_mul);
        }
        return;
    }



    // 1. Whitespace normalization (if not pre)
    let normalized_ws: String;
    let text = if !style.white_space_pre {
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

    // 2. Apply text-transform before breaking into words
    let transformed: String;
    let text = match style.text_transform {
        TextTransform::Uppercase  => { transformed = text.to_uppercase();  &transformed as &str }
        TextTransform::Lowercase  => { transformed = text.to_lowercase();  &transformed as &str }
        TextTransform::Capitalize => {
            transformed = text.split_whitespace()
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        None    => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            &transformed as &str
        }
        TextTransform::None => text,
    };

    let mut line = String::new();
    let has_content = text.chars().any(|c| !c.is_whitespace());

    // If the node is just whitespace, ensure it advances the cursor correctly (collapsed to a single space).
    if !has_content && !text.is_empty() {
        // Skip leading whitespace at the start of a line — the space would only
        // add an unwanted indent before the first visible content.
        if ls.cursor_x <= ls.margin_left + ls.indent {
            return;
        }
        let (sw, _) = measure_text(fonts, " ", style);
        if ls.cursor_x + sw > max_w && ls.cursor_x > ls.margin_left + ls.indent {
            ls.newline(style.font_size, style.line_height_mul);
        }
        ls.cursor_x += sw;
        return;
    }

    // Attempt to render the entire node as a single chunk if it fits.
    let (tw, _) = measure_text(fonts, text, style);
    if ls.cursor_x + tw <= max_w {
        flush_line(ls, canvas, tc, fonts, text, style, max_w);
        return;
    } else if ls.cursor_x > ls.margin_left + ls.indent {
        ls.newline(style.font_size, style.line_height_mul);
        if tw <= max_w - ls.cursor_x {
            flush_line(ls, canvas, tc, fonts, text, style, max_w);
            return;
        }
    }

    // Fallback: Split by a single space to preserve word-level metrics.
    use crate::dom::node::WordBreak;

    let mut started = false;
    for word in text.split(' ') {
        let mut cur_word = word;

        while !cur_word.is_empty() {
            let test = if !started || line.is_empty() {
                cur_word.to_string()
            } else {
                format!("{} {}", line, cur_word)
            };
            let (tw, _) = measure_text(fonts, &test, style);
            let available_w = max_w - ls.cursor_x;

            if tw > available_w {
                // If word-break: break-all is set, we try to fill the current line by breaking the word.
                if style.word_break == WordBreak::BreakAll {
                    let prefix = if line.is_empty() { String::new() } else { format!("{} ", line) };
                    let mut split_idx = 0;
                    for (idx, _) in cur_word.char_indices() {
                        let (sw, _) = measure_text(fonts, &format!("{}{}", prefix, &cur_word[..idx]), style);
                        if sw > available_w { break; }
                        split_idx = idx;
                    }

                    if split_idx > 0 {
                        flush_line(ls, canvas, tc, fonts, &format!("{}{}", prefix, &cur_word[..split_idx]), style, max_w);
                        ls.newline(style.font_size, style.line_height_mul);
                        cur_word = &cur_word[split_idx..];
                        line = String::new();
                        started = true;
                        continue;
                    } else if !line.is_empty() {
                        // Could not fit even one char of the word on the same line (even with break-all),
                        // so flush the current line and try the word on the next line.
                        flush_line(ls, canvas, tc, fonts, &line, style, max_w);
                        ls.newline(style.font_size, style.line_height_mul);
                        line = String::new();
                        continue;
                    } else if ls.cursor_x > ls.margin_left + ls.indent {
                        // Line is empty (for this node), but we aren't at the start of a physical line.
                        ls.newline(style.font_size, style.line_height_mul);
                        continue;
                    }
                    // If we are here, line is empty, ls.cursor_x is at marginal/indent, and split_idx is 0.
                    // This means not even one character fits on a fresh line.
                    // Force one character anyway to avoid infinite loop.
                    split_idx = cur_word.chars().next().unwrap().len_utf8();
                    flush_line(ls, canvas, tc, fonts, &cur_word[..split_idx], style, max_w);
                    ls.newline(style.font_size, style.line_height_mul);
                    cur_word = &cur_word[split_idx..];
                    line = String::new();
                    started = true;
                    continue;
                }

                // Normal behavior or break-word
                if !line.is_empty() {
                    flush_line(ls, canvas, tc, fonts, &line, style, max_w);
                    ls.newline(style.font_size, style.line_height_mul);
                    line = String::new();
                    continue;
                } else if ls.cursor_x > ls.margin_left + ls.indent {
                    ls.newline(style.font_size, style.line_height_mul);
                    continue;
                } else {
                    // Line is empty and we are at the margin, but it still doesn't fit.
                    if style.word_break == WordBreak::BreakWord {
                        // For break-word, we break if it doesn't fit on a fresh line.
                        let mut split_idx = 0;
                        for (idx, _) in cur_word.char_indices() {
                            if idx == 0 { continue; }
                            let (sw, _) = measure_text(fonts, &cur_word[..idx], style);
                            if sw > available_w { break; }
                            split_idx = idx;
                        }
                        if split_idx == 0 { split_idx = cur_word.chars().next().unwrap().len_utf8(); }

                        flush_line(ls, canvas, tc, fonts, &cur_word[..split_idx], style, max_w);
                        ls.newline(style.font_size, style.line_height_mul);
                        cur_word = &cur_word[split_idx..];
                        line = String::new();
                        started = true;
                    } else {
                        // Normal: just let it overflow.
                        line = cur_word.to_string();
                        cur_word = "";
                        started = true;
                    }
                }
            } else {
                line = test;
                cur_word = "";
                started = true;
            }
        }
    }

    if !line.is_empty() {
        flush_line(ls, canvas, tc, fonts, &line, style, max_w);
    }
}

/// Paint one complete line, applying text-align, background highlight,
/// underline, and strikethrough.
pub fn flush_line(
    ls:     &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc:     &TextureCreator<WindowContext>,
    fonts:  &mut FontCache,
    text:   &str,
    style:  &Style,
    max_w:  i32,
) {
    if text.is_empty() { return; }

    let (tw, th) = measure_text(fonts, text, style);

    let x = match style.text_align {
        TextAlign::Left   => ls.cursor_x,
        TextAlign::Center => {
            let container_l = ls.margin_left + ls.indent;
            let container_w = (max_w - MARGIN_RIGHT - container_l).max(0);
            container_l + ((container_w - tw) / 2).max(0)
        }
        TextAlign::Right => (max_w - MARGIN_RIGHT - tw).max(ls.margin_left + ls.indent),
    };

    // For now, use top-alignment to prevent stairstepping between elements
    // of different sizes on the same line.
    let paint_y = ls.cursor_y;

    // paint_text now handles bg_color, underline, and strikethrough internally.
    let (sw, sh) = paint_text(
        canvas, tc, fonts, text, style, x, paint_y,
        ls.rounding_clip.as_ref(),
        ls.ctx.scroll_y, ls.ctx.viewport_height,
    );
    if sh > ls.line_height { ls.line_height = sh; }

    ls.cursor_x = x + sw;
}
