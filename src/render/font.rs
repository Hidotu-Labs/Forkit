use sdl2::ttf::{Font, Sdl2TtfContext};
use sdl2::render::TextureCreator;
use sdl2::video::WindowContext;
use crate::dom::node::FontFamily;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontKey {
    size:      u16,
    bold:      bool,
    italic:    bool,
    underline: bool,
    family:    FontFamily,
}

/// A resolved font + fallback font pair for rendering a single text run.
pub struct FontRun<'a, 'ttf> {
    pub primary:  &'a Font<'ttf, 'static>,
    pub fallback: Option<&'a Font<'ttf, 'static>>,
}

pub struct FontCache<'ttf> {
    ttf:      &'ttf Sdl2TtfContext,
    cache:    HashMap<FontKey, Font<'ttf, 'static>>,
    /// Custom `@font-face` registry: Maps (name, bold, italic) -> local path or URL
    registry: HashMap<(String, bool, bool), String>,
    /// Per-font glyph coverage cache: maps FontKey -> set of covered Unicode scalar values.
    /// Populated lazily on first use; avoids O(chars) `find_glyph` calls per frame.
    glyph_coverage: HashMap<FontKey, HashSet<char>>,
    /// Text measurement cache: maps (text, FontKey) -> (width, height).
    /// Avoids repeated `font.size_of()` calls for the same string/font combo.
    measure_cache: HashMap<(String, FontKey), (i32, i32)>,
}

impl<'ttf> FontCache<'ttf> {
    pub fn new(ttf: &'ttf Sdl2TtfContext) -> Self {
        FontCache {
            ttf,
            cache:          HashMap::new(),
            registry:       HashMap::new(),
            glyph_coverage: HashMap::new(),
            measure_cache:  HashMap::new(),
        }
    }

    /// Register a custom font file path for a given family name.
    pub fn register(&mut self, name: &str, bold: bool, italic: bool, path: &str) {
        self.registry.insert((name.to_ascii_lowercase(), bold, italic), path.to_string());
        // Clear cache entries for this family to force reload if already used.
        self.cache.retain(|key, _| {
            match &key.family {
                FontFamily::Custom(n) if n.to_ascii_lowercase() == name.to_ascii_lowercase() => {
                    key.bold != bold || key.italic != italic
                }
                _ => true,
            }
        });
        // Also evict glyph coverage for the same family.
        self.glyph_coverage.retain(|key, _| {
            match &key.family {
                FontFamily::Custom(n) if n.to_ascii_lowercase() == name.to_ascii_lowercase() => {
                    key.bold != bold || key.italic != italic
                }
                _ => true,
            }
        });
        // And measurement cache.
        self.measure_cache.retain(|(_, key), _| {
            match &key.family {
                FontFamily::Custom(n) if n.to_ascii_lowercase() == name.to_ascii_lowercase() => {
                    key.bold != bold || key.italic != italic
                }
                _ => true,
            }
        });
    }

    /// Return `true` if the font identified by `key` covers `ch`.
    /// Results are memoised — `find_glyph` is only called the first time
    /// a character is encountered for a given font.
    pub fn font_has_glyph(&mut self, size: u16, bold: bool, italic: bool, family: FontFamily, ch: char) -> bool {
        let size = size.clamp(8, 96);
        let key = FontKey { size, bold, italic, underline: false, family: family.clone() };
        // We need to ensure the font is loaded first.
        let _ = self.get_family(size, bold, italic, false, family);
        if !self.glyph_coverage.contains_key(&key) {
            self.glyph_coverage.insert(key.clone(), HashSet::new());
        }
        if let Some(coverage) = self.glyph_coverage.get(&key) {
            if coverage.contains(&ch) {
                return true;
            }
        }
        // Not yet cached — call find_glyph and store result.
        let has = self.cache.get(&key)
            .map(|f| f.find_glyph(ch).is_some())
            .unwrap_or(false);
        if has {
            self.glyph_coverage.entry(key).or_default().insert(ch);
        }
        has
    }

    /// Return `true` if the fallback symbols font covers `ch`.
    pub fn fallback_has_glyph(&mut self, size: u16, ch: char) -> bool {
        let size = size.clamp(8, 96);
        let key = FontKey { size, bold: false, italic: false, underline: false, family: FontFamily::Custom("__fallback__".into()) };
        let _ = self.get_fallback(size);
        if !self.glyph_coverage.contains_key(&key) {
            self.glyph_coverage.insert(key.clone(), HashSet::new());
        }
        if let Some(coverage) = self.glyph_coverage.get(&key) {
            if coverage.contains(&ch) { return true; }
        }
        let has = self.cache.get(&key)
            .map(|f| f.find_glyph(ch).is_some())
            .unwrap_or(false);
        if has {
            self.glyph_coverage.entry(key).or_default().insert(ch);
        }
        has
    }

    /// Return `true` if the math fallback font covers `ch`.
    pub fn math_has_glyph(&mut self, size: u16, ch: char) -> bool {
        let size = size.clamp(8, 96);
        let key = FontKey { size, bold: false, italic: false, underline: false, family: FontFamily::Custom("__math__".into()) };
        let _ = self.get_math_fallback(size);
        if !self.glyph_coverage.contains_key(&key) {
            self.glyph_coverage.insert(key.clone(), HashSet::new());
        }
        if let Some(coverage) = self.glyph_coverage.get(&key) {
            if coverage.contains(&ch) { return true; }
        }
        let has = self.cache.get(&key)
            .map(|f| f.find_glyph(ch).is_some())
            .unwrap_or(false);
        if has {
            self.glyph_coverage.entry(key).or_default().insert(ch);
        }
        has
    }

    /// Measure `text` using the given font, with results memoised per (text, FontKey).
    /// Returns `(width, height)` or a fallback estimate.
    pub fn size_of_cached(&mut self, text: &str, size: u16, bold: bool, italic: bool, family: FontFamily) -> (i32, i32) {
        let size = size.clamp(8, 96);
        let key = FontKey { size, bold, italic, underline: false, family: family.clone() };
        let cache_key = (text.to_owned(), key.clone());
        if let Some(&cached) = self.measure_cache.get(&cache_key) {
            return cached;
        }
        let result = self.get_family(size, bold, italic, false, family)
            .and_then(|f| f.size_of(text).ok())
            .map(|(w, h)| (w as i32, h as i32))
            .unwrap_or((text.len() as i32 * (size as i32 / 2).max(4), size as i32));
        // Only cache reasonably short strings to bound memory use.
        if text.len() <= 256 {
            self.measure_cache.insert(cache_key, result);
        }
        result
    }

    /// Measure a single character, with caching.
    pub fn char_width_cached(&mut self, ch: char, size: u16, bold: bool, italic: bool, family: FontFamily) -> i32 {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        self.size_of_cached(s, size, bold, italic, family).0
    }

    pub fn get(&mut self, size: u16, bold: bool, italic: bool) -> Option<&Font<'ttf, 'static>> {
        self.get_family(size, bold, italic, false, FontFamily::SansSerif)
    }

    pub fn measure_text(&mut self, text: &str, size: u16, bold: bool, italic: bool) -> (i32, i32) {
        self.size_of_cached(text, size, bold, italic, FontFamily::SansSerif)
    }

    pub fn get_text_texture<'a>(
        &mut self,
        tc: &'a TextureCreator<WindowContext>,
        text: &str,
        size: u16,
        color: [u8; 3],
        bold: bool,
        italic: bool,
        underline: bool,
    ) -> Option<sdl2::render::Texture<'a>> {
        let font = self.get_family(size, bold, italic, underline, FontFamily::SansSerif)?;

        let c = sdl2::pixels::Color::RGB(color[0], color[1], color[2]);
        let surf = font.render(text).blended(c).ok()?;
        tc.create_texture_from_surface(&surf).ok()
    }

    pub fn get_family(
        &mut self,
        size:      u16,
        bold:      bool,
        italic:    bool,
        underline: bool,
        family:    FontFamily,
    ) -> Option<&Font<'ttf, 'static>> {
        let size = size.clamp(8, 96);
        let key  = FontKey { size, bold, italic, underline, family: family.clone() };

        if !self.cache.contains_key(&key) {
            let mut font = self.load_font(size, bold, italic, family)?;
            let mut style = sdl2::ttf::FontStyle::NORMAL;
            if bold      { style |= sdl2::ttf::FontStyle::BOLD; }
            if italic    { style |= sdl2::ttf::FontStyle::ITALIC; }
            if underline { style |= sdl2::ttf::FontStyle::UNDERLINE; }
            font.set_style(style);
            self.cache.insert(key.clone(), font);
        }

        self.cache.get(&key)
    }

    /// Get the symbol/fallback font at the given size.
    /// Used for glyphs not covered by the primary NotoSans fonts
    /// (e.g. arrows U+2190–U+21FF, misc symbols U+2600–U+26FF, etc.)
    pub fn get_fallback(&mut self, size: u16) -> Option<&Font<'ttf, 'static>> {
        let size = size.clamp(8, 96);
        // Use a sentinel key: Custom("__fallback__") bold=false italic=false
        let key = FontKey {
            size,
            bold:      false,
            italic:    false,
            underline: false,
            family:    FontFamily::Custom("__fallback__".into()),
        };
        if !self.cache.contains_key(&key) {
            let font = self.load_fallback_font(size);
            self.cache.insert(key.clone(), font?);
        }
        self.cache.get(&key)
    }

    /// Get the math fallback font (NotoSansMath) at the given size.
    pub fn get_math_fallback(&mut self, size: u16) -> Option<&Font<'ttf, 'static>> {
        let size = size.clamp(8, 96);
        let key = FontKey {
            size,
            bold:      false,
            italic:    false,
            underline: false,
            family:    FontFamily::Custom("__math__".into()),
        };
        if !self.cache.contains_key(&key) {
            let font = self.load_math_font(size);
            self.cache.insert(key.clone(), font?);
        }
        self.cache.get(&key)
    }

    fn load_fallback_font(&self, size: u16) -> Option<Font<'ttf, 'static>> {
        // NotoSansSymbols-Regular covers arrows (U+2190–U+21FF), misc symbols
        // (U+2600–U+26FF), dingbats, card suits, etc.
        // NotoSansSymbols2 has broader historic/emoji coverage but lacks basic arrows.
        let candidates = [
            "/usr/share/fonts/noto/NotoSansSymbols-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
            "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
        ];
        for path in &candidates {
            if std::path::Path::new(path).exists() {
                if let Ok(font) = self.ttf.load_font(std::path::Path::new(path), size) {
                    return Some(font);
                }
            }
        }
        None
    }

    fn load_math_font(&self, size: u16) -> Option<Font<'ttf, 'static>> {
        let candidates = [
            "/usr/share/fonts/noto/NotoSansMath-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansMath-Regular.ttf",
        ];
        for path in &candidates {
            if std::path::Path::new(path).exists() {
                if let Ok(font) = self.ttf.load_font(std::path::Path::new(path), size) {
                    return Some(font);
                }
            }
        }
        None
    }

    fn load_font(
        &self,
        size:   u16,
        bold:   bool,
        italic: bool,
        family: FontFamily,
    ) -> Option<Font<'ttf, 'static>> {
        // 1. Check custom registry first
        if let FontFamily::Custom(ref name) = family {
            if let Some(path_str) = self.registry.get(&(name.to_ascii_lowercase(), bold, italic)) {
                println!("[font] Loading custom font: {} (bold={}, italic={}) from {:?}", name, bold, italic, path_str);
                match self.ttf.load_font(std::path::Path::new(path_str), size) {
                    Ok(font) => return Some(font),
                    Err(e) => eprintln!("[font] Failed to load custom font file {:?}: {}", path_str, e),
                }
            }
            // Fallback: If custom name is not found, try to strip quotes and check again
            let clean = name.trim_matches(|c| c == '"' || c == '\'').to_ascii_lowercase();
            if let Some(path_str) = self.registry.get(&(clean.clone(), bold, italic)) {
                println!("[font] Loading custom font (clean): {} from {:?}", clean, path_str);
                match self.ttf.load_font(std::path::Path::new(path_str), size) {
                    Ok(font) => return Some(font),
                    Err(e) => eprintln!("[font] Failed to load custom font file {:?}: {}", path_str, e),
                }
            }
        }

        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));

        let search_paths = vec![
            exe_dir.join("assets/fonts"),
            std::path::PathBuf::from("assets/fonts"),
            std::path::PathBuf::from("/usr/share/fonts/noto"),
            std::path::PathBuf::from("/usr/share/fonts/truetype/noto"),
            std::path::PathBuf::from("/usr/share/fonts/google-noto"),
            std::path::PathBuf::from("/usr/local/share/fonts/noto"),
            std::path::PathBuf::from("/System/Library/Fonts"),
            std::path::PathBuf::from("/Library/Fonts"),
            std::path::PathBuf::from("C:/Windows/Fonts"),
        ];

        let (regular, bold_font, italic_font, bold_italic) = match family {
            FontFamily::Monospace => (
                "NotoSansMono-Regular.ttf",
                "NotoSansMono-Bold.ttf",
                "NotoSansMono-Regular.ttf",
                "NotoSansMono-Bold.ttf",
            ),
            FontFamily::SansSerif | FontFamily::Serif | FontFamily::Custom(_) => (
                "NotoSans-Regular.ttf",
                "NotoSans-Bold.ttf",
                "NotoSans-Italic.ttf",
                "NotoSans-BoldItalic.ttf",
            ),
        };

        let filename = match (bold, italic) {
            (true,  true)  => bold_italic,
            (true,  false) => bold_font,
            (false, true)  => italic_font,
            (false, false) => regular,
        };

        for base_path in &search_paths {
            let font_path = base_path.join(filename);
            if font_path.exists() {
                if let Ok(font) = self.ttf.load_font(&font_path, size) {
                    return Some(font);
                }
            }
        }

        for font_path in self.find_system_fonts() {
            if let Ok(font) = self.ttf.load_font(&font_path, size) {
                return Some(font);
            }
        }

        eprintln!("Failed to load font: {} (size {})", filename, size);
        None
    }

    fn find_system_fonts(&self) -> Vec<std::path::PathBuf> {
        let mut fonts = Vec::new();

        let font_dirs = [
            "/usr/share/fonts",
            "/usr/local/share/fonts",
            "/System/Library/Fonts",
            "/Library/Fonts",
            "C:/Windows/Fonts",
        ];

        for dir in font_dirs {
            let path = std::path::Path::new(dir);
            if path.exists() {
                if let Ok(entries) = std::fs::read_dir(path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if let Some(ext) = path.extension() {
                            if ext == "ttf" || ext == "otf" {
                                fonts.push(path);
                                if fonts.len() > 10 {
                                    return fonts;
                                }
                            }
                        }
                    }
                }
            }
        }

        fonts
    }
}
