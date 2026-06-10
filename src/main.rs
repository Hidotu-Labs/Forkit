mod app;
mod dom;
mod net;
mod render;
mod ui;
mod window;

use std::env;

use app::browser::Browser;
use app::events::handle_event;

// ---------------------------------------------------------------------------
// Safe SDL event polling
// Bypasses sdl2-0.37's panicking transmute on unknown event type values
// (e.g. 0x207 SDL_DROPFILE emitted by the sdl2-compat layer).
// ---------------------------------------------------------------------------

fn is_known_event_type(t: u32) -> bool {
    matches!(t,
        0x100            // SDL_QUIT
        | 0x200..=0x203  // SDL_WINDOWEVENT, SDL_SYSWMEVENT
        | 0x300..=0x303  // SDL_KEYDOWN/UP, SDL_TEXTEDITING, SDL_TEXTINPUT
        | 0x400..=0x403  // SDL_MOUSEMOTION, SDL_MOUSEBUTTONDOWN/UP, SDL_MOUSEWHEEL
        | 0x8000..=0xFFFF
    )
}

fn next_event() -> Option<sdl2::event::Event> {
    loop {
        let mut raw = unsafe { std::mem::zeroed::<sdl2::sys::SDL_Event>() };
        if unsafe { sdl2::sys::SDL_PollEvent(&mut raw) } == 0 { return None; }
        let t = unsafe { raw.type_ };
        if !is_known_event_type(t) { continue; }
        return Some(sdl2::event::Event::from_ll(raw));
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = env::args().collect();
    let initial = args.get(1).map(String::as_str).unwrap_or("assets/test.html");

    let sdl     = sdl2::init().expect("SDL2 init failed");
    let ttf_ctx = sdl2::ttf::init().expect("TTF init failed");

    let mut browser = Browser::new(&sdl, &ttf_ctx, initial)
        .expect("Failed to create browser");

    // Enable SDL text input so typing reaches the address bar
    let video      = sdl.video().expect("video subsystem");
    let text_input = video.text_input();
    text_input.start();

    let _event_pump = sdl.event_pump().expect("Event pump failed");

    'main: loop {
        // --- Process events ---
        while let Some(event) = next_event() {
            // Escape outside any focused widget quits
            if let sdl2::event::Event::KeyDown {
                keycode: Some(sdl2::keyboard::Keycode::Escape), ..
            } = &event {
                if !browser.bar.focused && browser.tab().focused_input.is_none() {
                    break 'main;
                }
            }

            if handle_event(&mut browser, event) { break 'main; }
        }

        // --- Navigate if the bar submitted a URL ---
        if let Some(url) = browser.bar.pending.take() {
            browser.navigate(&url);
        }

        // --- Render ---
        if browser.need_draw {
            browser.draw();
        } else {
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    }
}
