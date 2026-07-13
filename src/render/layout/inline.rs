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
    let font_size = state.current_font_size;
    let bold = state.current_bold;
    let italic = state.current_italic;
    let family = state.current_font_family.clone();
    
    let transformed_text = match state.current_text_transform {
        crate::render::layout::state::TextTransform::Uppercase => text.to_uppercase(),
        crate::render::layout::state::TextTransform::Lowercase => text.to_lowercase(),
        crate::render::layout::state::TextTransform::Capitalize => {
            text.split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        },
        crate::render::layout::state::TextTransform::None => text.to_string(),
    };

    let _words = transformed_text.split_whitespace();
    
    let is_link = state.active_link.is_some();
    let mut text_color = if is_link { [0, 60, 200, 255] } else { state.current_color };
    // Maintain better text legibility by not being as aggressive with foreground opacity
    let text_opacity = if state.current_opacity < 1.0 {
        (state.current_opacity * 1.5).clamp(0.0, 1.0)
    } else {
        1.0
    };
    text_color[3] = (text_color[3] as f32 * text_opacity) as u8;

    canvas.set_draw_color(Color::RGBA(text_color[0], text_color[1], text_color[2], text_color[3]));

    let (space_width, _) = fonts.measure_family(" ", font_size, bold, italic, family.clone());

    for word in transformed_text.split_whitespace() {
        let (w, h) = fonts.measure_family(word, font_size, bold, italic, family.clone());
        if state.cursor_x + w > max_w {
            state.cursor_y += state.line_height;
            state.cursor_x = state.line_start_x;
        }
        
        if state.paint {
            let is_link = state.active_link.is_some();
            let tex = fonts.get_text_texture_family(tc, word, font_size, text_color, bold, italic, is_link, family.clone());
            if let Some(t) = tex {
                let y_offset = (state.line_height - h) / 2;
                let target = sdl2::rect::Rect::new(state.cursor_x, state.cursor_y + y_offset - state.ctx.scroll_y, w as u32, h as u32);
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
        }
        
        state.cursor_x += w + space_width;
    }
}
