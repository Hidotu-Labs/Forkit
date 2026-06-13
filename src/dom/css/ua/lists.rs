use crate::dom::node::{Style, ListStyleType};

pub fn apply_list_defaults(tag: &str, s: &mut Style) {
    match tag {
        "ul" => { s.padding.left = 28; s.margin.top = 4; s.margin.bottom = 4; }
        "ol" => {
            s.padding.left    = 28;
            s.margin.top      = 4;
            s.margin.bottom   = 4;
            s.list_style_type = ListStyleType::Decimal;
        }
        "li" => { s.margin.top = 2; s.margin.bottom = 2; }
        "dl" => { s.margin.top = 8; s.margin.bottom = 8; }
        "dd" => { s.margin.left = 40; }
        "dt" => { s.bold = true; }
        _ => {}
    }
}
