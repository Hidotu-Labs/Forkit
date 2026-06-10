use sdl2::ttf::{Font, Sdl2TtfContext};
use std::collections::HashMap;
use std::path::Path;

// Sans-serif (default)
// NotoSans covers Latin, Latin Extended (Turkish, etc.), Greek, Cyrillic and more.
const SANS_REGULAR:    &str = "/usr/share/fonts/noto/NotoSans-Regular.ttf";
const SANS_BOLD:       &str = "/usr/share/fonts/noto/NotoSans-Bold.ttf";
const SANS_ITALIC:     &str = "/usr/share/fonts/noto/NotoSans-Italic.ttf";
const SANS_BOLDITALIC: &str = "/usr/share/fonts/noto/NotoSans-BoldItalic.ttf";

// Monospace
const MONO_REGULAR:    &str = "/usr/share/fonts/Adwaita/AdwaitaMono-Regular.ttf";
const MONO_BOLD:       &str = "/usr/share/fonts/Adwaita/AdwaitaMono-Bold.ttf";
const MONO_ITALIC:     &str = "/usr/share/fonts/Adwaita/AdwaitaMono-Italic.ttf";
const MONO_BOLDITALIC: &str = "/usr/share/fonts/Adwaita/AdwaitaMono-BoldItalic.ttf";

// Noto Sans Mono — better Unicode coverage than Adwaita for code blocks
const NOTO_MONO_REGULAR: &str = "/usr/share/fonts/noto/NotoSansMono-Regular.ttf";
const NOTO_MONO_BOLD:    &str = "/usr/share/fonts/noto/NotoSansMono-Bold.ttf";

// System-level broad-coverage fallbacks (DejaVu is almost always available)
const DEJAVU_SANS:       &str = "/usr/share/fonts/dejavu/DejaVuSans.ttf";
const DEJAVU_SANS_BOLD:  &str = "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf";
const DEJAVU_MONO:       &str = "/usr/share/fonts/dejavu/DejaVuSansMono.ttf";

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
        // Each family has a primary path, a same-family fallback, then broad
        // Unicode fallbacks (DejaVu covers Latin Extended A/B, Greek, Cyrillic,
        // Arabic, Hebrew, and more — including all Turkish characters).
        let paths: &[&str] = match family {
            FontFamily::Monospace => &[
                match (bold, italic) {
                    (true,  true)  => MONO_BOLDITALIC,
                    (true,  false) => MONO_BOLD,
                    (false, true)  => MONO_ITALIC,
                    (false, false) => MONO_REGULAR,
                },
                // Noto Sans Mono as secondary (better Unicode than Adwaita)
                if bold { NOTO_MONO_BOLD } else { NOTO_MONO_REGULAR },
                MONO_REGULAR,
                DEJAVU_MONO,
                DEJAVU_SANS,
                SANS_REGULAR,
            ],
            FontFamily::SansSerif | FontFamily::Serif => &[
                match (bold, italic) {
                    (true,  true)  => SANS_BOLDITALIC,
                    (true,  false) => SANS_BOLD,
                    (false, true)  => SANS_ITALIC,
                    (false, false) => SANS_REGULAR,
                },
                SANS_REGULAR,
                if bold { DEJAVU_SANS_BOLD } else { DEJAVU_SANS },
                DEJAVU_SANS,
            ],
        };

        for path in paths {
            if let Ok(font) = self.ttf.load_font(Path::new(path), size) {
                return Some(font);
            }
        }
        None
    }
}
