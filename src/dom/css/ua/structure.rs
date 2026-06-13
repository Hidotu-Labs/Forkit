use crate::dom::node::{Style, Border, Borders, BoxSpacing, TextAlign};

pub fn apply_structure_defaults(tag: &str, s: &mut Style) {
    match tag {
        "body" => {
            s.margin = BoxSpacing { top: 8, right: 8, bottom: 8, left: 8 };
        }
        "figure" => {
            s.margin = BoxSpacing { top: 8, right: 40, bottom: 8, left: 40 };
        }
        "figcaption" => {
            s.italic     = true;
            s.font_size  = 13;
            s.color      = [100, 100, 100];
            s.text_align = TextAlign::Center;
            s.margin.top = 4;
        }
        "details" => {
            s.borders       = Borders::uniform(Border { width: 1, color: [220, 220, 220] });
            s.border_radius = [4, 4, 4, 4];
            s.padding       = BoxSpacing { top: 4, right: 8, bottom: 4, left: 8 };
            s.margin.top    = 4;
            s.margin.bottom = 4;
        }
        "summary" => {
            s.bold  = true;
            s.color = [0, 80, 160];
        }
        "address" => {
            s.italic = true;
        }
        "nav" => {
            s.margin.top    = 4;
            s.margin.bottom = 4;
        }
        "header" | "footer" => {
            s.padding = BoxSpacing { top: 8, right: 0, bottom: 8, left: 0 };
        }
        "hr" => {
            s.margin.top    = 8;
            s.margin.bottom = 8;
        }
        _ => {}
    }
}

/// Tags that carry `width`/`height` HTML attributes which map directly to
/// CSS size. Pulled here so `mod.rs` can reference the list cleanly.
pub const SIZED_TAGS: &[&str] = &["img", "video", "canvas", "audio"];
