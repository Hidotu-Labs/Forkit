#![allow(dead_code)]

use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::dom::node::{Node, Style};
use super::font::FontCache;
use super::renderer::RenderCtx;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MARGIN_LEFT:  i32 = 16;
const MARGIN_RIGHT: i32 = 16;
const MARGIN_TOP:   i32 = 8;
const LINE_SPACING: i32 = 4;
const BLOCK_MARGIN: i32 = 6;
const LIST_INDENT:  i32 = 20;

// ---------------------------------------------------------------------------
// LayoutBox
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

// ---------------------------------------------------------------------------
// Free helpers  (take tc explicitly so no borrow aliasing)
// ---------------------------------------------------------------------------

/// Paint `text` at canvas position `(x, y - scroll_y)`.
/// Returns `(rendered_width, rendered_height)`.
fn paint_text(
    canvas:          &mut Canvas<Window>,
    tc:              &TextureCreator<WindowContext>,
    fonts:           &mut FontCache,
    text:            &str,
    style:           &Style,
    x:               i32,
    y:               i32,
    scroll_y:        i32,
    viewport_height: i32,
) -> (i32, i32) {
    if text.is_empty() { return (0, 0); }
    let font = match fonts.get(style.font_size, style.bold, style.italic) {
        Some(f) => f,
        None    => return (0, 0),
    };
    let color   = Color::RGB(style.color[0], style.color[1], style.color[2]);
    let surface = match font.render(text).blended(color) {
        Ok(s)  => s,
        Err(_) => return (0, 0),
    };
    let (sw, sh) = (surface.width() as i32, surface.height() as i32);
    let ry = y - scroll_y;
    if ry + sh > 0 && ry < viewport_height {
        if let Ok(tex) = tc.create_texture_from_surface(&surface) {
            let _ = canvas.copy(&tex, None, Rect::new(x, ry, sw as u32, sh as u32));
        }
    }
    (sw, sh)
}

/// Measure `text` without painting. Returns `(width, height)`.
fn measure_text(fonts: &mut FontCache, text: &str, style: &Style) -> (i32, i32) {
    let font = match fonts.get(style.font_size, style.bold, style.italic) {
        Some(f) => f,
        None    => return (0, 0),
    };
    font.size_of(text).map(|(w, h)| (w as i32, h as i32)).unwrap_or((0, 0))
}

// ---------------------------------------------------------------------------
// LayoutState
// ---------------------------------------------------------------------------

pub struct LayoutState<'ctx> {
    ctx:         &'ctx RenderCtx,
    cursor_x:    i32,
    cursor_y:    i32,
    line_height: i32,
    indent:      i32,
    boxes:       Vec<LayoutBox>,
}

impl<'ctx> LayoutState<'ctx> {
    pub fn new(ctx: &'ctx RenderCtx) -> Self {
        LayoutState {
            ctx,
            cursor_x:    MARGIN_LEFT,
            cursor_y:    MARGIN_TOP,
            line_height: 16,
            indent:      0,
            boxes:       Vec::new(),
        }
    }

    pub fn into_boxes(self) -> Vec<LayoutBox> { self.boxes }

    // -----------------------------------------------------------------------
    // Word-wrapped text rendering
    // -----------------------------------------------------------------------

    fn paint_wrapped(
        &mut self,
        canvas:    &mut Canvas<Window>,
        tc:        &TextureCreator<WindowContext>,
        fonts:     &mut FontCache,
        text:      &str,
        style:     &Style,
        max_width: i32,
    ) {
        if max_width - self.cursor_x < 40 {
            self.cursor_y   += self.line_height + LINE_SPACING;
            self.cursor_x    = MARGIN_LEFT + self.indent;
            self.line_height = style.font_size as i32;
        }

        let mut line = String::new();

        for word in text.split_whitespace() {
            let test = if line.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", line, word)
            };

            let (tw, _) = measure_text(fonts, &test, style);

            if tw > max_width - self.cursor_x && !line.is_empty() {
                // flush current line
                let (_, sh) = paint_text(
                    canvas, tc, fonts, &line, style,
                    self.cursor_x, self.cursor_y,
                    self.ctx.scroll_y, self.ctx.viewport_height,
                );
                if sh > self.line_height { self.line_height = sh; }

                self.cursor_y   += self.line_height + LINE_SPACING;
                self.cursor_x    = MARGIN_LEFT + self.indent;
                self.line_height = style.font_size as i32;
                line             = word.to_string();
            } else {
                line = test;
            }
        }

        // flush remainder
        if !line.is_empty() {
            let (sw, sh) = paint_text(
                canvas, tc, fonts, &line, style,
                self.cursor_x, self.cursor_y,
                self.ctx.scroll_y, self.ctx.viewport_height,
            );
            if sh > self.line_height { self.line_height = sh; }
            self.cursor_x += sw + 4;
        }
    }

    // -----------------------------------------------------------------------
    // Recursive layout pass
    // -----------------------------------------------------------------------

    pub fn layout_node(
        &mut self,
        canvas:    &mut Canvas<Window>,
        tc:        &TextureCreator<WindowContext>,
        fonts:     &mut FontCache,
        node:      &Node,
        max_width: i32,
    ) {
        match node {
            Node::Text(t) =>
                self.paint_wrapped(canvas, tc, fonts, &t.text, &t.style, max_width),
            Node::Element(el) =>
                self.layout_element(canvas, tc, fonts, el, max_width),
        }
    }

    fn layout_element(
        &mut self,
        canvas:    &mut Canvas<Window>,
        tc:        &TextureCreator<WindowContext>,
        fonts:     &mut FontCache,
        el:        &crate::dom::node::Element,
        max_width: i32,
    ) {
        let tag = el.tag.as_str();
        let s   = &el.style;

        // structural pass-throughs
        if matches!(tag, "#document" | "html" | "body") {
            for child in &el.children {
                self.layout_node(canvas, tc, fonts, child, max_width);
            }
            return;
        }

        // <br>
        if tag == "br" {
            let lh = self.line_height.max(s.font_size as i32);
            self.cursor_y   += lh + LINE_SPACING;
            self.cursor_x    = MARGIN_LEFT + self.indent;
            self.line_height = s.font_size as i32;
            return;
        }

        // <hr>
        if tag == "hr" {
            self.cursor_y += self.line_height + LINE_SPACING + BLOCK_MARGIN;
            self.cursor_x  = MARGIN_LEFT;
            let ry = self.cursor_y - self.ctx.scroll_y;
            if ry >= 0 && ry < self.ctx.viewport_height {
                canvas.set_draw_color(Color::RGB(180, 180, 180));
                let _ = canvas.fill_rect(Rect::new(
                    MARGIN_LEFT, ry,
                    (max_width - MARGIN_RIGHT * 2).max(0) as u32,
                    2,
                ));
            }
            self.cursor_y   += 2 + BLOCK_MARGIN;
            self.line_height = s.font_size as i32;
            return;
        }

        let is_block = s.display_block;

        // block open
        if is_block {
            if self.cursor_x > MARGIN_LEFT + self.indent {
                self.cursor_y += self.line_height + LINE_SPACING;
            }
            self.cursor_y   += BLOCK_MARGIN;
            self.cursor_x    = MARGIN_LEFT + self.indent;
            self.line_height = s.font_size as i32;

            // extra top-margin for headings h1–h6
            if tag.len() == 2
                && tag.starts_with('h')
                && tag.as_bytes()[1].is_ascii_digit()
            {
                self.cursor_y += s.font_size as i32 / 2;
            }
        }

        let start_y = self.cursor_y;

        // list-item bullet
        if tag == "li" {
            self.indent  += LIST_INDENT;
            self.cursor_x = MARGIN_LEFT + self.indent;
            let bstyle = Style { font_size: s.font_size, color: s.color, ..Default::default() };
            let bx     = MARGIN_LEFT + self.indent - LIST_INDENT;
            paint_text(
                canvas, tc, fonts, "\u{2022} ", &bstyle,
                bx, self.cursor_y,
                self.ctx.scroll_y, self.ctx.viewport_height,
            );
        }

        // recurse into children
        let saved_indent = self.indent;
        for child in &el.children {
            self.layout_node(canvas, tc, fonts, child, max_width);
        }
        self.indent = saved_indent;

        // block close
        if is_block {
            let end_y = self.cursor_y + self.line_height;
            self.boxes.push(LayoutBox {
                x: MARGIN_LEFT,
                y: start_y,
                w: (max_width - MARGIN_LEFT - MARGIN_RIGHT).max(0),
                h: (end_y - start_y).max(0),
            });

            if self.cursor_x > MARGIN_LEFT {
                self.cursor_y += self.line_height + LINE_SPACING;
            }
            self.cursor_y   += BLOCK_MARGIN;
            self.cursor_x    = MARGIN_LEFT;
            self.line_height = 16;
        }
    }
}
