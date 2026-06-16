use sdl2::pixels::Color;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::dom::node::{Element, Style, Node};
use crate::render::layout::state::{LayoutState, LINE_SPACING, InputArea, ButtonArea, ButtonAction, InputKind};
use crate::render::font::FontCache;
use crate::render::layout::paint::{
    paint_text, measure_text, fill_rect_alpha, fill_rounded_rect,
    draw_rounded_rect,
};

pub fn paint_form_control(
    ls:     &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc:     &TextureCreator<WindowContext>,
    fonts:  &mut FontCache,
    el:     &Element,
    s:      &Style,
    max_w:  i32,
) {
    let tag = el.tag.as_str();
    let input_type = crate::dom::parser::get_attr(&el.attrs_raw, "type")
        .unwrap_or("text")
        .to_ascii_lowercase();

    if input_type == "hidden" { return; }

    let kind = match tag {
        "textarea" => InputKind::TextArea,
        "input" => match input_type.as_str() {
            "password" => InputKind::Password,
            "text" | "email" | "search" | "tel" | "url" | "date" => InputKind::Text,
            "number" => InputKind::Number,
            "checkbox" => InputKind::Checkbox,
            "radio"    => InputKind::Radio,
            "range"    => InputKind::Range,
            "color"    => InputKind::Color,
            _ => InputKind::Other,
        },
        _ => InputKind::Other,
    };

    let label: String = if tag == "button" {
        String::new()
    } else {
        crate::dom::parser::get_attr(&el.attrs_raw, "value")
            .map(|v| crate::dom::parser::decode_entities(v))
            .unwrap_or_else(|| match input_type.as_str() {
                "submit"  => "Submit".to_owned(),
                "reset"   => "Reset".to_owned(),
                "checkbox"| "radio" => String::new(),
                _         => crate::dom::parser::get_attr(&el.attrs_raw, "placeholder")
                                .map(|p| crate::dom::parser::decode_entities(p))
                                .unwrap_or_default(),
            })
    };

    let (mut ctrl_w, ctrl_h) = if matches!(input_type.as_str(), "checkbox" | "radio") {
        (16, 16)
    } else if input_type == "range" {
        // If an explicit width was set via CSS, use it; otherwise fill available space
        // so that `flex: 1` / `width: 100%` range inputs stretch correctly.
        let w = if let Some(explicit_w) = s.size.width {
            explicit_w
        } else {
            let ml = s.margin.left;
            let mr = s.margin.right;
            let avail = (max_w - ls.cursor_x - ml - mr).max(20);
            // Respect min-width if set, fall back to a sensible default minimum.
            let min_w = s.size.min_width.unwrap_or(80);
            avail.max(min_w)
        };
        let h = s.size.height.unwrap_or(20).max(20);
        (w, h)
    } else if input_type == "color" {
        (50, 28)
    } else {
        let w = s.size.width.unwrap_or(if tag == "textarea" { 300 } else { 200 });
        let h = s.size.height.unwrap_or(if tag == "textarea" { 80 } else { 28 });
        (w, h)
    };

    if s.size.width.is_none() && (tag == "button" || matches!(input_type.as_str(), "button" | "submit" | "reset")) {
        if s.display_block {
            let ml = s.margin.left;
            let mr = s.margin.right;
            ctrl_w = (max_w - ls.margin_left - ls.indent - ml - mr).max(40);
        } else if s.flex_grow > 0.0 {
            // Inside a flex container with flex-grow: fill the space allocated by the
            // parent flex layout (max_w is set to the flex-allocated right boundary).
            let ml = s.margin.left;
            let mr = s.margin.right;
            let avail = (max_w - ls.cursor_x - ml - mr).max(40);
            ctrl_w = avail;
        } else {
            if tag == "button" {
                let mut total_w = 0;
                for child in &el.children {
                    if let Node::Text(t) = child {
                        let ts = Style { font_size: s.font_size, bold: s.bold, italic: s.italic, ..Default::default() };
                        let (tw, _) = measure_text(fonts, &t.text, &ts);
                        total_w += tw;
                    }
                }
                ctrl_w = total_w + s.padding.left + s.padding.right;
            } else {
                let ts = Style { font_size: s.font_size, bold: s.bold, italic: s.italic, ..Default::default() };
                let (tw, _) = measure_text(fonts, &label, &ts);
                ctrl_w = tw + s.padding.left + s.padding.right;
            }
            ctrl_w = ctrl_w.max(40);
        }
    }

    let x = ls.cursor_x;
    let y = ls.cursor_y;

    let input_name = crate::dom::parser::get_attr(&el.attrs_raw, "name").unwrap_or("").to_owned();
    let input_index = if matches!(kind, InputKind::Text | InputKind::Password | InputKind::TextArea | InputKind::Checkbox | InputKind::Radio | InputKind::Range | InputKind::Color | InputKind::Number) {
        let idx = ls.input_count;
        ls.input_count += 1;
        let is_disabled = crate::dom::parser::get_attr(&el.attrs_raw, "disabled").is_some();
        let is_readonly = crate::dom::parser::get_attr(&el.attrs_raw, "readonly").is_some();
        ls.input_areas.push(InputArea {
            x, y, w: ctrl_w, h: ctrl_h,
            index: idx,
            kind: kind.clone(),
            name: input_name,
            default_value: label.clone(),
            disabled: is_disabled,
            readonly: is_readonly,
        });
        Some(idx)
    } else {
        None
    };

    let live_value: Option<String> = input_index.and_then(|idx| {
        ls.input_values.get(idx).map(|v| v.clone())
    });
    let is_focused = input_index.map(|idx| ls.focused_input == Some(idx)).unwrap_or(false);

    if input_type == "range" {
        let min = crate::dom::parser::get_attr(&el.attrs_raw, "min")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        let max = crate::dom::parser::get_attr(&el.attrs_raw, "max")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(100.0);
        let val_str = live_value.filter(|v| !v.is_empty()).unwrap_or_else(|| {
            crate::dom::parser::get_attr(&el.attrs_raw, "value").unwrap_or("50").to_owned()
        });
        let val = val_str.parse::<f64>().unwrap_or((min + max) / 2.0);
        
        let ratio = ((val - min) / (max - min).max(0.001)).clamp(0.0, 1.0);
        
        let track_h = 6i32;
        let thumb_r = 9i32;
        let by_track = y + (ctrl_h - track_h) / 2;
        
        fill_rounded_rect(canvas, Color::RGB(225, 225, 225), 255, x, by_track, ctrl_w, track_h, [track_h as u16 / 2; 4], ls.ctx.scroll_y, ls.ctx.viewport_height);
        
        let fill_w = (ctrl_w as f64 * ratio) as i32;
        if fill_w > 0 {
            fill_rounded_rect(canvas, Color::RGB(66, 133, 244), 255, x, by_track, fill_w, track_h, [track_h as u16 / 2; 4], ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
        
        let tx = x + fill_w - thumb_r;
        let ty = y + (ctrl_h - thumb_r*2) / 2;
        fill_rounded_rect(canvas, Color::RGB(0, 0, 0), 40, tx, ty + 1, thumb_r*2, thumb_r*2, [thumb_r as u16; 4], ls.ctx.scroll_y, ls.ctx.viewport_height);
        fill_rounded_rect(canvas, Color::RGB(66, 133, 244), 255, tx, ty, thumb_r*2, thumb_r*2, [thumb_r as u16; 4], ls.ctx.scroll_y, ls.ctx.viewport_height);
        
        ls.cursor_x += ctrl_w + 8;
        if ctrl_h > ls.line_height { ls.line_height = ctrl_h; }
        return;
    }

    if input_type == "color" {
        let val = live_value.filter(|v| !v.is_empty()).unwrap_or_else(|| {
            crate::dom::parser::get_attr(&el.attrs_raw, "value").unwrap_or("#000000").to_owned()
        });
        let rgb = if val.starts_with('#') && val.len() == 7 {
            let r = u8::from_str_radix(&val[1..3], 16).unwrap_or(0);
            let g = u8::from_str_radix(&val[3..5], 16).unwrap_or(0);
            let b = u8::from_str_radix(&val[5..7], 16).unwrap_or(0);
            [r, g, b]
        } else if val.starts_with('#') && val.len() == 4 {
            let r = u8::from_str_radix(&val[1..2], 16).unwrap_or(0) * 17;
            let g = u8::from_str_radix(&val[2..3], 16).unwrap_or(0) * 17;
            let b = u8::from_str_radix(&val[3..4], 16).unwrap_or(0) * 17;
            [r, g, b]
        } else {
            [0, 0, 0]
        };
        
        fill_rounded_rect(canvas, Color::RGB(255, 255, 255), 255, x, y, ctrl_w, ctrl_h, [4; 4], ls.ctx.scroll_y, ls.ctx.viewport_height);
        draw_rounded_rect(canvas, Color::RGB(180, 180, 180), 255, x, y, ctrl_w, ctrl_h, [4; 4], ls.ctx.scroll_y, ls.ctx.viewport_height);
        
        fill_rect_alpha(canvas, Color::RGB(rgb[0], rgb[1], rgb[2]), 255, x + 4, y + 4, ctrl_w - 8, ctrl_h - 8, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
        canvas.set_draw_color(Color::RGB(100, 100, 100));
        let _ = canvas.draw_rect(sdl2::rect::Rect::new(x + 4, y + 4 - ls.ctx.scroll_y, (ctrl_w - 8) as u32, (ctrl_h - 8) as u32));
        
        ls.cursor_x += ctrl_w + 8;
        if ctrl_h > ls.line_height { ls.line_height = ctrl_h; }
        return;
    }

    if matches!(input_type.as_str(), "checkbox" | "radio") {
        let is_radio = input_type == "radio";
        let is_checked = match live_value.as_deref() {
            Some("true")  => true,
            Some("false") => false,
            _             => crate::dom::parser::get_attr(&el.attrs_raw, "checked").is_some(),
        };
        let sz = 16i32;

        let radii = if is_radio { [sz as u16 / 2; 4] } else { [2; 4] };

        let bg_color = if is_checked { Color::RGB(66, 133, 244) } else { Color::RGB(255, 255, 255) };
        fill_rounded_rect(canvas, bg_color, 255, x, y, sz, sz, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);

        let border_color = if is_checked { Color::RGB(66, 133, 244) } else { Color::RGB(150, 150, 150) };
        draw_rounded_rect(canvas, border_color, 255, x, y, sz, sz, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);

        if is_checked {
            if is_radio {
                let dot_sz = 6i32;
                let dot_off = (sz - dot_sz) / 2;
                let dot_radii = [dot_sz as u16 / 2; 4];
                fill_rounded_rect(canvas, Color::RGB(255, 255, 255), 255, x + dot_off, y + dot_off, dot_sz, dot_sz, dot_radii, ls.ctx.scroll_y, ls.ctx.viewport_height);
            } else {
                let white = Color::RGB(255, 255, 255);
                for i in 0..3 {
                    fill_rect_alpha(canvas, white, 255, x + 4 + i, y + 8 + i, 2, 2, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
                }
                for i in 0..6 {
                    fill_rect_alpha(canvas, white, 255, x + 7 + i, y + 11 - i, 2, 2, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
                }
            }
        }

        ls.cursor_x   += sz + 6;
        if sz > ls.line_height { ls.line_height = sz; }
        return;
    }

    let is_disabled = crate::dom::parser::get_attr(&el.attrs_raw, "disabled").is_some();
    let is_readonly = crate::dom::parser::get_attr(&el.attrs_raw, "readonly").is_some();

    // Background: grey for disabled, white for readonly/normal
    let bg = if is_disabled {
        [229, 231, 235] // light grey — matches CSS :disabled UA style
    } else {
        s.bg_color.unwrap_or([255, 255, 255])
    };
    let radii = s.border_radius;

    if radii != [0, 0, 0, 0] {
        fill_rounded_rect(canvas, Color::RGB(bg[0], bg[1], bg[2]), 255,
                          x, y, ctrl_w, ctrl_h, radii,
                          ls.ctx.scroll_y, ls.ctx.viewport_height);
    } else {
        fill_rect_alpha(canvas, Color::RGB(bg[0], bg[1], bg[2]), 255,
                        x, y, ctrl_w, ctrl_h, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    // Border: hidden for disabled, normal grey for readonly, blue when focused
    let border_color = if is_disabled {
        Color::RGB(209, 213, 219) // barely-visible grey
    } else if is_focused {
        Color::RGB(66, 133, 244)
    } else {
        Color::RGB(180, 180, 180)
    };
    let border_width = if is_focused && !is_disabled { 2i32 } else { 1i32 };

    if radii != [0, 0, 0, 0] {
        draw_rounded_rect(canvas, border_color, 255,
                          x, y, ctrl_w, ctrl_h, radii,
                          ls.ctx.scroll_y, ls.ctx.viewport_height);
    } else {
        for bw in 0..border_width {
            let bx = x - bw; let by2 = y - bw;
            let bw2 = ctrl_w + bw * 2; let bh2 = ctrl_h + bw * 2;
            fill_rect_alpha(canvas, border_color, 255, bx,           by2,            bw2, 1, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            fill_rect_alpha(canvas, border_color, 255, bx,           by2 + bh2 - 1, bw2, 1, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            fill_rect_alpha(canvas, border_color, 255, bx,           by2,            1, bh2, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            fill_rect_alpha(canvas, border_color, 255, bx + bw2 - 1, by2,            1, bh2, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
    }

    let live_nonempty = live_value.as_deref().filter(|v| !v.is_empty()).map(|v| v.to_owned());
    let display_text: String = if let Some(ref v) = live_nonempty {
        if kind == InputKind::Password {
            "•".repeat(v.chars().count())
        } else {
            v.clone()
        }
    } else {
        String::new()
    };

    let is_placeholder = display_text.is_empty() && !label.is_empty();
    let text_color = if is_disabled {
        [156, 163, 175] // grey — matches CSS :disabled text color
    } else if is_placeholder {
        [160, 160, 160]
    } else {
        // Use the cascaded CSS color if it was explicitly set (non-default),
        // otherwise fall back to a dark neutral that works on light backgrounds.
        if s.color != [0, 0, 0] { s.color } else { [30, 30, 30] }
    };

    let text_style = Style {
        font_size: s.font_size,
        color: text_color,
        bold: s.bold,
        italic: s.italic,
        underline: s.underline,
        strikethrough: s.strikethrough,
        ..Default::default()
    };
    let render_text = if is_placeholder { &label } else { &display_text };
    if !render_text.is_empty() {
        let text_x = if tag == "button" || matches!(input_type.as_str(), "button" | "submit" | "reset") {
            let (tw, _) = measure_text(fonts, render_text, &text_style);
            x + (ctrl_w - tw) / 2
        } else {
            x + s.padding.left.max(6)
        };
        paint_text(canvas, tc, fonts, render_text, &text_style,
                   text_x,
                   y + (ctrl_h - s.font_size as i32) / 2,
                   ls.rounding_clip.as_ref(),
                   ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    if is_focused {
        let cursor_style = Style { font_size: s.font_size, ..Default::default() };
        // Use the same text that's actually displayed so the cursor aligns correctly.
        let display_before_cursor = if kind == InputKind::Password {
            "•".repeat(display_text.chars().count())
        } else {
            display_text.clone()
        };
        let (cx_off, _) = measure_text(fonts, &display_before_cursor, &cursor_style);
        let cx = x + s.padding.left.max(6) + cx_off;
        let cy_top    = y + (ctrl_h - s.font_size as i32) / 2;
        let cy_bottom = cy_top + s.font_size as i32;
        fill_rect_alpha(canvas, Color::RGB(30, 30, 30), 255,
                        cx, cy_top, 1, (cy_bottom - cy_top).max(2),
                        ls.rounding_clip.as_ref(),
                        ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    if input_type == "number" {
        // Only draw and register steppers when the input is focused.
        if is_focused {
            let arrow_w = 16i32;
            let half_h  = ctrl_h / 2;
            let ax = x + ctrl_w - arrow_w;

            // Divider line between text area and arrows, and between the two halves
            fill_rect_alpha(canvas, Color::RGB(180, 180, 180), 255,
                            ax, y, 1, ctrl_h,
                            ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            let mid_y = y + half_h;
            fill_rect_alpha(canvas, Color::RGB(180, 180, 180), 255,
                            ax, mid_y, arrow_w, 1,
                            ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);

            let ac = Color::RGB(80, 80, 80);
            // Up arrow (▲)
            let up_cy = y + half_h / 2;
            for i in 0..4i32 {
                fill_rect_alpha(canvas, ac, 255,
                                ax + arrow_w / 2 - i, up_cy + i, (i * 2 + 1).max(1), 1,
                                ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            }
            // Down arrow (▼)
            let dn_cy = mid_y + half_h / 2 - 3;
            for i in 0..4i32 {
                let row = 3 - i;
                fill_rect_alpha(canvas, ac, 255,
                                ax + arrow_w / 2 - row, dn_cy + i, (row * 2 + 1).max(1), 1,
                                ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            }

            // Register clickable stepper areas (only when an index exists)
            if let Some(idx) = input_index {
                ls.button_areas.push(crate::render::layout::state::ButtonArea {
                    x: ax, y, w: arrow_w, h: half_h,
                    action: crate::render::layout::state::ButtonAction::StepUp(idx),
                });
                ls.button_areas.push(crate::render::layout::state::ButtonArea {
                    x: ax, y: y + half_h, w: arrow_w, h: ctrl_h - half_h,
                    action: crate::render::layout::state::ButtonAction::StepDown(idx),
                });
            }
        }
    }

    if input_type == "date" {
        let icon_w = 16i32;
        let icon_h = 16i32;
        let icon_x = x + ctrl_w - icon_w - 8;
        let icon_y = y + (ctrl_h - icon_h) / 2;
        let bc = Color::RGB(80, 80, 80);

        let ry = icon_y - ls.ctx.scroll_y;
        if ry + icon_h > 0 && ry < ls.ctx.viewport_height {
            fill_rect_alpha(canvas, bc, 255, icon_x, icon_y + 3, icon_w, 1, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            fill_rect_alpha(canvas, bc, 255, icon_x, icon_y + 3, 1, icon_h - 3, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            fill_rect_alpha(canvas, bc, 255, icon_x + icon_w - 1, icon_y + 3, 1, icon_h - 3, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            fill_rect_alpha(canvas, bc, 255, icon_x, icon_y + icon_h - 1, icon_w, 1, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            
            fill_rect_alpha(canvas, bc, 255, icon_x, icon_y + 3, icon_w, 3, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            fill_rect_alpha(canvas, bc, 255, icon_x + 3,  icon_y, 2, 4, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
            fill_rect_alpha(canvas, bc, 255, icon_x + 11, icon_y, 2, 4, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
    }

    if tag == "button" && !el.children.is_empty() {
        let saved_x = ls.cursor_x;
        let saved_y = ls.cursor_y;
        let saved_ml = ls.margin_left;

        let mut children_w = 0;
        for child in &el.children {
            if let Node::Text(t) = child {
                let ts = Style { font_size: s.font_size, ..Default::default() };
                children_w += measure_text(fonts, &t.text, &ts).0;
            }
        }

        ls.cursor_x  = x + (ctrl_w - children_w) / 2;
        ls.cursor_y  = y + s.padding.top.max(4);
        ls.margin_left = ls.cursor_x;
        for child in &el.children {
            if let Node::Text(t) = child {
                let ts = Style { font_size: s.font_size, color: s.color, bold: s.bold, ..Default::default() };
                paint_text(canvas, tc, fonts, &t.text, &ts,
                           ls.cursor_x, ls.cursor_y,
                           ls.rounding_clip.as_ref(),
                           ls.ctx.scroll_y, ls.ctx.viewport_height);
            }
        }
        ls.cursor_x   = saved_x;
        ls.cursor_y   = saved_y;
        ls.margin_left = saved_ml;
    }

    let btn_action = if tag == "button" || input_type == "submit" {
        ButtonAction::Submit(ls.form_action.clone())
    } else if input_type == "reset" {
        ButtonAction::Reset
    } else {
        ButtonAction::None
    };
    if btn_action != ButtonAction::None {
        ls.button_areas.push(ButtonArea { x, y, w: ctrl_w, h: ctrl_h, action: btn_action });
    }

    ls.cursor_x += ctrl_w + 4;
    if ctrl_h > ls.line_height { ls.line_height = ctrl_h; }
}

pub fn paint_progress(
    ls:     &mut LayoutState,
    canvas: &mut Canvas<Window>,
    el:     &Element,
    s:      &Style,
    _max_w: i32,
) {
    let x = ls.cursor_x;
    let y = ls.cursor_y;
    let w = s.size.width.unwrap_or(200);
    let h = s.size.height.unwrap_or(16);
    let radii = s.border_radius;
    let bg = s.bg_color.unwrap_or([220, 220, 220]);

    let tag = el.tag.as_str();

    let (min_val, max_val, value) = if tag == "progress" {
        let max = crate::dom::parser::get_attr(&el.attrs_raw, "max")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(1.0);
        let val = crate::dom::parser::get_attr(&el.attrs_raw, "value")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        (0.0, max, val)
    } else {
        let min = crate::dom::parser::get_attr(&el.attrs_raw, "min")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        let max = crate::dom::parser::get_attr(&el.attrs_raw, "max")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(1.0);
        let val = crate::dom::parser::get_attr(&el.attrs_raw, "value")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        (min, max, val)
    };

    let range = (max_val - min_val).max(0.001);
    let ratio = ((value - min_val) / range).clamp(0.0, 1.0);
    let fill_w = (w as f64 * ratio).max(0.0) as i32;

    let fill_color = if tag == "meter" {
        let low = crate::dom::parser::get_attr(&el.attrs_raw, "low")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(min_val);
        let high = crate::dom::parser::get_attr(&el.attrs_raw, "high")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(max_val);

        if value < low || value > high {
            [220, 53, 69]
        } else if (value - low).abs() < (high - low) * 0.2
               || (value - high).abs() < (high - low) * 0.2 {
            [255, 193, 7]
        } else {
            [40, 167, 69]
        }
    } else {
        [66, 133, 244]
    };

    if radii != [0, 0, 0, 0] {
        fill_rounded_rect(canvas, Color::RGB(bg[0], bg[1], bg[2]), 255,
                          x, y, w, h, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);
        if fill_w > 0 {
            fill_rounded_rect(canvas, Color::RGB(fill_color[0], fill_color[1], fill_color[2]), 255,
                              x, y, fill_w, h, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
        draw_rounded_rect(canvas, Color::RGB(160, 160, 160), 255,
                          x, y, w, h, radii, ls.ctx.scroll_y, ls.ctx.viewport_height);
    } else {
        fill_rect_alpha(canvas, Color::RGB(bg[0], bg[1], bg[2]), 255,
                        x, y, w, h, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
        if fill_w > 0 {
            fill_rect_alpha(canvas, Color::RGB(fill_color[0], fill_color[1], fill_color[2]), 255,
                            x, y, fill_w, h, ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);
        }
    }

    ls.cursor_x += w;
    if h > ls.line_height { ls.line_height = h; }
}
