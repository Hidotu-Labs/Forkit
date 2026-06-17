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
pub const CONSOLE_DEFAULT_W: i32 = 360;
/// Minimum / maximum widths the user can drag to.
pub const CONSOLE_MIN_W: i32 = 200;
pub const CONSOLE_MAX_W: i32 = 900;
/// Width of the left-edge drag handle.
pub const RESIZE_HANDLE_W: i32 = 4;
/// Width of the close button in the header.
pub const CLOSE_BTN_W: i32 = 30;

const HEADER_H:  i32 = 32;
const FONT_SIZE: u16 = 12;
const LINE_H:    i32 = 20;
const PAD_X:     i32 = 8;

const BG:        (u8,u8,u8) = (255, 255, 255);
const HEADER_BG: (u8,u8,u8) = (240, 240, 245);
const HANDLE_BG: (u8,u8,u8) = (215, 215, 225);
const HANDLE_HL: (u8,u8,u8) = (0,   120, 215);
const BORDER:    (u8,u8,u8) = (200, 200, 215);
const LOG_FG:    (u8,u8,u8) = (40,  40,  50 );
const WARN_FG:   (u8,u8,u8) = (150, 110, 0  );
const WARN_BG:   (u8,u8,u8) = (255, 250, 230);
const ERR_FG:    (u8,u8,u8) = (200, 40,  40 );
const ERR_BG:    (u8,u8,u8) = (255, 235, 235);
const HEADER_FG: (u8,u8,u8) = (60,  60,  80 );
const CLOSE_FG:  (u8,u8,u8) = (200, 50,  50 );
const ACCENT:    (u8,u8,u8) = (0,   120, 215);
const LOG_BADGE: (u8,u8,u8) = (220, 225, 240);
const WARN_BADGE:(u8,u8,u8) = (255, 235, 180);
const ERR_BADGE: (u8,u8,u8) = (255, 200, 200);

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

    // Accent top border
    let (ar, ag, ab) = ACCENT;
    canvas.set_draw_color(Color::RGB(ar, ag, ab));
    let _ = canvas.fill_rect(Rect::new(inner_x, chrome_h, inner_w as u32, 2));

    // Header bottom border
    canvas.set_draw_color(Color::RGB(bor, bog, bob));
    let _ = canvas.fill_rect(Rect::new(inner_x, chrome_h + HEADER_H - 1, inner_w as u32, 1));

    // ── × close button ───────────────────────────────────────────────────────
    let close_x    = panel_x + console_w - CLOSE_BTN_W;
    let close_rect = Rect::new(close_x, chrome_h, CLOSE_BTN_W as u32, HEADER_H as u32);
    canvas.set_draw_color(Color::RGB(240, 210, 210));
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
        let label = "Console";
        if let Ok(surf) = font.render(label).blended(Color::RGB(fr, fg_c, fb)) {
            if let Ok(tex) = tc.create_texture_from_surface(&surf) {
                let sw = surf.width() as i32;
                let sh = surf.height() as i32;
                let ty = chrome_h + (HEADER_H - sh) / 2;
                let max_label_w = (close_x - inner_x - PAD_X * 2 - 60).max(0) as u32;
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
        // Entry count badge
        let badge_text = format!("{}", entries.len());
        let badge_col  = if entries.iter().any(|e| matches!(e.level, ConsoleLevel::Error)) {
            Color::RGB(ERR_FG.0, ERR_FG.1, ERR_FG.2)
        } else if entries.iter().any(|e| matches!(e.level, ConsoleLevel::Warn)) {
            Color::RGB(WARN_FG.0, WARN_FG.1, WARN_FG.2)
        } else {
            Color::RGB(HEADER_FG.0, HEADER_FG.1, HEADER_FG.2)
        };
        if !badge_text.is_empty() {
            if let Ok(surf) = font.render(&badge_text).blended(badge_col) {
                if let Ok(tex) = tc.create_texture_from_surface(&surf) {
                    let sw = surf.width() as i32;
                    let sh = surf.height() as i32;
                    let bx = inner_x + PAD_X + 70;
                    let by = chrome_h + (HEADER_H - sh) / 2;
                    let badge_w = (sw + 10).max(20);
                    let badge_h = sh + 4;
                    let bg = if entries.iter().any(|e| matches!(e.level, ConsoleLevel::Error)) {
                        Color::RGB(ERR_BADGE.0, ERR_BADGE.1, ERR_BADGE.2)
                    } else if entries.iter().any(|e| matches!(e.level, ConsoleLevel::Warn)) {
                        Color::RGB(WARN_BADGE.0, WARN_BADGE.1, WARN_BADGE.2)
                    } else {
                        Color::RGB(LOG_BADGE.0, LOG_BADGE.1, LOG_BADGE.2)
                    };
                    canvas.set_draw_color(bg);
                    let _ = canvas.fill_rect(Rect::new(bx - 5, by - 2, badge_w as u32, badge_h as u32));
                    let _ = canvas.copy(&tex, None, Rect::new(bx, by, sw as u32, sh as u32));
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
                ConsoleLevel::Warn =>
                    { let (wr, wg, wb) = WARN_BG; canvas.set_draw_color(Color::RGB(wr, wg, wb)); }
                ConsoleLevel::Error =>
                    { let (er, eg, eb) = ERR_BG; canvas.set_draw_color(Color::RGB(er, eg, eb)); }
                ConsoleLevel::Log => { canvas.set_draw_color(Color::RGB(BG.0, BG.1, BG.2)); }
            }
            let _ = canvas.fill_rect(Rect::new(inner_x, row_y, inner_w as u32, LINE_H as u32));

            // Level pill badge
            let (badge_label, badge_bg, badge_fg) = match entry.level {
                ConsoleLevel::Log   => ("LOG ", LOG_BADGE,  LOG_FG),
                ConsoleLevel::Warn  => ("WARN", WARN_BADGE, WARN_FG),
                ConsoleLevel::Error => ("ERR ", ERR_BADGE,  ERR_FG),
            };
            let badge_w = 34_i32;
            let badge_h = LINE_H - 4;
            let badge_x = inner_x + 4;
            let badge_y = row_y + 2;
            let (bbr, bbg, bbb) = badge_bg;
            canvas.set_draw_color(Color::RGB(bbr, bbg, bbb));
            let _ = canvas.fill_rect(Rect::new(badge_x, badge_y, badge_w as u32, badge_h as u32));
            if let Ok(bsurf) = font.render(badge_label).blended(Color::RGB(badge_fg.0, badge_fg.1, badge_fg.2)) {
                if let Ok(btex) = tc.create_texture_from_surface(&bsurf) {
                    let bsw = bsurf.width() as i32;
                    let bsh = bsurf.height() as i32;
                    let btx = badge_x + (badge_w - bsw) / 2;
                    let bty = badge_y + (badge_h - bsh) / 2;
                    let draw_w = (bsw as u32).min(badge_w as u32);
                    if draw_w > 0 {
                        let _ = canvas.copy(&btex,
                            Some(Rect::new(0, 0, draw_w, bsh as u32)),
                            Rect::new(btx, bty, draw_w, bsh as u32));
                    }
                }
            }

            // Message text
            let text_x = inner_x + badge_w + 10;
            let (fr, fg_c, fb) = badge_fg;
            let line = &entry.message;
            if let Ok(surf) = font.render(line).blended(Color::RGB(fr, fg_c, fb)) {
                if let Ok(tex) = tc.create_texture_from_surface(&surf) {
                    let sw = surf.width() as i32;
                    let sh = surf.height() as i32;
                    let ty = row_y + (LINE_H - sh) / 2;
                    let max_w = (inner_w - (text_x - inner_x) - PAD_X).max(0) as u32;
                    let draw_w = (sw as u32).min(max_w);
                    if draw_w > 0 {
                        let _ = canvas.copy(
                            &tex,
                            Some(Rect::new(0, 0, draw_w, sh as u32)),
                            Rect::new(text_x, ty, draw_w, sh as u32),
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
