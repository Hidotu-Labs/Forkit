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
    // ---- white-space: pre — line-by-line, no wrapping ----
    if style.white_space_pre {
        for line in text.lines() {
            let (sw, sh) = paint_text(
                canvas, tc, fonts, line, style,
                ls.cursor_x, ls.cursor_y,
                ls.ctx.scroll_y, ls.ctx.viewport_height,
            );
            if sh > ls.line_height { ls.line_height = sh; }
            ls.cursor_x += sw;
            ls.newline(style.font_size, style.line_height_mul);
        }
        return;
    }

    // ---- normal word-wrap ----
    if max_w - ls.cursor_x < 40 {
        ls.cursor_y   += ls.line_height + LINE_SPACING;
        ls.cursor_x    = ls.margin_left + ls.indent;
        ls.line_height = style.font_size as i32;
    }

    // Apply text-transform before breaking into words
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

    for word in text.split_whitespace() {
        let test = if line.is_empty() { word.to_string() }
                   else               { format!("{} {}", line, word) };
        let (tw, _) = measure_text(fonts, &test, style);

        if tw > max_w - ls.cursor_x && !line.is_empty() {
            flush_line(ls, canvas, tc, fonts, &line, style, max_w);
            ls.newline(style.font_size, style.line_height_mul);
            line = word.to_string();
        } else {
            line = test;
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
            let avail = max_w - ls.margin_left - MARGIN_RIGHT;
            ls.margin_left + ((avail - tw) / 2).max(0)
        }
        TextAlign::Right => (max_w - MARGIN_RIGHT - tw).max(ls.margin_left),
    };

    // Background (mark, code, etc.) — use blend mode directly, no pre-compositing
    if let Some(bg) = style.bg_color {
        let alpha = style.bg_alpha;
        fill_rect_alpha(
            canvas, Color::RGBA(bg[0], bg[1], bg[2], alpha),
            alpha,
            x, ls.cursor_y, tw, th,
            ls.ctx.scroll_y, ls.ctx.viewport_height,
        );
    }

    let (sw, sh) = paint_text(
        canvas, tc, fonts, text, style, x, ls.cursor_y,
        ls.ctx.scroll_y, ls.ctx.viewport_height,
    );
    if sh > ls.line_height { ls.line_height = sh; }

    let c = Color::RGB(style.color[0], style.color[1], style.color[2]);

    // Underline
    if style.underline {
        fill_rect(canvas, c, x, ls.cursor_y + sh - 2, sw, 1,
                  ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    // Strikethrough
    if style.strikethrough {
        fill_rect(canvas, c, x, ls.cursor_y + sh / 2, sw, 1,
                  ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    ls.cursor_x = x + sw + 4;
}
