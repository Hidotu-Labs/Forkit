use crate::dom::node::{
    Element, Border, Borders, BoxSpacing, ListStyleType, TextAlign, Display, FontFamilyHint,
};

/// Tags that render as block-level elements by default.
const BLOCK_TAGS: &[&str] = &[
    "div", "p", "h1", "h2", "h3", "h4", "h5", "h6",
    "ul", "ol", "li", "dl", "dt", "dd",
    "nav", "header", "footer", "main",
    "section", "article", "aside", "figure", "figcaption",
    "body", "html", "blockquote", "pre", "details", "summary",
    "table", "thead", "tbody", "tfoot", "tr", "th", "td", "caption",
    "form", "fieldset", "legend",
    "address", "dialog",
    "video", "audio",           // media placeholders
    "canvas",                   // placeholder box
    "output", "meter", "progress",
];

/// Apply browser UA-stylesheet defaults for the given element's tag.
pub fn apply_tag_defaults(el: &mut Element) {
    let t  = el.tag.as_str();
    let s  = &mut el.style;

    if BLOCK_TAGS.contains(&t) {
        s.display_block = true;
        s.display       = Display::Block;
    } else if t != "#document" {
        // Inline elements must not inherit display:block from a block parent.
        s.display_block = false;
        if s.display == Display::Block { s.display = Display::Inline; }
    }

    // Pull href from attrs_raw into the style (used by the link-click handler)
    if t == "a" {
        if let Some(href) = crate::dom::parser::get_attr(&el.attrs_raw, "href") {
            s.href = Some(href.to_owned());
        }
    }

    // Pull width/height attrs on <img>, <video>, <canvas> into size
    if matches!(t, "img" | "video" | "canvas" | "audio") {
        if let Some(w) = crate::dom::parser::get_attr(&el.attrs_raw, "width") {
            if let Ok(n) = w.trim_end_matches("px").parse::<i32>() {
                s.size.width = Some(n);
            }
        }
        if let Some(h) = crate::dom::parser::get_attr(&el.attrs_raw, "height") {
            if let Ok(n) = h.trim_end_matches("px").parse::<i32>() {
                s.size.height = Some(n);
            }
        }
    }

    match t {
        // ── Headings ──────────────────────────────────────────────────────
        "h1" => { s.font_size = 32; s.bold = true; s.margin.top = 16; s.margin.bottom = 8; }
        "h2" => { s.font_size = 26; s.bold = true; s.margin.top = 14; s.margin.bottom = 6; }
        "h3" => { s.font_size = 22; s.bold = true; s.margin.top = 12; s.margin.bottom = 4; }
        "h4" => { s.font_size = 18; s.bold = true; s.margin.top = 8;  s.margin.bottom = 4; }
        "h5" => { s.font_size = 16; s.bold = true; s.margin.top = 6;  s.margin.bottom = 2; }
        "h6" => { s.font_size = 14; s.bold = true; s.margin.top = 4;  s.margin.bottom = 2; }

        // ── Inline text semantics ──────────────────────────────────────────
        "b" | "strong"                        => { s.bold = true; }
        "i" | "em" | "cite" | "dfn"  => { s.italic = true; }
        "var" => {
            s.italic      = true;
            s.font_family = FontFamilyHint::Monospace;
        }        "u" | "ins"                           => { s.underline = true; }
        "s" | "del" | "strike"                => { s.strikethrough = true; }
        "small"                               => { s.font_size = 12; }
        "big"                                 => { s.font_size = 20; }
        "mark"                                => { s.bg_color = Some([255, 255, 0]); }
        "sub" | "sup"                         => { s.font_size = 12; }
        "abbr"                                => { s.underline = true; s.color = [80, 80, 80]; }
        "q"                                   => { s.italic = true; }
        "address"                             => { s.italic = true; }
        "time"                                => { s.color = [80, 80, 80]; }

        // ── Links ──────────────────────────────────────────────────────────
        "a" => { s.color = [0, 102, 204]; s.underline = true; }

        // ── Code / monospace ───────────────────────────────────────────────
        "code" | "samp" | "tt" => {
            s.bg_color    = Some([240, 240, 240]);
            s.font_family = FontFamilyHint::Monospace;
            s.font_size   = (s.font_size as f32 * 0.9) as u16;
            s.border_radius = [3, 3, 3, 3];
            s.padding     = BoxSpacing { top: 1, right: 4, bottom: 1, left: 4 };
        }
        "kbd" => {
            s.bg_color    = Some([240, 240, 240]);
            s.font_family = FontFamilyHint::Monospace;
            s.font_size   = (s.font_size as f32 * 0.9) as u16;
            s.borders     = Borders::uniform(Border { width: 1, color: [180, 180, 180] });
            s.border_radius = [3, 3, 3, 3];
            s.padding     = BoxSpacing { top: 1, right: 5, bottom: 1, left: 5 };
        }
        "pre" => {
            s.white_space_pre = true;
            s.font_family     = FontFamilyHint::Monospace;
            s.bg_color        = Some([248, 248, 248]);
            s.borders         = Borders::uniform(Border { width: 1, color: [220, 220, 220] });
            s.border_radius   = [4, 4, 4, 4];
            s.padding         = BoxSpacing { top: 12, right: 12, bottom: 12, left: 12 };
            s.margin.top      = 8;
            s.margin.bottom   = 8;
        }

        // ── Blockquote ─────────────────────────────────────────────────────
        "blockquote" => {
            s.margin       = BoxSpacing { top: 8, right: 16, bottom: 8, left: 24 };
            s.color        = [80, 80, 80];
            s.borders.left = Border { width: 4, color: [180, 180, 180] };
            s.padding.left = 16;
        }

        // ── Paragraphs / sections ──────────────────────────────────────────
        "p" => {
            s.margin.top    = 8;
            s.margin.bottom = 8;
        }

        // ── Body / HTML default page margin ───────────────────────────────
        "body" => {
            s.margin = BoxSpacing { top: 8, right: 8, bottom: 8, left: 8 };
        }

        // ── Lists ──────────────────────────────────────────────────────────
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

        // ── Tables ─────────────────────────────────────────────────────────
        "table" => {
            // No default border on the table element itself — the cell borders
            // (td/th) form the grid, matching browser border-collapse behaviour.
            // An explicit CSS border="" or border attribute will still apply.
            s.margin.top    = 8;
            s.margin.bottom = 8;
        }
        "th" => {
            s.bold       = true;
            s.padding    = BoxSpacing { top: 6, right: 12, bottom: 6, left: 12 };
            s.text_align = TextAlign::Center;
        }
        "td" => {
            s.padding = BoxSpacing { top: 6, right: 12, bottom: 6, left: 12 };
        }
        "caption" => { s.text_align = TextAlign::Center; s.bold = true; s.margin.bottom = 4; }

        // ── Figure ─────────────────────────────────────────────────────────
        "figure" => {
            s.margin = BoxSpacing { top: 8, right: 40, bottom: 8, left: 40 };
        }
        "figcaption" => {
            s.italic    = true;
            s.font_size = 13;
            s.color     = [100, 100, 100];
            s.text_align = TextAlign::Center;
            s.margin.top = 4;
        }

        // ── Details / summary ──────────────────────────────────────────────
        "details" => {
            s.borders    = Borders::uniform(Border { width: 1, color: [220, 220, 220] });
            s.border_radius = [4, 4, 4, 4];
            s.padding    = BoxSpacing { top: 4, right: 8, bottom: 4, left: 8 };
            s.margin.top = 4; s.margin.bottom = 4;
        }
        "summary" => {
            s.bold  = true;
            s.color = [0, 80, 160];
        }

        // ── Form elements ──────────────────────────────────────────────────
        "fieldset" => {
            s.borders     = Borders::uniform(Border { width: 1, color: [200, 200, 200] });
            s.border_radius = [4, 4, 4, 4];
            s.padding     = BoxSpacing { top: 8, right: 12, bottom: 8, left: 12 };
            s.margin.top  = 8; s.margin.bottom = 8;
        }
        "legend" => {
            s.bold      = true;
            s.padding   = BoxSpacing { top: 0, right: 6, bottom: 0, left: 6 };
        }
        "label" => {
            s.bold = true;
        }
        "input" | "textarea" => {
            let input_type = crate::dom::parser::get_attr(&el.attrs_raw, "type")
                .unwrap_or("text")
                .to_ascii_lowercase();
            if matches!(input_type.as_str(), "hidden") {
                s.display = Display::Hidden;
            } else {
                s.bg_color      = Some([255, 255, 255]);
                s.borders       = Borders::uniform(Border { width: 1, color: [180, 180, 180] });
                s.border_radius = [4, 4, 4, 4];
                s.padding       = BoxSpacing { top: 4, right: 8, bottom: 4, left: 8 };
                if t == "textarea" {
                    s.size.width    = Some(300);
                    s.size.height   = Some(80);
                    s.font_family   = FontFamilyHint::Monospace;
                    s.white_space_pre = true;
                } else {
                    s.size.width = Some(200);
                }
            }
        }
        "select" => {
            s.bg_color      = Some([255, 255, 255]);
            s.borders       = Borders::uniform(Border { width: 1, color: [180, 180, 180] });
            s.border_radius = [4, 4, 4, 4];
            s.padding       = BoxSpacing { top: 4, right: 8, bottom: 4, left: 8 };
            s.size.width    = Some(200);
        }
        "button" => {
            s.bg_color      = Some([240, 240, 240]);
            s.borders       = Borders::uniform(Border { width: 1, color: [180, 180, 180] });
            s.border_radius = [4, 4, 4, 4];
            s.padding       = BoxSpacing { top: 6, right: 14, bottom: 6, left: 14 };
        }
        "option" => {
            s.display_block = false;
        }

        // ── Progress / meter ───────────────────────────────────────────────
        "progress" | "meter" => {
            s.size.width  = Some(200);
            s.size.height = Some(16);
            s.bg_color    = Some([220, 220, 220]);
            s.borders     = Borders::uniform(Border { width: 1, color: [180, 180, 180] });
            s.border_radius = [8, 8, 8, 8];
        }

        // ── Data element (machine-readable value) ───────────────────────────
        "data" => {
            // Render inline like a span, with optional muted color to indicate
            // machine-readable data
            s.display_block = false;
            s.display       = Display::Inline;
            s.color         = [80, 80, 80];
        }

        // ── Output element (form calculation result) ────────────────────────
        "output" => {
            s.bg_color    = Some([248, 248, 248]);
            s.borders     = Borders::uniform(Border { width: 1, color: [200, 200, 200] });
            s.border_radius = [4, 4, 4, 4];
            s.padding     = BoxSpacing { top: 4, right: 8, bottom: 4, left: 8 };
            s.margin.top  = 4;
            s.margin.bottom = 4;
        }

        // ── Media placeholders ────────────────────────────────────────────
        "video" | "canvas" => {
            if s.size.width.is_none()  { s.size.width  = Some(320); }
            if s.size.height.is_none() { s.size.height = Some(180); }
            s.bg_color    = Some([30, 30, 30]);
            s.border_radius = [4, 4, 4, 4];
        }
        "audio" => {
            if s.size.width.is_none()  { s.size.width  = Some(300); }
            if s.size.height.is_none() { s.size.height = Some(36); }
            s.bg_color    = Some([50, 50, 50]);
            s.border_radius = [18, 18, 18, 18];
        }

        // ── Navigation ────────────────────────────────────────────────────
        "nav" => {
            s.margin.top    = 4;
            s.margin.bottom = 4;
        }

        // ── Sectioning ───────────────────────────────────────────────────
        "header" | "footer" => {
            s.padding     = BoxSpacing { top: 8, right: 0, bottom: 8, left: 0 };
        }

        // ── Horizontal rule (handled in block.rs, but give it margins) ───
        "hr" => {
            s.margin.top    = 8;
            s.margin.bottom = 8;
        }

        _ => {}
    }
}
