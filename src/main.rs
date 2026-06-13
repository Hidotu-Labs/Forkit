mod app;
mod dom;
mod js;
mod net;
mod render;
mod ui;
mod window;

use std::env;

use app::browser::Browser;
use app::events::handle_event;

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

fn main() {
    let args: Vec<String> = env::args().collect();
    let initial = args.get(1).map(String::as_str).unwrap_or("assets/test.html");

    let sdl     = sdl2::init().expect("SDL2 init failed");
    let ttf_ctx = sdl2::ttf::init().expect("TTF init failed");

    // Initialise SDL2_mixer for <audio> playback.  Non-fatal: if the library
    // is missing the browser still works, audio elements just won't play.
    if let Err(e) = render::audio::init_mixer() {
        eprintln!("[audio] SDL2_mixer init failed (audio playback disabled): {e}");
    }

    let mut browser = Browser::new(&sdl, &ttf_ctx, initial)
        .expect("Failed to create browser");

    let video      = sdl.video().expect("video subsystem");
    let text_input = video.text_input();
    text_input.start();

    let _event_pump = sdl.event_pump().expect("Event pump failed");

    'main: loop {
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

        browser.poll_tabs();

        if let Some(url) = browser.bar.pending.take() {
            browser.navigate(&url);
        }

        if browser.need_draw {
            browser.draw();
        } else {
            let any_loading = browser.tabs.iter().any(|t| t.is_loading());
            let any_audio   = browser.tabs.iter().any(|t| t.audio_engines.iter().any(|e| e.playing));
            if any_loading || any_audio {
                // Keep redrawing to animate the spinner / audio scrubber
                browser.need_draw = true;
                browser.draw();
            } else {
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }
    }
}
