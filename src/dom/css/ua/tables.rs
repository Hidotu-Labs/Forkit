use crate::dom::node::{Style, BoxSpacing, TextAlign};

pub fn apply_table_defaults(tag: &str, s: &mut Style) {
    match tag {
        "table" => { s.margin.top = 8; s.margin.bottom = 8; }
        "th" => {
            s.bold       = true;
            s.padding    = BoxSpacing { top: 6, right: 12, bottom: 6, left: 12 };
            s.text_align = TextAlign::Center;
        }
        "td" => {
            s.padding = BoxSpacing { top: 6, right: 12, bottom: 6, left: 12 };
        }
        "caption" => {
            s.text_align    = TextAlign::Center;
            s.bold          = true;
            s.margin.bottom = 4;
        }
        _ => {}
    }
}
