use sdl2::ttf::{Font, Sdl2TtfContext};
use crate::dom::node::FontFamily;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontKey {
    size:   u16,
    bold:   bool,
    italic: bool,
    family: FontFamily,
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
}

impl<'ttf> FontCache<'ttf> {
    pub fn new(ttf: &'ttf Sdl2TtfContext) -> Self {
        FontCache {
            ttf,
            cache:    HashMap::new(),
            registry: HashMap::new(),
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
    }

    pub fn get(&mut self, size: u16, bold: bool, italic: bool) -> Option<&Font<'ttf, 'static>> {
        self.get_family(size, bold, italic, FontFamily::SansSerif)
    }

    pub fn get_family(
        &mut self,
        size:   u16,
        bold:   bool,
        italic: bool,
        family: FontFamily,
    ) -> Option<&Font<'ttf, 'static>> {
        let size = size.clamp(8, 96);
        let key  = FontKey { size, bold, italic, family: family.clone() };

        if !self.cache.contains_key(&key) {
            let font = self.load_font(size, bold, italic, family);
            self.cache.insert(key.clone(), font?);
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
            bold:   false,
            italic: false,
            family: FontFamily::Custom("__fallback__".into()),
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
            bold:   false,
            italic: false,
            family: FontFamily::Custom("__math__".into()),
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
