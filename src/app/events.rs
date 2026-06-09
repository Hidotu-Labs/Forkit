use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::{Keycode, Mod};
use sdl2::mouse::MouseButton;

use crate::ui::searchbar::BarRegion;
use crate::ui::tabbar::TAB_BAR_HEIGHT;
use crate::window::window::SCROLL_STEP;
use super::browser::Browser;

/// Process one SDL event.  Returns `true` if the app should quit.
pub fn handle_event(browser: &mut Browser, event: Event) -> bool {
    match event {
        Event::Quit { .. } => return true,

        // ---- Mouse button ----
        Event::MouseButtonDown { mouse_btn: MouseButton::Left, x, y, .. } => {
            let (win_w, _) = browser.window.canvas.output_size()
                .map(|(w, h)| (w as i32, h as i32))
                .unwrap_or((1024, 768));

            // Tab bar region
            if y < TAB_BAR_HEIGHT {
                browser.handle_click(x, y);
                return false;
            }

            // Address bar region (y relative to tab strip bottom)
            let bar_y = y - TAB_BAR_HEIGHT;
            if bar_y < crate::ui::searchbar::BAR_HEIGHT {
                let region = browser.bar.on_click(x, bar_y, win_w);
                browser.need_draw = true;
                match region {
                    BarRegion::Back    => browser.go_back(),
                    BarRegion::Forward => browser.go_forward(),
                    BarRegion::Input   => { /* focus set by on_click */ }
                    BarRegion::None    => {}
                }
                return false;
            }

            // Content area — let browser dispatch link clicks
            browser.handle_click(x, y);
        }

        // ---- Text input into the address bar ----
        Event::TextInput { text, .. } => {
            browser.bar.on_text_input(&text);
            browser.need_draw = true;
        }

        // ---- Key events ----
        Event::KeyDown { keycode: Some(k), keymod, .. } => {
            handle_key(browser, k, keymod);
        }

        // ---- Mouse wheel ----
        Event::MouseWheel { y, .. } => {
            if !browser.bar.focused {
                browser.scroll_by(-y * SCROLL_STEP);
            }
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
            // Tab while bar is focused: open new tab
            Keycode::T if ctrl                 => {
                browser.bar.focused = false;
                browser.open_blank_tab();
            }
            _ => {}
        }
        return;
    }

    // ---- Tab management ----
    match k {
        Keycode::T if ctrl => {
            browser.open_blank_tab();
            return;
        }
        Keycode::W if ctrl => {
            let i = browser.active;
            browser.close_tab(i);
            return;
        }
        Keycode::Tab if ctrl && shift => {
            browser.prev_tab();
            return;
        }
        Keycode::Tab if ctrl => {
            browser.next_tab();
            return;
        }
        // Ctrl+1..9 — jump to tab by index
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

    // ---- Page-level keybindings ----
    match k {
        // Navigation
        Keycode::Left  if alt => browser.go_back(),
        Keycode::Right if alt => browser.go_forward(),

        // Scrolling
        Keycode::Down  | Keycode::J  => browser.scroll_by( SCROLL_STEP),
        Keycode::Up    | Keycode::K  => browser.scroll_by(-SCROLL_STEP),
        Keycode::PageDown            => browser.page_down(),
        Keycode::PageUp              => browser.page_up(),
        Keycode::Home                => browser.scroll_to_top(),

        // Reload
        Keycode::R | Keycode::F5     => browser.reload(),

        // Focus address bar
        Keycode::L | Keycode::F6     => { browser.bar.focused = true; browser.need_draw = true; }

        _ => {}
    }
}
