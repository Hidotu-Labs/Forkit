use sdl2::ttf::{Font, Sdl2TtfContext};
use std::collections::HashMap;
use std::path::Path;

// Sans-serif (default)
const SANS_REGULAR:    &str = "/usr/share/fonts/noto/NotoSans-Regular.ttf";
const SANS_BOLD:       &str = "/usr/share/fonts/noto/NotoSans-Bold.ttf";
const SANS_ITALIC:     &str = "/usr/share/fonts/noto/NotoSans-Italic.ttf";
const SANS_BOLDITALIC: &str = "/usr/share/fonts/noto/NotoSans-BoldItalic.ttf";

// Monospace
const MONO_REGULAR:    &str = "/usr/share/fonts/Adwaita/AdwaitaMono-Regular.ttf";
const MONO_BOLD:       &str = "/usr/share/fonts/Adwaita/AdwaitaMono-Bold.ttf";
const MONO_ITALIC:     &str = "/usr/share/fonts/Adwaita/AdwaitaMono-Italic.ttf";
const MONO_BOLDITALIC: &str = "/usr/share/fonts/Adwaita/AdwaitaMono-BoldItalic.ttf";

/// Font family hint — controls which face is loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontFamily {
    #[default]
    SansSerif,
    Monospace,
    // Serif falls back to sans-serif for now (no Noto Serif on all systems)
    Serif,
}

impl FontFamily {
    /// Parse a CSS `font-family` value into a `FontFamily`.
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

/// Font cache — wraps `Sdl2TtfContext` and keeps loaded fonts alive.
/// Lifetime is tied to the TTF context.
pub struct FontCache<'ttf> {
    ttf:   &'ttf Sdl2TtfContext,
    cache: HashMap<FontKey, Font<'ttf, 'static>>,
}

impl<'ttf> FontCache<'ttf> {
    pub fn new(ttf: &'ttf Sdl2TtfContext) -> Self {
        FontCache { ttf, cache: HashMap::new() }
    }

    /// Get a font by size/style/family, loading and caching on first use.
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
        let (pri, fb1, fb2) = match family {
            FontFamily::Monospace => {
                let pri = match (bold, italic) {
                    (true,  true)  => MONO_BOLDITALIC,
                    (true,  false) => MONO_BOLD,
                    (false, true)  => MONO_ITALIC,
                    (false, false) => MONO_REGULAR,
                };
                (pri, MONO_REGULAR, SANS_REGULAR)
            }
            FontFamily::SansSerif | FontFamily::Serif => {
                let pri = match (bold, italic) {
                    (true,  true)  => SANS_BOLDITALIC,
                    (true,  false) => SANS_BOLD,
                    (false, true)  => SANS_ITALIC,
                    (false, false) => SANS_REGULAR,
                };
                (pri, SANS_REGULAR, MONO_REGULAR)
            }
        };

        self.ttf.load_font(Path::new(pri), size)
            .or_else(|_| self.ttf.load_font(Path::new(fb1), size))
            .or_else(|_| self.ttf.load_font(Path::new(fb2), size))
            .ok()
    }
}
