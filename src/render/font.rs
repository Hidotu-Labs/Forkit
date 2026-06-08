use sdl2::ttf::{Font, Sdl2TtfContext};
use std::collections::HashMap;
use std::path::Path;

const FONT_REGULAR:    &str = "/usr/share/fonts/noto/NotoSans-Regular.ttf";
const FONT_BOLD:       &str = "/usr/share/fonts/noto/NotoSans-Bold.ttf";
const FONT_ITALIC:     &str = "/usr/share/fonts/noto/NotoSans-Italic.ttf";
const FONT_BOLDITALIC: &str = "/usr/share/fonts/noto/NotoSans-BoldItalic.ttf";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontKey {
    size:   u16,
    bold:   bool,
    italic: bool,
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

    pub fn get(&mut self, size: u16, bold: bool, italic: bool) -> Option<&Font<'ttf, 'static>> {
        let size = size.clamp(8, 96);
        let key  = FontKey { size, bold, italic };

        if !self.cache.contains_key(&key) {
            let path = match (bold, italic) {
                (true,  true)  => FONT_BOLDITALIC,
                (true,  false) => FONT_BOLD,
                (false, true)  => FONT_ITALIC,
                (false, false) => FONT_REGULAR,
            };
            let font = self.ttf.load_font(Path::new(path), size)
                .or_else(|_| self.ttf.load_font(Path::new(FONT_REGULAR), size))
                .ok()?;
            self.cache.insert(key.clone(), font);
        }

        self.cache.get(&key)
    }
}
