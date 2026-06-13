use crate::dom::node::{
    Style, TextAlign, ListStyleType, Display, Visibility, TextTransform, Overflow,
    Border, Borders, FontFamilyHint, WordBreak, BoxShadow, BgSize, BgRepeat, BgPosition,
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
            let lower_val = val.trim().to_ascii_lowercase();
            if lower_val == "none" || lower_val == "transparent" {
                s.bg_color = None;
                s.bg_alpha = 0;
                if prop == "background" { s.bg_image_url = None; }
                return;
            }

            // If this is purely background-color (the property name), don't look for url()
            let is_color_only_prop = prop == "background-color";

            // Handle background-image url() inside the shorthand
            if !is_color_only_prop && lower_val.contains("url(") {
                if let Some(url) = extract_css_url(val) {
                    s.bg_image_url = Some(url);
                }
                // Parse remaining tokens for repeat, size, position, and color.
                // Tokenise respecting parenthesised groups.
                let tokens = tokenise_bg_shorthand(val);
                for tok in &tokens {
                    let tl = tok.to_ascii_lowercase();
                    if tl.starts_with("url(") { continue; }
                    match tl.as_str() {
                        "no-repeat"       => { s.bg_repeat = BgRepeat::NoRepeat; }
                        "repeat-x"        => { s.bg_repeat = BgRepeat::RepeatX; }
                        "repeat-y"        => { s.bg_repeat = BgRepeat::RepeatY; }
                        "repeat"          => { s.bg_repeat = BgRepeat::Repeat; }
                        "cover"           => { s.bg_size   = BgSize::Cover; }
                        "contain"         => { s.bg_size   = BgSize::Contain; }
                        "center"          => {} // position — default is fine
                        _ => {
                            // Try as colour
                            if let Some((rgb, alpha)) = parse_color_alpha(tok) {
                                s.bg_color = Some(rgb);
                                s.bg_alpha = alpha;
                            }
                        }
                    }
                }
                return;
            }

            // No url() — treat as a plain background-color value.
            // rgba() and hsla() contain spaces, so we must NOT split on whitespace for them.
            let is_functional_color = lower_val.starts_with("rgba(")
                || lower_val.starts_with("rgb(")
                || lower_val.starts_with("hsla(")
                || lower_val.starts_with("hsl(")
                || lower_val.starts_with('#');
            let tok = if is_functional_color || !val.contains(' ') {
                val
            } else {
                // Multi-word non-url value: take the first token as the colour.
                val.split_whitespace().next().unwrap_or(val)
            };
            if let Some((rgb, alpha)) = parse_color_alpha(tok) {
                s.bg_color = Some(rgb);
                s.bg_alpha = alpha;
            }
        }
        "background-image" => {
            let lower = val.to_ascii_lowercase();
            if lower == "none" {
                s.bg_image_url = None;
            } else if let Some(url) = extract_css_url(val) {
                s.bg_image_url = Some(url);
            }
        }
        "background-size" => {
            s.bg_size = match val.to_ascii_lowercase().as_str() {
                "cover"   => BgSize::Cover,
                "contain" => BgSize::Contain,
                _         => BgSize::Auto,
            };
        }
        "background-repeat" => {
            s.bg_repeat = match val.to_ascii_lowercase().as_str() {
                "no-repeat"       => BgRepeat::NoRepeat,
                "repeat-x"        => BgRepeat::RepeatX,
                "repeat-y"        => BgRepeat::RepeatY,
                _                 => BgRepeat::Repeat,
            };
        }
        "background-position" => {
            let tokens: Vec<&str> = val.split_whitespace().collect();
            let resolve_axis = |tok: &str, is_x: bool| -> i32 {
                match tok.to_ascii_lowercase().as_str() {
                    "left"   => 0,
                    "right"  => if is_x { 10000 } else { 0 },   // sentinel: 10000 = 100%
                    "top"    => if !is_x { 0 } else { 0 },
                    "bottom" => if !is_x { 10000 } else { 0 },
                    "center" => 5000,                            // sentinel: 5000 = 50%
                    t => parse_length(t, base, 0).unwrap_or(0),
                }
            };
            match tokens.as_slice() {
                [x_tok, y_tok] => {
                    s.bg_position = BgPosition {
                        x: resolve_axis(x_tok, true),
                        y: resolve_axis(y_tok, false),
                    };
                }
                [single] => {
                    let v = resolve_axis(single, true);
                    s.bg_position = BgPosition { x: v, y: v };
                }
                _ => {}
            }
        }
        "opacity" => {
            if let Ok(n) = val.parse::<f32>() {
                s.opacity = (n.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }

        // ---- font ----
        "font-size" => {
            // Viewport-relative units (vw, vh) and calc() cannot be resolved
            // until layout time when the real viewport dimensions are known.
            // Store them raw; block.rs will re-resolve them.
            let lv = val.to_ascii_lowercase();
            if lv.ends_with("vw") || lv.ends_with("vh") || lv.starts_with("calc(") {
                s.font_size_raw = Some(val.to_owned());
            } else {
                s.font_size_raw = None;
                if let Some(n) = parse_length(val, base, 16) {
                    if n > 0 { s.font_size = n.clamp(8, 96) as u16; }
                }
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
            s.visibility = match val.to_ascii_lowercase().as_str() {
                "hidden"   => Visibility::Hidden,
                "collapse" => Visibility::Collapse,
                _          => Visibility::Visible,
            };
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
        // Absolute units (px, em, pt, rem) are resolved immediately.
        // Viewport-relative (vw, vh) and percentage (%) values are stored raw
        // so they can be re-resolved in block.rs with the real containing-block
        // width, height, and viewport dimensions.
        "width" => {
            let lv = val.to_ascii_lowercase();
            if lv.ends_with('%') || lv.ends_with("vw") || lv.ends_with("vh") {
                s.size.width     = None;
                s.size.width_raw = Some(val.to_owned());
            } else {
                s.size.width_raw = None;
                s.size.width     = parse_length(val, base, 0).filter(|&n| n > 0);
            }
        }
        "height" => {
            let lv = val.to_ascii_lowercase();
            if lv.ends_with('%') || lv.ends_with("vw") || lv.ends_with("vh") {
                s.size.height     = None;
                s.size.height_raw = Some(val.to_owned());
            } else {
                s.size.height_raw = None;
                s.size.height     = parse_length(val, base, 0).filter(|&n| n > 0);
            }
        }
        "max-width" => {
            if val.eq_ignore_ascii_case("none") {
                s.size.max_width     = None;
                s.size.max_width_raw = None;
                return;
            }
            let lv = val.to_ascii_lowercase();
            if lv.ends_with('%') || lv.ends_with("vw") || lv.ends_with("vh") {
                s.size.max_width     = None;
                s.size.max_width_raw = Some(val.to_owned());
            } else {
                s.size.max_width_raw = None;
                s.size.max_width     = parse_length(val, base, 0).filter(|&n| n > 0);
            }
        }
        "min-width" => {
            let lv = val.to_ascii_lowercase();
            if lv.ends_with('%') || lv.ends_with("vw") || lv.ends_with("vh") {
                s.size.min_width     = None;
                s.size.min_width_raw = Some(val.to_owned());
            } else {
                s.size.min_width_raw = None;
                s.size.min_width     = parse_length(val, base, 0).filter(|&n| n > 0);
            }
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
        "margin"         => {
            // Handle "auto" in shorthand: "auto" alone → auto on all sides
            let lv = val.trim().to_ascii_lowercase();
            if lv == "auto" {
                s.margin_auto_left  = true;
                s.margin_auto_right = true;
                s.margin = crate::dom::node::BoxSpacing::default();
            } else {
                // Still handle tokens like "0 auto" or "10px auto"
                let parts: Vec<&str> = val.split_whitespace().collect();
                // Resolve each part: if "auto" → 0 and set flag
                let resolve = |i: usize, parts: &Vec<&str>, base: u16| -> i32 {
                    parts.get(i).map(|v| {
                        if v.eq_ignore_ascii_case("auto") { 0 }
                        else { super::length::parse_length(v, base, 0).unwrap_or(0) }
                    }).unwrap_or(0)
                };
                match parts.len() {
                    1 => {
                        s.margin = super::length::parse_box_spacing(val, base);
                    }
                    2 => {
                        let v_tb = resolve(0, &parts, base);
                        let h_auto = parts[1].eq_ignore_ascii_case("auto");
                        let h = if h_auto { 0 } else { resolve(1, &parts, base) };
                        s.margin = crate::dom::node::BoxSpacing { top: v_tb, right: h, bottom: v_tb, left: h };
                        if h_auto { s.margin_auto_left = true; s.margin_auto_right = true; }
                    }
                    3 => {
                        let r_auto = parts[1].eq_ignore_ascii_case("auto");
                        let r = if r_auto { 0 } else { resolve(1, &parts, base) };
                        s.margin = crate::dom::node::BoxSpacing {
                            top:    resolve(0, &parts, base),
                            right:  r,
                            bottom: resolve(2, &parts, base),
                            left:   r,
                        };
                        if r_auto { s.margin_auto_left = true; s.margin_auto_right = true; }
                    }
                    4 => {
                        let l_auto = parts[3].eq_ignore_ascii_case("auto");
                        let r_auto = parts[1].eq_ignore_ascii_case("auto");
                        s.margin = crate::dom::node::BoxSpacing {
                            top:    resolve(0, &parts, base),
                            right:  if r_auto { 0 } else { resolve(1, &parts, base) },
                            bottom: resolve(2, &parts, base),
                            left:   if l_auto { 0 } else { resolve(3, &parts, base) },
                        };
                        s.margin_auto_left  = l_auto;
                        s.margin_auto_right = r_auto;
                    }
                    _ => {}
                }
            }
        }
        "margin-top"     => { s.margin.top    = parse_length(val, base, 0).unwrap_or(0); }
        "margin-right"   => {
            if val.eq_ignore_ascii_case("auto") {
                s.margin_auto_right = true;
                s.margin.right = 0;
            } else {
                s.margin_auto_right = false;
                s.margin.right = parse_length(val, base, 0).unwrap_or(0);
            }
        }
        "margin-bottom"  => { s.margin.bottom = parse_length(val, base, 0).unwrap_or(0); }
        "margin-left"    => {
            if val.eq_ignore_ascii_case("auto") {
                s.margin_auto_left = true;
                s.margin.left = 0;
            } else {
                s.margin_auto_left = false;
                s.margin.left = parse_length(val, base, 0).unwrap_or(0);
            }
        }

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
            if val.eq_ignore_ascii_case("none") {
                s.size.max_height     = None;
                s.size.max_height_raw = None;
                return;
            }
            let lv = val.to_ascii_lowercase();
            if lv.ends_with('%') || lv.ends_with("vw") || lv.ends_with("vh") {
                s.size.max_height     = None;
                s.size.max_height_raw = Some(val.to_owned());
            } else {
                s.size.max_height_raw = None;
                s.size.max_height     = parse_length(val, base, 0).filter(|&n| n > 0);
            }
        }
        "min-height" => {
            let lv = val.to_ascii_lowercase();
            if lv.ends_with('%') || lv.ends_with("vw") || lv.ends_with("vh") {
                s.size.min_height     = None;
                s.size.min_height_raw = Some(val.to_owned());
            } else {
                s.size.min_height_raw = None;
                s.size.min_height     = parse_length(val, base, 0).filter(|&n| n > 0);
            }
        }

        _ => {} // unrecognised property — silently ignore
    }
}

// ---------------------------------------------------------------------------
// URL extractor
// ---------------------------------------------------------------------------

/// Tokenise a CSS `background` shorthand value, keeping parenthesised groups
/// (like `url(...)` or `rgb(...)`) intact as single tokens.
/// Splits on whitespace outside of parens; also splits on `/` (for `pos/size`).
fn tokenise_bg_shorthand(val: &str) -> Vec<String> {
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
                    let t = cur.trim().to_string();
                    if !t.is_empty() { tokens.push(t); }
                    cur.clear();
                }
            }
            ' ' | '\t' if depth == 0 => {
                let t = cur.trim().to_string();
                if !t.is_empty() { tokens.push(t); }
                cur.clear();
            }
            '/' if depth == 0 => {
                // position/size separator — treat as a token boundary
                let t = cur.trim().to_string();
                if !t.is_empty() { tokens.push(t); }
                cur.clear();
            }
            _ => { cur.push(ch); }
        }
    }
    let t = cur.trim().to_string();
    if !t.is_empty() { tokens.push(t); }
    tokens
}

/// Extract the URL string from a CSS `url(...)` token.
/// Handles both quoted (`url("foo.png")`) and unquoted (`url(foo.png)`) forms.
/// Also handles values where url() is surrounded by other tokens (e.g. background shorthand).
pub(crate) fn extract_css_url(val: &str) -> Option<String> {
    let trimmed = val.trim();
    let lower = trimmed.to_ascii_lowercase();

    // Find the start of url(
    let url_start = lower.find("url(")?;
    let after_open = url_start + 4; // skip "url("

    // Find the matching closing paren, tracking nested parens
    let chars: Vec<char> = trimmed[after_open..].chars().collect();
    let mut depth = 1usize;
    let mut end = 0usize;
    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 { end = i; break; }
            }
            _ => {}
        }
    }
    if depth != 0 { return None; }

    let byte_end = after_open + chars[..end].iter().map(|c| c.len_utf8()).sum::<usize>();
    let inner = trimmed[after_open..byte_end].trim();

    // Strip optional quotes
    let inner = if (inner.starts_with('"') && inner.ends_with('"'))
        || (inner.starts_with('\'') && inner.ends_with('\''))
    {
        &inner[1..inner.len() - 1]
    } else {
        inner
    };
    if inner.is_empty() { None } else { Some(inner.to_owned()) }
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
