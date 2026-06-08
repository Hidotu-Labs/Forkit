use super::node::{Style, Element};

// ---------------------------------------------------------------------------
// Color parsing
// ---------------------------------------------------------------------------

/// Parse a CSS color string into an RGB triple.
/// Supports: named colors, `#rrggbb`, `#rgb`, `rgb(r,g,b)`.
pub fn parse_color(val: &str) -> Option<[u8; 3]> {
    let v = val.trim();

    // Named colors
    let named: &[(&str, [u8; 3])] = &[
        ("black",   [0,   0,   0  ]), ("white",   [255, 255, 255]),
        ("red",     [255, 0,   0  ]), ("green",   [0,   128, 0  ]),
        ("blue",    [0,   0,   255]), ("yellow",  [255, 255, 0  ]),
        ("orange",  [255, 165, 0  ]), ("purple",  [128, 0,   128]),
        ("gray",    [128, 128, 128]), ("grey",    [128, 128, 128]),
        ("silver",  [192, 192, 192]), ("navy",    [0,   0,   128]),
        ("teal",    [0,   128, 128]), ("maroon",  [128, 0,   0  ]),
        ("lime",    [0,   255, 0  ]), ("cyan",    [0,   255, 255]),
        ("magenta", [255, 0,   255]), ("pink",    [255, 192, 203]),
    ];
    for (name, rgb) in named {
        if v.eq_ignore_ascii_case(name) {
            return Some(*rgb);
        }
    }

    // #rrggbb or #rgb
    if let Some(hex_str) = v.strip_prefix('#') {
        let hex = u32::from_str_radix(hex_str, 16).ok()?;
        return match hex_str.len() {
            6 => Some([
                ((hex >> 16) & 0xff) as u8,
                ((hex >>  8) & 0xff) as u8,
                ( hex        & 0xff) as u8,
            ]),
            3 => {
                let r = ((hex >> 8) & 0xf) as u8;
                let g = ((hex >> 4) & 0xf) as u8;
                let b = ( hex       & 0xf) as u8;
                Some([r | (r << 4), g | (g << 4), b | (b << 4)])
            }
            _ => None,
        };
    }

    // rgb(r,g,b)
    if let Some(inner) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            return Some([r, g, b]);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Inline style application
// ---------------------------------------------------------------------------

/// Apply a `"property:value;…"` inline style string to a `Style`.
pub fn apply_inline(css: &str, s: &mut Style) {
    for decl in css.split(';') {
        let decl = decl.trim();
        if decl.is_empty() { continue; }
        if let Some(colon) = decl.find(':') {
            let prop = decl[..colon].trim().to_ascii_lowercase();
            let val  = decl[colon + 1..].trim();
            match prop.as_str() {
                "color" => {
                    if let Some(rgb) = parse_color(val) { s.color = rgb; }
                }
                "background-color" | "background" => {
                    s.bg_color = parse_color(val);
                }
                "font-size" => {
                    // strip "px", "pt", etc.
                    let digits: String = val.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(n) = digits.parse::<u16>() { s.font_size = n; }
                }
                "font-weight" => {
                    s.bold = val.eq_ignore_ascii_case("bold");
                }
                "font-style" => {
                    s.italic = val.eq_ignore_ascii_case("italic");
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// UA (browser default) stylesheet
// ---------------------------------------------------------------------------

const BLOCK_TAGS: &[&str] = &[
    "div", "p", "h1", "h2", "h3", "h4", "h5", "h6",
    "ul", "ol", "li", "nav", "header", "footer", "main",
    "section", "article", "body", "html", "blockquote", "pre",
];

/// Apply browser-default styles for a tag to an `Element`.
pub fn apply_tag_defaults(el: &mut Element) {
    let t = el.tag.as_str();
    let s = &mut el.style;

    if BLOCK_TAGS.contains(&t) {
        s.display_block = true;
    }

    match t {
        "h1" => { s.font_size = 32; s.bold = true; }
        "h2" => { s.font_size = 26; s.bold = true; }
        "h3" => { s.font_size = 22; s.bold = true; }
        "h4" => { s.font_size = 18; s.bold = true; }
        "h5" | "h6" => { s.font_size = 16; s.bold = true; }
        "b" | "strong" => { s.bold = true; }
        "i" | "em"     => { s.italic = true; }
        "small"        => { s.font_size = 12; }
        "a"            => { s.color = [0, 102, 204]; }
        _ => {}
    }
}
