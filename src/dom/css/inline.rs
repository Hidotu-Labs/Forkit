use crate::dom::node::{
    Style, TextAlign, ListStyleType, Display, TextTransform, Overflow,
    Border, Borders, FontFamilyHint, WordBreak, BoxShadow,
};
use super::color::parse_color_alpha;
use super::length::{parse_length, parse_length_ctx, parse_box_spacing, LengthContext};
/// Apply a `"property: value; …"` inline style string onto an existing `Style`.
pub fn apply_inline(css: &str, s: &mut Style) {
    let base = s.font_size;
    for decl in css.split(';') {
        let decl = decl.trim();
        if decl.is_empty() { continue; }
        // Use find(':') but skip data-URIs — just take the first colon
        if let Some(colon) = decl.find(':') {
            let prop = decl[..colon].trim().to_ascii_lowercase();
            let val  = decl[colon+1..].trim();
            apply_property(&prop, val, base, s);
        }
    }
}

pub(super) fn apply_property(prop: &str, val: &str, base: u16, s: &mut Style) {
    match prop {
        // ---- color ----
        "color" => {
            if let Some((rgb, alpha)) = parse_color_alpha(val) {
                s.color = rgb;
                s.color_alpha = alpha;
            }
        }
        "background-color" | "background" => {
            // Skip gradient / url() values; pass everything else whole to the parser.
            // rgba() and hsla() contain spaces, so we must NOT split on whitespace for them.
            let lower_val = val.to_ascii_lowercase();
            let is_functional_color = lower_val.starts_with("rgba(")
                || lower_val.starts_with("rgb(")
                || lower_val.starts_with("hsla(")
                || lower_val.starts_with("hsl(")
                || lower_val.starts_with('#');
            let tok = if is_functional_color || !val.contains(' ') {
                val
            } else {
                // Multi-word value like "linear-gradient(...)" or "url(...)":
                // take the first space-separated token to see if it's a plain color.
                val.split_whitespace().next().unwrap_or(val)
            };
            if let Some((rgb, alpha)) = parse_color_alpha(tok) {
                s.bg_color = Some(rgb);
                s.bg_alpha = alpha;
            }
        }
        "opacity" => {
            if let Ok(n) = val.parse::<f32>() {
                s.opacity = (n.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }

        // ---- font ----
        "font-size" => {
            if let Some(n) = parse_length(val, base, 16) {
                if n > 0 { s.font_size = n.clamp(8, 96) as u16; }
            }
        }
        "font-weight" => {
            s.bold = matches!(
                val.to_ascii_lowercase().as_str(),
                "bold" | "bolder" | "700" | "800" | "900"
            );
        }
        "font-style" => {
            s.italic = val.eq_ignore_ascii_case("italic")
                || val.eq_ignore_ascii_case("oblique");
        }
        "font-variant" | "font-variant-caps" => {
            s.font_variant_caps = val.eq_ignore_ascii_case("small-caps");
        }

        // ---- text ----
        "text-decoration" | "text-decoration-line" => {
            let lv = val.to_ascii_lowercase();
            s.underline     = lv.contains("underline");
            s.strikethrough = lv.contains("line-through");
        }
        "text-align" => {
            s.text_align = match val.to_ascii_lowercase().as_str() {
                "center"        => TextAlign::Center,
                "right" | "end" => TextAlign::Right,
                _               => TextAlign::Left,
            };
        }
        "text-transform" => {
            s.text_transform = match val.to_ascii_lowercase().as_str() {
                "uppercase"  => TextTransform::Uppercase,
                "lowercase"  => TextTransform::Lowercase,
                "capitalize" => TextTransform::Capitalize,
                _            => TextTransform::None,
            };
        }
        "line-height" => {
            let bare = val.trim_end_matches('%').trim_end_matches("px");
            if let Ok(n) = bare.parse::<f32>() {
                s.line_height_mul = if val.ends_with('%')   { n / 100.0 }
                                    else if val.ends_with("px") { n / base as f32 }
                                    else { n };
            }
        }
        "letter-spacing" => {
            if let Some(n) = parse_length(val, base, 0) { s.letter_spacing = n; }
        }
        "word-spacing" => {
            if let Some(n) = parse_length(val, base, 0) { s.word_spacing = n; }
        }
        "white-space" => {
            s.white_space_pre = matches!(
                val.to_ascii_lowercase().as_str(),
                "pre" | "pre-wrap" | "pre-line"
            );
        }

        // ---- display / visibility ----
        "display" => {
            match val.to_ascii_lowercase().as_str() {
                "none"         => { s.display = Display::Hidden;      s.display_block = false; }
                "block" | "flex" | "grid" | "list-item" | "table" => {
                                   s.display = Display::Block;         s.display_block = true; }
                "inline-block" => { s.display = Display::InlineBlock; s.display_block = false; }
                _              => { s.display = Display::Inline;      s.display_block = false; }
            }
        }
        "visibility" => {
            if val.eq_ignore_ascii_case("hidden") {
                s.display = Display::Hidden;
            }
        }
        "overflow" | "overflow-y" => {
            s.overflow = match val.to_ascii_lowercase().as_str() {
                "hidden" => Overflow::Hidden,
                "scroll" => Overflow::Scroll,
                "auto"   => Overflow::Auto,
                _        => Overflow::Visible,
            };
        }

        // ---- sizing ----
        // These use parse_length_ctx so that viewport-relative units (vw, vh)
        // and percentage values resolve against the correct dimension.
        "width" => {
            let ctx = LengthContext {
                base_font_size:  base,
                percent_base:    800,
                viewport_width:  800,
                viewport_height: 600,
            };
            s.size.width = parse_length_ctx(val, &ctx).filter(|&n| n > 0);
        }
        "height" => {
            let ctx = LengthContext {
                base_font_size:  base,
                percent_base:    600,
                viewport_width:  800,
                viewport_height: 600,
            };
            s.size.height = parse_length_ctx(val, &ctx).filter(|&n| n > 0);
        }
        "max-width" => {
            let ctx = LengthContext {
                base_font_size:  base,
                percent_base:    800,
                viewport_width:  800,
                viewport_height: 600,
            };
            s.size.max_width = parse_length_ctx(val, &ctx).filter(|&n| n > 0);
        }
        "min-width" => {
            let ctx = LengthContext {
                base_font_size:  base,
                percent_base:    800,
                viewport_width:  800,
                viewport_height: 600,
            };
            s.size.min_width = parse_length_ctx(val, &ctx).filter(|&n| n > 0);
        }

        // ---- borders ----
        "border" => parse_border_shorthand(val, base, &mut s.borders, BorderSide::All),
        "border-top"    => parse_border_shorthand(val, base, &mut s.borders, BorderSide::Top),
        "border-right"  => parse_border_shorthand(val, base, &mut s.borders, BorderSide::Right),
        "border-bottom" => parse_border_shorthand(val, base, &mut s.borders, BorderSide::Bottom),
        "border-left"   => parse_border_shorthand(val, base, &mut s.borders, BorderSide::Left),
        "border-width"  => {
            let w = parse_length(val, base, 0).unwrap_or(0).clamp(0, 20) as u8;
            s.borders.top.width    = w;
            s.borders.right.width  = w;
            s.borders.bottom.width = w;
            s.borders.left.width   = w;
        }
        "border-color" => {
            if let Some((rgb, _alpha)) = parse_color_alpha(val) {
                s.borders.top.color    = rgb;
                s.borders.right.color  = rgb;
                s.borders.bottom.color = rgb;
                s.borders.left.color   = rgb;
            }
        }

        // ---- border radius ----
        "border-radius" => {
            let ctx = LengthContext {
                base_font_size:  base,
                percent_base:    0,
                viewport_width:  800,
                viewport_height: 600,
            };
            let tokens: Vec<&str> = val.split_whitespace().collect();
            let parsed: Vec<u16> = tokens.iter()
                .filter_map(|t| parse_length_ctx(t, &ctx))
                .map(|n| n.clamp(0, u16::MAX as i32) as u16)
                .collect();
            match parsed.len() {
                1 => {
                    s.border_radius = [parsed[0], parsed[0], parsed[0], parsed[0]];
                }
                2 => {
                    // top-left & bottom-right = v[0], top-right & bottom-left = v[1]
                    s.border_radius = [parsed[0], parsed[1], parsed[0], parsed[1]];
                }
                3 => {
                    // top-left = v[0], top-right & bottom-left = v[1], bottom-right = v[2]
                    s.border_radius = [parsed[0], parsed[1], parsed[2], parsed[1]];
                }
                4 => {
                    // top-left, top-right, bottom-right, bottom-left
                    s.border_radius = [parsed[0], parsed[1], parsed[2], parsed[3]];
                }
                _ => {}
            }
        }
        "border-top-left-radius" => {
            let ctx = LengthContext { base_font_size: base, percent_base: 0,
                viewport_width: 800, viewport_height: 600 };
            if let Some(n) = parse_length_ctx(val, &ctx) {
                s.border_radius[0] = n.clamp(0, u16::MAX as i32) as u16;
            }
        }
        "border-top-right-radius" => {
            let ctx = LengthContext { base_font_size: base, percent_base: 0,
                viewport_width: 800, viewport_height: 600 };
            if let Some(n) = parse_length_ctx(val, &ctx) {
                s.border_radius[1] = n.clamp(0, u16::MAX as i32) as u16;
            }
        }
        "border-bottom-right-radius" => {
            let ctx = LengthContext { base_font_size: base, percent_base: 0,
                viewport_width: 800, viewport_height: 600 };
            if let Some(n) = parse_length_ctx(val, &ctx) {
                s.border_radius[2] = n.clamp(0, u16::MAX as i32) as u16;
            }
        }
        "border-bottom-left-radius" => {
            let ctx = LengthContext { base_font_size: base, percent_base: 0,
                viewport_width: 800, viewport_height: 600 };
            if let Some(n) = parse_length_ctx(val, &ctx) {
                s.border_radius[3] = n.clamp(0, u16::MAX as i32) as u16;
            }
        }

        // ---- spacing ----
        "padding"        => { s.padding = parse_box_spacing(val, base); }
        "padding-top"    => { s.padding.top    = parse_length(val, base, 0).unwrap_or(0); }
        "padding-right"  => { s.padding.right  = parse_length(val, base, 0).unwrap_or(0); }
        "padding-bottom" => { s.padding.bottom = parse_length(val, base, 0).unwrap_or(0); }
        "padding-left"   => { s.padding.left   = parse_length(val, base, 0).unwrap_or(0); }
        "margin"         => { s.margin = parse_box_spacing(val, base); }
        "margin-top"     => { s.margin.top    = parse_length(val, base, 0).unwrap_or(0); }
        "margin-right"   => { s.margin.right  = parse_length(val, base, 0).unwrap_or(0); }
        "margin-bottom"  => { s.margin.bottom = parse_length(val, base, 0).unwrap_or(0); }
        "margin-left"    => { s.margin.left   = parse_length(val, base, 0).unwrap_or(0); }

        // ---- list ----
        "list-style-type" | "list-style" => {
            s.list_style_type = match val.to_ascii_lowercase().as_str() {
                "none"    => ListStyleType::None,
                "circle"  => ListStyleType::Circle,
                "square"  => ListStyleType::Square,
                "decimal" => ListStyleType::Decimal,
                _         => ListStyleType::Disc,
            };
        }

        // ---- font family ----
        "font-family" => {
            s.font_family = match crate::render::font::FontFamily::from_css(val) {
                crate::render::font::FontFamily::Monospace => FontFamilyHint::Monospace,
                crate::render::font::FontFamily::Serif     => FontFamilyHint::Serif,
                _                                          => FontFamilyHint::SansSerif,
            };
        }

        // ---- word break / overflow wrap ----
        "word-break" | "overflow-wrap" | "word-wrap" => {
            s.word_break = match val.to_ascii_lowercase().as_str() {
                "break-all"                     => WordBreak::BreakAll,
                "break-word" | "anywhere"       => WordBreak::BreakWord,
                _                               => WordBreak::Normal,
            };
        }

        // ---- box-shadow ----
        "box-shadow" => {
            if val.eq_ignore_ascii_case("none") {
                s.box_shadow = None;
            } else {
                s.box_shadow = parse_box_shadow(val, base);
            }
        }

        // ---- sizing (max-height / min-height) ----
        "max-height" => {
            let ctx = LengthContext {
                base_font_size:  base,
                percent_base:    600,
                viewport_width:  800,
                viewport_height: 600,
            };
            s.size.max_height = parse_length_ctx(val, &ctx).filter(|&n| n > 0);
        }
        "min-height" => {
            let ctx = LengthContext {
                base_font_size:  base,
                percent_base:    600,
                viewport_width:  800,
                viewport_height: 600,
            };
            s.size.min_height = parse_length_ctx(val, &ctx).filter(|&n| n > 0);
        }

        _ => {} // unrecognised property — silently ignore
    }
}

// ---------------------------------------------------------------------------
// Border shorthand parser
// ---------------------------------------------------------------------------

enum BorderSide { All, Top, Right, Bottom, Left }

/// Parse `"1px solid #ccc"` or `"2px"` etc. and apply to the given side(s).
fn parse_border_shorthand(val: &str, base: u16, borders: &mut Borders, side: BorderSide) {
    let lv = val.trim().to_ascii_lowercase();
    if lv == "none" || lv == "0" {
        let b = Border::default();
        apply_border(borders, &side, b);
        return;
    }
    let mut width: u8 = 1;
    let mut color: [u8; 3] = [0, 0, 0];
    for token in val.split_whitespace() {
        if let Some(w) = parse_length(token, base, 0) {
            width = w.clamp(0, 20) as u8;
        } else if let Some((c, _alpha)) = parse_color_alpha(token) {
            color = c;
        }
        // "solid" / "dashed" / "dotted" — style ignored, we always draw solid
    }
    apply_border(borders, &side, Border { width, color });
}

fn apply_border(borders: &mut Borders, side: &BorderSide, b: Border) {
    match side {
        BorderSide::All    => { borders.top = b; borders.right = b; borders.bottom = b; borders.left = b; }
        BorderSide::Top    => { borders.top = b; }
        BorderSide::Right  => { borders.right = b; }
        BorderSide::Bottom => { borders.bottom = b; }
        BorderSide::Left   => { borders.left = b; }
    }
}

// ---------------------------------------------------------------------------
// Box-shadow shorthand parser
// ---------------------------------------------------------------------------

/// Parse a CSS `box-shadow` value like `2px 4px 6px rgba(0,0,0,0.3)`.
/// We support a single shadow (no inset, no spread radius).
fn parse_box_shadow(val: &str, base: u16) -> Option<BoxShadow> {
    use super::color::parse_color_alpha;
    use super::length::parse_length;

    // Split on whitespace but keep functional expressions intact
    let mut lengths: Vec<i32> = Vec::new();
    let mut color: [u8; 3] = [0, 0, 0];
    let mut alpha: u8 = 80; // default semi-transparent

    // Tokenise — group parenthesised expressions
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for ch in val.chars() {
        match ch {
            '(' => { depth += 1; cur.push(ch); }
            ')' => {
                if depth > 0 { depth -= 1; }
                cur.push(ch);
                if depth == 0 {
                    tokens.push(cur.trim().to_string());
                    cur.clear();
                }
            }
            ' ' | '\t' if depth == 0 => {
                let t = cur.trim().to_string();
                if !t.is_empty() { tokens.push(t); }
                cur.clear();
            }
            _ => { cur.push(ch); }
        }
    }
    if !cur.trim().is_empty() { tokens.push(cur.trim().to_string()); }

    for tok in &tokens {
        let t = tok.as_str();
        if t.eq_ignore_ascii_case("inset") { continue; } // unsupported, skip
        if let Some(n) = parse_length(t, base, 0) {
            lengths.push(n);
        } else if let Some((c, a)) = parse_color_alpha(t) {
            color = c;
            alpha = a;
        }
    }

    if lengths.len() < 2 { return None; }

    Some(BoxShadow {
        offset_x: lengths[0],
        offset_y: lengths[1],
        blur:     lengths.get(2).copied().unwrap_or(0).max(0),
        color,
        alpha,
    })
}
