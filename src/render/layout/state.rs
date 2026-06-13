use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::dom::node::{Node, Visibility};
use crate::render::font::FontCache;
use crate::render::image::ImageCache;
use crate::render::renderer::RenderCtx;

use super::block;
use super::inline;

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub x: i32, pub y: i32, pub w: i32, pub h: i32,
}

// ---------------------------------------------------------------------------
// LinkArea — clickable region bound to an href
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LinkArea {
    pub x:    i32,
    pub y:    i32,
    pub w:    i32,
    pub h:    i32,
    pub href: String,
}

impl LinkArea {
    /// Returns true if `(px, py)` is inside this area (scroll-adjusted).
    pub fn contains(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let ay = self.y - scroll_y;
        px >= self.x && px < self.x + self.w
            && py >= ay && py < ay + self.h
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    Text,
    Password,
    TextArea,
    Checkbox,
    Radio,
    Range,
    Color,
    Other,
}

#[derive(Debug, Clone)]
pub struct InputArea {
    pub x:    i32,
    pub y:    i32,
    pub w:    i32,
    pub h:    i32,
    /// Unique index among all inputs on the page (assigned at layout time).
    pub index: usize,
    pub kind:  InputKind,
    /// Value of the HTML `name` attribute (used for radio button grouping).
    pub name:  String,
}


impl InputArea {
    /// Returns true if `(px, py)` is inside this area (scroll-adjusted).
    pub fn contains(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let ay = self.y - scroll_y;
        px >= self.x && px < self.x + self.w
            && py >= ay && py < ay + self.h
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ButtonAction {
    /// Navigate to this URL (href from <a> styled as button, or form action).
    Navigate(String),
    /// Submit the form: navigate to `action` URL (empty string = current page).
    Submit(String),
    /// Reset all form inputs on the page.
    Reset,
    /// Generic button with no built-in action.
    None,
}

#[derive(Debug, Clone)]
pub struct ButtonArea {
    pub x:      i32,
    pub y:      i32,
    pub w:      i32,
    pub h:      i32,
    pub action: ButtonAction,
}

impl ButtonArea {
    pub fn contains(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let ay = self.y - scroll_y;
        px >= self.x && px < self.x + self.w
            && py >= ay && py < ay + self.h
    }
}

#[derive(Debug, Clone)]
pub struct DetailsArea {
    pub x:           i32,
    pub y:           i32,
    pub w:           i32,
    pub h:           i32,
    pub element_ptr: usize,
}

impl DetailsArea {
    pub fn contains(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let ay = self.y - scroll_y;
        px >= self.x && px < self.x + self.w
            && py >= ay && py < ay + self.h
    }
}

pub const MARGIN_LEFT:  i32 = 0;
pub const MARGIN_RIGHT: i32 = 0;
pub const MARGIN_TOP:   i32 = 0;
pub const LINE_SPACING: i32 = 1;
pub const BLOCK_MARGIN: i32 = 0;

/// Fallback page margin used when body has no margin/padding CSS set.
const DEFAULT_PAGE_MARGIN: i32 = 8;

#[derive(Debug, Clone)]
pub struct RoundedClip {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub radii: [u16; 4],
}

pub struct LayoutState<'ctx> {
    pub ctx:         &'ctx RenderCtx,
    pub cursor_x:    i32,
    pub cursor_y:    i32,
    pub line_height: i32,
    pub indent:      i32,
    /// Left edge offset set by body/html margin+padding (replaces MARGIN_LEFT for content).
    pub margin_left: i32,
    pub boxes:       Vec<LayoutBox>,
    pub link_areas:  Vec<LinkArea>,
    pub input_areas: Vec<InputArea>,
    pub button_areas: Vec<ButtonArea>,
    pub details_areas: Vec<DetailsArea>,
    /// Counter for assigning unique indices to input areas.
    pub input_count: usize,
    /// Live values for each focusable input (indexed by input order on the page).
    pub input_values: Vec<String>,
    /// Which input index is currently focused (if any).
    pub focused_input: Option<usize>,
    /// The action URL of the nearest enclosing <form> (passed down during layout).
    pub form_action: String,
    /// Counter stack for ordered lists.
    pub ol_stack:    Vec<u32>,
    /// Total document height in pixels (set after layout completes).
    pub content_height: i32,
    /// Optional rounded clip to apply to child elements (experimental).
    pub rounding_clip: Option<RoundedClip>,
}

impl<'ctx> LayoutState<'ctx> {
    pub fn new(ctx: &'ctx RenderCtx) -> Self {
        LayoutState {
            ctx,
            cursor_x:      0,
            cursor_y:      0,
            line_height:   16,
            indent:        0,
            margin_left:   0,
            boxes:         Vec::new(),
            link_areas:    Vec::new(),
            input_areas:   Vec::new(),
            button_areas:  Vec::new(),
            details_areas: Vec::new(),
            input_count:   0,
            input_values:  Vec::new(),
            focused_input: None,
            form_action:   String::new(),
            ol_stack:      Vec::new(),
            content_height: 0,
            rounding_clip:  None,
        }
    }

    /// Seed the layout state with live input data before rendering.
    pub fn set_input_state(&mut self, values: Vec<String>, focused: Option<usize>) {
        self.input_values  = values;
        self.focused_input = focused;
    }

    pub fn into_boxes(self) -> Vec<LayoutBox> { self.boxes }

    /// Advance to the next line using the current style's line-height multiplier.
    pub fn newline(&mut self, font_size: u16, line_height_mul: f32) {
        let lh = (font_size as f32 * line_height_mul) as i32;
        self.cursor_y   += self.line_height.max(lh) + LINE_SPACING;
        self.cursor_x    = self.margin_left + self.indent;
        self.line_height = font_size as i32;
    }

    /// Hit-test a click at page coordinates `(px, py)` (scroll-adjusted).
    /// Returns the href of the first matching link area, if any.
    pub fn link_at(&self, px: i32, py: i32, scroll_y: i32) -> Option<&str> {
        self.link_areas.iter()
            .rev() // last-painted (topmost) wins
            .find(|a| a.contains(px, py, scroll_y))
            .map(|a| a.href.as_str())
    }

    // -----------------------------------------------------------------------
    // Public entry — dispatch to inline or block renderer
    // -----------------------------------------------------------------------

    pub fn layout_node(
        &mut self,
        canvas:   &mut Canvas<Window>,
        tc:       &TextureCreator<WindowContext>,
        fonts:    &mut FontCache,
        images:   &mut ImageCache,
        base_url: &str,
        node:     &Node,
        max_w:    i32,
    ) {
        match node {
            Node::Text(t)    => {
                if t.style.visibility == Visibility::Hidden {
                    block::advance_text_invisible(self, fonts, &t.text, &t.style);
                } else {
                    // Re-resolve viewport-relative font-size (vw/vh/calc) now
                    // that we have the real viewport dimensions.
                    if let Some(raw) = &t.style.font_size_raw {
                        let ctx = crate::dom::css::LengthContext {
                            base_font_size:  t.style.font_size,
                            percent_base:    16,
                            viewport_width:  self.ctx.viewport_width,
                            viewport_height: self.ctx.viewport_height,
                        };
                        if let Some(n) = crate::dom::css::parse_length_ctx(raw, &ctx) {
                            let resolved = n.clamp(8, 96) as u16;
                            // Clone and patch so we don't mutate the stored DOM
                            let mut patched = t.style.clone();
                            patched.font_size = resolved;
                            return inline::paint_wrapped(self, canvas, tc, fonts, &t.text, &patched, max_w);
                        }
                    }
                    inline::paint_wrapped(self, canvas, tc, fonts, &t.text, &t.style, max_w);
                }
            }
            Node::Element(e) => block::layout_element(self, canvas, tc, fonts, images, base_url, e, max_w),
        }
        // Update total document height after each top-level node
        let bottom = self.cursor_y + self.line_height;
        if bottom > self.content_height {
            self.content_height = bottom;
        }
    }
}
