use std::fs;
use std::path::Path;

use crate::dom::node::Node;
use crate::dom::parser::{parse, extract_page_meta};
use crate::net;

/// Page metadata extracted from the document head.
pub struct PageMeta {
    pub title:       String,
    pub favicon_url: Option<String>,
}

/// Load an HTML document from a local file path, a `file://` URI, or an
/// `http(s)://` URL.  Returns `(resolved_url, Node, PageMeta)` or `None` on
/// hard error.
pub fn load_dom(source: &str) -> Option<(String, Node, PageMeta)> {
    let (final_url, html) = fetch_html(source)?;
    let (title, favicon_url) = extract_page_meta(&html, &final_url);
    let dom  = parse(&html, &final_url);
    let meta = PageMeta { title, favicon_url };
    Some((final_url, dom, meta))
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
