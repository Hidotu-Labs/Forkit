use std::fs;
use std::path::Path;

use crate::html5::node::Node;
use crate::html5::parser::{parse_dom, extract_page_meta, extract_head_meta};
use crate::js;
use crate::net;

pub use crate::js::{ConsoleEntry, ConsoleLevel};

#[derive(Debug)]
pub struct PageMeta {
    pub title:       String,
    pub favicon_url: Option<String>,
    /// True when `<meta name="viewport" content="width=device-width, …">` is present.
    pub viewport_width_device: bool,
    /// `initial-scale` from the viewport meta, defaults to 1.0.
    pub viewport_initial_scale: f32,
}

pub fn load_dom(source: &str) -> Option<(String, Node, PageMeta, Vec<ConsoleEntry>, crate::js::scope::Scope, Vec<super::browser::JsTimer>)> {
    let (final_url, html) = fetch_html(source)?;

    // --- Parse <meta charset>, <meta viewport>, <base href> from the HTML ----
    let head_meta = extract_head_meta(&html);

    // Resolve <base href> against the fetched URL so that all relative links
    // in the page use the declared base, not the fetch URL.
    let effective_base = if !head_meta.base_href.is_empty() {
        crate::net::resolve_url(&head_meta.base_href, &final_url)
    } else {
        final_url.clone()
    };

    // Re-decode the HTML body if the <meta charset> disagrees with what the
    // network layer already decoded.  (fetch_html returns a UTF-8 String; if
    // the charset was wrong we need to re-read the raw bytes.)
    // For simplicity we only re-fetch when the head declares a different
    // single-byte encoding that the network did not see (rare in practice).
    // If the charset is already UTF-8 or empty we leave the string alone.
    let html = maybe_redecode_html(html, &head_meta.charset, source);

    let (title, favicon_url) = extract_page_meta(&html, &effective_base);
    let mut dom = parse_dom(&html);
    let meta = PageMeta {
        title,
        favicon_url,
        viewport_width_device:   head_meta.viewport_width_device,
        viewport_initial_scale:  head_meta.viewport_initial_scale,
    };

    let mut console_entries: Vec<ConsoleEntry> = Vec::new();
    let js_dom = js::JsDom::with_title(&dom, meta.title.clone());
    let mut js_scope = js::scope::Scope::new();
    for (_label, src) in extract_scripts(&html, &effective_base) {
        for entry in js::interpreter::execute_with_dom_and_scope(&src, &js_dom, &mut js_scope) {
            console_entries.push(entry);
        }
    }
    let mut initial_timers = Vec::new();
    let mutations = js_dom.take_mutations();
    if !mutations.is_empty() {
        for muta in mutations {
            match muta {
                js::DomMutation::SetTimeout { callback, delay_ms } => {
                    initial_timers.push(super::browser::JsTimer {
                        fire_at:  std::time::Instant::now() + std::time::Duration::from_millis(delay_ms as u64),
                        callback,
                    });
                }
                _ => js::apply_one(&mut dom, muta),
            }
        }
    }

    Some((final_url, dom, meta, console_entries, js_scope, initial_timers))
}

/// If the HTML head declares a single-byte charset that differs from UTF-8,
/// and the source is a local file (no HTTP charset header), re-read the raw
/// bytes and decode them properly.  For URLs the network layer already handled
/// the charset from the Content-Type header; a mismatch is unusual so we skip
/// re-fetching and just log it.
fn maybe_redecode_html(html: String, declared_charset: &str, source: &str) -> String {
    if declared_charset.is_empty()
        || declared_charset == "utf-8"
        || declared_charset == "utf8"
    {
        return html; // nothing to do
    }

    // Only re-read for local files; HTTP already applied charset decoding.
    let is_local = source.starts_with("file://") || (!source.starts_with("http://") && !source.starts_with("https://"));
    if !is_local {
        eprintln!("[loader] HTML declares charset={declared_charset} but HTTP already decoded; skipping re-decode.");
        return html;
    }

    let path = source.trim_start_matches("file://");
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return html,
    };

    // Reuse the same decode tables the network layer uses.
    // We mirror the charset matching logic from net::fetch_url.
    let cs = declared_charset;
    let table: &[char; 128] = if cs.contains("1254") || cs == "iso-8859-9"
        || cs == "iso8859-9" || cs == "latin5" || cs == "l5"
    {
        &crate::net::ISO_8859_9_HIGH
    } else if cs.contains("1252") || cs == "windows-1252" || cs == "win-1252" {
        &crate::net::WINDOWS_1252_HIGH
    } else if cs.contains("1250") || cs == "windows-1250" {
        &crate::net::WINDOWS_1250_HIGH
    } else {
        &crate::net::ISO_8859_1_HIGH
    };

    crate::net::decode_single_byte_pub(&bytes, table)
}

fn md5_hash(s: &str) -> u64 {
    let mut h = 0u64;
    for b in s.as_bytes() {
        h = h.wrapping_add(*b as u64).wrapping_mul(31);
    }
    h
}

fn fetch_html(source: &str) -> Option<(String, String)> {
    if source == "about:blank" || source == "about:newtab" {
        return Some((source.to_owned(), "<html><body style=\"background:white\"></body></html>".to_owned()));
    }

    if source == "forkit://history" {
        let store = crate::app::history::HistoryStore::load();
        let mut html = String::from("<html><head><title>History</title>\
            <style>\
                body { font-family: sans-serif; background: #f8f8fb; color: #333; margin: 0; padding: 40px; }\
                .container { max-width: 800px; margin: 0 auto; background: white; padding: 30px; border: 1px solid #ddd; }\
                h1 { margin-top: 0; color: #222; font-weight: 600; font-size: 24px; border-bottom: 2px solid #eee; padding-bottom: 10px; }\
                .entry { border-bottom: 1px solid #eee; padding: 16px 0; }\
                .entry:last-child { border-bottom: none; }\
                .entry-title { font-weight: 500; color: #000; text-decoration: none; font-size: 16px; display: block; }\
                .entry-title:hover { text-decoration: underline; color: #0078d7; }\
                .entry-url { color: #0066cc; font-size: 13px; text-decoration: none; display: block; }\
            </style></head><body><div class=\"container\">");
        
        html.push_str("<h1>History</h1>");
        
        if store.entries.is_empty() {
            html.push_str("<p style=\"color:#888;text-align:center;padding:40px 0;\">No history yet.</p>");
        } else {
            for entry in store.entries.iter().rev() {
                let display_title = if entry.title.is_empty() { &entry.url } else { &entry.title };
                html.push_str("<div class=\"entry\">");
                html.push_str(&format!("<a class=\"entry-title\" href=\"{}\">{}</a>", entry.url, display_title));
                html.push_str(&format!("<a class=\"entry-url\" href=\"{}\">{}</a>", entry.url, entry.url));
                html.push_str("</div>");
            }
        }
        
        html.push_str("</div></body></html>");
        return Some((source.to_owned(), html));
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
        let body = fs::read_to_string(Path::new(source))
            .map_err(|e| eprintln!("Cannot open {}: {}", source, e))
            .ok()?;
        Some((source.to_owned(), body))
    }
}

const MAX_EXTERNAL_SCRIPTS: usize = 10;

fn extract_scripts(html: &str, base_url: &str) -> Vec<(String, String)> {
    let mut scripts   = Vec::new();
    let lower         = html.to_ascii_lowercase();
    let mut pos       = 0;
    let mut ext_count = 0;

    while let Some(rel) = lower[pos..].find("<script") {
        let abs = pos + rel;

        let tag_end = match lower[abs..].find('>') {
            Some(p) => abs + p + 1,
            None    => break,
        };

        let tag_orig = &html[abs..tag_end.min(html.len())];

        if let Some(src) = crate::html5::parser::get_attr(tag_orig, "src") {
            if !src.is_empty() && ext_count < MAX_EXTERNAL_SCRIPTS {
                let url = crate::net::resolve_url(src, base_url);
                match crate::net::fetch_url(&url) {
                    Ok((_, body)) => {
                        scripts.push((url, body));
                        ext_count += 1;
                    }
                    Err(e) => eprintln!("[js] fetch {url}: {e}"),
                }
            }
            pos = lower[tag_end..]
                .find("</script")
                .map(|p| tag_end + p + "</script>".len())
                .unwrap_or(tag_end);
        } else {
            let close_rel = match lower[tag_end..].find("</script") {
                Some(p) => p,
                None    => break,
            };

            let text = &html[tag_end..tag_end + close_rel];
            if !text.trim().is_empty() {
                scripts.push(("<inline>".to_owned(), text.to_owned()));
            }

            pos = tag_end + close_rel + "</script>".len();
        }

        if pos >= lower.len() {
            break;
        }
    }

    scripts
}
