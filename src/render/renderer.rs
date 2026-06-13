use sdl2::pixels::Color;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::dom::node::Node;
use super::font::FontCache;
use super::image::ImageCache;
use super::layout::{LayoutBox, LayoutState};

/// Everything the renderer needs to know about the viewport.
pub struct RenderCtx {
    pub viewport_width:  i32,
    pub viewport_height: i32,
    pub scroll_y:        i32,
    pub base_url:        String,
}

/// Clear the canvas and run the layout + paint pass.
/// Returns the list of computed layout boxes.
pub fn render(
    canvas:   &mut Canvas<Window>,
    fonts:    &mut FontCache,
    images:   &mut ImageCache,
    base_url: &str,
    ctx:      &RenderCtx,
    root:     &Node,
) -> Vec<LayoutBox> {
    canvas.set_draw_color(Color::WHITE);
    canvas.clear();

    // TextureCreator must outlive any Texture created from it.
    // We create it here so it covers the entire layout pass.
    let tc = canvas.texture_creator();

    let mut state = LayoutState::new(ctx);
    state.layout_node(canvas, &tc, fonts, images, base_url, root, ctx.viewport_width - 16);
    state.into_boxes()
}
