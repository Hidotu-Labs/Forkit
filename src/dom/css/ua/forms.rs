use crate::dom::node::{Element, Border, Borders, BoxSpacing, Display, FontFamily};

pub fn apply_form_defaults(tag: &str, el: &mut Element) {
    let s = &mut el.style;
    match tag {
        "fieldset" => {
            s.borders       = Borders::uniform(Border { width: 1, color: [200, 200, 200], ..Default::default() });
            s.border_radius = [4, 4, 4, 4];
            s.padding       = BoxSpacing { top: 8, right: 12, bottom: 8, left: 12 };
            s.margin.top    = 8;
            s.margin.bottom = 8;
        }
        "legend" => {
            s.bold    = true;
            s.padding = BoxSpacing { top: 0, right: 6, bottom: 0, left: 6 };
        }
        "label" => {
            s.bold = true;
        }
        "input" | "textarea" => {
            let input_type = crate::dom::parser::get_attr(&el.attrs_raw, "type")
                .unwrap_or("text")
                .to_ascii_lowercase();
            if input_type == "hidden" {
                s.display = Display::Hidden;
            } else {
                s.border_radius = [4, 4, 4, 4];
                s.borders       = Borders::uniform(Border { width: 1, color: [180, 180, 180], ..Default::default() });

                let is_button = input_type == "button" || input_type == "submit" || input_type == "reset";

                if is_button {
                    s.bg_color = Some([245, 245, 245]);
                    s.padding  = BoxSpacing { top: 6, right: 14, bottom: 6, left: 14 };
                    s.color    = [60, 64, 67];
                    if input_type == "submit" {
                        s.bold     = true;
                        // For a more "premium" feel that doesn't clash, 
                        // we can use a very subtle border or a slightly different grey.
                        s.bg_color = Some([238, 238, 238]);
                    }
                } else {
                    s.bg_color = Some([255, 255, 255]);
                    s.padding  = BoxSpacing { top: 4, right: 8, bottom: 4, left: 8 };
                    if tag == "textarea" {
                        s.size.width  = Some(300);
                        s.size.height = Some(80);
                        s.font_family = FontFamily::Monospace;
                        s.white_space_pre = true;
                    } else if !matches!(input_type.as_str(), "checkbox" | "radio" | "range" | "color") {
                        s.size.width = Some(200);
                    }
                }
            }
        }
        "select" => {
            s.bg_color      = Some([255, 255, 255]);
            s.borders       = Borders::uniform(Border { width: 1, color: [180, 180, 180], ..Default::default() });
            s.border_radius = [4, 4, 4, 4];
            s.padding       = BoxSpacing { top: 4, right: 8, bottom: 4, left: 8 };
            s.size.width    = Some(200);
        }
        "button" => {
            s.bg_color      = Some([240, 240, 240]);
            s.borders       = Borders::uniform(Border { width: 1, color: [180, 180, 180], ..Default::default() });
            s.border_radius = [4, 4, 4, 4];
            s.padding       = BoxSpacing { top: 6, right: 14, bottom: 6, left: 14 };
        }
        "option" => {
            s.display_block = false;
        }
        "progress" | "meter" => {
            s.display       = Display::InlineBlock;
            s.display_block = false;
            s.size.width    = Some(200);
            s.size.height   = Some(16);
            s.bg_color      = Some([220, 220, 220]);
            s.borders       = Borders::uniform(Border { width: 1, color: [180, 180, 180], ..Default::default() });
            s.border_radius = [8, 8, 8, 8];
        }
        "data" => {
            s.display_block = false;
            s.display       = Display::Inline;
            s.color         = [80, 80, 80];
        }
        "output" => {
            s.display       = Display::Inline;
            s.display_block = false;
        }
        _ => {}
    }
}
