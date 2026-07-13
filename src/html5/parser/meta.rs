use super::attr::get_attr;
use super::entities::decode_entities;

/// Values extracted from `<meta>` and `<base>` tags in the `<head>`.
#[derive(Debug, Default, Clone)]
pub struct HeadMeta {
    /// `<meta charset="…">` or `<meta http-equiv="Content-Type" content="…; charset=…">`.
    /// Lowercase, e.g. `"utf-8"`, `"iso-8859-1"`.  Empty string = not declared.
    pub charset: String,

    /// Parsed fields from `<meta name="viewport" content="…">`.
    /// `width=device-width` sets `width_device` = true.
    /// `initial-scale=N` is stored in `initial_scale`.
    pub viewport_width_device: bool,
    pub viewport_initial_scale: f32,

    /// `<base href="…">` — the document base URL for relative links.
    /// Empty string = not present.
    pub base_href: String,
}

/// Scan the raw HTML (bytes not yet decoded) for `<meta charset>`,
/// `<meta name="viewport">`, and `<base href>`.
///
/// Stops scanning once `</head>` is found so it stays cheap.
pub fn extract_head_meta(html: &str) -> HeadMeta {
    let mut meta = HeadMeta {
        viewport_initial_scale: 1.0,
        ..Default::default()
    };

    let lower = html.to_ascii_lowercase();

    // Only scan the <head> section.
    let head_end = lower.find("</head>").unwrap_or(lower.len());
    let head_lower = &lower[..head_end];
    let head_orig  = &html[..head_end];

    let mut pos = 0;
    while pos < head_lower.len() {
        // Find next '<'
        let Some(rel) = head_lower[pos..].find('<') else { break };
        let abs = pos + rel;

        let tag_end = head_lower[abs..].find('>').map(|p| abs + p + 1).unwrap_or(head_lower.len());
        let tag_lower = &head_lower[abs..tag_end];
        let tag_orig  = &head_orig[abs..tag_end.min(head_orig.len())];

        if tag_lower.starts_with("<meta") {
            // --- charset via <meta charset="…"> --------------------------------
            if let Some(cs) = get_attr(tag_lower, "charset") {
                if !cs.is_empty() && meta.charset.is_empty() {
                    meta.charset = cs.to_ascii_lowercase();
                }
            }

            // --- charset via <meta http-equiv="Content-Type" content="…"> ------
            let http_equiv = get_attr(tag_lower, "http-equiv").unwrap_or("").to_ascii_lowercase();
            if http_equiv == "content-type" {
                if let Some(content) = get_attr(tag_orig, "content") {
                    let cl = content.to_ascii_lowercase();
                    if let Some(cs_pos) = cl.find("charset=") {
                        let rest = &content[cs_pos + 8..].trim_matches(|c: char| c == '"' || c == '\'');
                        let end = rest.find(|c: char| c == ';' || c.is_ascii_whitespace() || c == '"' || c == '\'')
                            .unwrap_or(rest.len());
                        if meta.charset.is_empty() {
                            meta.charset = rest[..end].to_ascii_lowercase();
                        }
                    }
                }
            }

            // --- viewport -------------------------------------------------------
            let name = get_attr(tag_lower, "name").unwrap_or("").to_ascii_lowercase();
            if name == "viewport" {
                if let Some(content) = get_attr(tag_orig, "content") {
                    parse_viewport(content, &mut meta);
                }
            }
        } else if tag_lower.starts_with("<base") {
            // --- base href ------------------------------------------------------
            if let Some(href) = get_attr(tag_orig, "href") {
                if !href.is_empty() && meta.base_href.is_empty() {
                    meta.base_href = href.to_owned();
                }
            }
        }

        pos = tag_end;
    }

    meta
}

/// Parse `content="width=device-width, initial-scale=1"` style strings.
fn parse_viewport(content: &str, meta: &mut HeadMeta) {
    for part in content.split(',') {
        let part = part.trim();
        let mut kv = part.splitn(2, '=');
        let key = kv.next().unwrap_or("").trim().to_ascii_lowercase();
        let val = kv.next().unwrap_or("").trim();
        match key.as_str() {
            "width" => {
                if val.eq_ignore_ascii_case("device-width") {
                    meta.viewport_width_device = true;
                }
            }
            "initial-scale" => {
                if let Ok(f) = val.parse::<f32>() {
                    meta.viewport_initial_scale = f;
                }
            }
            _ => {}
        }
    }
}

/// Extract the page title and best-guess favicon URL from raw HTML.
pub fn extract_page_meta(html: &str, base_url: &str) -> (String, Option<String>) {
    let lower = html.to_ascii_lowercase();
    let title   = find_title(html, &lower);
    let favicon = find_favicon(&lower, html, base_url);
    (title, favicon)
}

fn find_title(html: &str, lower: &str) -> String {
    let open = match lower.find("<title") {
        Some(p) => p,
        None    => return String::new(),
    };
    let after_open = match lower[open..].find('>') {
        Some(p) => open + p + 1,
        None    => return String::new(),
    };
    let close = match lower[after_open..].find("</title") {
        Some(p) => after_open + p,
        None    => return String::new(),
    };
    let raw     = &html[after_open..close];
    let decoded = decode_entities(raw);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_favicon(lower: &str, html: &str, base_url: &str) -> Option<String> {
    let mut best: Option<String> = None;
    let mut pos = 0;

    while let Some(rel_start) = lower[pos..].find("<link") {
        let abs     = pos + rel_start;
        let tag_end = lower[abs..].find('>').map(|p| abs + p + 1).unwrap_or(lower.len());
        let tag_lower = &lower[abs..tag_end];
        let tag_orig  = &html[abs..tag_end.min(html.len())];

        let rel = get_attr(tag_lower, "rel").unwrap_or("").to_ascii_lowercase();
        if rel.contains("icon") {
            if let Some(href) = get_attr(tag_orig, "href") {
                if !href.is_empty() {
                    let resolved = crate::net::resolve_url(href, base_url);
                    if best.is_none() || rel == "icon" || rel == "shortcut icon" {
                        best = Some(resolved);
                    }
                }
            }
        }

        pos = tag_end;
        if pos >= lower.len() { break; }
    }

    if best.is_none() {
        let origin = if let Some(p) = base_url.find("://") {
            let rest = &base_url[p + 3..];
            let end  = rest.find('/').unwrap_or(rest.len());
            format!("{}://{}", &base_url[..p], &rest[..end])
        } else {
            String::new()
        };
        if !origin.is_empty() {
            best = Some(format!("{}/favicon.ico", origin));
        }
    }

    best
}
