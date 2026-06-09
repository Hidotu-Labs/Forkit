/// Parse a CSS color string → RGB triple.
/// Supports named colors, `#rrggbb`, `#rgb`, `#rrggbbaa` (alpha ignored),
/// `rgb(r,g,b)`, `rgba(r,g,b,a)` (alpha ignored), `hsl(h,s%,l%)`.
pub fn parse_color(val: &str) -> Option<[u8; 3]> {
    let v = val.trim();

    // ---- named colors ----
    const NAMED: &[(&str, [u8; 3])] = &[
        ("transparent",      [255,255,255]),
        ("black",            [  0,  0,  0]), ("white",         [255,255,255]),
        ("red",              [255,  0,  0]), ("green",         [  0,128,  0]),
        ("blue",             [  0,  0,255]), ("yellow",        [255,255,  0]),
        ("orange",           [255,165,  0]), ("purple",        [128,  0,128]),
        ("gray",             [128,128,128]), ("grey",          [128,128,128]),
        ("silver",           [192,192,192]), ("navy",          [  0,  0,128]),
        ("teal",             [  0,128,128]), ("maroon",        [128,  0,  0]),
        ("lime",             [  0,255,  0]), ("cyan",          [  0,255,255]),
        ("aqua",             [  0,255,255]), ("fuchsia",       [255,  0,255]),
        ("magenta",          [255,  0,255]), ("pink",          [255,192,203]),
        ("hotpink",          [255,105,180]), ("deeppink",      [255, 20,147]),
        ("coral",            [255,127, 80]), ("salmon",        [250,128,114]),
        ("tomato",           [255, 99, 71]), ("orangered",     [255, 69,  0]),
        ("gold",             [255,215,  0]), ("khaki",         [240,230,140]),
        ("violet",           [238,130,238]), ("indigo",        [ 75,  0,130]),
        ("brown",            [165, 42, 42]), ("sienna",        [160, 82, 45]),
        ("chocolate",        [210,105, 30]), ("peru",          [205,133, 63]),
        ("tan",              [210,180,140]), ("beige",         [245,245,220]),
        ("ivory",            [255,255,240]), ("lavender",      [230,230,250]),
        ("aliceblue",        [240,248,255]), ("mintcream",     [245,255,250]),
        ("honeydew",         [240,255,240]), ("azure",         [240,255,255]),
        ("lightgray",        [211,211,211]), ("lightgrey",     [211,211,211]),
        ("darkgray",         [169,169,169]), ("darkgrey",      [169,169,169]),
        ("dimgray",          [105,105,105]), ("dimgrey",       [105,105,105]),
        ("slategray",        [112,128,144]), ("lightblue",     [173,216,230]),
        ("skyblue",          [135,206,235]), ("deepskyblue",   [  0,191,255]),
        ("dodgerblue",       [ 30,144,255]), ("steelblue",     [ 70,130,180]),
        ("royalblue",        [ 65,105,225]), ("midnightblue",  [ 25, 25,112]),
        ("lightgreen",       [144,238,144]), ("limegreen",     [ 50,205, 50]),
        ("mediumgreen",      [ 60,179,113]), ("darkgreen",     [  0,100,  0]),
        ("olive",            [128,128,  0]), ("olivedrab",     [107,142, 35]),
        ("yellowgreen",      [154,205, 50]), ("chartreuse",    [127,255,  0]),
        ("springgreen",      [  0,255,127]), ("turquoise",     [ 64,224,208]),
        ("mediumturquoise",  [ 72,209,204]), ("darkturquoise", [  0,206,209]),
        ("lightcoral",       [240,128,128]), ("indianred",     [205, 92, 92]),
        ("crimson",          [220, 20, 60]), ("firebrick",     [178, 34, 34]),
        ("darkred",          [139,  0,  0]), ("lightpink",     [255,182,193]),
        ("mediumpurple",     [147,112,219]), ("blueviolet",    [138, 43,226]),
        ("darkviolet",       [148,  0,211]), ("darkorchid",    [153, 50,204]),
        ("mediumorchid",     [186, 85,211]), ("plum",          [221,160,221]),
        ("thistle",          [216,191,216]), ("wheat",         [245,222,179]),
        ("moccasin",         [255,228,181]), ("bisque",        [255,228,196]),
        ("linen",            [250,240,230]), ("snow",          [255,250,250]),
        ("ghostwhite",       [248,248,255]), ("whitesmoke",    [245,245,245]),
        ("seashell",         [255,245,238]), ("floralwhite",   [255,250,240]),
        ("oldlace",          [253,245,230]), ("antiquewhite",  [250,235,215]),
        ("papayawhip",       [255,239,213]), ("blanchedalmond",[255,235,205]),
        ("peachpuff",        [255,218,185]), ("navajowhite",   [255,222,173]),
        ("mistyrose",        [255,228,225]), ("lightyellow",   [255,255,224]),
        ("cornsilk",         [255,248,220]), ("lemonchiffon",  [255,250,205]),
        ("lightcyan",        [224,255,255]), ("paleturquoise", [175,238,238]),
        ("palegreen",        [152,251,152]), ("lightsteelblue",[176,196,222]),
        ("powderblue",       [176,224,230]), ("cadetblue",     [ 95,158,160]),
        ("darkcyan",         [  0,139,139]), ("darkslategray", [ 47, 79, 79]),
        ("darkslategrey",    [ 47, 79, 79]), ("slateblue",     [106, 90,205]),
        ("mediumslateblue",  [123,104,238]),
    ];
    for (name, rgb) in NAMED {
        if v.eq_ignore_ascii_case(name) { return Some(*rgb); }
    }

    // ---- hex ----
    if let Some(hex_str) = v.strip_prefix('#') {
        let hex = u64::from_str_radix(hex_str, 16).ok()?;
        return match hex_str.len() {
            8 => Some([
                ((hex >> 24) & 0xff) as u8,
                ((hex >> 16) & 0xff) as u8,
                ((hex >>  8) & 0xff) as u8,
            ]),
            6 => Some([
                ((hex >> 16) & 0xff) as u8,
                ((hex >>  8) & 0xff) as u8,
                ( hex        & 0xff) as u8,
            ]),
            3 => {
                let r = ((hex >> 8) & 0xf) as u8;
                let g = ((hex >> 4) & 0xf) as u8;
                let b = ( hex       & 0xf) as u8;
                Some([r|(r<<4), g|(g<<4), b|(b<<4)])
            }
            _ => None,
        };
    }

    let lower = v.to_ascii_lowercase();

    // ---- rgb / rgba ----
    if let Some(inner) = lower.strip_prefix("rgba(").or_else(|| lower.strip_prefix("rgb(")) {
        let inner = inner.trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() >= 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            return Some([r, g, b]);
        }
    }

    // ---- hsl ----
    if let Some(inner) = lower.strip_prefix("hsl(") {
        let inner = inner.trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let h = parts[0].trim().parse::<f32>().ok()?;
            let s = parts[1].trim().trim_end_matches('%').parse::<f32>().ok()? / 100.0;
            let l = parts[2].trim().trim_end_matches('%').parse::<f32>().ok()? / 100.0;
            return Some(hsl_to_rgb(h, s, l));
        }
    }

    None
}

/// Parse a CSS color string → (RGB triple, alpha u8).
/// Handles `rgba(r,g,b,a)` and `hsla(h,s%,l%,a)` with a [0.0,1.0] alpha.
/// For all other formats, delegates to `parse_color` and returns alpha 255.
pub fn parse_color_alpha(val: &str) -> Option<([u8; 3], u8)> {
    let v = val.trim();
    let lower = v.to_ascii_lowercase();

    // rgba(r, g, b, a)
    if let Some(inner) = lower.strip_prefix("rgba(") {
        let inner = inner.trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 4 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            let a = parts[3].trim().parse::<f32>().ok()?;
            let alpha = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
            return Some(([r, g, b], alpha));
        }
    }

    // hsla(h, s%, l%, a)
    if let Some(inner) = lower.strip_prefix("hsla(") {
        let inner = inner.trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 4 {
            let h = parts[0].trim().parse::<f32>().ok()?;
            let s = parts[1].trim().trim_end_matches('%').parse::<f32>().ok()? / 100.0;
            let l = parts[2].trim().trim_end_matches('%').parse::<f32>().ok()? / 100.0;
            let a = parts[3].trim().parse::<f32>().ok()?;
            let rgb = hsl_to_rgb(h, s, l);
            let alpha = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
            return Some((rgb, alpha));
        }
    }

    // Fall through: use parse_color, return alpha 255
    parse_color(v).map(|rgb| (rgb, 255))
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h as u32 {
        0..=59   => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179=> (0.0, c, x),
        180..=239=> (0.0, x, c),
        240..=299=> (x, 0.0, c),
        _        => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_fully_transparent() {
        let (rgb, a) = parse_color_alpha("rgba(0,0,0,0)").unwrap();
        assert_eq!(rgb, [0, 0, 0]);
        assert_eq!(a, 0);
    }

    #[test]
    fn rgba_fully_opaque() {
        let (rgb, a) = parse_color_alpha("rgba(255,0,0,1.0)").unwrap();
        assert_eq!(rgb, [255, 0, 0]);
        assert_eq!(a, 255);
    }

    #[test]
    fn rgba_half_alpha() {
        let (_, a) = parse_color_alpha("rgba(0,0,0,0.5)").unwrap();
        assert_eq!(a, 128);
    }

    #[test]
    fn hsla_fully_opaque() {
        let (rgb, a) = parse_color_alpha("hsla(0, 100%, 50%, 1.0)").unwrap();
        assert_eq!(rgb, [255, 0, 0]);
        assert_eq!(a, 255);
    }

    #[test]
    fn named_color_fallthrough() {
        let (rgb, a) = parse_color_alpha("red").unwrap();
        assert_eq!(rgb, [255, 0, 0]);
        assert_eq!(a, 255);
    }

    #[test]
    fn hex_color_fallthrough() {
        let (rgb, a) = parse_color_alpha("#ff0000").unwrap();
        assert_eq!(rgb, [255, 0, 0]);
        assert_eq!(a, 255);
    }
}
