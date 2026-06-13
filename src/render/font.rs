use sdl2::ttf::{Font, Sdl2TtfContext};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontFamily {
    #[default]
    SansSerif,
    Monospace,
    Serif,
}

impl FontFamily {
    pub fn from_css(val: &str) -> Self {
        let v = val.to_ascii_lowercase();
        if v.contains("mono") || v.contains("courier") || v.contains("consolas")
            || v.contains("code") || v.contains("terminal") || v.contains("vera")
        {
            FontFamily::Monospace
        } else if v.contains("serif") && !v.contains("sans") {
            FontFamily::Serif
        } else {
            FontFamily::SansSerif
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontKey {
    size:   u16,
    bold:   bool,
    italic: bool,
    family: FontFamily,
}

pub struct FontCache<'ttf> {
    ttf:   &'ttf Sdl2TtfContext,
    cache: HashMap<FontKey, Font<'ttf, 'static>>,
}

impl<'ttf> FontCache<'ttf> {
    pub fn new(ttf: &'ttf Sdl2TtfContext) -> Self {
        FontCache { ttf, cache: HashMap::new() }
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
        let key  = FontKey { size, bold, italic, family };

        if !self.cache.contains_key(&key) {
            let font = self.load_font(size, bold, italic, family);
            self.cache.insert(key.clone(), font?);
        }

        self.cache.get(&key)
    }

    fn load_font(
        &self,
        size:   u16,
        bold:   bool,
        italic: bool,
        family: FontFamily,
    ) -> Option<Font<'ttf, 'static>> {
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
            FontFamily::SansSerif | FontFamily::Serif => (
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
