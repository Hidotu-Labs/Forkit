use std::fs;
use std::path::Path;

use crate::dom::node::Node;
use crate::dom::parser::{parse_with_sheets, extract_page_meta};
use crate::dom::css::StyleSheet;
use crate::js;
use crate::net;

pub use crate::js::{ConsoleEntry, ConsoleLevel};

#[derive(Debug)]
pub struct PageMeta {
    pub title:       String,
    pub favicon_url: Option<String>,
}

pub fn load_dom(source: &str) -> Option<(String, Node, PageMeta, Vec<StyleSheet>, Vec<(String, bool, bool, String)>, Vec<ConsoleEntry>)> {
    let (final_url, html) = fetch_html(source)?;
    let (title, favicon_url) = extract_page_meta(&html, &final_url);
    let (mut dom, sheets) = parse_with_sheets(&html, &final_url);
    let meta = PageMeta { title, favicon_url };

    // Download fonts
    let mut downloaded_fonts = Vec::new();
    let temp_dir = std::env::temp_dir().join("forkit_fonts");
    let _ = std::fs::create_dir_all(&temp_dir);

    for sheet in &sheets {
        for face in &sheet.font_faces {
            for src_url in &face.srcs {
                let font_url = crate::net::resolve_url(src_url, &final_url);
                let ext = if font_url.contains(".otf") { "otf" } 
                          else if font_url.contains(".woff2") { "woff2" }
                          else if font_url.contains(".woff") { "woff" }
                          else { "ttf" };
                let hash = format!("{:x}", md5_hash(&font_url));
                let local_path = temp_dir.join(format!("{}.{}", hash, ext));

                if !local_path.exists() {
                    println!("[loader] Downloading font: {} -> {:?}", font_url, local_path);
                    if let Ok((_, bytes)) = crate::net::fetch_url_bytes(&font_url) {
                        let _ = std::fs::write(&local_path, bytes);
                    } else {
                        eprintln!("[loader] Failed to download font: {}", font_url);
                    }
                }

                if local_path.exists() {
                    // Stop at the first successfully downloaded source for this face
                    downloaded_fonts.push((face.family.clone(), face.bold, face.italic, local_path.to_string_lossy().to_string()));
                    break;
                }
            }
        }
    }

    let mut console_entries: Vec<ConsoleEntry> = Vec::new();
    let js_dom = js::JsDom::with_title(&dom, meta.title.clone());
    for (_label, src) in extract_scripts(&html, &final_url) {
        for entry in js::execute_with_dom(&src, &js_dom) {
            console_entries.push(entry);
        }
    }
    let mutations = js_dom.take_mutations();
    if !mutations.is_empty() {
        js::apply_mutations(&mut dom, mutations);
        crate::dom::css::apply_cascade(&mut dom, &sheets);
    }

    Some((final_url, dom, meta, sheets, downloaded_fonts, console_entries))
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

        if let Some(src) = crate::dom::parser::get_attr(tag_orig, "src") {
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
