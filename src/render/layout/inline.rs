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
    
    let mut text_color = state.current_color;
    // Maintain better text legibility by not being as aggressive with foreground opacity
    let text_opacity = if state.current_opacity < 1.0 {
        (state.current_opacity * 1.5).clamp(0.0, 1.0)
    } else {
        1.0
    };
    text_color[3] = (text_color[3] as f32 * text_opacity) as u8;

    canvas.set_draw_color(Color::RGBA(text_color[0], text_color[1], text_color[2], text_color[3]));

    let (space_width, _) = fonts.measure_family(" ", font_size, bold, italic, family.clone());

    // Break text into word/character tokens respecting CJK scripts
    let mut words: Vec<String> = Vec::new();
    for token in transformed_text.split_whitespace() {
        let mut cur_chunk = String::new();
        for ch in token.chars() {
            let is_cjk = ('\u{4E00}'..='\u{9FFF}').contains(&ch) || ('\u{3400}'..='\u{4DBF}').contains(&ch) || ('\u{3000}'..='\u{303F}').contains(&ch) || ('\u{FF00}'..='\u{FFEF}').contains(&ch);
            if is_cjk {
                if !cur_chunk.is_empty() {
                    words.push(cur_chunk);
                    cur_chunk = String::new();
                }
                words.push(ch.to_string());
            } else {
                cur_chunk.push(ch);
            }
        }
        if !cur_chunk.is_empty() {
            words.push(cur_chunk);
        }
    }
    if words.is_empty() {
        return;
    }

    struct WordInfo {
        word: String,
        w: i32,
        h: i32,
    }

    let (first_w, _) = if let Some(first_word) = words.first() {
        fonts.measure_family(first_word, font_size, bold, italic, family.clone())
    } else {
        (0, 0)
    };
    if state.cursor_x > state.line_start_x && state.cursor_x + first_w > max_w {
        state.cursor_y += state.line_height;
        state.cursor_x = state.line_start_x;
    }

    let mut lines: Vec<Vec<WordInfo>> = Vec::new();
    let mut cur_line: Vec<WordInfo> = Vec::new();
    let mut cur_line_w = 0;

    for word in words {
        let (w, h) = fonts.measure_family(&word, font_size, bold, italic, family.clone());
        let needed = if cur_line.is_empty() { w } else { space_width + w };
        let line_start = if lines.is_empty() { state.cursor_x } else { state.line_start_x };
        if !cur_line.is_empty() && (line_start + cur_line_w + needed > max_w) {
            lines.push(cur_line);
            cur_line = Vec::new();
            cur_line_w = 0;
        }
        cur_line_w += if cur_line.is_empty() { w } else { space_width + w };
        cur_line.push(WordInfo { word, w, h });
    }
    if !cur_line.is_empty() {
        lines.push(cur_line);
    }

    for (line_idx, line) in lines.into_iter().enumerate() {
        if line_idx > 0 {
            state.cursor_y += state.line_height;
            state.cursor_x = state.line_start_x;
        }

        let line_w: i32 = line.iter().enumerate().map(|(i, wi)| {
            wi.w + if i > 0 { space_width } else { 0 }
        }).sum();

        let start_x = if state.cursor_x > state.line_start_x {
            state.cursor_x
        } else {
            let avail_w = (max_w - state.line_start_x).max(0);
            match state.text_align {
                crate::render::layout::state::TextAlign::Center => {
                    state.line_start_x + (avail_w - line_w).max(0) / 2
                }
                crate::render::layout::state::TextAlign::Right => {
                    state.line_start_x + (avail_w - line_w).max(0)
                }
                crate::render::layout::state::TextAlign::Left => {
                    state.cursor_x
                }
            }
        };

        state.cursor_x = start_x;

        for wi in line {
            if state.paint && state.visibility && state.current_opacity > 0.001 {
                let underline = state.text_decoration_underline;
                let tex = fonts.get_text_texture_family(tc, &wi.word, font_size, text_color, bold, italic, underline, family.clone());
                if let Some(t) = tex {
                    let y_offset = (state.line_height - wi.h) / 2;
                    let target = sdl2::rect::Rect::new(state.cursor_x, state.cursor_y + y_offset - state.ctx.scroll_y, wi.w as u32, wi.h as u32);
                    let _ = canvas.copy(&t, None, Some(target));

                    if let Some(href) = &state.active_link {
                        state.link_areas.push(crate::render::layout::state::LinkArea {
                            x: state.cursor_x,
                            y: state.cursor_y,
                            w: wi.w,
                            h: wi.h,
                            href: href.clone(),
                        });
                    }
                }
            }

            state.cursor_x += wi.w + space_width;
            state.max_x = state.max_x.max(state.cursor_x);
        }
    }
}
