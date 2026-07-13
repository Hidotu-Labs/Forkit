use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::html5::node::Node;
use crate::render::font::FontCache;
use crate::render::image::ImageCache;
use crate::render::renderer::RenderCtx;

use super::block;
use super::inline;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    Text, Password, TextArea, Checkbox, Radio, Range, Color, Number, Other,
}

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub x: i32, pub y: i32, pub w: i32, pub h: i32,
}

#[derive(Debug, Clone)]
pub struct LinkArea {
    pub x: i32, pub y: i32, pub w: i32, pub h: i32, pub href: String,
}
impl LinkArea {
    pub fn contains(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let ay = self.y - scroll_y;
        px >= self.x && px < self.x + self.w && py >= ay && py < ay + self.h
    }
}

#[derive(Debug, Clone)]
pub struct InputArea {
    pub x: i32, pub y: i32, pub w: i32, pub h: i32,
    pub index: usize, pub kind: InputKind, pub name: String,
    pub default_value: String, pub disabled: bool, pub readonly: bool,
}
impl InputArea {
    pub fn contains(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let ay = self.y - scroll_y;
        px >= self.x && px < self.x + self.w && py >= ay && py < ay + self.h
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ButtonAction {
    Navigate(String), Submit(String), Reset, StepUp(usize), StepDown(usize), None,
}

#[derive(Debug, Clone)]
pub struct ButtonArea {
    pub x: i32, pub y: i32, pub w: i32, pub h: i32, pub action: ButtonAction,
}
impl ButtonArea {
    pub fn contains(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let ay = self.y - scroll_y;
        px >= self.x && px < self.x + self.w && py >= ay && py < ay + self.h
    }
}

#[derive(Debug, Clone)]
pub struct DetailsArea {
    pub x: i32, pub y: i32, pub w: i32, pub h: i32, pub element_ptr: usize,
}
impl DetailsArea {
    pub fn contains(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let ay = self.y - scroll_y;
        px >= self.x && px < self.x + self.w && py >= ay && py < ay + self.h
    }
}

#[derive(Debug, Clone)]
pub struct EventArea {
    pub x: i32, pub y: i32, pub w: i32, pub h: i32, pub element_ptr: usize, pub event_type: String,
}
impl EventArea {
    pub fn contains(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let ay = self.y - scroll_y;
        px >= self.x && px < self.x + self.w && py >= ay && py < ay + self.h
    }
}

#[derive(Debug, Clone)]
pub struct AudioArea {
    pub x: i32, pub y: i32, pub w: i32, pub h: i32, pub index: usize, pub src: String,
    pub play_btn: (i32, i32, i32, i32), pub scrubber: (i32, i32, i32, i32),
}
impl AudioArea {
    pub fn contains(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let ay = self.y - scroll_y;
        px >= self.x && px < self.x + self.w && py >= ay && py < ay + self.h
    }
    pub fn play_btn_hit(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let (bx, by, bw, bh) = self.play_btn;
        let ay = by - scroll_y;
        px >= bx && px < bx + bw && py >= ay && py < ay + bh
    }
    pub fn scrubber_hit(&self, px: i32, py: i32, scroll_y: i32) -> bool {
        let (sx, sy, sw, sh) = self.scrubber;
        let ay = sy - scroll_y;
        px >= sx && px < sx + sw && py >= ay && py < ay + sh
    }
    pub fn scrubber_ratio(&self, px: i32) -> f64 {
        let (sx, _, sw, _) = self.scrubber;
        ((px - sx) as f64 / sw as f64).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AudioPlayback {
    pub playing: bool, pub progress: f64, pub position_secs: f64, pub duration_secs: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    InlineBlock,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Margin {
    Px(i32),
    Auto,
}

impl Margin {
    pub fn get_px(&self) -> i32 {
        match self {
            Margin::Px(v) => *v,
            Margin::Auto => 0,
        }
    }
}

pub struct LayoutState<'ctx> {
    pub ctx:         &'ctx RenderCtx,
    pub cursor_x:    i32,
    pub cursor_y:    i32,
    pub line_start_x: i32,
    pub line_height: i32,
    pub boxes:       Vec<LayoutBox>,
    pub link_areas:  Vec<LinkArea>,
    pub input_areas: Vec<InputArea>,
    pub button_areas: Vec<ButtonArea>,
    pub details_areas: Vec<DetailsArea>,
    pub audio_areas: Vec<AudioArea>,
    pub event_areas: Vec<EventArea>,
    pub audio_playback: std::collections::HashMap<usize, AudioPlayback>,
    pub input_values: Vec<String>,
    pub focused_input: Option<usize>,
    pub active_link: Option<String>,
    pub content_height: i32,
    pub current_color: [u8; 4],
    pub current_bg_color: Option<[u8; 4]>,
    pub current_font_size: u16,
    pub current_bold: bool,
    pub current_italic: bool,
    pub current_text_transform: TextTransform,
    pub current_opacity: f32,
    pub current_border_radius: i32,
    pub fixed_width: Option<i32>,
    pub padding_top: i32,
    pub padding_bottom: i32,
    pub padding_left: i32,
    pub padding_right: i32,
    pub margin_top: i32,
    pub margin_bottom: i32,
    pub margin_left: Margin,
    pub margin_right: Margin,
    pub last_margin_bottom: i32,
    pub current_display: Display,
    pub root_font_size: f32,
    pub stylesheets: Vec<crate::css::Stylesheet>,
    pub paint: bool,
}

impl<'ctx> LayoutState<'ctx> {
    pub fn new(ctx: &'ctx RenderCtx) -> Self {
        LayoutState {
            ctx,
            cursor_x:      8,
            cursor_y:      8,
            line_start_x:  8,
            line_height:   16,
            boxes:         Vec::new(),
            link_areas:    Vec::new(),
            input_areas:   Vec::new(),
            button_areas:  Vec::new(),
            details_areas: Vec::new(),
            audio_areas:   Vec::new(),
            event_areas:   Vec::new(),
            audio_playback: std::collections::HashMap::new(),
            input_values:  Vec::new(),
            focused_input: None,
            active_link:   None,
            content_height: 0,
            current_color: [0, 0, 0, 255],
            current_bg_color: None,
            current_font_size: 16,
            current_bold: false,
            current_italic: false,
            current_text_transform: TextTransform::None,
            current_opacity: 1.0,
            current_border_radius: 0,
            fixed_width: None,
            padding_top: 0,
            padding_bottom: 0,
            padding_left: 0,
            padding_right: 0,
            margin_top: 0,
            margin_bottom: 0,
            margin_left: Margin::Px(0),
            margin_right: Margin::Px(0),
            last_margin_bottom: 0,
            current_display: Display::Inline,
            root_font_size: 16.0,
            stylesheets: Vec::new(),
            paint: true,
        }
    }

    pub fn collect_styles(&mut self, node: &crate::html5::node::Node) {
        match node {
            crate::html5::node::Node::Element(el) => {
                if el.tag == "style" {
                    let mut css_text = String::new();
                    for child in &el.children {
                        if let crate::html5::node::Node::Text(txt) = child {
                            css_text.push_str(&txt.text);
                        }
                    }
                    if !css_text.is_empty() {
                        self.stylesheets.push(crate::css::parse_stylesheet(&css_text));
                    }
                }
                for child in &el.children {
                    self.collect_styles(child);
                }
            }
            crate::html5::node::Node::Text(_) => {}
        }
    }

    pub fn set_input_state(&mut self, values: Vec<String>, focused: Option<usize>) {
        self.input_values = values;
        self.focused_input = focused;
    }

    pub fn set_audio_state(&mut self, playback: std::collections::HashMap<usize, AudioPlayback>) {
        self.audio_playback = playback;
    }

    pub fn into_boxes(self) -> Vec<LayoutBox> { self.boxes }

    pub fn layout_node(
        &mut self,
        canvas:   &mut Canvas<Window>,
        tc:       &TextureCreator<WindowContext>,
        fonts:    &mut FontCache,
        images:   &mut ImageCache,
        base_url: &str,
        node:     &Node,
        max_w:    i32,
        ancestors: &[&crate::html5::node::Element],
    ) {
        match node {
            Node::Text(t)    => {
                inline::paint_text(self, canvas, tc, fonts, &t.text, max_w);
            }
            Node::Element(e) => block::layout_element(self, canvas, tc, fonts, images, base_url, e, max_w, ancestors),
        }
        let bottom = self.cursor_y + self.line_height;
        if bottom > self.content_height {
            self.content_height = bottom;
        }
    }
}
