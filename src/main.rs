mod dom;
mod render;
mod window;

use std::env;
use std::fs;
use std::path::Path;

use sdl2::keyboard::Keycode;

use dom::parser::parse;
use dom::node::Node;
use render::font::FontCache;
use render::renderer::render;
use window::window::{AppWindow, DEFAULT_H, SCROLL_STEP};

// ---------------------------------------------------------------------------
// File loading
// ---------------------------------------------------------------------------

fn load_dom(path: &str) -> Option<Node> {
    let html = fs::read_to_string(Path::new(path))
        .map_err(|e| eprintln!("Cannot open {}: {}", path, e))
        .ok()?;
    Some(parse(&html))
}

// ---------------------------------------------------------------------------
// Safe event polling — bypasses sdl2-0.37's panicking transmute on unknown
// SDL event type values (e.g. 0x207 SDL_DROPFILE from the sdl2-compat layer).
// We read raw events, skip any whose type we don't recognise, and convert the
// rest with Event::from_ll.
// ---------------------------------------------------------------------------

/// Known SDL2 event type ranges we actually care about.
fn is_known_event_type(t: u32) -> bool {
    matches!(t,
        0x100        // SDL_QUIT
        | 0x200..=0x203  // SDL_WINDOWEVENT, SDL_SYSWMEVENT
        | 0x300..=0x303  // SDL_KEYDOWN/UP, SDL_TEXTEDITING, SDL_TEXTINPUT
        | 0x400..=0x403  // SDL_MOUSEMOTION, SDL_MOUSEBUTTONDOWN/UP, SDL_MOUSEWHEEL
        | 0x8000..=0xFFFF // user events
    )
}

fn next_event() -> Option<sdl2::event::Event> {
    loop {
        let mut raw = unsafe { std::mem::zeroed::<sdl2::sys::SDL_Event>() };
        if unsafe { sdl2::sys::SDL_PollEvent(&mut raw) } == 0 {
            return None; // queue empty
        }
        let t = unsafe { raw.type_ };
        if !is_known_event_type(t) {
            continue; // drop unknown types silently
        }
        return Some(sdl2::event::Event::from_ll(raw));
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = env::args().collect();
    let html_file = args.get(1).map(String::as_str).unwrap_or("assets/test.html");

    // --- SDL2 init ---
    let sdl     = sdl2::init().expect("SDL2 init failed");
    let ttf_ctx = sdl2::ttf::init().expect("TTF init failed");

    let mut app_window = AppWindow::new(&sdl, html_file)
        .expect("Failed to create window");

    let mut fonts     = FontCache::new(&ttf_ctx);
    let mut dom       = load_dom(html_file).expect("Failed to load HTML");
    let mut scroll_y  = 0i32;
    let mut need_draw = true;

    // EventPump must be alive to keep SDL's event queue pumping, even though
    // we poll manually via SDL_PollEvent.
    let _event_pump = sdl.event_pump().expect("Event pump failed");

    'main: loop {
        // --- Events ---
        while let Some(event) = next_event() {
            use sdl2::event::{Event, WindowEvent};

            match event {
                Event::Quit { .. } => break 'main,

                Event::KeyDown { keycode: Some(k), .. } => match k {
                    Keycode::Q | Keycode::Escape => break 'main,

                    Keycode::Down | Keycode::J => {
                        scroll_y  += SCROLL_STEP;
                        need_draw  = true;
                    }
                    Keycode::Up | Keycode::K => {
                        scroll_y   = (scroll_y - SCROLL_STEP).max(0);
                        need_draw  = true;
                    }
                    Keycode::PageDown => {
                        scroll_y  += DEFAULT_H as i32;
                        need_draw  = true;
                    }
                    Keycode::PageUp => {
                        scroll_y   = (scroll_y - DEFAULT_H as i32).max(0);
                        need_draw  = true;
                    }
                    Keycode::Home => {
                        scroll_y  = 0;
                        need_draw = true;
                    }
                    Keycode::R => {
                        if let Some(new_dom) = load_dom(html_file) {
                            dom      = new_dom;
                            scroll_y = 0;
                        }
                        need_draw = true;
                    }
                    _ => {}
                },

                Event::MouseWheel { y, .. } => {
                    scroll_y  = (scroll_y - y * SCROLL_STEP).max(0);
                    need_draw = true;
                }

                Event::Window { win_event: WindowEvent::Resized(..)
                              | WindowEvent::Exposed, .. } => {
                    need_draw = true;
                }

                _ => {}
            }
        }

        // --- Render ---
        if need_draw {
            let ctx = app_window.make_ctx(scroll_y);
            let _boxes = render(&mut app_window.canvas, &mut fonts, &ctx, &dom);
            app_window.canvas.present();
            need_draw = false;
        } else {
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    }
}
