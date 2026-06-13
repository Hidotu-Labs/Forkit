use sdl2::pixels::Color;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::image::ImageRWops;

use crate::dom::node::Style;
use crate::render::font::FontCache;
use crate::render::image::ImageCache;
use crate::render::layout::paint::{
    paint_text, measure_text, fill_rect_alpha, fill_rounded_rect,
};
use crate::render::layout::state::{AudioArea, LayoutState, LINE_SPACING, BLOCK_MARGIN};

// ── Geometric icon helpers ────────────────────────────────────────────────

/// Draw a filled triangle (play icon ▶) with apex pointing right.
fn draw_play_icon(canvas: &mut Canvas<Window>, cx: i32, cy: i32, size: i32, color: Color, scroll_y: i32) {
    let half  = size / 2;
    let shift = size / 5; // shift right so it looks centred in the circle
    let tip_x = cx + half - shift + size / 8;
    let top_y = cy - half + size / 8;
    let bot_y = cy + half - size / 8;
    let lft_x = cx - half + shift;
    // Fill as a series of horizontal scanlines between left edge and right tip
    for y in top_y..=bot_y {
        // linear interpolation: at top_y → x = lft_x, at bot_y → x = lft_x
        // left edge is always lft_x; right edge narrows to tip
        let t  = (y - top_y) as f32 / (bot_y - top_y).max(1) as f32;
        // Mirror t so we go from lft → tip → lft
        let t2 = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
        let rx = lft_x + ((tip_x - lft_x) as f32 * t2) as i32;
        let ry = y - scroll_y;
        if rx > lft_x && ry >= 0 {
            canvas.set_draw_color(color);
            let _ = canvas.draw_line((lft_x, ry), (rx, ry));
        }
    }
}

/// Draw two vertical bars side by side (pause icon ⏸).
fn draw_pause_icon(canvas: &mut Canvas<Window>, cx: i32, cy: i32, size: i32, color: Color, scroll_y: i32) {
    let bar_w = (size / 5).max(2);
    let bar_h = size * 3 / 4;
    let gap   = (size / 6).max(2);
    let top_y = cy - bar_h / 2;
    let bot_y = cy + bar_h / 2;
    let left_x  = cx - gap / 2 - bar_w;
    let right_x = cx + gap / 2;
    canvas.set_draw_color(color);
    for x in left_x..left_x + bar_w {
        let _ = canvas.draw_line((x, top_y - scroll_y), (x, bot_y - scroll_y));
    }
    for x in right_x..right_x + bar_w {
        let _ = canvas.draw_line((x, top_y - scroll_y), (x, bot_y - scroll_y));
    }
}

/// Draw a simple note glyph (stem + flag).
fn draw_note_icon(canvas: &mut Canvas<Window>, cx: i32, cy: i32, size: i32, color: Color, scroll_y: i32) {
    let head_r  = (size / 4).max(3);
    let stem_h  = size / 2;
    let head_cx = cx - size / 8;
    let head_cy = cy + size / 4;
    // Oval head (2:1 ratio)
    for dy in -head_r / 2..=head_r / 2 {
        let dx = (head_r as f32 * (1.0 - (dy as f32 / (head_r as f32 / 2.0)).powi(2)).sqrt()) as i32;
        canvas.set_draw_color(color);
        let _ = canvas.draw_line(
            (head_cx - dx, head_cy + dy - scroll_y),
            (head_cx + dx, head_cy + dy - scroll_y),
        );
    }
    // Stem
    let stem_x = head_cx + head_r;
    canvas.set_draw_color(color);
    let _ = canvas.draw_line((stem_x, head_cy - scroll_y), (stem_x, head_cy - stem_h - scroll_y));
}

pub fn paint_image(
    ls:          &mut LayoutState,
    canvas:      &mut Canvas<Window>,
    tc:          &TextureCreator<WindowContext>,
    images:      &mut ImageCache,
    base_url:    &str,
    src:         &str,
    width_hint:  Option<i32>,
    height_hint: Option<i32>,
    max_w:       i32,
    style:       &crate::dom::node::Style,
) {
    let ml = style.margin.left;
    let mr = style.margin.right;
    let mt = style.margin.top;
    let mb = style.margin.bottom;
    let pl = style.padding.left;
    let pr = style.padding.right;
    let pt = style.padding.top;
    let pb = style.padding.bottom;

    let bytes = match images.get_bytes(src, base_url) {
        Some(b) => b,
        None    => return,
    };
    let fmt = crate::render::image::sniff_image_type(bytes);
    // For SVG files: pre-process to fix <style> block CSS that nanosvg ignores
    let preprocessed: Vec<u8>;
    let svg_alpha: u8;
    let bytes = if fmt == "SVG" {
        let (p, a) = crate::render::image::preprocess_svg(bytes);
        preprocessed = p;
        svg_alpha = a;
        &preprocessed[..]
    } else {
        svg_alpha = 255;
        bytes
    };
    let rwops = match sdl2::rwops::RWops::from_bytes(bytes) {
        Ok(r)  => r,
        Err(_) => return,
    };
    let surface = match rwops.load_typed(fmt) {
        Ok(s)  => s,
        Err(_) => return,
    };
    let (nat_w, nat_h) = (surface.width() as i32, surface.height() as i32);
    let (dw, dh) = match (width_hint, height_hint) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None)    => { let h = if nat_w > 0 { w * nat_h / nat_w } else { nat_h }; (w, h) }
        (None, Some(h))    => { let w = if nat_h > 0 { h * nat_w / nat_h } else { nat_w }; (w, h) }
        (None, None) => {
            let avail = (max_w - ls.cursor_x - ml - pl - pr - mr).max(1);
            if nat_w > avail {
                let h = if nat_w > 0 { avail * nat_h / nat_w } else { nat_h };
                (avail, h)
            } else {
                (nat_w, nat_h)
            }
        }
    };
    if dw <= 0 || dh <= 0 { return; }

    let total_w = ml + pl + dw + pr + mr;

    if ls.cursor_x + total_w > max_w && ls.cursor_x > ls.margin_left {
        ls.cursor_y += ls.line_height + LINE_SPACING;
        ls.cursor_x  = ls.margin_left + ls.indent;
        ls.line_height = 0;
    }

    let box_x = ls.cursor_x + ml;
    let img_x = box_x + pl;
    let box_y = ls.cursor_y + mt;
    let img_y = box_y + pt;

    if let Some(bg) = style.bg_color {
        let box_w = (pl + dw + pr).max(0) as u32;
        let box_h = (pt + dh + pb).max(0) as u32;
        let screen_y = box_y - ls.ctx.scroll_y;
        if box_w > 0 && box_h > 0 && screen_y + box_h as i32 > 0 && screen_y < ls.ctx.viewport_height {
            canvas.set_draw_color(Color::RGBA(bg[0], bg[1], bg[2], style.bg_alpha));
            let _ = canvas.fill_rect(sdl2::rect::Rect::new(box_x, screen_y, box_w, box_h));
        }
    }

    let screen_y = img_y - ls.ctx.scroll_y;
    if screen_y + dh > 0 && screen_y < ls.ctx.viewport_height {
        if let Ok(mut tex) = tc.create_texture_from_surface(&surface) {
            let _ = tex.set_blend_mode(sdl2::render::BlendMode::Blend);
            let _ = tex.set_alpha_mod(svg_alpha);
            let _ = canvas.copy(&tex, None, sdl2::rect::Rect::new(img_x, screen_y, dw as u32, dh as u32));
        }
    }

    ls.cursor_x += total_w;

    let full_h = mt + pt + dh + pb + mb;
    if full_h > ls.line_height { ls.line_height = full_h; }
}

pub fn paint_media_placeholder(
    ls:     &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc:     &TextureCreator<WindowContext>,
    fonts:  &mut FontCache,
    kind:   &str,
    dw:     i32,
    dh:     i32,
    s:      &Style,
    _max_w:  i32,
) {
    if ls.cursor_x > ls.margin_left + ls.indent {
        ls.cursor_y += ls.line_height + LINE_SPACING;
        ls.cursor_x  = ls.margin_left + ls.indent;
    }
    ls.cursor_y += BLOCK_MARGIN;

    let x  = ls.cursor_x;
    let y  = ls.cursor_y;
    let ry = y - ls.ctx.scroll_y;
    let bg = s.bg_color.unwrap_or([30, 30, 30]);

    fill_rect_alpha(canvas, Color::RGB(bg[0], bg[1], bg[2]), 255,
                    x, y, dw, dh, ls.rounding_clip.as_ref(),
                    ls.ctx.scroll_y, ls.ctx.viewport_height);

    let label = match kind {
        "video"  => "video",
        "audio"  => "audio",
        "canvas" => "canvas",
        _        => kind,
    };
    let label_style = Style { color: [200, 200, 200], font_size: 13, ..Default::default() };
    if ry + dh > 0 && ry < ls.ctx.viewport_height {
        // Draw a note icon for audio, a triangle for video
        if kind == "audio" {
            draw_note_icon(canvas, x + 20, y + dh / 2, 18, Color::RGB(200, 200, 200), ls.ctx.scroll_y);
        } else if kind == "video" {
            draw_play_icon(canvas, x + 20, y + dh / 2, 18, Color::RGB(200, 200, 200), ls.ctx.scroll_y);
        }
        let (lw, lh2) = measure_text(fonts, label, &label_style);
        let lx = x + (dw - lw) / 2;
        let ly = y + (dh - lh2) / 2;
        paint_text(canvas, tc, fonts, label, &label_style, lx, ly,
                   ls.rounding_clip.as_ref(),
                   ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    ls.cursor_y += dh + BLOCK_MARGIN;
    ls.cursor_x  = ls.margin_left;
    ls.line_height = 16;
}

/// Paint a default HTML5-style audio player and register an [`AudioArea`]
/// so that click events can drive playback.
///
/// Layout:
/// ```text
/// ┌──────────────────────────────────────────────────────┐
/// │ ♪  [▶]  ══════════════════●══════  0:00 / 0:00      │
/// └──────────────────────────────────────────────────────┘
/// ```
pub fn paint_audio_player(
    ls:    &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc:    &TextureCreator<WindowContext>,
    fonts: &mut FontCache,
    src:   &str,
    dw:    i32,
    dh:    i32,
    s:     &Style,
) {
    // Assign a unique per-page index to this player.
    let idx = ls.audio_count;
    ls.audio_count += 1;

    // Look up playback snapshot by index (defaults to stopped/zero).
    let pb = ls.audio_playback.get(&idx).cloned().unwrap_or_default();
    let playing       = pb.playing;
    let progress      = pb.progress;
    let duration_secs = pb.duration_secs;
    let position_secs = pb.position_secs;

    // ── Block-level: break to its own line ───────────────────────────────
    if ls.cursor_x > ls.margin_left + ls.indent {
        ls.cursor_y += ls.line_height + LINE_SPACING;
        ls.cursor_x  = ls.margin_left + ls.indent;
    }
    ls.cursor_y += BLOCK_MARGIN;

    let x  = ls.cursor_x;
    let y  = ls.cursor_y;
    let ry = y - ls.ctx.scroll_y;

    // Only paint if at least partially visible
    if ry + dh <= 0 || ry >= ls.ctx.viewport_height {
        ls.cursor_y   += dh + BLOCK_MARGIN;
        ls.cursor_x    = ls.margin_left;
        ls.line_height = 16;
        return;
    }

    // ── Background ───────────────────────────────────────────────────────
    let bg = s.bg_color.unwrap_or([40, 40, 40]);
    let radii = s.border_radius;
    fill_rounded_rect(canvas, Color::RGB(bg[0], bg[1], bg[2]), 255,
                      x, y, dw, dh, radii,
                      ls.ctx.scroll_y, ls.ctx.viewport_height);

    let pad = 10i32;
    let cy  = y + dh / 2;

    // ── Note icon (geometric) ─────────────────────────────────────────────
    let icon_size = 14i32;
    let nx = x + pad;
    draw_note_icon(canvas, nx + icon_size / 2, cy, icon_size,
                   Color::RGB(160, 160, 160), ls.ctx.scroll_y);

    // ── Play / Pause button ───────────────────────────────────────────────
    let btn_size = 28i32;
    let btn_x    = nx + icon_size + 8;
    let btn_y    = cy - btn_size / 2;
    let btn_cx   = btn_x + btn_size / 2;
    let btn_cy   = cy;

    fill_rounded_rect(canvas, Color::RGB(80, 80, 80), 255,
                      btn_x, btn_y, btn_size, btn_size,
                      [btn_size as u16 / 2; 4],
                      ls.ctx.scroll_y, ls.ctx.viewport_height);

    let icon_col = Color::RGB(220, 220, 220);
    if playing {
        draw_pause_icon(canvas, btn_cx, btn_cy, btn_size - 8, icon_col, ls.ctx.scroll_y);
    } else {
        draw_play_icon(canvas, btn_cx, btn_cy, btn_size - 10, icon_col, ls.ctx.scroll_y);
    }

    // ── Time display (right side) ─────────────────────────────────────────
    let time_style = Style { color: [180, 180, 180], font_size: 11, ..Default::default() };
    let time_str   = format!("{} / {}", fmt_time(position_secs), fmt_time(duration_secs));
    let (tw, _th)  = measure_text(fonts, &time_str, &time_style);
    let time_x     = x + dw - pad - tw;
    let time_y     = cy - 7;
    paint_text(canvas, tc, fonts, &time_str, &time_style, time_x, time_y,
               ls.rounding_clip.as_ref(), ls.ctx.scroll_y, ls.ctx.viewport_height);

    // ── Scrubber track ────────────────────────────────────────────────────
    let scr_x   = btn_x + btn_size + 10;
    let scr_w   = (time_x - scr_x - 8).max(20);
    let track_h = 4i32;
    let scr_y   = cy - track_h / 2;

    fill_rounded_rect(canvas, Color::RGB(90, 90, 90), 255,
                      scr_x, scr_y, scr_w, track_h,
                      [track_h as u16 / 2; 4],
                      ls.ctx.scroll_y, ls.ctx.viewport_height);

    let fill_w = ((scr_w as f64 * progress.clamp(0.0, 1.0)) as i32).max(0);
    if fill_w > 0 {
        fill_rounded_rect(canvas, Color::RGB(30, 160, 230), 255,
                          scr_x, scr_y, fill_w, track_h,
                          [track_h as u16 / 2; 4],
                          ls.ctx.scroll_y, ls.ctx.viewport_height);
    }

    // Thumb circle
    let thumb_r  = 7i32;
    let thumb_cx = scr_x + fill_w;
    let thumb_x  = thumb_cx - thumb_r;
    let thumb_y  = cy - thumb_r;
    fill_rounded_rect(canvas, Color::RGB(0, 0, 0), 60,
                      thumb_x - 1, thumb_y - 1, thumb_r * 2 + 2, thumb_r * 2 + 2,
                      [thumb_r as u16 + 1; 4],
                      ls.ctx.scroll_y, ls.ctx.viewport_height);
    fill_rounded_rect(canvas, Color::RGB(30, 160, 230), 255,
                      thumb_x, thumb_y, thumb_r * 2, thumb_r * 2,
                      [thumb_r as u16; 4],
                      ls.ctx.scroll_y, ls.ctx.viewport_height);

    // ── Register AudioArea ────────────────────────────────────────────────
    ls.audio_areas.push(AudioArea {
        x, y, w: dw, h: dh,
        index: idx,
        src: src.to_owned(),
        play_btn: (btn_x, btn_y, btn_size, btn_size),
        scrubber: (scr_x, scr_y - 6, scr_w, track_h + 12),
    });

    // ── Advance cursor ────────────────────────────────────────────────────
    ls.cursor_y   += dh + BLOCK_MARGIN;
    ls.cursor_x    = ls.margin_left;
    ls.line_height = 16;
}

/// Format seconds as `m:ss` (e.g. 75.0 → "1:15", 0.0 → "0:00").
fn fmt_time(secs: f64) -> String {
    if secs <= 0.0 || !secs.is_finite() {
        return "0:00".to_owned();
    }
    let total = secs as u64;
    let m     = total / 60;
    let s     = total % 60;
    format!("{m}:{s:02}")
}

