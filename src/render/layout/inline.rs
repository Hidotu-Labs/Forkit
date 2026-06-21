use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::pixels::Color;
use crate::render::font::FontCache;
use super::LayoutState;

pub fn paint_text(
    state:  &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc:     &TextureCreator<WindowContext>,
    fonts:  &mut FontCache,
    text:   &str,
    max_w:  i32,
) {
    let font_size = 16;
    let words = text.split_whitespace();
    
    let is_link = state.active_link.is_some();
    let text_color = if is_link { [0, 60, 200] } else { [0, 0, 0] };
    canvas.set_draw_color(Color::RGB(text_color[0], text_color[1], text_color[2]));

    for word in words {
        let (w, h) = fonts.measure_text(word, font_size, false, false);
        if state.cursor_x + w > max_w {
            state.cursor_y += state.line_height + 2;
            state.cursor_x = 8;
        }
        
        let is_link = state.active_link.is_some();
        let tex = fonts.get_text_texture(tc, word, font_size, text_color, false, false, is_link);
        if let Some(t) = tex {
            let target = sdl2::rect::Rect::new(state.cursor_x, state.cursor_y, w as u32, h as u32);
            let _ = canvas.copy(&t, None, Some(target));

            if let Some(href) = &state.active_link {
                state.link_areas.push(crate::render::layout::state::LinkArea {
                    x: state.cursor_x,
                    y: state.cursor_y,
                    w,
                    h,
                    href: href.clone(),
                });
            }
        }
        
        state.cursor_x += w + 4;
        state.line_height = state.line_height.max(h);
    }
}
