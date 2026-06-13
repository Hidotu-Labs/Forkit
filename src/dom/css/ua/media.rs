use crate::dom::node::Style;

pub fn apply_media_defaults(tag: &str, s: &mut Style) {
    match tag {
        "video" | "canvas" => {
            if s.size.width.is_none()  { s.size.width  = Some(320); }
            if s.size.height.is_none() { s.size.height = Some(180); }
            s.bg_color      = Some([30, 30, 30]);
            s.border_radius = [4, 4, 4, 4];
        }
        "audio" => {
            if s.size.width.is_none()  { s.size.width  = Some(300); }
            if s.size.height.is_none() { s.size.height = Some(54); }
            s.bg_color      = Some([40, 40, 40]);
            s.border_radius = [8, 8, 8, 8];
        }
        _ => {}
    }
}
