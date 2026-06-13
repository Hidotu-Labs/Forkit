use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Keycode, Mod};
use sdl2::mouse::MouseButton;

use crate::ui::searchbar::BarRegion;
use crate::ui::tabbar::TAB_BAR_HEIGHT;
use crate::ui::console;
use crate::window::window::SCROLL_STEP;
use super::browser::{Browser, chrome_height, SCROLLBAR_W};

/// Process one SDL event.  Returns `true` if the app should quit.
pub fn handle_event(browser: &mut Browser, event: Event) -> bool {
    match event {
        Event::Quit { .. } => return true,

        // ---- Mouse button down ----
        Event::MouseButtonDown { mouse_btn: MouseButton::Left, x, y, .. } => {
            let (win_w, win_h) = browser.window.canvas.output_size()
                .map(|(w, h)| (w as i32, h as i32))
                .unwrap_or((1024, 768));

            let chrome_h  = chrome_height();
            let console_w = if browser.console_open { browser.console_w } else { 0 };
            let content_w = win_w - console_w;
            let content_h = (win_h - chrome_h).max(0);

            // Console × close button
            if let Some(btn) = browser.console_close_btn {
                if btn.contains_point(sdl2::rect::Point::new(x, y)) {
                    browser.console_open        = false;
                    browser.console_close_btn   = None;
                    browser.console_resize_rect = None;
                    browser.need_draw = true;
                    return false;
                }
            }

            // Console resize handle — start drag
            if let Some(handle) = browser.console_resize_rect {
                if handle.contains_point(sdl2::rect::Point::new(x, y)) {
                    browser.console_resize_drag = Some((x, browser.console_w));
                    return false;
                }
            }

            // Tab bar region
            if y < TAB_BAR_HEIGHT {
                browser.handle_click(x, y);
                return false;
            }

            // Address bar region
            let bar_y = y - TAB_BAR_HEIGHT;
            if bar_y < crate::ui::searchbar::BAR_HEIGHT {
                let region = browser.bar.on_click(x, bar_y, win_w);
                browser.need_draw = true;
                match region {
                    BarRegion::Back    => browser.go_back(),
                    BarRegion::Forward => browser.go_forward(),
                    BarRegion::Input   => {}
                    BarRegion::None    => {}
                }
                return false;
            }

            // Block clicks that land inside the console panel body
            if browser.console_open && x >= win_w - browser.console_w {
                return false;
            }

            // Scrollbar strip (right edge of content area)
            let content_y = y - chrome_h;
            if x >= content_w - SCROLLBAR_W && x < content_w {
                if let Some((_track_y, _track_h, thumb_y, thumb_h)) =
                    browser.scrollbar_geometry(content_h)
                {
                    if content_y >= thumb_y && content_y < thumb_y + thumb_h {
                        browser.scrollbar_drag = Some(content_y - thumb_y);
                    } else {
                        let new_scroll = browser.thumb_y_to_scroll(content_y, content_h);
                        browser.tabs[browser.active].scroll_y = new_scroll;
                        browser.clamp_scroll();
                        browser.need_draw = true;
                    }
                }
                return false;
            }

            // Content area
            browser.handle_click(x, y);
        }

        // ---- Mouse button up ----
        Event::MouseButtonUp { mouse_btn: MouseButton::Left, .. } => {
            if browser.scrollbar_drag.take().is_some() {
                browser.need_draw = true;
            }
            if browser.console_resize_drag.take().is_some() {
                browser.need_draw = true;
            }
        }

        // ---- Mouse motion ----
        Event::MouseMotion { x, y, .. } => {
            let (win_w, win_h) = browser.window.canvas.output_size()
                .map(|(w, h)| (w as i32, h as i32))
                .unwrap_or((1024, 768));
            let chrome_h  = chrome_height();
            let content_y = y - chrome_h;

            browser.mouse_x = x;
            browser.mouse_y = content_y;

            // Hover detection on element for :hover
            let hovered = browser.tab().find_element_at(x, content_y);
            if browser.tab().hovered_ptr != hovered {
                browser.tab_mut().hovered_ptr = hovered;
                browser.need_draw = true;
            }

            // ── Console resize drag ───────────────────────────────────────
            if let Some((drag_start_x, drag_start_w)) = browser.console_resize_drag {
                // Dragging left increases panel width (handle is on the left edge)
                let new_w = (drag_start_w + (drag_start_x - x))
                    .clamp(console::CONSOLE_MIN_W, console::CONSOLE_MAX_W)
                    .min(win_w - 200); // always leave at least 200px for the page
                if new_w != browser.console_w {
                    browser.console_w = new_w;
                    browser.need_draw = true;
                }
                return false;
            }

            // ── Resize handle hover highlight ─────────────────────────────
            if browser.console_open {
                let is_hot = browser.console_resize_rect
                    .map(|r| r.contains_point(sdl2::rect::Point::new(x, y)))
                    .unwrap_or(false);
                if is_hot != browser.console_resize_hot {
                    browser.console_resize_hot = is_hot;
                    browser.need_draw = true;
                }
            }

            // ── Page scrollbar drag ───────────────────────────────────────
            if let Some(drag_offset) = browser.scrollbar_drag {
                let content_h = (win_h - chrome_height()).max(0);
                let thumb_top = y - chrome_height() - drag_offset;
                let new_scroll = browser.thumb_y_to_scroll(thumb_top, content_h);
                browser.tabs[browser.active].scroll_y = new_scroll;
                browser.clamp_scroll();
                browser.need_draw = true;
            }
        }

        // ---- Text input ----
        Event::TextInput { text, .. } => {
            if browser.tab().focused_input.is_some() {
                browser.tab_mut().type_text(&text);
                browser.need_draw = true;
            } else {
                browser.bar.on_text_input(&text);
                browser.need_draw = true;
            }
        }

        // ---- Key events ----
        Event::KeyDown { keycode: Some(k), keymod, .. } => {
            handle_key(browser, k, keymod);
        }

        // ---- Mouse wheel ----
        Event::MouseWheel { y, .. } => {
            if browser.bar.focused { return false; }

            // Scroll console if cursor is inside the panel
            if browser.console_open {
                let (win_w, win_h) = browser.window.canvas.output_size()
                    .map(|(w, h)| (w as i32, h as i32))
                    .unwrap_or((1024, 768));
                let panel_x = win_w - browser.console_w;
                if browser.mouse_x >= panel_x {
                    let panel_h = (win_h - chrome_height()).max(0);
                    let entries = &browser.tabs[browser.active].console_entries;
                    let max = console::max_scroll(entries, panel_h);
                    let tab = &mut browser.tabs[browser.active];
                    tab.console_scroll = (tab.console_scroll - y * SCROLL_STEP).clamp(0, max);
                    browser.need_draw = true;
                    return false;
                }
            }

            browser.scroll_by(-y * SCROLL_STEP);
        }

        // ---- Window resize / expose ----
        Event::Window {
            win_event: WindowEvent::Resized(..) | WindowEvent::Exposed, ..
        } => {
            browser.need_draw = true;
        }

        _ => {}
    }

    false
}

fn handle_key(browser: &mut Browser, k: Keycode, mods: Mod) {
    let ctrl  = mods.contains(Mod::LCTRLMOD) || mods.contains(Mod::RCTRLMOD);
    let shift = mods.contains(Mod::LSHIFTMOD) || mods.contains(Mod::RSHIFTMOD);
    let alt   = mods.contains(Mod::LALTMOD)  || mods.contains(Mod::RALTMOD);

    if browser.bar.focused {
        match k {
            Keycode::Backspace                 => { browser.bar.on_backspace(); browser.need_draw = true; }
            Keycode::Return | Keycode::KpEnter => { browser.bar.on_enter();    browser.need_draw = true; }
            Keycode::Escape                    => { browser.bar.focused = false; browser.need_draw = true; }
            Keycode::A if ctrl                 => { browser.bar.url.clear();   browser.need_draw = true; }
            Keycode::T if ctrl                 => {
                browser.bar.focused = false;
                browser.open_blank_tab();
            }
            _ => {}
        }
        return;
    }

    if browser.tab().focused_input.is_some() {
        match k {
            Keycode::Backspace => { browser.tab_mut().backspace(); browser.need_draw = true; }
            Keycode::Escape    => { browser.tab_mut().focused_input = None; browser.need_draw = true; }
            Keycode::A if ctrl => {
                if let Some(idx) = browser.tab().focused_input {
                    browser.tab_mut().ensure_input_slot(idx);
                    browser.tab_mut().input_values[idx].clear();
                    browser.need_draw = true;
                }
            }
            _ => {}
        }
        return;
    }

    // Tab management
    match k {
        Keycode::T if ctrl => { browser.open_blank_tab(); return; }
        Keycode::W if ctrl => { let i = browser.active; browser.close_tab(i); return; }
        Keycode::Tab if ctrl && shift => { browser.prev_tab(); return; }
        Keycode::Tab if ctrl          => { browser.next_tab(); return; }
        Keycode::Num1 if ctrl => { browser.switch_tab(0); return; }
        Keycode::Num2 if ctrl => { browser.switch_tab(1); return; }
        Keycode::Num3 if ctrl => { browser.switch_tab(2); return; }
        Keycode::Num4 if ctrl => { browser.switch_tab(3); return; }
        Keycode::Num5 if ctrl => { browser.switch_tab(4); return; }
        Keycode::Num6 if ctrl => { browser.switch_tab(5); return; }
        Keycode::Num7 if ctrl => { browser.switch_tab(6); return; }
        Keycode::Num8 if ctrl => { browser.switch_tab(7); return; }
        Keycode::Num9 if ctrl => { browser.switch_tab(8); return; }
        _ => {}
    }

    // Page-level bindings
    match k {
        Keycode::Left  if alt => browser.go_back(),
        Keycode::Right if alt => browser.go_forward(),

        Keycode::Down  | Keycode::J  => browser.scroll_by( SCROLL_STEP),
        Keycode::Up    | Keycode::K  => browser.scroll_by(-SCROLL_STEP),
        Keycode::PageDown            => browser.page_down(),
        Keycode::PageUp              => browser.page_up(),
        Keycode::Home                => browser.scroll_to_top(),

        Keycode::R | Keycode::F5     => browser.reload(),

        Keycode::L | Keycode::F6     => { browser.bar.focused = true; browser.need_draw = true; }

        // Toggle developer console
        Keycode::F12 => {
            browser.console_open = !browser.console_open;
            browser.need_draw = true;
        }

        _ => {}
    }
}
