use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use crate::dom::node::Element;
use crate::render::font::FontCache;
use crate::render::image::ImageCache;
use super::LayoutState;

pub fn layout_element(
    state:    &mut LayoutState,
    canvas:   &mut Canvas<Window>,
    tc:       &TextureCreator<WindowContext>,
    fonts:    &mut FontCache,
    images:   &mut ImageCache,
    base_url: &str,
    el:       &Element,
    max_w:    i32,
) {
    let tag = el.tag.to_lowercase();
    let is_block = matches!(tag.as_str(), "div" | "p" | "h1" | "h2" | "h3" | "ul" | "li" | "body" | "html" | "header" | "footer" | "section");

    let old_link = state.active_link.clone();
    if tag == "a" && !el.href.is_empty() {
        state.active_link = Some(el.href.clone());
    }

    if is_block && state.cursor_x > 8 {
        state.cursor_y += state.line_height + 4;
        state.cursor_x = 8;
    }

    for child in &el.children {
        state.layout_node(canvas, tc, fonts, images, base_url, child, max_w);
    }

    if is_block {
        state.cursor_y += state.line_height + 4;
        state.cursor_x = 8;
    }
    
    state.active_link = old_link;
}
