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
        // PNG: 89 50 4E 47
        if bytes.starts_with(b"\x89PNG") { return "PNG"; }
        // JPEG: FF D8
        if bytes[0] == 0xFF && bytes[1] == 0xD8 { return "JPG"; }
        // GIF: GIF8
        if bytes.starts_with(b"GIF8") { return "GIF"; }
        // BMP: BM
        if bytes.starts_with(b"BM") { return "BMP"; }
        // WebP: RIFF....WEBP
        if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
            return "WEBP";
        }
        // AVIF / generic ISO BMFF — ftyp box
        if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" { return "AVIF"; }
        // ICO
        if bytes[0] == 0x00 && bytes[1] == 0x00 && bytes[2] == 0x01 { return "ICO"; }
    }
    "PNG" // fallback guess
}

/// Fetch image bytes from a URL or local path.
fn fetch_image(url: &str, base_url: &str) -> Option<Vec<u8>> {
    use crate::net;

    // data: URI — decode inline
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
        // Local file path
        let path = resolved.strip_prefix("file://").unwrap_or(&resolved);
        std::fs::read(path)
            .map_err(|e| eprintln!("Image read {path}: {e}"))
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
