/// CSS4 Color Support for Forkit.
/// 
/// This module implements parsing for modern CSS color syntax, including:
/// - Hex: #RGB, #RGBA, #RRGGBB, #RRGGBBAA
/// - Functional: rgb(), rgba(), hsl(), hsla(), hwb()
/// - Advanced: lab(), lch(), oklab(), oklch()
/// - Relative: color-mix()
/// - Named colors

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CssColor {
    pub r: f32, // 0.0 to 1.0
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl CssColor {
    pub const TRANSPARENT: CssColor = CssColor { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const BLACK:       CssColor = CssColor { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE:       CssColor = CssColor { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const RED:         CssColor = CssColor { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const GREEN:       CssColor = CssColor { r: 0.0, g: 0.5, b: 0.0, a: 1.0 };
    pub const LIME:        CssColor = CssColor { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
    pub const BLUE:        CssColor = CssColor { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };

    pub fn to_rgba8(&self) -> (u8, u8, u8, u8) {
        (
            (self.r * 255.0 + 0.5) as u8,
            (self.g * 255.0 + 0.5) as u8,
            (self.b * 255.0 + 0.5) as u8,
            (self.a * 255.0 + 0.5) as u8,
        )
    }

    /// Parse a CSS color string.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        if s.is_empty() { return None; }

        if s.starts_with('#') {
            return parse_hex(&s);
        }

        if s.starts_with("rgb") {
            return parse_func_rgb(&s);
        }

        if s.starts_with("hsl") {
            return parse_func_hsl(&s);
        }

        if s.starts_with("hwb") {
            return parse_func_hwb(&s);
        }

        if s.starts_with("lab") {
            return parse_func_lab(&s);
        }

        if s.starts_with("lch") {
            return parse_func_lch(&s);
        }

        if s.starts_with("oklab") {
            return parse_func_oklab(&s);
        }

        if s.starts_with("oklch") {
            return parse_func_oklch(&s);
        }

        if s.starts_with("color-mix") {
            return parse_color_mix(&s);
        }

        // Named colors
        parse_named_color(&s)
    }
}

// ---------------------------------------------------------------------------
// Hex Parsing
// ---------------------------------------------------------------------------

fn parse_hex(s: &str) -> Option<CssColor> {
    let hex = &s[1..];
    let len = hex.len();
    
    if len == 3 {
        let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()? as f32 / 255.0;
        return Some(CssColor { r, g, b, a: 1.0 });
    }
    
    if len == 4 {
        let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()? as f32 / 255.0;
        let a = u8::from_str_radix(&hex[3..4].repeat(2), 16).ok()? as f32 / 255.0;
        return Some(CssColor { r, g, b, a });
    }
    
    if len == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
        return Some(CssColor { r, g, b, a: 1.0 });
    }
    
    if len == 8 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
        let a = u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0;
        return Some(CssColor { r, g, b, a });
    }

    None
}

// ---------------------------------------------------------------------------
// Functional Parsing (Modern space-separated syntax)
// ---------------------------------------------------------------------------

fn parse_func_rgb(s: &str) -> Option<CssColor> {
    let inner = extract_inner(s, "rgb")?;
    let parts = split_parts(&inner);
    
    if parts.len() < 3 { return None; }
    
    let r = parse_unit(&parts[0], 255.0)?;
    let g = parse_unit(&parts[1], 255.0)?;
    let b = parse_unit(&parts[2], 255.0)?;
    let a = if parts.len() > 3 { parse_unit(&parts[3], 1.0)? } else { 1.0 };
    
    Some(CssColor { r, g, b, a })
}

fn parse_func_hsl(s: &str) -> Option<CssColor> {
    let inner = extract_inner(s, "hsl")?;
    let parts = split_parts(&inner);
    
    if parts.len() < 3 { return None; }
    
    let h = parse_angle(&parts[0])?;
    let s_val = parse_unit(&parts[1], 100.0)?;
    let l = parse_unit(&parts[2], 100.0)?;
    let a = if parts.len() > 3 { parse_unit(&parts[3], 1.0)? } else { 1.0 };
    
    let (r, g, b) = hsl_to_rgb(h, s_val, l);
    Some(CssColor { r, g, b, a })
}

fn parse_func_hwb(s: &str) -> Option<CssColor> {
    let inner = extract_inner(s, "hwb")?;
    let parts = split_parts(&inner);
    
    if parts.len() < 3 { return None; }
    
    let h = parse_angle(&parts[0])?;
    let w = parse_unit(&parts[1], 100.0)?;
    let b_val = parse_unit(&parts[2], 100.0)?;
    let a = if parts.len() > 3 { parse_unit(&parts[3], 1.0)? } else { 1.0 };
    
    let (r, g, b) = hwb_to_rgb(h, w, b_val);
    Some(CssColor { r, g, b, a })
}

fn parse_func_lab(s: &str) -> Option<CssColor> {
    let inner = extract_inner(s, "lab")?;
    let parts = split_parts(&inner);
    if parts.len() < 3 { return None; }
    
    let l = parse_unit(&parts[0], 100.0)?;
    let a_val = parse_float(&parts[1])?;
    let b_val = parse_float(&parts[2])?;
    let alpha = if parts.len() > 3 { parse_unit(&parts[3], 1.0)? } else { 1.0 };
    
    let (r, g, b) = lab_to_rgb(l * 100.0, a_val, b_val);
    Some(CssColor { r, g, b, a: alpha })
}

fn parse_func_lch(s: &str) -> Option<CssColor> {
    let inner = extract_inner(s, "lch")?;
    let parts = split_parts(&inner);
    if parts.len() < 3 { return None; }
    
    let l = parse_unit(&parts[0], 100.0)?;
    let c = parse_float(&parts[1])?;
    let h = parse_angle(&parts[2])?;
    let alpha = if parts.len() > 3 { parse_unit(&parts[3], 1.0)? } else { 1.0 };
    
    let (r, g, b) = lch_to_rgb(l * 100.0, c, h);
    Some(CssColor { r, g, b, a: alpha })
}

fn parse_func_oklab(s: &str) -> Option<CssColor> {
    let inner = extract_inner(s, "oklab")?;
    let parts = split_parts(&inner);
    if parts.len() < 3 { return None; }
    
    let l = parse_unit(&parts[0], 1.0)?;
    let a_val = parse_float(&parts[1])?;
    let b_val = parse_float(&parts[2])?;
    let alpha = if parts.len() > 3 { parse_unit(&parts[3], 1.0)? } else { 1.0 };
    
    let (r, g, b) = oklab_to_rgb(l, a_val, b_val);
    Some(CssColor { r, g, b, a: alpha })
}

fn parse_func_oklch(s: &str) -> Option<CssColor> {
    let inner = extract_inner(s, "oklch")?;
    let parts = split_parts(&inner);
    if parts.len() < 3 { return None; }
    
    let l = parse_unit(&parts[0], 1.0)?;
    let c = parse_float(&parts[1])?;
    let h = parse_angle(&parts[2])?;
    let alpha = if parts.len() > 3 { parse_unit(&parts[3], 1.0)? } else { 1.0 };
    
    let (r, g, b) = oklch_to_rgb(l, c, h);
    Some(CssColor { r, g, b, a: alpha })
}

fn parse_color_mix(s: &str) -> Option<CssColor> {
    // color-mix(in srgb, red 40%, blue)
    // Very simplified parser for now
    let inner = extract_inner(s, "color-mix")?;
    let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
    if parts.len() < 2 { return None; }
    
    // First part is "in <space>"
    // Second and third are colors with optional percentages
    let c1_part = parts.get(1)?;
    let c2_part = parts.get(2)?;
    
    let (c1, p1) = parse_color_with_percent(c1_part)?;
    let (c2, p2) = parse_color_with_percent(c2_part).unwrap_or((CssColor::TRANSPARENT, Some(1.0 - p1.unwrap_or(1.0))));
    
    let w1 = p1.unwrap_or(1.0 - p2.unwrap_or(0.5));
    let w2 = p2.unwrap_or(1.0 - w1);
    
    let total = w1 + w2;
    if total == 0.0 { return Some(CssColor::TRANSPARENT); }
    
    Some(CssColor {
        r: (c1.r * w1 + c2.r * w2) / total,
        g: (c1.g * w1 + c2.g * w2) / total,
        b: (c1.b * w1 + c2.b * w2) / total,
        a: (c1.a * w1 + c2.a * w2) / total,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_inner<'a>(s: &'a str, _prefix: &str) -> Option<&'a str> {
    let start = s.find('(')? + 1;
    let end = s.rfind(')')?;
    if end <= start { return None; }
    Some(&s[start..end])
}

fn split_parts(s: &str) -> Vec<String> {
    // Handle both comma-separated and space-separated
    // Also handle '/' for alpha
    let mut parts = Vec::new();
    let normalized = s.replace(',', " ").replace('/', " ");
    for p in normalized.split_whitespace() {
        parts.push(p.to_owned());
    }
    parts
}

fn parse_unit(s: &str, max: f32) -> Option<f32> {
    if let Some(val) = s.strip_suffix('%') {
        let p = val.parse::<f32>().ok()?;
        return Some(p / 100.0);
    }
    let val = s.parse::<f32>().ok()?;
    // For RGB, 255-based values are common
    if max > 1.0 {
        return Some(val / max);
    }
    Some(val)
}

fn parse_float(s: &str) -> Option<f32> {
    s.parse::<f32>().ok()
}

fn parse_angle(s: &str) -> Option<f32> {
    if let Some(val) = s.strip_suffix("deg") {
        return val.parse::<f32>().ok();
    }
    if let Some(val) = s.strip_suffix("grad") {
        return Some(val.parse::<f32>().ok()? * 0.9);
    }
    if let Some(val) = s.strip_suffix("rad") {
        return Some(val.parse::<f32>().ok()? * 180.0 / std::f32::consts::PI);
    }
    if let Some(val) = s.strip_suffix("turn") {
        return Some(val.parse::<f32>().ok()? * 360.0);
    }
    s.parse::<f32>().ok()
}

fn parse_color_with_percent(s: &str) -> Option<(CssColor, Option<f32>)> {
    let mut parts = s.split_whitespace();
    let color_name = parts.next()?;
    let color = CssColor::parse(color_name)?;
    let percent = parts.next().and_then(|p| p.strip_suffix('%')).and_then(|p| p.parse::<f32>().ok()).map(|p| p / 100.0);
    Some((color, percent))
}

// ---------------------------------------------------------------------------
// Math Conversions
// ---------------------------------------------------------------------------

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let h = (h % 360.0 + 360.0) % 360.0 / 360.0;
    if s == 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    (
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    )
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0 / 2.0 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}

fn hwb_to_rgb(h: f32, w: f32, b: f32) -> (f32, f32, f32) {
    let mut w = w;
    let mut b = b;
    if w + b >= 1.0 {
        let s = w + b;
        w /= s;
        b /= s;
        return (w, w, w);
    }
    let (r, g, b_val) = hsl_to_rgb(h, 1.0, 0.5);
    (
        r * (1.0 - w - b) + w,
        g * (1.0 - w - b) + w,
        b_val * (1.0 - w - b) + w,
    )
}

// Simple LAB/LCH to RGB conversion (D65)
fn lab_to_rgb(l: f32, a: f32, b: f32) -> (f32, f32, f32) {
    let y = (l + 16.0) / 116.0;
    let x = a / 500.0 + y;
    let z = y - b / 200.0;

    let x = if x.powi(3) > 0.008856 { x.powi(3) } else { (x - 16.0 / 116.0) / 7.787 };
    let y = if y.powi(3) > 0.008856 { y.powi(3) } else { (y - 16.0 / 116.0) / 7.787 };
    let z = if z.powi(3) > 0.008856 { z.powi(3) } else { (z - 16.0 / 116.0) / 7.787 };

    let x = x * 0.95047;
    let y = y * 1.00000;
    let z = z * 1.08883;

    let r = x *  3.2406 + y * -1.5372 + z * -0.4986;
    let g = x * -0.9689 + y *  1.8758 + z *  0.0415;
    let b = x *  0.0557 + y * -0.2040 + z *  1.0570;

    (clamp01(srgb_companding(r)), clamp01(srgb_companding(g)), clamp01(srgb_companding(b)))
}

fn lch_to_rgb(l: f32, c: f32, h: f32) -> (f32, f32, f32) {
    let h_rad = h * std::f32::consts::PI / 180.0;
    lab_to_rgb(l, c * h_rad.cos(), c * h_rad.sin())
}

// Oklab/Oklch are much better
fn oklab_to_rgb(l: f32, a: f32, b: f32) -> (f32, f32, f32) {
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;

    let l = l_.powi(3);
    let m = m_.powi(3);
    let s = s_.powi(3);

    let r =  4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;

    (clamp01(srgb_companding(r)), clamp01(srgb_companding(g)), clamp01(srgb_companding(b)))
}

fn oklch_to_rgb(l: f32, c: f32, h: f32) -> (f32, f32, f32) {
    let h_rad = h * std::f32::consts::PI / 180.0;
    oklab_to_rgb(l, c * h_rad.cos(), c * h_rad.sin())
}

fn srgb_companding(v: f32) -> f32 {
    if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

fn clamp01(v: f32) -> f32 {
    v.max(0.0).min(1.0)
}

// ---------------------------------------------------------------------------
// Named Colors
// ---------------------------------------------------------------------------

fn parse_named_color(s: &str) -> Option<CssColor> {
    match s {
        "transparent" => Some(CssColor::TRANSPARENT),
        "black"       => Some(CssColor::BLACK),
        "white"       => Some(CssColor::WHITE),
        "red"         => Some(CssColor::RED),
        "green"       => Some(CssColor::GREEN),
        "lime"        => Some(CssColor::LIME),
        "blue"        => Some(CssColor::BLUE),
        "gray" | "grey" => Some(CssColor { r: 0.5, g: 0.5, b: 0.5, a: 1.0 }),
        "silver"      => Some(CssColor { r: 0.75, g: 0.75, b: 0.75, a: 1.0 }),
        "gold"        => Some(CssColor { r: 1.0, g: 0.84, b: 0.0, a: 1.0 }),
        "orange"      => Some(CssColor { r: 1.0, g: 0.65, b: 0.0, a: 1.0 }),
        "purple"      => Some(CssColor { r: 0.5, g: 0.0, b: 0.5, a: 1.0 }),
        "pink"        => Some(CssColor { r: 1.0, g: 0.75, b: 0.8, a: 1.0 }),
        "rebeccapurple" => Some(CssColor { r: 0.4, g: 0.2, b: 0.6, a: 1.0 }),
        _ => None,
    }
}
