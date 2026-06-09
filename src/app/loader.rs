use std::fs;
use std::path::Path;

use crate::dom::node::Node;
use crate::dom::parser::parse;
use crate::net;

/// Load an HTML document from a local file path, a `file://` URI, or an
/// `http(s)://` URL.  Returns `(resolved_url, Node)` or `None` on hard error.
pub fn load_dom(source: &str) -> Option<(String, Node)> {
    let (final_url, html) = fetch_html(source)?;
    Some((final_url.clone(), parse(&html, &final_url)))
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
