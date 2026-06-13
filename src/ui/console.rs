/// Right-side developer console panel (Chrome DevTools style).
///
/// - Docked to the right edge of the window.
/// - Resizable by dragging the left border handle.
/// - Toggle with F12, close with the × button.
/// - Scrollable with mouse-wheel when the cursor is over it.

use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::app::loader::{ConsoleEntry, ConsoleLevel};
use crate::render::font::FontCache;

/// Default width of the panel.
pub const CONSOLE_DEFAULT_W: i32 = 340;
/// Minimum / maximum widths the user can drag to.
pub const CONSOLE_MIN_W: i32 = 180;
pub const CONSOLE_MAX_W: i32 = 900;
/// Width of the left-edge drag handle.
pub const RESIZE_HANDLE_W: i32 = 4;
/// Width of the close button in the header.
pub const CLOSE_BTN_W: i32 = 28;

const HEADER_H:  i32 = 28;
const FONT_SIZE: u16 = 13;
const LINE_H:    i32 = 18;
const PAD_X:     i32 = 8;

const BG:        (u8,u8,u8) = (28,  28,  28 );
const HEADER_BG: (u8,u8,u8) = (42,  42,  42 );
const HANDLE_BG: (u8,u8,u8) = (60,  60,  65 );
const HANDLE_HL: (u8,u8,u8) = (100, 140, 220);
const BORDER:    (u8,u8,u8) = (70,  70,  75 );
const LOG_FG:    (u8,u8,u8) = (212, 212, 212);
const WARN_FG:   (u8,u8,u8) = (255, 200,  50);
const WARN_BG:   (u8,u8,u8) = (48,  38,   8 );
const ERR_FG:    (u8,u8,u8) = (255,  90,  80);
const ERR_BG:    (u8,u8,u8) = (48,  14,  14 );
const HEADER_FG: (u8,u8,u8) = (175, 175, 180);
const CLOSE_FG:  (u8,u8,u8) = (200, 100, 100);

/// Result of one draw call — rects needed for hit-testing.
pub struct DrawResult {
    /// The × close button rect (window-absolute).
    pub close_btn:     Rect,
    /// The resize drag handle rect (window-absolute).
    pub resize_handle: Rect,
}

/// Draw the right-side console panel.
///
/// * `panel_x`     — left edge of the panel in window coords (= `win_w - console_w`)
/// * `chrome_h`    — height of the tab-bar + address-bar chrome
/// * `console_w`   — current panel width (includes the resize handle)
/// * `resize_hot`  — whether the cursor is currently hovering the resize handle
pub fn draw(
    canvas:     &mut Canvas<Window>,
    tc:         &TextureCreator<WindowContext>,
    fonts:      &mut FontCache,
    panel_x:    i32,
    chrome_h:   i32,
    win_h:      i32,
    console_w:  i32,
    entries:    &[ConsoleEntry],
    scroll:     i32,
    resize_hot: bool,
) -> DrawResult {
    let panel_h = (win_h - chrome_h).max(0);
    let inner_x = panel_x + RESIZE_HANDLE_W;   // content starts after the handle
    let inner_w = (console_w - RESIZE_HANDLE_W).max(0);

    // ── Resize handle ────────────────────────────────────────────────────────
    let resize_rect = Rect::new(panel_x, chrome_h, RESIZE_HANDLE_W as u32, panel_h as u32);
    let (hr, hg, hb) = if resize_hot { HANDLE_HL } else { HANDLE_BG };
    canvas.set_draw_color(Color::RGB(hr, hg, hb));
    let _ = canvas.fill_rect(resize_rect);

    // ── Panel background ─────────────────────────────────────────────────────
    let (br, bg_c, bb) = BG;
    canvas.set_draw_color(Color::RGB(br, bg_c, bb));
    let _ = canvas.fill_rect(Rect::new(inner_x, chrome_h, inner_w as u32, panel_h as u32));

    // ── Left border line (after handle) ──────────────────────────────────────
    let (bor, bog, bob) = BORDER;
    canvas.set_draw_color(Color::RGB(bor, bog, bob));
    let _ = canvas.fill_rect(Rect::new(inner_x, chrome_h, 1, panel_h as u32));

    // ── Header bar ───────────────────────────────────────────────────────────
    let (hdr_r, hdr_g, hdr_b) = HEADER_BG;
    canvas.set_draw_color(Color::RGB(hdr_r, hdr_g, hdr_b));
    let _ = canvas.fill_rect(Rect::new(inner_x, chrome_h, inner_w as u32, HEADER_H as u32));

    // Header bottom border
    canvas.set_draw_color(Color::RGB(bor, bog, bob));
    let _ = canvas.fill_rect(Rect::new(inner_x, chrome_h + HEADER_H - 1, inner_w as u32, 1));

    // ── × close button ───────────────────────────────────────────────────────
    let close_x    = panel_x + console_w - CLOSE_BTN_W;
    let close_rect = Rect::new(close_x, chrome_h, CLOSE_BTN_W as u32, HEADER_H as u32);
    canvas.set_draw_color(Color::RGB(70, 28, 28));
    let _ = canvas.fill_rect(close_rect);

    if let Some(font) = fonts.get(FONT_SIZE, true, false) {
        let (cr, cg, cb) = CLOSE_FG;
        if let Ok(surf) = font.render("×").blended(Color::RGB(cr, cg, cb)) {
            if let Ok(tex) = tc.create_texture_from_surface(&surf) {
                let sw = surf.width() as i32;
                let sh = surf.height() as i32;
                let tx = close_x + (CLOSE_BTN_W - sw) / 2;
                let ty = chrome_h + (HEADER_H - sh) / 2;
                let _ = canvas.copy(&tex, None,
                    Rect::new(tx, ty, sw as u32, sh as u32));
            }
        }
    }

    // ── Header label ─────────────────────────────────────────────────────────
    if let Some(font) = fonts.get(FONT_SIZE, true, false) {
        let (fr, fg_c, fb) = HEADER_FG;
        let label = format!("Console  ({} entries)", entries.len());
        if let Ok(surf) = font.render(&label).blended(Color::RGB(fr, fg_c, fb)) {
            if let Ok(tex) = tc.create_texture_from_surface(&surf) {
                let sw = surf.width() as i32;
                let sh = surf.height() as i32;
                let ty = chrome_h + (HEADER_H - sh) / 2;
                let max_label_w = (close_x - inner_x - PAD_X * 2).max(0) as u32;
                let draw_w = (sw as u32).min(max_label_w);
                if draw_w > 0 {
                    let _ = canvas.copy(
                        &tex,
                        Some(Rect::new(0, 0, draw_w, sh as u32)),
                        Rect::new(inner_x + PAD_X, ty, draw_w, sh as u32),
                    );
                }
            }
        }
    }

    // ── Log rows ─────────────────────────────────────────────────────────────
    let list_top    = chrome_h + HEADER_H;
    let list_h      = panel_h - HEADER_H;
    let visible     = list_h / LINE_H + 1;
    let total       = entries.len() as i32;
    let first       = (scroll / LINE_H).clamp(0, (total - 1).max(0)) as usize;

    if let Some(font) = fonts.get(FONT_SIZE, false, false) {
        for i in 0..=visible as usize {
            let idx = first + i;
            if idx >= entries.len() { break; }

            let entry = &entries[idx];
            let row_y = list_top + (i as i32 * LINE_H) - (scroll % LINE_H);

            // Row tint
            match entry.level {
                ConsoleLevel::Warn => {
                    let (wr, wg, wb) = WARN_BG;
                    canvas.set_draw_color(Color::RGB(wr, wg, wb));
                    let _ = canvas.fill_rect(Rect::new(
                        inner_x, row_y, inner_w as u32, LINE_H as u32));
                }
                ConsoleLevel::Error => {
                    let (er, eg, eb) = ERR_BG;
                    canvas.set_draw_color(Color::RGB(er, eg, eb));
                    let _ = canvas.fill_rect(Rect::new(
                        inner_x, row_y, inner_w as u32, LINE_H as u32));
                }
                ConsoleLevel::Log => {}
            }

            // Text
            let (prefix, fg) = match entry.level {
                ConsoleLevel::Log   => ("LOG  ", LOG_FG),
                ConsoleLevel::Warn  => ("WARN ", WARN_FG),
                ConsoleLevel::Error => ("ERR  ", ERR_FG),
            };
            let (fr, fg_c, fb) = fg;
            let line = format!("{}{}", prefix, entry.message);
            if let Ok(surf) = font.render(&line).blended(Color::RGB(fr, fg_c, fb)) {
                if let Ok(tex) = tc.create_texture_from_surface(&surf) {
                    let sw = surf.width() as i32;
                    let sh = surf.height() as i32;
                    let ty = row_y + (LINE_H - sh) / 2;
                    let max_w = (inner_w - PAD_X * 2).max(0) as u32;
                    let draw_w = (sw as u32).min(max_w);
                    if draw_w > 0 {
                        let _ = canvas.copy(
                            &tex,
                            Some(Rect::new(0, 0, draw_w, sh as u32)),
                            Rect::new(inner_x + PAD_X, ty, draw_w, sh as u32),
                        );
                    }
                }
            }
        }
    }

    DrawResult { close_btn: close_rect, resize_handle: resize_rect }
}

/// Maximum scroll value so the last entry stays visible.
pub fn max_scroll(entries: &[ConsoleEntry], panel_h: i32) -> i32 {
    let total_h  = entries.len() as i32 * LINE_H;
    let visible_h = (panel_h - HEADER_H).max(0);
    (total_h - visible_h).max(0)
}

/// Returns true if window-absolute `x` is inside the panel.
pub fn hit_panel(panel_x: i32, win_w: i32, x: i32) -> bool {
    x >= panel_x && x < win_w
}
