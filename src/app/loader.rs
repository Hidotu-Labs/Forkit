use std::fs;
use std::path::Path;

use crate::dom::node::Node;
use crate::dom::parser::{parse_with_sheets, extract_page_meta};
use crate::dom::css::StyleSheet;
use crate::js;
use crate::net;

/// Page metadata extracted from the document head.
#[derive(Debug)]
pub struct PageMeta {
    pub title:       String,
    pub favicon_url: Option<String>,
}

/// Load an HTML document from a local file path, a `file://` URI, or an
/// `http(s)://` URL.  Returns `(resolved_url, Node, PageMeta, Vec<StyleSheet>)` or `None` on
/// hard error.
pub fn load_dom(source: &str) -> Option<(String, Node, PageMeta, Vec<StyleSheet>)> {
    let (final_url, html) = fetch_html(source)?;
    let (title, favicon_url) = extract_page_meta(&html, &final_url);
    let (dom, sheets)  = crate::dom::parser::parse_with_sheets(&html, &final_url);
    let meta = PageMeta { title, favicon_url };

    // Run inline + external <script> blocks — output goes to the terminal only.
    for (label, src) in extract_scripts(&html, &final_url) {
        for entry in js::execute(&src) {
            match entry.level {
                js::ConsoleLevel::Log  => println!("[console.log]  ({label}) {}", entry.message),
                js::ConsoleLevel::Warn => eprintln!("[console.warn] ({label}) {}", entry.message),
            }
        }
    }

    Some((final_url, dom, meta, sheets))
}

fn fetch_html(source: &str) -> Option<(String, String)> {
    // about:blank — return an empty page immediately
    if source == "about:blank" || source == "about:newtab" {
        return Some((source.to_owned(), "<html><body></body></html>".to_owned()));
    }

    if source.starts_with("http://") || source.starts_with("https://") {
        match net::fetch_with_auto_https(source) {
            Ok((url, body)) => Some((url, body)),
            Err(e) => {
                eprintln!("Fetch error for {}: {}", source, e);
                let error_page = format!(
                    "<html><body>\
                     <h2>Could not load page</h2>\
                     <p><code>{}</code></p>\
                     </body></html>",
                    e
                );
                Some((source.to_owned(), error_page))
            }
        }
    } else if source.starts_with("file://") {
        let path = source.trim_start_matches("file://");
        let body = fs::read_to_string(Path::new(path))
            .map_err(|e| eprintln!("Cannot open {}: {}", path, e))
            .ok()?;
        Some((source.to_owned(), body))
    } else {
        // Plain filesystem path
        let body = fs::read_to_string(Path::new(source))
            .map_err(|e| eprintln!("Cannot open {}: {}", source, e))
            .ok()?;
        Some((source.to_owned(), body))
    }
}

// ---------------------------------------------------------------------------
// Script extraction
// ---------------------------------------------------------------------------

/// Pull script source from every `<script>` element in raw HTML.
/// - Inline scripts: text content extracted directly.
/// - External scripts (`src="..."`): fetched over the network (up to
///   `MAX_EXTERNAL_SCRIPTS` to avoid hanging on heavy pages).
///
/// Returns a list of `(label, source)` pairs where `label` is the script
/// origin (URL or `"<inline>"`) for diagnostic output.
const MAX_EXTERNAL_SCRIPTS: usize = 10;

fn extract_scripts(html: &str, base_url: &str) -> Vec<(String, String)> {
    let mut scripts  = Vec::new();
    let lower        = html.to_ascii_lowercase();
    let mut pos      = 0;
    let mut ext_count = 0;

    while let Some(rel) = lower[pos..].find("<script") {
        let abs = pos + rel;

        // Find end of opening tag
        let tag_end = match lower[abs..].find('>') {
            Some(p) => abs + p + 1,
            None    => break,
        };

        let tag_lower = &lower[abs..tag_end];
        let tag_orig  = &html [abs..tag_end.min(html.len())];

        // External script — fetch it
        if tag_lower.contains("src=") {
            if ext_count < MAX_EXTERNAL_SCRIPTS {
                if let Some(src) = crate::dom::parser::get_attr(tag_orig, "src") {
                    if !src.is_empty() {
                        let url = crate::net::resolve_url(src, base_url);
                        match crate::net::fetch_url(&url) {
                            Ok((_, body)) => {
                                scripts.push((url, body));
                                ext_count += 1;
                            }
                            Err(e) => eprintln!("[js] fetch {url}: {e}"),
                        }
                    }
                }
            }
            // Find </script> to advance pos correctly
            let close_tag = lower[tag_end..].find("</script")
                .map(|p| tag_end + p + "</script>".len())
                .unwrap_or(tag_end);
            pos = close_tag;
            if pos >= lower.len() { break; }
            continue;
        }

        // Inline script
        let close = match lower[tag_end..].find("</script") {
            Some(p) => tag_end + p,
            None    => break,
        };

        let text = &html[tag_end..close];
        if !text.trim().is_empty() {
            scripts.push(("<inline>".to_owned(), text.to_owned()));
        }

        pos = close + "</script>".len();
        if pos >= lower.len() { break; }
    }

    scripts
}
