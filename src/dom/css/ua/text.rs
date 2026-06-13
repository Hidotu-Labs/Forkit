use crate::dom::node::{Style, Border, Borders, BoxSpacing, FontFamilyHint};

pub fn apply_text_defaults(tag: &str, s: &mut Style) {
    match tag {
        "h1" => { s.font_size = 32; s.bold = true; s.margin.top = 16; s.margin.bottom = 8; }
        "h2" => { s.font_size = 24; s.bold = true; s.margin.top = 14; s.margin.bottom = 6; }
        "h3" => { s.font_size = 17; s.bold = true; s.margin.top = 12; s.margin.bottom = 4; }
        "h4" => { s.font_size = 16; s.bold = true; s.margin.top = 8;  s.margin.bottom = 4; }
        "h5" => { s.font_size = 12; s.bold = true; s.margin.top = 6;  s.margin.bottom = 2; }
        "h6" => { s.font_size = 10; s.bold = true; s.margin.top = 4;  s.margin.bottom = 2; }

        "b" | "strong"           => { s.bold = true; }
        "i" | "em" | "cite" | "dfn" => { s.italic = true; }
        "var" => { s.italic = true; s.font_family = FontFamilyHint::Monospace; }
        "u" | "ins"              => { s.underline = true; }
        "s" | "del" | "strike"   => { s.strikethrough = true; }
        "small"                  => { s.font_size = 12; }
        "big"                    => { s.font_size = 20; }
        "mark"                   => { s.bg_color = Some([255, 255, 0]); }
        "sub" | "sup"            => { s.font_size = 12; }
        "abbr"                   => { s.underline = true; s.color = [80, 80, 80]; }
        "q"                      => { s.italic = true; }
        "time"                   => { s.color = [80, 80, 80]; }

        "a" => { s.color = [0, 102, 204]; s.underline = true; }

        "code" | "samp" | "tt" => {
            s.bg_color      = Some([240, 240, 240]);
            s.font_family   = FontFamilyHint::Monospace;
            s.font_size     = (s.font_size as f32 * 0.9) as u16;
            s.border_radius = [3, 3, 3, 3];
            s.padding       = BoxSpacing { top: 1, right: 4, bottom: 1, left: 4 };
        }
        "kbd" => {
            s.bg_color      = Some([240, 240, 240]);
            s.font_family   = FontFamilyHint::Monospace;
            s.font_size     = (s.font_size as f32 * 0.9) as u16;
            s.borders       = Borders::uniform(Border { width: 1, color: [180, 180, 180], ..Default::default() });
            s.border_radius = [3, 3, 3, 3];
            s.padding       = BoxSpacing { top: 1, right: 5, bottom: 1, left: 5 };
        }
        "pre" => {
            s.white_space_pre = true;
            s.font_family     = FontFamilyHint::Monospace;
            s.bg_color        = Some([248, 248, 248]);
            s.borders         = Borders::uniform(Border { width: 1, color: [220, 220, 220], ..Default::default() });
            s.border_radius   = [4, 4, 4, 4];
            s.padding         = BoxSpacing { top: 12, right: 12, bottom: 12, left: 12 };
            s.margin.top      = 8;
            s.margin.bottom   = 8;
        }

        "blockquote" => {
            s.margin       = BoxSpacing { top: 8, right: 16, bottom: 8, left: 24 };
            s.color        = [80, 80, 80];
            s.borders.left = Border { width: 4, color: [180, 180, 180], ..Default::default() };
            s.padding.left = 16;
        }

        "p" => { s.margin.top = 8; s.margin.bottom = 8; }

        _ => {}
    }
}
