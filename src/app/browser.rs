use std::sync::mpsc::{self, Receiver};

use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::ttf::Sdl2TtfContext;

use crate::dom::node::Node;
use crate::net::resolve_url;
use crate::render::font::FontCache;
use crate::render::image::ImageCache;
use crate::render::layout::LayoutState;
use crate::render::layout::state::{LinkArea, InputArea, ButtonArea, ButtonAction, InputKind, DetailsArea, AudioArea, AudioPlayback};
use crate::render::renderer::RenderCtx;
use crate::render::audio::AudioEngine;
use crate::ui::searchbar::{SearchBar, BAR_HEIGHT};
use crate::ui::tabbar::{TabBar, TAB_BAR_HEIGHT};
use crate::ui::console;
use crate::window::window::{AppWindow, DEFAULT_H};
use super::history::HistoryStore;

use super::loader::{load_dom, PageMeta, ConsoleEntry};

// Scrollbar appearance constants
pub const SCROLLBAR_W:         i32 = 8;
const SCROLLBAR_MIN_THUMB: i32 = 28;
const SCROLLBAR_TRACK_COLOR: (u8, u8, u8) = (240, 240, 245);
const SCROLLBAR_THUMB_COLOR: (u8, u8, u8) = (200, 200, 210);
const SCROLLBAR_THUMB_HOVER: (u8, u8, u8) = (160, 160, 175);

// ---------------------------------------------------------------------------
// History (per-tab)
// ---------------------------------------------------------------------------

struct History {
    entries: Vec<String>,
    pos:     usize,
}

impl History {
    fn new(initial: &str) -> Self {
        History { entries: vec![initial.to_owned()], pos: 0 }
    }

    fn current(&self) -> &str { &self.entries[self.pos] }

    fn push(&mut self, url: String) {
        self.entries.truncate(self.pos + 1);
        self.entries.push(url);
        self.pos = self.entries.len() - 1;
    }

    fn can_back(&self)    -> bool { self.pos > 0 }
    fn can_forward(&self) -> bool { self.pos + 1 < self.entries.len() }

    fn go_back(&mut self)    -> Option<&str> {
        if self.can_back()    { self.pos -= 1; Some(self.current()) } else { None }
    }
    fn go_forward(&mut self) -> Option<&str> {
        if self.can_forward() { self.pos += 1; Some(self.current()) } else { None }
    }
}

// ---------------------------------------------------------------------------
// LoadState — tracks an in-progress or crashed background load for a tab
// ---------------------------------------------------------------------------

/// The result sent back from a background loader thread.
pub struct LoadResult {
    /// The final resolved URL after redirects.
    pub final_url: String,
    pub dom:       Node,
    pub meta:      PageMeta,
    /// Pre-fetched image bytes collected during load (optional optimisation).
    pub images:    ImageCache,
    /// Whether this load should push a new history entry (`true`) or replace
    /// the current entry (`false`, used for back/forward/reload).
    pub push_history: bool,
    /// Console entries produced by JS execution on this page.
    pub console_entries: Vec<ConsoleEntry>,
    /// Global scope after executing initial scripts.
    pub js_scope:        crate::js::scope::Scope,
    /// Timers created during initial script execution.
    pub timers:          Vec<JsTimer>,
}

/// Lifecycle state of a single tab's background loader.
pub enum LoadState {
    /// No load in progress.
    Idle,
    /// A load thread is running; we poll this receiver each frame.
    Loading(Receiver<Result<LoadResult, String>>),
    /// The most recent load panicked or failed; stores the error message.
    Crashed(String),
}

// ---------------------------------------------------------------------------
// Tab — one browser tab, owns its own DOM / history / scroll / image cache
// ---------------------------------------------------------------------------

pub struct JsTimer {
    pub fire_at:  std::time::Instant,
    pub callback: crate::js::types::JsFunction,
}

pub struct Tab {
    pub dom:        Node,
    pub scroll_y:   i32,
    pub images:     ImageCache,
    history:        History,
    /// Link areas from the last rendered frame.
    link_areas:     Vec<LinkArea>,
    /// Input areas (text fields, textareas) from the last rendered frame.
    pub input_areas: Vec<InputArea>,
    /// Button/submit/reset areas from the last rendered frame.
    pub button_areas: Vec<ButtonArea>,
    /// Details/summary areas from the last rendered frame.
    pub details_areas: Vec<DetailsArea>,
    /// Audio player areas from the last rendered frame.
    pub audio_areas: Vec<AudioArea>,
    /// Per-tab audio playback engines, one per `<audio>` element index.
    /// Grows as new players are encountered; never shrinks (to preserve state
    /// across redraws).
    pub audio_engines: Vec<AudioEngine>,
    /// Live text content for each input, indexed by the order they appear on the page.
    pub input_values: Vec<String>,
    /// Which input index is currently focused (if any).
    pub focused_input: Option<usize>,
    /// Total document height in pixels (updated after each draw).
    pub content_height: i32,
    /// Page `<title>` text extracted from the document.
    pub page_title: String,
    /// Resolved URL of the page favicon (from `<link rel="icon">` or /favicon.ico).
    pub favicon_url: Option<String>,
    /// Background load state for this tab.
    pub load_state: LoadState,
    /// Console output produced by JS execution on this page.
    pub console_entries: Vec<ConsoleEntry>,
    /// Whether the console panel is open.
    pub console_open: bool,
    /// Scroll offset within the console panel.
    pub console_scroll: i32,
    /// JavaScript global scope for this tab.
    pub js_scope:       crate::js::scope::Scope,
    /// Hit-testable event listener regions from the last frame.
    pub event_areas:    Vec<crate::render::layout::state::EventArea>,
    /// Active timeouts for this tab.
    pub timers:         Vec<JsTimer>,
    pub stylesheets:    Vec<crate::dom::css::Stylesheet>,
}

impl Tab {
    /// Synchronously load the initial tab (only used for startup — after that
    /// all navigation is async).
    fn new(url: &str) -> Option<Self> {
        let (resolved, dom, meta, console_entries, js_scope, timers) = load_dom(url)?;
        let mut tab = Tab {
            dom,
            scroll_y:       0,
            images:         ImageCache::new(),
            history:        History::new(&resolved),
            link_areas:     Vec::new(),
            input_areas:    Vec::new(),
            button_areas:   Vec::new(),
            details_areas:  Vec::new(),
            audio_areas:    Vec::new(),
            audio_engines:  Vec::new(),
            input_values:   Vec::new(),
            focused_input:  None,
            content_height: 0,
            page_title:     meta.title,
            favicon_url:    meta.favicon_url,
            load_state:     LoadState::Idle,
            console_entries,
            console_open:   false,
            console_scroll: 0,
            js_scope,
            event_areas:    Vec::new(),
            timers,
            stylesheets:    Vec::new(),
        };
        tab.collect_styles();
        Some(tab)
    }

    // ---- accessors ----

    pub fn current_url(&self) -> &str { self.history.current() }

    pub fn can_back(&self)    -> bool { self.history.can_back() }
    pub fn can_forward(&self) -> bool { self.history.can_forward() }

    /// Returns true when a background load is in progress.
    pub fn is_loading(&self) -> bool {
        matches!(self.load_state, LoadState::Loading(_))
    }

    /// Returns true when the last load crashed.
    pub fn is_crashed(&self) -> bool {
        matches!(self.load_state, LoadState::Crashed(_))
    }

    /// Short display title — page `<title>` text, falling back to the hostname.
    pub fn title(&self) -> String {
        if self.is_crashed() {
            return "Crashed".to_owned();
        }
        self.title_from_meta()
    }

    fn title_from_meta(&self) -> String {
        if !self.page_title.is_empty() {
            let t = &self.page_title;
            if t.chars().count() > 40 {
                let s: String = t.chars().take(38).collect();
                return format!("{}…", s);
            }
            return t.clone();
        }
        let url = self.history.current();
        if let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
            let host = rest.split('/').next().unwrap_or(rest);
            host.to_owned()
        } else if let Some(rest) = url.strip_prefix("file://") {
            rest.rsplit('/').next().unwrap_or(rest).to_owned()
        } else {
            url.chars().take(24).collect()
        }
    }

    // ---- async navigation ----

    /// Spawn a background thread to load `url`, pushing a new history entry
    /// when the load completes.
    pub fn navigate_async(&mut self, url: &str) {
        let resolved = resolve_url(url, self.history.current());
        self.spawn_load(resolved, true);
    }

    /// Spawn a background thread to load `url` without touching history
    /// (back, forward, reload).
    pub fn navigate_async_no_history(&mut self, url: &str) {
        let url = url.to_owned();
        self.spawn_load(url, false);
    }

    /// Internal: cancel any in-flight load and start a new one.
    pub(super) fn spawn_load(&mut self, url: String, push_history: bool) {
        // Dropping the old Receiver will disconnect the channel; the old
        // thread will eventually notice a SendError and exit cleanly.
        let (tx, rx) = mpsc::channel();
        self.load_state = LoadState::Loading(rx);

        std::thread::spawn(move || {
            // Catch panics so a bad page cannot kill the whole application.
            let result = std::panic::catch_unwind(|| load_dom(&url))
                .map_err(|e| {
                    // Try to extract a string from the panic payload.
                    if let Some(s) = e.downcast_ref::<&str>() {
                        format!("panic: {s}")
                    } else if let Some(s) = e.downcast_ref::<String>() {
                        format!("panic: {s}")
                    } else {
                        "panic: unknown".to_owned()
                    }
                });

            let send_val = match result {
                Err(msg) => Err(msg),
                Ok(None) => Err(format!("Failed to load: {url}")),
                Ok(Some((final_url, dom, meta, console_entries, js_scope, timers))) => Ok(LoadResult {
                    final_url,
                    dom,
                    meta,
                    images: ImageCache::new(),
                    push_history,
                    console_entries,
                    js_scope,
                    timers,
                }),
            };

            // If the receiver was dropped (tab closed) this is a no-op.
            let _ = tx.send(send_val);
        });
    }

    /// Poll the background receiver.  Returns `true` if state changed and a
    /// redraw is needed.
    pub fn poll_load(&mut self) -> bool {
        // We need to temporarily take the load_state to avoid borrow issues.
        let mut state = LoadState::Idle;
        std::mem::swap(&mut self.load_state, &mut state);

        match state {
            LoadState::Loading(rx) => {
                match rx.try_recv() {
                    Ok(Ok(result)) => {
                        // Success — apply the loaded page.
                        self.dom           = result.dom;
                        self.scroll_y      = 0;
                        self.input_values  = Vec::new();
                        self.focused_input = None;
                        self.button_areas  = Vec::new();
                        self.details_areas = Vec::new();
                        self.audio_areas   = Vec::new();
                        self.page_title    = result.meta.title;
                        self.favicon_url   = result.meta.favicon_url;
                        self.console_entries = result.console_entries;
                        self.console_scroll  = 0;
                        if result.push_history {
                            self.history.push(result.final_url);
                        }
                        self.js_scope   = result.js_scope;
                        self.timers     = result.timers;
                        self.load_state = LoadState::Idle;
                        self.collect_styles();
                        true
                    }
                    Ok(Err(msg)) => {
                        // Load failed or panicked — show an error page.
                        eprintln!("Tab load error: {msg}");
                        let error_html = format!(
                            "<html><body>\
                             <h2 style=\"color:#cc3333\">Tab crashed</h2>\
                             <p>{msg}</p>\
                             </body></html>"
                        );
                        // Parse the error page directly.
                        let error_node = crate::dom::parser::parse_dom(&error_html);
                        self.dom          = error_node;
                        self.page_title   = "Error".to_owned();
                        self.favicon_url  = None;
                        self.scroll_y     = 0;
                        self.input_values = Vec::new();
                        self.focused_input = None;
                        self.button_areas = Vec::new();
                        self.details_areas = Vec::new();
                        self.audio_areas   = Vec::new();
                        self.console_entries = Vec::new();
                        self.console_scroll  = 0;
                        self.js_scope   = crate::js::scope::Scope::new();
                        self.load_state   = LoadState::Crashed(msg);
                        true
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        // Still loading — put the receiver back.
                        self.load_state = LoadState::Loading(rx);
                        false
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // Thread exited without sending — treat as a crash.
                        self.load_state = LoadState::Crashed("loader thread disconnected".to_owned());
                        true
                    }
                }
            }
            other => {
                self.load_state = other;
                false
            }
        }
    }

    pub fn poll_timers(&mut self) -> bool {
        let mut changed = false;
        let now = std::time::Instant::now();
        
        // Find timers that are ready to fire
        let mut ready = Vec::new();
        self.timers.retain(|t| {
            if t.fire_at <= now {
                ready.push(t.callback.clone());
                false
            } else {
                true
            }
        });

        for func in ready {
            let (entries, mutations) = {
                let js_dom = crate::js::dom::JsDom::with_title(&self.dom, self.page_title.clone());
                let entries = crate::js::interpreter::execute_function(&func, vec![], &js_dom, &mut self.js_scope);
                (entries, js_dom.take_mutations())
            };
            
            self.console_entries.extend(entries);
            if !mutations.is_empty() {
                for muta in mutations {
                    match muta {
                        crate::js::dom::DomMutation::SetTimeout { callback, delay_ms } => {
                            self.timers.push(JsTimer {
                                fire_at: std::time::Instant::now() + std::time::Duration::from_millis(delay_ms as u64),
                                callback,
                            });
                        }
                        _ => crate::js::dom::apply_one(&mut self.dom, muta),
                    }
                }
                changed = true;
            }
        }
        changed
    }

    // ---- legacy synchronous navigation (kept for back/forward/reload) ----

    pub fn navigate(&mut self, url: &str) {
        // resolve_url is also called inside navigate_async — call it once here
        // to keep the single-resolve semantics, then pass the already-resolved URL.
        let resolved = resolve_url(url, self.history.current());
        self.spawn_load(resolved, true);
    }

    pub fn navigate_absolute(&mut self, url: &str) {
        self.spawn_load(url.to_owned(), true);
    }

    pub fn go_back(&mut self) {
        if let Some(url) = self.history.go_back() {
            let url = url.to_owned();
            self.navigate_async_no_history(&url);
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(url) = self.history.go_forward() {
            let url = url.to_owned();
            self.navigate_async_no_history(&url);
        }
    }

    pub fn reload(&mut self) {
        let url = self.history.current().to_owned();
        self.navigate_async_no_history(&url);
    }

    // ---- scrolling ----

    pub fn scroll_by(&mut self, delta: i32) {
        self.scroll_y = (self.scroll_y + delta).max(0);
    }

    pub fn scroll_to_top(&mut self) { self.scroll_y = 0; }

    // ---- link hit test ----

    pub fn link_at(&self, x: i32, y: i32) -> Option<&str> {
        self.link_areas.iter().rev()
            .find(|a| a.contains(x, y, self.scroll_y))
            .map(|a| a.href.as_str())
    }

    // ---- input hit test / focus ----

    /// Returns the index of the input area at `(x, y)`, if any.
    pub fn input_at(&self, x: i32, y: i32) -> Option<usize> {
        self.input_areas.iter().rev()
            .find(|a| a.contains(x, y, self.scroll_y))
            .map(|a| a.index)
    }

    /// Ensure the input_values Vec is large enough for `index`.
    pub fn ensure_input_slot(&mut self, index: usize) {
        if self.input_values.len() <= index {
            self.input_values.resize(index + 1, String::new());
        }
    }

    /// Append typed text to the focused input.
    pub fn type_text(&mut self, text: &str) {
        if let Some(idx) = self.focused_input {
            // Block input for disabled or readonly inputs.
            let flags = self.input_areas.iter()
                .find(|a| a.index == idx)
                .map(|a| (a.disabled || a.readonly, a.kind.clone()));
            if flags.as_ref().map(|(b, _)| *b).unwrap_or(false) { return; }
            self.ensure_input_slot(idx);
            // For number inputs, only allow digits, minus sign, and decimal point.
            if flags.map(|(_, k)| k) == Some(InputKind::Number) {
                let filtered: String = text.chars().filter(|c| c.is_ascii_digit() || *c == '-' || *c == '.').collect();
                self.input_values[idx].push_str(&filtered);
            } else {
                self.input_values[idx].push_str(text);
            }
        }
    }

    /// Increment or decrement a number input by `delta` (typically ±1).
    pub fn step_number(&mut self, idx: usize, delta: i32) {
        self.ensure_input_slot(idx);
        // If the live value is empty, seed it from the HTML default first.
        if self.input_values[idx].is_empty() {
            if let Some(default) = self.input_areas.iter()
                .find(|a| a.index == idx)
                .map(|a| a.default_value.clone())
            {
                self.input_values[idx] = default;
            }
        }
        let current: f64 = self.input_values[idx].parse().unwrap_or(0.0);
        let next = current + delta as f64;
        // Format as integer when the result is whole, otherwise keep decimals.
        self.input_values[idx] = if next.fract() == 0.0 {
            format!("{}", next as i64)
        } else {
            format!("{}", next)
        };
        // Keep the input focused after stepping.
        self.focused_input = Some(idx);
    }

    /// Handle backspace in the focused input.
    pub fn backspace(&mut self) {
        if let Some(idx) = self.focused_input {
            let is_blocked = self.input_areas.iter()
                .find(|a| a.index == idx)
                .map(|a| a.disabled || a.readonly)
                .unwrap_or(false);
            if is_blocked { return; }
            self.ensure_input_slot(idx);
            let mut chars = self.input_values[idx].chars();
            chars.next_back();
            self.input_values[idx] = chars.as_str().to_string();
        }
    }

    // ---- button hit test ----

    /// Returns the action of the button at `(x, y)`, if any.
    pub fn button_at(&self, x: i32, y: i32) -> Option<&ButtonAction> {
        self.button_areas.iter().rev()
            .find(|b| b.contains(x, y, self.scroll_y))
            .map(|b| &b.action)
    }

    /// Reset all input values (called by a Reset button).
    pub fn reset_inputs(&mut self) {
        self.input_values.clear();
        self.focused_input = None;
    }

    // ---- details hit test ----

    pub fn details_at(&self, x: i32, y: i32) -> Option<usize> {
        self.details_areas.iter().rev()
            .find(|d: &&DetailsArea| d.contains(x, y, self.scroll_y))
            .map(|d| d.element_ptr)
    }

    /// Returns a clone of the audio area at `(x, y)`, if any.
    pub fn audio_at(&self, x: i32, y: i32) -> Option<AudioArea> {
        self.audio_areas.iter().rev()
            .find(|a| a.contains(x, y, self.scroll_y))
            .cloned()
    }
    pub fn toggle_details(&mut self, ptr: usize) {
        if let Some(el) = find_element_mut_by_ptr(&mut self.dom, ptr) {
            let is_open = crate::dom::parser::get_attr(&el.attrs_raw, "open").is_some();
            if is_open {
                // Remove "open"
                el.attrs_raw = el.attrs_raw.replace(" open", "").replace("open ", "").replace("open", "");
            } else {
                // Add "open"
                el.attrs_raw.push_str(" open");
            }
        }
    }

    /// Find the DOM element at content coordinates `(x, y)` and return its
    /// raw pointer (as `usize`) for use with `:hover` pseudo-class matching.
    ///
    /// Currently uses link_areas and input_areas as a lightweight proxy for
    /// element hit-testing.  Full DOM hit-testing requires layout boxes that
    /// carry element pointers — this is the initial implementation.
    pub fn find_element_at(&self, _x: i32, _y: i32) -> Option<usize> {
        None
    }

    pub fn collect_styles(&mut self) {
        self.stylesheets.clear();
        fn traverse(node: &crate::dom::node::Node, sheets: &mut Vec<crate::dom::css::Stylesheet>) {
            match node {
                crate::dom::node::Node::Element(el) => {
                    if el.tag == "style" {
                        let mut css_text = String::new();
                        for child in &el.children {
                            if let crate::dom::node::Node::Text(txt) = child {
                                css_text.push_str(&txt.text);
                            }
                        }
                        if !css_text.is_empty() {
                            sheets.push(crate::dom::css::parse_stylesheet(&css_text));
                        }
                    }
                    for child in &el.children {
                        traverse(child, sheets);
                    }
                }
                crate::dom::node::Node::Text(_) => {}
            }
        }
        traverse(&self.dom, &mut self.stylesheets);
    }
}

fn find_element_mut_by_ptr(node: &mut Node, ptr: usize) -> Option<&mut crate::dom::node::Element> {
    match node {
        Node::Element(el) => {
            if (el as *mut crate::dom::node::Element as usize) == ptr {
                return Some(el);
            }
            for child in &mut el.children {
                if let Some(found) = find_element_mut_by_ptr(child, ptr) {
                    return Some(found);
                }
            }
            None
        }
        Node::Text(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Browser — owns the window, font cache, and all tabs
// ---------------------------------------------------------------------------

/// Total chrome height above the content area.
pub fn chrome_height() -> i32 { TAB_BAR_HEIGHT + BAR_HEIGHT }

pub struct Browser<'ttf> {
    pub window:    AppWindow,
    pub fonts:     FontCache<'ttf>,
    pub tabs:      Vec<Tab>,
    pub active:    usize,
    pub bar:       SearchBar,
    pub need_draw: bool,
    /// If the user is dragging the scrollbar thumb, stores the y offset within the thumb
    /// where the drag started (screen-relative to the content area top).
    pub scrollbar_drag: Option<i32>,
    /// Current mouse X position in window coordinates (for :hover matching).
    pub mouse_x: i32,
    /// Current mouse Y position in content-area coordinates (for :hover matching).
    pub mouse_y: i32,
    /// Whether the developer console panel is shown.
    pub console_open: bool,
    /// Current width of the right-side console panel.
    pub console_w: i32,
    /// If the user is dragging the resize handle, stores the initial mouse X
    /// and the panel width at drag-start.
    pub console_resize_drag: Option<(i32, i32)>,
    /// Whether the cursor is hovering the resize handle (for highlight).
    pub console_resize_hot: bool,
    /// Screen rect of the console × close button (updated each frame while open).
    pub console_close_btn: Option<sdl2::rect::Rect>,
    /// Screen rect of the resize handle (updated each frame while open).
    pub console_resize_rect: Option<sdl2::rect::Rect>,
    /// Persistent browsing history store (loaded from disk at startup).
    pub history_store: HistoryStore,
}

impl<'ttf> Browser<'ttf> {
    pub fn new(
        sdl:     &sdl2::Sdl,
        ttf_ctx: &'ttf Sdl2TtfContext,
        initial: &str,
    ) -> Result<Self, String> {
        let video      = sdl.video().expect("video subsystem");
        let text_input = video.text_input();
        text_input.start();

        let _event_pump = sdl.event_pump().expect("Event pump failed");

        let fonts_cache = FontCache::new(ttf_ctx);

        let window = AppWindow::new(sdl, "Forkit")?;

        let tab = Tab::new(initial)
            .ok_or_else(|| format!("Failed to load initial page: {initial}"))?;

        let bar_url = tab.current_url().to_owned();

        let mut history_store = HistoryStore::load();
        history_store.push(&bar_url, &tab.page_title);

        Ok(Browser {
            window,
            fonts:          fonts_cache,
            tabs:           vec![tab],
            active:         0,
            bar:            SearchBar::new(&bar_url),
            need_draw:      true,
            scrollbar_drag: None,
            mouse_x:        0,
            mouse_y:        0,
            console_open:   false,
            console_w:      console::CONSOLE_DEFAULT_W,
            console_resize_drag: None,
            console_resize_hot:  false,
            console_close_btn:   None,
            console_resize_rect: None,
            history_store,
        })
    }

    // ---- active-tab helpers ----

    pub fn tab(&self)      -> &Tab      { &self.tabs[self.active] }
    pub fn tab_mut(&mut self) -> &mut Tab { &mut self.tabs[self.active] }

    // ---- tab management ----

    /// Open a new tab navigating to `url` and switch to it.
    pub fn open_tab(&mut self, url: &str) {
        // Create a stub tab with a blank page, then immediately kick off an
        // async load for the real URL.  We call spawn_load directly with the
        // already-resolved URL so resolve_url never runs against "about:blank".
        if let Some(mut tab) = Tab::new("about:blank") {
            tab.page_title = url.to_owned();
            // Use the fully-qualified URL directly — no relative resolution needed here.
            let target = crate::net::resolve_url(url, "https://example.com");
            tab.spawn_load(target.clone(), true);
            self.tabs.push(tab);
            self.active = self.tabs.len() - 1;
            self.bar    = SearchBar::new(&target);
        }
        self.need_draw = true;
    }

    /// Open a blank new tab (navigates to example.com).
    pub fn open_blank_tab(&mut self) {
        self.open_tab("https://example.com");
    }

    /// Close the tab at `index`. If it's the last tab, do nothing.
    pub fn close_tab(&mut self, index: usize) {
        if self.tabs.len() <= 1 { return; }
        self.tabs.remove(index);
        // Keep active within bounds
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if self.active > index {
            self.active -= 1;
        }
        self.sync_bar();
        self.need_draw = true;
    }

    /// Switch to tab `index`.
    pub fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
            self.sync_bar();
            self.need_draw = true;
        }
    }

    /// Cycle forward through tabs.
    pub fn next_tab(&mut self) {
        let n = self.tabs.len();
        self.switch_tab((self.active + 1) % n);
    }

    /// Cycle backward through tabs.
    pub fn prev_tab(&mut self) {
        let n = self.tabs.len();
        self.switch_tab((self.active + n - 1) % n);
    }

    /// Poll all tab background loaders.  Call this each frame before drawing.
    /// Returns `true` if any tab changed state and a redraw is needed.
    pub fn poll_tabs(&mut self) -> bool {
        let mut changed = false;
        for tab in &mut self.tabs {
            if tab.poll_load() {
                changed = true;
            }
            // Check for expired timers
            if tab.poll_timers() {
                changed = true;
            }
        }
        if changed {
            // Keep the address bar in sync with the active tab if it just finished loading.
            self.sync_bar();
            self.need_draw = true;
            // Record the visit in the persistent history store.
            let url   = self.tabs[self.active].current_url().to_owned();
            let title = self.tabs[self.active].page_title.clone();
            self.history_store.push(&url, &title);
        }
        changed
    }

    /// Keep the address bar URL in sync with the active tab.
    fn sync_bar(&mut self) {
        self.bar.url = self.tabs[self.active].current_url().to_owned();
        self.bar.focused = false;
    }

    // ---- navigation (delegated to active tab) ----

    pub fn navigate(&mut self, url: &str) {
        self.tab_mut().navigate(url);
        self.sync_bar();
        self.need_draw = true;
    }

    pub fn navigate_absolute(&mut self, url: &str) {
        self.tab_mut().navigate_absolute(url);
        self.sync_bar();
        self.need_draw = true;
    }

    pub fn go_back(&mut self) {
        self.tab_mut().go_back();
        self.sync_bar();
        self.need_draw = true;
    }

    pub fn go_forward(&mut self) {
        self.tab_mut().go_forward();
        self.sync_bar();
        self.need_draw = true;
    }

    pub fn reload(&mut self) {
        self.tab_mut().reload();
        self.need_draw = true;
    }

    pub fn can_back(&self)    -> bool { self.tab().can_back() }
    pub fn can_forward(&self) -> bool { self.tab().can_forward() }


    // ---- click handling ----

    /// Top-level click dispatcher. `y` is window-absolute.
    pub fn handle_click(&mut self, x: i32, y: i32) {
        let (win_w, _) = self.window.canvas.output_size()
            .map(|(w, h)| (w as i32, h as i32))
            .unwrap_or((1024, 768));

        // Tab bar strip
        if y < TAB_BAR_HEIGHT {
            let titles: Vec<String> = self.tabs.iter().map(|t| t.title()).collect();
            match TabBar::region_at(x, y, win_w, &titles, self.active) {
                crate::ui::tabbar::TabBarRegion::Tab(i)   => self.switch_tab(i),
                crate::ui::tabbar::TabBarRegion::Close(i) => self.close_tab(i),
                crate::ui::tabbar::TabBarRegion::NewTab    => self.open_blank_tab(),
                crate::ui::tabbar::TabBarRegion::None      => {}
            }
            return;
        }

        // Address bar strip
        if y < chrome_height() { return; } // handled by bar widget in events.rs

        // Content area
        let content_y = y - chrome_height();

        // Check for <details> summary click
        if let Some(ptr) = self.tab().details_at(x, content_y) {
            self.tab_mut().toggle_details(ptr);
            self.need_draw = true;
            return;
        }

        // Check for audio player click (play/pause button or scrubber)
        if let Some(area) = self.tab().audio_at(x, content_y) {
            // ... (keep existing audio logic)
            let base_url = self.tab().current_url().to_owned();
            let src      = area.src.clone();
            let idx      = area.index;
            // Grow the engines vec if needed (first time this player is clicked)
            while self.tab().audio_engines.len() <= idx {
                self.tab_mut().audio_engines.push(AudioEngine::new());
            }
            if area.play_btn_hit(x, content_y, self.tab().scroll_y) {
                self.tab_mut().audio_engines[idx].toggle(&src, &base_url);
                self.need_draw = true;
            } else if area.scrubber_hit(x, content_y, self.tab().scroll_y) {
                let ratio = area.scrubber_ratio(x);
                self.tab_mut().audio_engines[idx].seek(ratio, &base_url);
                self.need_draw = true;
            }
            return;
        }

        // --- JavaScript Click Events ---
        // We find all elements at the click position that have a "click" listener.
        let hits: Vec<_> = self.tab().event_areas.iter()
            .filter(|a| a.event_type == "click" && a.contains(x, content_y, self.tab().scroll_y))
            .rev() // top-painted first
            .cloned()
            .collect();

        for area in hits {
            // This is slightly expensive as it walks the DOM by pointer, but robust.
            if let Some(el) = find_element_mut_by_ptr(&mut self.tab_mut().dom, area.element_ptr) {
                let listeners = el.event_listeners.clone();
                for (etype, func) in listeners {
                    if etype == "click" {
                        let (entries, mutations) = {
                            let tab = self.tab_mut();
                            let js_dom = crate::js::dom::JsDom::with_title(&tab.dom, tab.page_title.clone());
                            let entries = crate::js::interpreter::execute_function(&func, vec![], &js_dom, &mut tab.js_scope);
                            (entries, js_dom.take_mutations())
                        };
                        
                        let tab = self.tab_mut();
                        tab.console_entries.extend(entries);
                        if !mutations.is_empty() {
                            for muta in mutations {
                                match muta {
                                    crate::js::dom::DomMutation::SetTimeout { callback, delay_ms } => {
                                        tab.timers.push(JsTimer {
                                            fire_at: std::time::Instant::now() + std::time::Duration::from_millis(delay_ms as u64),
                                            callback,
                                        });
                                    }
                                    _ => crate::js::dom::apply_one(&mut tab.dom, muta),
                                }
                            }
                            self.need_draw = true;
                        }
                    }
                }
            }
        }

        // Check button areas first (submit/reset)
        if let Some(action) = self.tab().button_at(x, content_y).cloned() {
            match action {
                ButtonAction::Submit(ref action_url) => {
                    let url = if action_url.is_empty() {
                        self.tab().current_url().to_owned()
                    } else {
                        let base = self.tab().current_url().to_owned();
                        crate::net::resolve_url(action_url, &base)
                    };
                    self.navigate(&url);
                }
                ButtonAction::Reset => {
                    self.tab_mut().reset_inputs();
                    self.need_draw = true;
                }
                ButtonAction::Navigate(ref url) => {
                    let url = url.clone();
                    self.navigate(&url);
                }
                ButtonAction::StepUp(idx) => {
                    self.tab_mut().step_number(idx, 1);
                    self.need_draw = true;
                }
                ButtonAction::StepDown(idx) => {
                    self.tab_mut().step_number(idx, -1);
                    self.need_draw = true;
                }
                ButtonAction::None => {}
            }
            return;
        }

        // Check for a focused input
        if let Some(idx) = self.tab().input_at(x, content_y) {
            let kind = self.tab().input_areas.iter().find(|a| a.index == idx).map(|a| a.kind.clone());
            match kind {
                Some(InputKind::Checkbox) => {
                    self.tab_mut().ensure_input_slot(idx);
                    // Three-state: "true" = checked, "false" = unchecked, "" = not yet interacted
                    let is_true = self.tab().input_values[idx] == "true";
                    self.tab_mut().input_values[idx] = if is_true { "false".to_owned() } else { "true".to_owned() };
                    self.need_draw = true;
                }
                Some(InputKind::Radio) => {
                    // Collect all radio inputs with the same `name` to enforce mutual exclusion
                    let my_name = self.tab().input_areas.iter()
                        .find(|a| a.index == idx)
                        .map(|a| a.name.clone())
                        .unwrap_or_default();
                    let same_group: Vec<usize> = self.tab().input_areas.iter()
                        .filter(|a| matches!(a.kind, InputKind::Radio) && a.name == my_name)
                        .map(|a| a.index)
                        .collect();
                    // Deselect all radios in the group
                    let max_idx = same_group.iter().copied().max().unwrap_or(idx);
                    self.tab_mut().ensure_input_slot(max_idx);
                    for ridx in same_group {
                        self.tab_mut().input_values[ridx] = "false".to_owned();
                    }
                    // Select the clicked one
                    self.tab_mut().input_values[idx] = "true".to_owned();
                    self.need_draw = true;
                }
                Some(InputKind::Range) => {
                    // Clone the area immediately to avoid borrow conflict with tab_mut
                    let area_opt = self.tab().input_areas.iter().find(|a| a.index == idx).cloned();
                    if let Some(area) = area_opt {
                        let ratio = ((x - area.x) as f64 / area.w as f64).clamp(0.0, 1.0);
                        let val = (ratio * 100.0) as i32;
                        self.tab_mut().ensure_input_slot(idx);
                        self.tab_mut().input_values[idx] = val.to_string();
                        self.need_draw = true;
                    }
                }
                _ => {
                    // disabled inputs cannot be focused; readonly can be focused but not edited
                    let is_disabled = self.tab().input_areas.iter()
                        .find(|a| a.index == idx)
                        .map(|a| a.disabled)
                        .unwrap_or(false);
                    if !is_disabled {
                        self.tab_mut().focused_input = Some(idx);
                        self.bar.focused = false;
                        self.need_draw = true;
                    }
                }
            }
            return;
        }
        // Clicking non-input content clears page focus
        self.tab_mut().focused_input = None;
        if let Some(href) = self.tab().link_at(x, content_y) {
            let href = href.to_owned();
            self.navigate(&href);
        }
    }

    // ---- scrolling ----

    pub fn scroll_by(&mut self, delta: i32) {
        self.tab_mut().scroll_by(delta);
        self.clamp_scroll();
        self.need_draw = true;
    }

    pub fn scroll_to_top(&mut self) {
        self.tab_mut().scroll_to_top();
        self.need_draw = true;
    }

    /// Clamp scroll_y so it never goes past the bottom of the document.
    pub fn clamp_scroll(&mut self) {
        let (win_w, win_h) = self.window.canvas.output_size()
            .map(|(w, h)| (w as i32, h as i32))
            .unwrap_or((1024, 768));
        let _ = win_w;
        let content_h  = (win_h - chrome_height()).max(0);
        let doc_h      = self.tabs[self.active].content_height;
        let max_scroll = (doc_h - content_h).max(0);
        let tab = &mut self.tabs[self.active];
        if tab.scroll_y > max_scroll { tab.scroll_y = max_scroll; }
        if tab.scroll_y < 0          { tab.scroll_y = 0; }
    }

    /// Scrollbar geometry: returns `(track_y, track_h, thumb_y, thumb_h)` in
    /// content-area-relative screen coordinates, or `None` when the content fits.
    pub fn scrollbar_geometry(&self, content_h: i32) -> Option<(i32, i32, i32, i32)> {
        let doc_h = self.tabs[self.active].content_height;
        if doc_h <= content_h { return None; }
        let track_y = 0;
        let track_h = content_h;
        let thumb_h = ((content_h as f64 / doc_h as f64) * track_h as f64) as i32;
        let thumb_h = thumb_h.max(SCROLLBAR_MIN_THUMB).min(track_h);
        let scroll_y = self.tabs[self.active].scroll_y;
        let max_scroll = (doc_h - content_h).max(1);
        let thumb_y = ((scroll_y as f64 / max_scroll as f64)
            * (track_h - thumb_h) as f64) as i32;
        Some((track_y, track_h, thumb_y, thumb_h))
    }

    /// Convert a thumb screen-y (within content area) back to a scroll_y value.
    pub fn thumb_y_to_scroll(&self, thumb_screen_y: i32, content_h: i32) -> i32 {
        let doc_h = self.tabs[self.active].content_height;
        if doc_h <= content_h { return 0; }
        let thumb_h = ((content_h as f64 / doc_h as f64) * content_h as f64) as i32;
        let thumb_h = thumb_h.max(SCROLLBAR_MIN_THUMB).min(content_h);
        let travel   = (content_h - thumb_h).max(1);
        let max_scroll = (doc_h - content_h).max(0);
        let ratio = (thumb_screen_y as f64 / travel as f64).clamp(0.0, 1.0);
        (ratio * max_scroll as f64) as i32
    }

    pub fn page_down(&mut self) { self.scroll_by( DEFAULT_H as i32); }
    pub fn page_up(&mut self)   { self.scroll_by(-(DEFAULT_H as i32)); }

    // ---- rendering ----

    pub fn draw(&mut self) {
        if !self.need_draw { return; }

        // Advance audio playback timers for the active tab
        for engine in &mut self.tabs[self.active].audio_engines {
            engine.tick();
        }

        let (win_w, win_h) = self.window.canvas.output_size()
            .map(|(w, h)| (w as i32, h as i32))
            .unwrap_or((1024, 768));

        let chrome_h   = chrome_height();
        let console_w  = if self.console_open { self.console_w } else { 0 };
        let content_w  = (win_w - console_w).max(0);
        let content_h  = (win_h - chrome_h).max(0);

        let tc = self.window.canvas.texture_creator();

        // Clear full window
        self.window.canvas.set_draw_color(Color::WHITE);
        self.window.canvas.clear();

        // ---- content ----
        let _ = self.window.canvas.set_viewport(Some(Rect::new(
            0, chrome_h, content_w as u32, content_h as u32,
        )));

        {
            let tab  = &mut self.tabs[self.active];

            let base_url = tab.current_url().to_owned();
            let ctx  = RenderCtx {
                viewport_width:  content_w,
                viewport_height: content_h,
                scroll_y:        tab.scroll_y,
                base_url:        base_url.clone(),
            };            let mut state = LayoutState::new(&ctx);
            state.stylesheets = tab.stylesheets.clone();
            state.set_input_state(tab.input_values.clone(), tab.focused_input);

            // Build audio playback snapshot keyed by per-page player index
            {
                let mut playback_map = std::collections::HashMap::new();
                for (idx, engine) in tab.audio_engines.iter().enumerate() {
                    playback_map.insert(idx, AudioPlayback {
                        playing:       engine.playing,
                        progress:      engine.progress(),
                        position_secs: engine.position_secs,
                        duration_secs: engine.duration_secs,
                    });
                }
                state.set_audio_state(playback_map);
            }

            state.layout_node(
                &mut self.window.canvas,
                &tc,
                &mut self.fonts,
                &mut tab.images,
                &base_url,
                &tab.dom,
                content_w - SCROLLBAR_W - 4,
            );
            tab.link_areas     = state.link_areas;
            tab.input_areas    = state.input_areas;
            tab.button_areas   = state.button_areas;
            tab.details_areas  = state.details_areas;
            tab.audio_areas    = state.audio_areas;
            tab.event_areas    = state.event_areas;
            tab.content_height = state.content_height;

            // Ensure one engine exists per discovered audio player and pre-fetch
            // duration so the time display is correct before the first click.
            let base_url_clone = base_url.clone();
            let areas: Vec<(usize, String)> = tab.audio_areas.iter()
                .map(|a| (a.index, a.src.clone()))
                .collect();
            for (idx, src) in areas {
                while tab.audio_engines.len() <= idx {
                    tab.audio_engines.push(crate::render::audio::AudioEngine::new());
                }
                tab.audio_engines[idx].prefetch_duration(&src, &base_url_clone);
            }
        }

        // Reset viewport before drawing the scrollbar so coordinates are
        // window-absolute (the content viewport is still active at this point).
        self.window.canvas.set_viewport(None);

        // ---- scrollbar ----
        if let Some((track_y, track_h, thumb_y, thumb_h)) =
            self.scrollbar_geometry(content_h)
        {
            let sx = content_w - SCROLLBAR_W;

            // Track (very subtle, nearly invisible)
            let (tr, tg, tb) = SCROLLBAR_TRACK_COLOR;
            self.window.canvas.set_draw_color(Color::RGB(tr, tg, tb));
            let _ = self.window.canvas.fill_rect(Rect::new(
                sx, chrome_h + track_y, SCROLLBAR_W as u32, track_h as u32,
            ));

            // Thumb — pill-shaped (filled circle + rect)
            let dragging = self.scrollbar_drag.is_some();
            let (cr, cg, cb) = if dragging { SCROLLBAR_THUMB_HOVER } else { SCROLLBAR_THUMB_COLOR };
            let tc_color = Color::RGB(cr, cg, cb);
            let thumb_x   = sx + 1;
            let thumb_abs_y = chrome_h + track_y + thumb_y;
            let thumb_inner_w = SCROLLBAR_W - 2;
            let r = thumb_inner_w / 2;
            self.window.canvas.set_draw_color(tc_color);
            // Top cap
            for dy in -r..=r {
                let half_w = ((r*r - dy*dy) as f64).sqrt() as i32;
                let _ = self.window.canvas.fill_rect(Rect::new(
                    thumb_x + r - half_w, thumb_abs_y + r + dy, (half_w * 2) as u32, 1
                ));
            }
            // Body (between caps)
            if thumb_h > r * 2 {
                let _ = self.window.canvas.fill_rect(Rect::new(
                    thumb_x, thumb_abs_y + r, thumb_inner_w as u32, (thumb_h - r * 2) as u32,
                ));
            }
            // Bottom cap
            let bot_cy = thumb_abs_y + thumb_h - r;
            for dy in -r..=r {
                let half_w = ((r*r - dy*dy) as f64).sqrt() as i32;
                let _ = self.window.canvas.fill_rect(Rect::new(
                    thumb_x + r - half_w, bot_cy + dy, (half_w * 2) as u32, 1
                ));
            }
        }

        // ---- console panel ----
        if self.console_open {
            let panel_x = win_w - self.console_w;
            let entries = self.tabs[self.active].console_entries.clone();
            let scroll  = self.tabs[self.active].console_scroll;
            let result  = console::draw(
                &mut self.window.canvas,
                &tc,
                &mut self.fonts,
                panel_x,
                chrome_h,
                win_h,
                self.console_w,
                &entries,
                scroll,
                self.console_resize_hot,
            );
            self.console_close_btn   = Some(result.close_btn);
            self.console_resize_rect = Some(result.resize_handle);
        } else {
            self.console_close_btn   = None;
            self.console_resize_rect = None;
        }

        // ---- chrome (tab bar + address bar) ----
        // (viewport already reset to None above)

        // Tab bar
        let titles: Vec<String> = self.tabs.iter().map(|t| t.title()).collect();
        let loading_states: Vec<bool> = self.tabs.iter().map(|t| t.is_loading()).collect();
        // Collect favicon bytes for each tab from its image cache
        let favicons: Vec<Option<Vec<u8>>> = self.tabs.iter_mut().map(|tab| {
            let url = match &tab.favicon_url {
                Some(u) => u.clone(),
                None    => return None,
            };
            let base = tab.current_url().to_owned();
            tab.images.get_bytes(&url, &base).map(|b| b.to_vec())
        }).collect();
        TabBar::draw(
            &mut self.window.canvas,
            &tc,
            &mut self.fonts,
            win_w,
            &titles,
            self.active,
            &favicons,
            &loading_states,
        );

        // Address bar (drawn below the tab strip)
        // Temporarily shift the viewport so SearchBar draws at y=0
        let _ = self.window.canvas.set_viewport(Some(Rect::new(
            0, TAB_BAR_HEIGHT, win_w as u32, BAR_HEIGHT as u32,
        )));
        let can_back    = self.tabs[self.active].can_back();
        let can_forward = self.tabs[self.active].can_forward();
        self.bar.draw(
            &mut self.window.canvas,
            &tc,
            &mut self.fonts,
            win_w,
            can_back,
            can_forward,
        );
        self.window.canvas.set_viewport(None);


        self.window.canvas.present();
        self.need_draw = false;
    }
}
