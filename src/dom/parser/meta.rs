use super::attr::get_attr;
use super::entities::decode_entities;

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
