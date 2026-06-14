use crate::dom::node::{
    Style, TextAlign, ListStyleType, Display, Visibility, TextTransform, Overflow,
    Border, BorderStyle, Borders, FontFamily, WordBreak, BoxShadow, BgSize, BgRepeat, BgPosition,
    LinearGradient, GradientStop, FlexDirection, FlexWrap, JustifyContent, AlignItems,
};
use super::color::parse_color_alpha;
use super::length::{parse_length, parse_length_ctx, parse_box_spacing, LengthContext};
/// Apply a `"property: value; …"` inline style string onto an existing `Style`.
pub fn apply_inline(css: &str, s: &mut Style) {
    let base = s.font_size;
    // Capture the parent-inherited state before any overrides so that
    // `inherit` in an inline style can restore the inherited value.
    let inherited = s.clone();
    for decl in css.split(';') {
        let decl = decl.trim();
        if decl.is_empty() { continue; }
        // Use find(':') but skip data-URIs — just take the first colon
        if let Some(colon) = decl.find(':') {
            let prop = decl[..colon].trim().to_ascii_lowercase();
            let val  = decl[colon+1..].trim();
            if val.eq_ignore_ascii_case("initial") {
                apply_inline_initial(&prop, s);
            } else if val.eq_ignore_ascii_case("inherit") {
                apply_inline_inherit(&prop, s, &inherited);
            } else {
                apply_property(&prop, val, base, s);
            }
        }
    }
}

/// Reset a single property to its CSS initial (browser default) value.
fn apply_inline_initial(prop: &str, s: &mut Style) {
    let def = Style::default();
    match prop {
        "color"               => { s.color = def.color; s.color_alpha = def.color_alpha; }
        "font-size"           => { s.font_size = def.font_size; s.font_size_raw = None; }
        "font-weight"         => { s.bold = def.bold; }
        "font-style"          => { s.italic = def.italic; }
        "text-align"          => { s.text_align = def.text_align; }
        "line-height"         => { s.line_height_mul = def.line_height_mul; }
        "letter-spacing"      => { s.letter_spacing = def.letter_spacing; }
        "word-spacing"        => { s.word_spacing = def.word_spacing; }
        "white-space"         => { s.white_space_pre = def.white_space_pre; }
        "text-transform"      => { s.text_transform = def.text_transform; }
        "font-variant-caps"   => { s.font_variant_caps = def.font_variant_caps; }
        "background-color"    => { s.bg_color = def.bg_color; s.bg_alpha = def.bg_alpha; }
        "background-image"    => { s.bg_image_url = None; s.bg_gradient = None; }
        "background-size"     => { s.bg_size = def.bg_size; }
        "background-repeat"   => { s.bg_repeat = def.bg_repeat; }
        "background-position" => { s.bg_position = def.bg_position; }
        "border-radius"       => { s.border_radius = def.border_radius; }
        "display"             => { s.display = def.display; s.display_block = def.display_block; }
        "visibility"          => { s.visibility = def.visibility; }
        "opacity"             => { s.opacity = def.opacity; }
        _ => {}
    }
}

/// Restore a single property to the value it had before inline overrides
/// (i.e. the value inherited from the cascade / parent).
fn apply_inline_inherit(prop: &str, s: &mut Style, inherited: &Style) {
    match prop {
        "color"               => { s.color = inherited.color; s.color_alpha = inherited.color_alpha; }
        "font-size"           => { s.font_size = inherited.font_size; s.font_size_raw = inherited.font_size_raw.clone(); }
        "font-weight"         => { s.bold = inherited.bold; }
        "font-style"          => { s.italic = inherited.italic; }
        "text-align"          => { s.text_align = inherited.text_align; }
        "line-height"         => { s.line_height_mul = inherited.line_height_mul; }
        "letter-spacing"      => { s.letter_spacing = inherited.letter_spacing; }
        "word-spacing"        => { s.word_spacing = inherited.word_spacing; }
        "white-space"         => { s.white_space_pre = inherited.white_space_pre; }
        "text-transform"      => { s.text_transform = inherited.text_transform; }
        "font-variant-caps"   => { s.font_variant_caps = inherited.font_variant_caps; }
        "background-color"    => { s.bg_color = inherited.bg_color; s.bg_alpha = inherited.bg_alpha; }
        "display"             => { s.display = inherited.display; s.display_block = inherited.display_block; }
        "visibility"          => { s.visibility = inherited.visibility; }
        "opacity"             => { s.opacity = inherited.opacity; }
        _ => {}
    }
}

pub(crate) fn apply_property(prop: &str, val: &str, base: u16, s: &mut Style) {
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
                if prop == "background" { s.bg_image_url = None; s.bg_gradient = None; }
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

            // Handle linear-gradient() inside the shorthand (no url())
            if !is_color_only_prop && lower_val.contains("linear-gradient(") {
                s.bg_gradient = parse_linear_gradient(val);
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
                s.bg_gradient  = None;
            } else if lower.contains("linear-gradient(") {
                s.bg_gradient = parse_linear_gradient(val);
                // Also check for a url() layer in addition to the gradient
                if lower.contains("url(") {
                    if let Some(url) = extract_css_url(val) {
                        s.bg_image_url = Some(url);
                    }
                }
            } else if let Some(url) = extract_css_url(val) {
                s.bg_image_url = Some(url);
            }
        }
        "background-size" => {
            let lv = val.to_ascii_lowercase();
            match lv.trim() {
                "cover"   => s.bg_size = BgSize::Cover,
                "contain" => s.bg_size = BgSize::Contain,
                "auto"    => s.bg_size = BgSize::Auto,
                _ => {
                    // Two-value explicit size like "1500px 300px" or "50% auto".
                    // Store as explicit pixel dimensions in BgSize::Explicit if
                    // we can parse the first value as a width.
                    // For now we treat any explicit width as Cover-like sizing
                    // (stretch to fill the declared dimensions). If the value
                    // contains two tokens and the first is a concrete length,
                    // store the width so the painter can use it.
                    let tokens: Vec<&str> = val.split_whitespace().collect();
                    match tokens.as_slice() {
                        [w_tok, _h_tok] => {
                            // Parse width — store as explicit size.
                            // We reuse the BgSize enum; add Explicit variant below.
                            if let Some(w) = parse_length(w_tok, base, 0) {
                                if let Some(h) = parse_length(_h_tok, base, 0) {
                                    s.bg_size = BgSize::Explicit(w, h);
                                } else {
                                    s.bg_size = BgSize::Explicit(w, w);
                                }
                            }
                        }
                        [single] => {
                            if let Some(w) = parse_length(single, base, 0) {
                                s.bg_size = BgSize::Explicit(w, w);
                            }
                        }
                        _ => {}
                    }
                }
            }
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
                    t => {
                        // Handle percentage values: store as sentinel (percent * 100).
                        // e.g. 25% → 2500, 75% → 7500.
                        if let Some(pct_str) = t.strip_suffix('%') {
                            if let Ok(pct) = pct_str.trim().parse::<f32>() {
                                return (pct * 100.0).round() as i32;
                            }
                        }
                        parse_length(t, base, 0).unwrap_or(0)
                    },
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
            // Viewport-relative units (vw, vh) and math functions (calc, clamp, min, max)
            // cannot be resolved until layout time when the real viewport dimensions are known.
            // Store them raw; block.rs will re-resolve them.
            let lv = val.to_ascii_lowercase();
            if lv.ends_with("vw") || lv.ends_with("vh")
                || lv.starts_with("calc(") || lv.starts_with("clamp(")
                || lv.starts_with("min(") || lv.starts_with("max(")
            {
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
                "flex" | "inline-flex" => {
                                   s.display = Display::Flex;          s.display_block = true; }
                "block" | "grid" | "list-item" | "table" => {
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
        // Viewport-relative (vw, vh), percentage (%), and math functions
        // (calc, clamp, min, max) are stored raw so they can be re-resolved
        // in block.rs with the real containing-block width, height, and
        // viewport dimensions.
        "width" => {
            let lv = val.to_ascii_lowercase();
            if needs_deferred_resolve(&lv) {
                s.size.width     = None;
                s.size.width_raw = Some(val.to_owned());
            } else {
                s.size.width_raw = None;
                s.size.width     = parse_length(val, base, 0).filter(|&n| n > 0);
            }
        }
        "height" => {
            let lv = val.to_ascii_lowercase();
            if needs_deferred_resolve(&lv) {
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
            if needs_deferred_resolve(&lv) {
                s.size.max_width     = None;
                s.size.max_width_raw = Some(val.to_owned());
            } else {
                s.size.max_width_raw = None;
                s.size.max_width     = parse_length(val, base, 0).filter(|&n| n > 0);
            }
        }
        "min-width" => {
            let lv = val.to_ascii_lowercase();
            if needs_deferred_resolve(&lv) {
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
        "border-style" => {
            if let Some(style) = parse_border_style_kw(val) {
                apply_border_style(&mut s.borders, &BorderSide::All, style);
            }
        }
        "border-top-style"    => {
            if let Some(style) = parse_border_style_kw(val) {
                apply_border_style(&mut s.borders, &BorderSide::Top, style);
            }
        }
        "border-right-style"  => {
            if let Some(style) = parse_border_style_kw(val) {
                apply_border_style(&mut s.borders, &BorderSide::Right, style);
            }
        }
        "border-bottom-style" => {
            if let Some(style) = parse_border_style_kw(val) {
                apply_border_style(&mut s.borders, &BorderSide::Bottom, style);
            }
        }
        "border-left-style"   => {
            if let Some(style) = parse_border_style_kw(val) {
                apply_border_style(&mut s.borders, &BorderSide::Left, style);
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
            s.font_family = crate::dom::node::FontFamily::from_css(val);
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
            if needs_deferred_resolve(&lv) {
                s.size.max_height     = None;
                s.size.max_height_raw = Some(val.to_owned());
            } else {
                s.size.max_height_raw = None;
                s.size.max_height     = parse_length(val, base, 0).filter(|&n| n > 0);
            }
        }
        "min-height" => {
            let lv = val.to_ascii_lowercase();
            if needs_deferred_resolve(&lv) {
                s.size.min_height     = None;
                s.size.min_height_raw = Some(val.to_owned());
            } else {
                s.size.min_height_raw = None;
                s.size.min_height     = parse_length(val, base, 0).filter(|&n| n > 0);
            }
        }

        // ---- positioning ----
        "position" => {
            s.position = match val.to_ascii_lowercase().as_str() {
                "relative" => crate::dom::node::Position::Relative,
                "absolute" => crate::dom::node::Position::Absolute,
                "fixed"    => crate::dom::node::Position::Fixed,
                "sticky"   => crate::dom::node::Position::Sticky,
                _          => crate::dom::node::Position::Static,
            };
        }
        "top" => {
            let lv = val.to_ascii_lowercase();
            if needs_deferred_resolve(&lv) {
                s.top = None;
                s.top_raw = Some(val.to_owned());
            } else {
                s.top_raw = None;
                s.top = parse_length(val, base, 0);
            }
        }
        "bottom" => {
            let lv = val.to_ascii_lowercase();
            if needs_deferred_resolve(&lv) {
                s.bottom = None;
                s.bottom_raw = Some(val.to_owned());
            } else {
                s.bottom_raw = None;
                s.bottom = parse_length(val, base, 0);
            }
        }
        "left" => {
            let lv = val.to_ascii_lowercase();
            if needs_deferred_resolve(&lv) {
                s.left = None;
                s.left_raw = Some(val.to_owned());
            } else {
                s.left_raw = None;
                s.left = parse_length(val, base, 0);
            }
        }
        "right" => {
            let lv = val.to_ascii_lowercase();
            if needs_deferred_resolve(&lv) {
                s.right = None;
                s.right_raw = Some(val.to_owned());
            } else {
                s.right_raw = None;
                s.right = parse_length(val, base, 0);
            }
        }

        // ---- flexbox ----
        "flex-direction" => {
            s.flex_direction = FlexDirection::from_css(val);
        }
        "flex-wrap" => {
            s.flex_wrap = FlexWrap::from_css(val);
        }
        "justify-content" => {
            s.justify_content = JustifyContent::from_css(val);
        }
        "align-items" => {
            s.align_items = AlignItems::from_css(val);
        }
        "align-content" => {
            // Minimal: treat align-content like align-items for now
        }
        "flex-grow" => {
            if let Ok(n) = val.parse::<f32>() { s.flex_grow = n.max(0.0); }
        }
        "flex-shrink" => {
            if let Ok(n) = val.parse::<f32>() { s.flex_shrink = n.max(0.0); }
        }
        "flex-basis" => {
            if val.eq_ignore_ascii_case("auto") {
                s.flex_basis = None;
            } else {
                s.flex_basis = parse_length(val, base, 0).filter(|&n| n >= 0);
            }
        }
        "flex" => {
            // Shorthand: <flex-grow> [<flex-shrink> [<flex-basis>]] | auto | none
            let lv = val.trim().to_ascii_lowercase();
            match lv.as_str() {
                "auto" => { s.flex_grow = 1.0; s.flex_shrink = 1.0; s.flex_basis = None; }
                "none" => { s.flex_grow = 0.0; s.flex_shrink = 0.0; s.flex_basis = Some(0); }
                _ => {
                    let parts: Vec<&str> = val.split_whitespace().collect();
                    if let Some(g) = parts.first().and_then(|v| v.parse::<f32>().ok()) {
                        s.flex_grow = g.max(0.0);
                    }
                    if let Some(sh) = parts.get(1).and_then(|v| v.parse::<f32>().ok()) {
                        s.flex_shrink = sh.max(0.0);
                    }
                    if let Some(basis_tok) = parts.get(2) {
                        s.flex_basis = if basis_tok.eq_ignore_ascii_case("auto") {
                            None
                        } else {
                            parse_length(basis_tok, base, 0).filter(|&n| n >= 0)
                        };
                    }
                }
            }
        }
        "gap" | "row-gap" | "column-gap" => {
            // Simplified: treat gap as uniform spacing between flex items
            if prop == "gap" {
                // gap can be two values: row-gap column-gap
                let first = val.split_whitespace().next().unwrap_or(val);
                s.gap = parse_length(first, base, 0).unwrap_or(0).max(0);
            } else {
                s.gap = parse_length(val, base, 0).unwrap_or(0).max(0);
            }
        }

        _ => {} // unrecognised property — silently ignore
    }
}

// ---------------------------------------------------------------------------
// Sizing helper
// ---------------------------------------------------------------------------

/// Returns true if a CSS length value needs to be deferred to layout time
/// because it references viewport dimensions or contains a math function.
/// The input should already be ASCII-lowercased.
#[inline]
fn needs_deferred_resolve(lv: &str) -> bool {
    lv.ends_with('%')
        || lv.ends_with("vw")
        || lv.ends_with("vh")
        || lv.starts_with("calc(")
        || lv.starts_with("clamp(")
        || lv.starts_with("min(")
        || lv.starts_with("max(")
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
    let mut style = BorderStyle::Solid;
    for token in val.split_whitespace() {
        if let Some(w) = parse_length(token, base, 0) {
            width = w.clamp(0, 20) as u8;
        } else if let Some((c, _alpha)) = parse_color_alpha(token) {
            color = c;
        } else if let Some(s) = parse_border_style_kw(token) {
            style = s;
        }
    }
    apply_border(borders, &side, Border { width, color, style });
}

fn parse_border_style_kw(token: &str) -> Option<BorderStyle> {
    match token.trim().to_ascii_lowercase().as_str() {
        "solid"  => Some(BorderStyle::Solid),
        "dashed" => Some(BorderStyle::Dashed),
        "dotted" => Some(BorderStyle::Dotted),
        "none"   => Some(BorderStyle::None),
        _        => None,
    }
}

fn apply_border_style(borders: &mut Borders, side: &BorderSide, style: BorderStyle) {
    match side {
        BorderSide::All    => {
            borders.top.style    = style;
            borders.right.style  = style;
            borders.bottom.style = style;
            borders.left.style   = style;
        }
        BorderSide::Top    => { borders.top.style    = style; }
        BorderSide::Right  => { borders.right.style  = style; }
        BorderSide::Bottom => { borders.bottom.style = style; }
        BorderSide::Left   => { borders.left.style   = style; }
    }
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

// ---------------------------------------------------------------------------
// linear-gradient() parser
// ---------------------------------------------------------------------------

/// Parse a CSS `linear-gradient(…)` value.
///
/// Supports:
/// - `linear-gradient(to right, #f00, #00f)`
/// - `linear-gradient(45deg, red, blue)`
/// - `linear-gradient(#f00 0%, #00f 100%)`
/// - `linear-gradient(to bottom, rgba(0,0,0,0.5), transparent)`
///
/// Returns `None` only if there are fewer than two colour stops after parsing.
pub(crate) fn parse_linear_gradient(val: &str) -> Option<LinearGradient> {
    // Extract the argument list inside `linear-gradient(…)`.
    let lower = val.to_ascii_lowercase();
    let grad_start = lower.find("linear-gradient(")?;
    let after_open = grad_start + "linear-gradient(".len();

    let chars: Vec<char> = val[after_open..].chars().collect();
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
    let inner = val[after_open..byte_end].trim();

    // Split the argument list on commas, respecting nested parens.
    let args = split_gradient_args(inner);
    if args.is_empty() { return None; }

    // ── 1. Direction / angle ────────────────────────────────────────────────
    let first_lc = args[0].trim().to_ascii_lowercase();
    let (angle_deg, stops_start) = if first_lc.ends_with("deg") {
        let a = first_lc.trim_end_matches("deg").trim().parse::<f32>().unwrap_or(180.0);
        (a, 1)
    } else if first_lc.starts_with("to ") {
        let dir = first_lc["to ".len()..].trim();
        let a = match dir {
            "top"          => 0.0,
            "right"        => 90.0,
            "bottom"       => 180.0,
            "left"         => 270.0,
            "top right" | "right top"       => 45.0,
            "bottom right" | "right bottom" => 135.0,
            "bottom left"  | "left bottom"  => 225.0,
            "top left"     | "left top"     => 315.0,
            _              => 180.0,
        };
        (a, 1)
    } else {
        // No explicit direction — default is "to bottom" (180°)
        (180.0, 0)
    };

    // ── 2. Colour stops ─────────────────────────────────────────────────────
    let stop_args = &args[stops_start..];
    if stop_args.is_empty() { return None; }

    let mut stops: Vec<GradientStop> = Vec::new();
    for raw in stop_args {
        let s = raw.trim();
        if let Some(stop) = parse_gradient_stop(s) {
            stops.push(stop);
        }
    }
    if stops.len() < 2 { return None; }

    // Resolve missing positions: distribute evenly.
    resolve_stop_positions(&mut stops);

    Some(LinearGradient { angle_deg, stops })
}

/// Parse a single gradient colour-stop token like `red`, `#abc`, `blue 40%`,
/// `rgba(0,0,0,0.5) 0%`, or `transparent`.
fn parse_gradient_stop(s: &str) -> Option<GradientStop> {
    // Tokenise: split on whitespace outside parens.
    let parts = tokenise_bg_shorthand(s);
    if parts.is_empty() { return None; }

    // The colour part is the first token that parses as a colour.
    // The optional position is the last token if it ends with % or px.
    let mut color_opt: Option<([u8; 3], u8)> = None;
    let mut pos_opt: Option<f32> = None;

    for (i, tok) in parts.iter().enumerate() {
        let tl = tok.to_ascii_lowercase();
        if (tl.ends_with('%') || tl.ends_with("px")) && i > 0 {
            // Try as a position
            if tl.ends_with('%') {
                if let Ok(n) = tl.trim_end_matches('%').parse::<f32>() {
                    pos_opt = Some(n / 100.0);
                    continue;
                }
            } else if tl.ends_with("px") {
                // px stops are unusual in gradients; store as a sentinel > 1 to indicate "unknown"
                // We'll leave it as None and let resolve_stop_positions handle it.
                continue;
            }
        }
        if color_opt.is_none() {
            if let Some(c) = parse_color_alpha(tok) {
                color_opt = Some(c);
            }
        }
    }

    let (color, alpha) = color_opt?;
    Some(GradientStop { color, alpha, pos: pos_opt })
}

/// Distribute `None` positions evenly between their neighbours.
fn resolve_stop_positions(stops: &mut Vec<GradientStop>) {
    let n = stops.len();
    if n == 0 { return; }

    // First stop defaults to 0, last to 1.
    if stops[0].pos.is_none()     { stops[0].pos = Some(0.0); }
    if stops[n-1].pos.is_none() { stops[n-1].pos = Some(1.0); }

    // Fill in gaps by linear interpolation.
    let mut i = 0;
    while i < n {
        if stops[i].pos.is_none() {
            // Find the next defined stop.
            let mut j = i + 1;
            while j < n && stops[j].pos.is_none() { j += 1; }
            if j < n {
                let p0 = stops[i-1].pos.unwrap_or(0.0);
                let p1 = stops[j].pos.unwrap_or(1.0);
                let count = (j - i + 1) as f32;
                for k in i..j {
                    stops[k].pos = Some(p0 + (p1 - p0) * (k - i + 1) as f32 / count);
                }
            }
        }
        i += 1;
    }
}

/// Split gradient argument list on top-level commas (respecting parens).
fn split_gradient_args(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '(' => { depth += 1; cur.push(ch); }
            ')' => {
                if depth > 0 { depth -= 1; }
                cur.push(ch);
            }
            ',' if depth == 0 => {
                let t = cur.trim().to_string();
                if !t.is_empty() { result.push(t); }
                cur.clear();
            }
            _ => { cur.push(ch); }
        }
    }
    let t = cur.trim().to_string();
    if !t.is_empty() { result.push(t); }
    result
}

/// Extracts all `url(...)` contents from a string.
pub(crate) fn extract_css_urls(val: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let lower = val.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(start) = lower[pos..].find("url(") {
        let after_open = pos + start + 4;
        let mut depth = 1usize;
        let mut i = after_open;
        let bytes = val.as_bytes();
        while i < bytes.len() {
            match bytes[i] as char {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        let inner = &val[after_open..i];
                        let clean = inner.trim_matches(|c| c == '"' || c == '\'').trim();
                        if !clean.is_empty() {
                            urls.push(clean.to_string());
                        }
                        pos = i + 1;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        if i >= bytes.len() { break; }
    }
    urls
}
