use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::app::history::HistoryEntry;
use crate::render::font::FontCache;

/// Maximum number of entries shown in the dropdown.
pub const MAX_VISIBLE: usize = 12;

const PANEL_W:    i32 = 380;
const ROW_H:      i32 = 46;
const PAD:        i32 = 10;
const FONT_TITLE: u16 = 13;
const FONT_URL:   u16 = 11;
const HEADER_H:   i32 = 28;

/// Result from `draw()` — tells the caller what (if anything) was clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelEvent {
    /// User clicked a history entry — contains the URL to navigate to.
    Navigate(String),
    /// User clicked the "Clear history" button.
    Clear,
    /// No actionable click (panel should stay open).
    None,
    /// Click landed outside the panel — close it.
    DismissOutside,
}

/// Stateless history dropdown panel.
pub struct HistoryPanel;

impl HistoryPanel {
    /// Draw the panel anchored below `anchor_x, anchor_y` (window coords).
    /// Returns the number of pixel rows actually drawn (panel height).
    pub fn draw(
        canvas:   &mut Canvas<Window>,
        tc:       &TextureCreator<WindowContext>,
        fonts:    &mut FontCache,
        entries:  &[&HistoryEntry],
        anchor_x: i32,
        anchor_y: i32,
        win_w:    i32,
    ) -> i32 {
        let count   = entries.len().min(MAX_VISIBLE);
        let panel_h = HEADER_H + count as i32 * ROW_H + PAD;

        // Clamp to window width
        let panel_x = (anchor_x).min(win_w - PANEL_W);

        // Shadow
        canvas.set_draw_color(Color::RGBA(0, 0, 0, 40));
        let _ = canvas.fill_rect(Rect::new(panel_x + 3, anchor_y + 3, PANEL_W as u32, panel_h as u32));

        // Background
        canvas.set_draw_color(Color::RGB(252, 252, 253));
        let _ = canvas.fill_rect(Rect::new(panel_x, anchor_y, PANEL_W as u32, panel_h as u32));

        // Border
        canvas.set_draw_color(Color::RGB(200, 200, 205));
        let _ = canvas.draw_rect(Rect::new(panel_x, anchor_y, PANEL_W as u32, panel_h as u32));

        // Header row
        canvas.set_draw_color(Color::RGB(240, 240, 244));
        let _ = canvas.fill_rect(Rect::new(panel_x, anchor_y, PANEL_W as u32, HEADER_H as u32));
        canvas.set_draw_color(Color::RGB(200, 200, 205));
        let _ = canvas.fill_rect(Rect::new(panel_x, anchor_y + HEADER_H - 1, PANEL_W as u32, 1));

        draw_text(canvas, fonts, tc, "Recent History", panel_x + PAD, anchor_y, PANEL_W, HEADER_H, FONT_TITLE, Color::RGB(50, 50, 60), false);

        // "Clear" button in the header
        let clear_x = panel_x + PANEL_W - 60 - PAD;
        let clear_y = anchor_y + (HEADER_H - 18) / 2;
        canvas.set_draw_color(Color::RGB(220, 80, 80));
        let _ = canvas.fill_rect(Rect::new(clear_x, clear_y, 60, 18));
        draw_text(canvas, fonts, tc, "Clear", clear_x, clear_y, 60, 18, FONT_URL, Color::WHITE, true);

        // Entry rows
        for (i, entry) in entries.iter().take(MAX_VISIBLE).enumerate() {
            let row_y = anchor_y + HEADER_H + i as i32 * ROW_H;

            // Alternating background
            if i % 2 == 1 {
                canvas.set_draw_color(Color::RGB(246, 246, 249));
                let _ = canvas.fill_rect(Rect::new(panel_x, row_y, PANEL_W as u32, ROW_H as u32));
            }

            // Divider
            canvas.set_draw_color(Color::RGB(230, 230, 234));
            let _ = canvas.fill_rect(Rect::new(panel_x + PAD, row_y + ROW_H - 1, (PANEL_W - PAD * 2) as u32, 1));

            // Title
            let display_title = if entry.title.is_empty() { &entry.url } else { &entry.title };
            let title_truncated = truncate(display_title, 42);
            draw_text(canvas, fonts, tc, &title_truncated, panel_x + PAD, row_y + 4, PANEL_W - PAD * 2, 20, FONT_TITLE, Color::RGB(20, 20, 30), false);

            // URL (smaller, grey)
            let url_truncated = truncate_url(&entry.url, 55);
            draw_text(canvas, fonts, tc, &url_truncated, panel_x + PAD, row_y + 24, PANEL_W - PAD * 2, 16, FONT_URL, Color::RGB(100, 100, 120), false);
        }

        if count == 0 {
            draw_text(canvas, fonts, tc, "No history yet", panel_x + PAD, anchor_y + HEADER_H + 8, PANEL_W, ROW_H, FONT_TITLE, Color::RGB(150, 150, 160), false);
        }

        panel_h
    }

    /// Hit-test a click at `(mx, my)`.
    pub fn hit_test(
        entries:  &[&HistoryEntry],
        mx: i32,
        my: i32,
        anchor_x: i32,
        anchor_y: i32,
        win_w:    i32,
    ) -> PanelEvent {
        let count   = entries.len().min(MAX_VISIBLE);
        let panel_h = HEADER_H + count as i32 * ROW_H + PAD;
        let panel_x = anchor_x.min(win_w - PANEL_W);

        // Outside the panel
        if mx < panel_x || mx >= panel_x + PANEL_W
            || my < anchor_y || my >= anchor_y + panel_h
        {
            return PanelEvent::DismissOutside;
        }

        // Clear button
        let clear_x = panel_x + PANEL_W - 60 - PAD;
        let clear_y = anchor_y + (HEADER_H - 18) / 2;
        if mx >= clear_x && mx < clear_x + 60 && my >= clear_y && my < clear_y + 18 {
            return PanelEvent::Clear;
        }

        // Header (but not clear button) — no action
        if my < anchor_y + HEADER_H {
            return PanelEvent::None;
        }

        // Entry rows
        let row_idx = ((my - anchor_y - HEADER_H) / ROW_H) as usize;
        if row_idx < count {
            return PanelEvent::Navigate(entries[row_idx].url.clone());
        }

        PanelEvent::None
    }
}

// ---- helpers ---------------------------------------------------------------

fn draw_text(
    canvas:  &mut Canvas<Window>,
    fonts:   &mut FontCache,
    tc:      &TextureCreator<WindowContext>,
    text:    &str,
    x: i32, y: i32,
    max_w: i32, height: i32,
    size:    u16,
    color:   Color,
    centre:  bool,
) {
    let Some(font) = fonts.get(size, false, false) else { return };
    let Ok(surf)   = font.render(text).blended(color) else { return };
    let Ok(tex)    = tc.create_texture_from_surface(&surf) else { return };
    let sw = surf.width() as i32;
    let sh = surf.height() as i32;
    let draw_w = sw.min(max_w).max(0) as u32;
    if draw_w == 0 { return; }
    let tx = if centre { x + (max_w - sw).max(0) / 2 } else { x };
    let ty = y + (height - sh).max(0) / 2;
    let _ = canvas.copy(
        &tex,
        Some(Rect::new(0, 0, draw_w, sh as u32)),
        Rect::new(tx, ty, draw_w, sh as u32),
    );
}

fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_owned()
    } else {
        let t: String = chars[..max_chars - 1].iter().collect();
        format!("{}…", t)
    }
}

/// Truncate a URL but keep the scheme and host visible.
fn truncate_url(url: &str, max_chars: usize) -> String {
    truncate(url, max_chars)
}
