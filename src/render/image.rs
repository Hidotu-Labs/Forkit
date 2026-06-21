/// Image cache — fetches images by URL (or local path) and caches the raw
/// bytes so repeated renders of the same `<img src="...">` don't re-fetch.
///
/// Actual SDL2 texture creation is done at render time because textures are
/// bound to a specific `TextureCreator` which has a frame lifetime.

use std::collections::HashMap;

/// Raw image bytes keyed by URL string.
pub struct ImageCache {
    bytes: HashMap<String, Option<Vec<u8>>>,
}

impl ImageCache {
    pub fn new() -> Self {
        ImageCache { bytes: HashMap::new() }
    }

    /// Return the cached bytes for `url`, fetching if not yet seen.
    /// Returns `None` if the fetch failed or the URL is unsupported.
    pub fn get_bytes(&mut self, url: &str, base_url: &str) -> Option<&[u8]> {
        // Resolve the URL first so we cache by the resolved form
        let resolved = if url.starts_with("http://") || url.starts_with("https://")
            || url.starts_with("file://") || url.starts_with("data:")
        {
            url.to_owned()
        } else {
            crate::net::resolve_url(url, base_url)
        };

        if !self.bytes.contains_key(&resolved) {
            let data = fetch_image(&resolved, base_url);
            self.bytes.insert(resolved.clone(), data);
        }
        self.bytes.get(&resolved)?.as_deref()
    }
}

/// Detect image format from magic bytes. Returns a SDL2_image type string.
pub fn sniff_image_type(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 4 {
        if bytes.starts_with(b"\x89PNG") { return "PNG"; }
        if bytes[0] == 0xFF && bytes[1] == 0xD8 { return "JPG"; }
        if bytes.starts_with(b"GIF8") { return "GIF"; }
        if bytes.starts_with(b"BM") { return "BMP"; }
        if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
            return "WEBP";
        }
        if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" { return "AVIF"; }
        if bytes[0] == 0x00 && bytes[1] == 0x00 && bytes[2] == 0x01 { return "ICO"; }
    }
    // SVG: starts with XML declaration, BOM, or the <svg tag directly
    {
        // Skip any leading whitespace / BOM
        let trimmed = bytes.iter().position(|&b| !b.is_ascii_whitespace()).unwrap_or(0);
        let head = &bytes[trimmed..bytes.len().min(trimmed + 512)];
        // UTF-8 BOM
        let head = if head.starts_with(b"\xEF\xBB\xBF") { &head[3..] } else { head };
        let head_lc = head.iter().map(|&b| b.to_ascii_lowercase()).collect::<Vec<u8>>();
        if head_lc.starts_with(b"<svg")
            || head_lc.starts_with(b"<?xml")
            || head_lc.windows(4).any(|w| w == b"<svg")
        {
            return "SVG";
        }
    }
    "PNG" // fallback guess
}

/// Fetch image bytes from a URL or local path.
fn fetch_image(url: &str, base_url: &str) -> Option<Vec<u8>> {
    use crate::net;

    if let Some(rest) = url.strip_prefix("data:") {
        if let Some(b64) = rest.split(',').nth(1) {
            return decode_base64(b64);
        }
        return None;
    }

    let resolved = net::resolve_url(url, base_url);

    if resolved.starts_with("http://") || resolved.starts_with("https://") {
        match ureq::get(&resolved)
            .set("User-Agent", "Forkit/0.1 (Rust browser)")
            .call()
        {
            Ok(resp) => {
                let mut buf = Vec::new();
                use std::io::Read;
                resp.into_reader().read_to_end(&mut buf).ok()?;
                Some(buf)
            }
            Err(e) => {
                eprintln!("Image fetch {resolved}: {e}");
                None
            }
        }
    } else {
        std::fs::read(&resolved)
            .map_err(|e| eprintln!("Image read {resolved}: {e}"))
            .ok()
    }
}

/// Minimal base64 decoder (no padding validation, ignores whitespace).
fn decode_base64(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 128] = b"\
        \xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\
        \xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\
        \xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\x3e\xff\xff\xff\x3f\
        \x34\x35\x36\x37\x38\x39\x3a\x3b\x3c\x3d\xff\xff\xff\xff\xff\xff\
        \xff\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\
        \x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\xff\xff\xff\xff\xff\
        \xff\x1a\x1b\x1c\x1d\x1e\x1f\x20\x21\x22\x23\x24\x25\x26\x27\x28\
        \x29\x2a\x2b\x2c\x2d\x2e\x2f\x30\x31\x32\x33\xff\xff\xff\xff\xff";

    let mut out = Vec::new();
    let bytes: Vec<u8> = input.bytes()
        .filter(|&b| b != b'=' && !b.is_ascii_whitespace())
        .collect();

    for chunk in bytes.chunks(4) {
        let mut vals = [0u8; 4];
        let mut n = 0;
        for &b in chunk {
            if b as usize >= TABLE.len() { return None; }
            let v = TABLE[b as usize];
            if v == 0xff { return None; }
            vals[n] = v;
            n += 1;
        }
        if n < 2 { break; }
        out.push((vals[0] << 2) | (vals[1] >> 4));
        if n >= 3 { out.push((vals[1] << 4) | (vals[2] >> 2)); }
        if n >= 4 { out.push((vals[2] << 6) | vals[3]); }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// SVG pre-processing
// ---------------------------------------------------------------------------

/// Returns modified_svg_bytes.
pub fn preprocess_svg(bytes: &[u8]) -> (Vec<u8>, u8) {
    let src = match std::str::from_utf8(bytes) {
        Ok(s)  => s,
        Err(_) => return (bytes.to_vec(), 255),
    };

    let lower = src.to_ascii_lowercase();
    if !lower.contains("<style") {
        return (bytes.to_vec(), 255);
    }

    let css = extract_svg_style_css(src);
    if css.is_empty() {
        return (bytes.to_vec(), 255);
    }

    let rules = parse_svg_css_rules(&css);
    if rules.is_empty() {
        return (bytes.to_vec(), 255);
    }

    let rewritten = rewrite_svg_elements(src, &rules);
    (rewritten.into_bytes(), 255)
}



/// Extract the raw CSS text from all `<style …>…</style>` sections,
/// unwrapping CDATA markers if present.
fn extract_svg_style_css(svg: &str) -> String {
    let mut css = String::new();
    let lower   = svg.to_ascii_lowercase();
    let mut pos = 0;

    while let Some(rel) = lower[pos..].find("<style") {
        let abs  = pos + rel;
        let rest = &lower[abs..];
        // Find the closing '>' of the opening tag
        let tag_end = match rest.find('>') {
            Some(i) => abs + i + 1,
            None    => break,
        };
        // Find </style>
        let close_rel = match lower[tag_end..].find("</style") {
            Some(i) => i,
            None    => break,
        };
        let inner = &svg[tag_end..tag_end + close_rel];
        // Unwrap CDATA
        let inner = strip_cdata(inner);
        css.push_str(inner);
        css.push('\n');
        pos = tag_end + close_rel + 8; // skip past "</style>"
    }
    css
}

fn strip_cdata(s: &str) -> &str {
    let s = s.trim();
    let s = if s.starts_with("<![CDATA[") { &s[9..] } else { s };
    let s = if s.ends_with("]]>") { &s[..s.len()-3] } else { s };
    s.trim()
}

/// A parsed CSS rule from inside an SVG `<style>` block.
struct SvgCssRule {
    /// Tag names this rule applies to (e.g. `["polygon", "path"]`).
    /// Empty means wildcard (*).
    tags: Vec<String>,
    /// Raw declaration string, e.g. `"fill: rgba(255,255,255,0.035)"`.
    declarations: String,
}

fn parse_svg_css_rules(css: &str) -> Vec<SvgCssRule> {
    let mut rules = Vec::new();
    let mut pos   = 0;
    let bytes     = css.as_bytes();
    let len       = css.len();

    while pos < len {
        // Skip whitespace and comments
        skip_css_whitespace_and_comments(css, &mut pos);
        if pos >= len { break; }

        // Read selector up to '{'
        let sel_start = pos;
        while pos < len && bytes[pos] != b'{' { pos += 1; }
        if pos >= len { break; }
        let selector = css[sel_start..pos].trim().to_ascii_lowercase();
        pos += 1; // skip '{'

        // Read declarations up to '}'
        let decl_start = pos;
        let mut depth = 1usize;
        while pos < len {
            if bytes[pos] == b'{' { depth += 1; }
            else if bytes[pos] == b'}' { depth -= 1; if depth == 0 { break; } }
            pos += 1;
        }
        let declarations = css[decl_start..pos].trim().to_owned();
        if pos < len { pos += 1; } // skip '}'

        if declarations.is_empty() { continue; }

        // Parse comma-separated selector list into tag names
        let mut tags: Vec<String> = Vec::new();
        for part in selector.split(',') {
            let part = part.trim();
            if part == "*" || part.is_empty() {
                tags.clear();
                break; // wildcard — apply to all
            }
            // Only handle simple tag selectors (letters/hyphens)
            if part.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '_') {
                tags.push(part.to_owned());
            }
        }
        rules.push(SvgCssRule { tags, declarations });
    }
    rules
}

fn skip_css_whitespace_and_comments(css: &str, pos: &mut usize) {
    let bytes = css.as_bytes();
    let len   = css.len();
    loop {
        while *pos < len && bytes[*pos].is_ascii_whitespace() { *pos += 1; }
        if *pos + 1 < len && bytes[*pos] == b'/' && bytes[*pos + 1] == b'*' {
            *pos += 2;
            while *pos + 1 < len {
                if bytes[*pos] == b'*' && bytes[*pos + 1] == b'/' { *pos += 2; break; }
                *pos += 1;
            }
        } else {
            break;
        }
    }
}

/// Rewrite SVG source: strip `<style>` blocks and inject `style="…"` attrs
/// onto shape elements based on the parsed CSS rules.
fn rewrite_svg_elements(svg: &str, rules: &[SvgCssRule]) -> String {
    // Shape tags nanosvg renders
    const SHAPES: &[&str] = &[
        "polygon", "path", "rect", "circle", "ellipse", "line", "polyline", "use", "g",
    ];

    let lower = svg.to_ascii_lowercase();
    let mut out = String::with_capacity(svg.len() + 256);
    let mut pos = 0usize;
    let len     = svg.len();

    while pos < len {
        // Look for '<' 
        let next_lt = match svg[pos..].find('<') {
            Some(r) => pos + r,
            None    => {
                out.push_str(&svg[pos..]);
                break;
            }
        };
        out.push_str(&svg[pos..next_lt]);
        pos = next_lt;

        // Peek at the tag name
        let tag_start = pos + 1;
        let is_close = tag_start < len && svg.as_bytes()[tag_start] == b'/';
        let name_start = if is_close { tag_start + 1 } else { tag_start };

        // Read tag name (letters/hyphens)
        let mut name_end = name_start;
        while name_end < len {
            let b = svg.as_bytes()[name_end];
            if b.is_ascii_alphabetic() || b == b'-' || b == b'_' { name_end += 1; }
            else { break; }
        }
        let tag_name = lower[name_start..name_end.min(len)].to_owned();

        // Is this a <style …>…</style> block? Strip it entirely.
        if !is_close && tag_name == "style" {
            let tag_end = match svg[pos..].find('>') {
                Some(r) => pos + r + 1,
                None    => { out.push('<'); pos += 1; continue; }
            };
            // Find </style>
            let close = lower[tag_end..].find("</style")
                .map(|r| tag_end + r);
            match close {
                Some(cs) => {
                    let after_close = lower[cs..].find('>')
                        .map(|r| cs + r + 1)
                        .unwrap_or(cs + 8);
                    // Skip entirely — don't push anything
                    pos = after_close;
                }
                None => {
                    // Malformed — just skip the opening tag
                    pos = tag_end;
                }
            }
            continue;
        }

        // Is this a shape element we should inject style into?
        if !is_close && SHAPES.contains(&tag_name.as_str()) {
            // Collect applicable declarations from rules
            let mut fill_val:   Option<String> = None;
            let mut stroke_val: Option<String> = None;
            let mut stroke_w:   Option<String> = None;

            for rule in rules {
                let applies = rule.tags.is_empty()
                    || rule.tags.iter().any(|t| t == &tag_name);
                if applies {
                    // Parse declarations into individual properties
                    for decl in rule.declarations.split(';') {
                        let decl = decl.trim();
                        if decl.is_empty() { continue; }
                        if let Some(colon) = decl.find(':') {
                            let prop = decl[..colon].trim().to_ascii_lowercase();
                            let val  = decl[colon+1..].trim().to_owned();
                            match prop.as_str() {
                                "fill"         => { fill_val   = Some(val); }
                                "stroke"       => { stroke_val = Some(val); }
                                "stroke-width" => { stroke_w   = Some(val); }
                                _ => {}
                            }
                        }
                    }
                }
            }

            if fill_val.is_none() && stroke_val.is_none() {
                let tag_end = find_tag_end(svg, pos);
                out.push_str(&svg[pos..tag_end]);
                pos = tag_end;
            } else {
                let tag_end = find_tag_end(svg, pos);
                let tag_src = &svg[pos..tag_end];

                // Trim off the closing '>' or '/>'
                let is_self_close = tag_src.trim_end().ends_with("/>");
                let trimmed = if is_self_close {
                    tag_src.trim_end().trim_end_matches('>').trim_end_matches('/').trim_end()
                } else {
                    tag_src.trim_end().trim_end_matches('>').trim_end()
                };

                // Remove any existing fill=, stroke=, stroke-width= attrs to avoid conflicts
                let cleaned = remove_xml_attr(trimmed, "fill");
                let cleaned = remove_xml_attr(&cleaned, "stroke");
                let cleaned = remove_xml_attr(&cleaned, "stroke-width");

                out.push_str(&cleaned);
                if let Some(f) = &fill_val   { out.push_str(&format!(" fill=\"{}\"", f)); }
                if let Some(s) = &stroke_val { out.push_str(&format!(" stroke=\"{}\"", s)); }
                if let Some(w) = &stroke_w   { out.push_str(&format!(" stroke-width=\"{}\"", w)); }
                if is_self_close { out.push('/'); }
                out.push('>');
                pos = tag_end;
            }
            continue;
        }

        // Everything else — copy '<' and advance
        out.push('<');
        pos += 1;
    }

    out
}

/// Return the index just past the closing `>` of the tag starting at `pos`.
fn find_tag_end(svg: &str, pos: usize) -> usize {
    let bytes = svg.as_bytes();
    let len   = svg.len();
    let mut i = pos + 1;
    let mut in_quote: Option<u8> = None;
    while i < len {
        let b = bytes[i];
        match in_quote {
            Some(q) if b == q => { in_quote = None; }
            Some(_) => {}
            None => match b {
                b'"' | b'\'' => { in_quote = Some(b); }
                b'>' => { return i + 1; }
                _ => {}
            }
        }
        i += 1;
    }
    len
}

/// Remove an XML attribute (e.g. `fill="..."` or `fill='...'`) from a tag string.
fn remove_xml_attr(tag: &str, attr: &str) -> String {
    let lower  = tag.to_ascii_lowercase();
    let needle = format!("{}=", attr);
    match lower.find(&needle) {
        None => tag.to_owned(),
        Some(start) => {
            let after_eq = start + needle.len();
            if after_eq >= tag.len() { return tag.to_owned(); }
            let quote = tag.as_bytes()[after_eq];
            let end = if quote == b'"' || quote == b'\'' {
                tag[after_eq + 1..]
                    .find(quote as char)
                    .map(|i| after_eq + 1 + i + 1)
                    .unwrap_or(tag.len())
            } else {
                // unquoted — end at next whitespace or '>'
                tag[after_eq..].find(|c: char| c.is_ascii_whitespace() || c == '>')
                    .map(|i| after_eq + i)
                    .unwrap_or(tag.len())
            };
            // Also eat any leading whitespace before the attr
            let trim_start = if start > 0 && tag.as_bytes()[start - 1] == b' ' {
                start - 1
            } else {
                start
            };
            format!("{}{}", &tag[..trim_start], &tag[end..])
        }
    }
}
