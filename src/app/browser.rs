use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::ttf::Sdl2TtfContext;

use crate::dom::node::Node;
use crate::net::resolve_url;
use crate::render::font::FontCache;
use crate::render::image::ImageCache;
use crate::render::layout::LayoutState;
use crate::render::layout::state::{LinkArea, InputArea, ButtonArea, ButtonAction};
use crate::render::renderer::RenderCtx;
use crate::ui::searchbar::{SearchBar, BAR_HEIGHT};
use crate::ui::tabbar::{TabBar, TAB_BAR_HEIGHT};
use crate::window::window::{AppWindow, DEFAULT_H};

use super::loader::load_dom;

// Scrollbar appearance constants
pub const SCROLLBAR_W:         i32 = 12;
const SCROLLBAR_MIN_THUMB: i32 = 24;
const SCROLLBAR_TRACK_COLOR: (u8, u8, u8) = (220, 220, 220);
const SCROLLBAR_THUMB_COLOR: (u8, u8, u8) = (160, 160, 160);
const SCROLLBAR_THUMB_HOVER: (u8, u8, u8) = (120, 120, 120);

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
// Tab — one browser tab, owns its own DOM / history / scroll / image cache
// ---------------------------------------------------------------------------

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
}

impl Tab {
    fn new(url: &str) -> Option<Self> {
        let (resolved, dom, meta) = load_dom(url)?;
        Some(Tab {
            dom,
            scroll_y:       0,
            images:         ImageCache::new(),
            history:        History::new(&resolved),
            link_areas:     Vec::new(),
            input_areas:    Vec::new(),
            button_areas:   Vec::new(),
            input_values:   Vec::new(),
            focused_input:  None,
            content_height: 0,
            page_title:     meta.title,
            favicon_url:    meta.favicon_url,
        })
    }

    // ---- accessors ----

    pub fn current_url(&self) -> &str { self.history.current() }

    pub fn can_back(&self)    -> bool { self.history.can_back() }
    pub fn can_forward(&self) -> bool { self.history.can_forward() }

    /// Short display title — page `<title>` text, falling back to the hostname.
    pub fn title(&self) -> String {
        if !self.page_title.is_empty() {
            // Truncate very long titles
            let t = &self.page_title;
            if t.chars().count() > 40 {
                let s: String = t.chars().take(38).collect();
                return format!("{}…", s);
            }
            return t.clone();
        }
        // Fallback: hostname
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

    // ---- navigation ----

    pub fn navigate(&mut self, url: &str) {
        let resolved = resolve_url(url, self.history.current());
        self.navigate_absolute(&resolved);
    }

    pub fn navigate_absolute(&mut self, url: &str) {
        if let Some((final_url, new_dom, meta)) = load_dom(url) {
            self.dom            = new_dom;
            self.scroll_y       = 0;
            self.input_values   = Vec::new();
            self.focused_input  = None;
            self.button_areas   = Vec::new();
            self.page_title     = meta.title;
            self.favicon_url    = meta.favicon_url;
            self.history.push(final_url);
        } else {
            eprintln!("navigate: failed to load {url}");
        }
    }

    pub fn go_back(&mut self) {
        if let Some(url) = self.history.go_back() {
            let url = url.to_owned();
            self.load_url_no_history(&url);
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(url) = self.history.go_forward() {
            let url = url.to_owned();
            self.load_url_no_history(&url);
        }
    }

    pub fn reload(&mut self) {
        let url = self.history.current().to_owned();
        self.load_url_no_history(&url);
    }

    fn load_url_no_history(&mut self, url: &str) {
        if let Some((final_url, new_dom, meta)) = load_dom(url) {
            self.dom           = new_dom;
            self.scroll_y      = 0;
            self.input_values  = Vec::new();
            self.focused_input = None;
            self.button_areas  = Vec::new();
            self.page_title    = meta.title;
            self.favicon_url   = meta.favicon_url;
            let _ = final_url;
        }
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
            self.ensure_input_slot(idx);
            self.input_values[idx].push_str(text);
        }
    }

    /// Handle backspace in the focused input.
    pub fn backspace(&mut self) {
        if let Some(idx) = self.focused_input {
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
}

impl<'ttf> Browser<'ttf> {
    pub fn new(
        sdl:     &sdl2::Sdl,
        ttf_ctx: &'ttf Sdl2TtfContext,
        initial: &str,
    ) -> Result<Self, String> {
        let window = AppWindow::new(sdl, "Forkit")?;
        let fonts  = FontCache::new(ttf_ctx);

        let tab = Tab::new(initial)
            .ok_or_else(|| format!("Failed to load initial page: {initial}"))?;

        let bar_url = tab.current_url().to_owned();

        Ok(Browser {
            window,
            fonts,
            tabs:           vec![tab],
            active:         0,
            bar:            SearchBar::new(&bar_url),
            need_draw:      true,
            scrollbar_drag: None,
        })
    }

    // ---- active-tab helpers ----

    pub fn tab(&self)      -> &Tab      { &self.tabs[self.active] }
    pub fn tab_mut(&mut self) -> &mut Tab { &mut self.tabs[self.active] }

    // ---- tab management ----

    /// Open a new tab navigating to `url` and switch to it.
    pub fn open_tab(&mut self, url: &str) {
        if let Some(tab) = Tab::new(url) {
            let bar_url = tab.current_url().to_owned();
            self.tabs.push(tab);
            self.active = self.tabs.len() - 1;
            self.bar    = SearchBar::new(&bar_url);
        }
        self.need_draw = true;
    }

    /// Open a blank new tab (about:blank placeholder).
    pub fn open_blank_tab(&mut self) {
        self.open_tab("about:blank");
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
                ButtonAction::None => {}
            }
            return;
        }

        // Check for a focused input
        if let Some(idx) = self.tab().input_at(x, content_y) {
            self.tab_mut().focused_input = Some(idx);
            // Defocus the address bar when clicking a page input
            self.bar.focused = false;
            self.need_draw = true;
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
        let (_, win_h) = self.window.canvas.output_size()
            .map(|(w, h)| (w as i32, h as i32))
            .unwrap_or((1024, 768));
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

        let (win_w, win_h) = self.window.canvas.output_size()
            .map(|(w, h)| (w as i32, h as i32))
            .unwrap_or((1024, 768));

        let chrome_h  = chrome_height();
        let content_h = (win_h - chrome_h).max(0);

        let tc = self.window.canvas.texture_creator();

        // Clear full window
        self.window.canvas.set_draw_color(Color::WHITE);
        self.window.canvas.clear();

        // ---- content ----
        let _ = self.window.canvas.set_viewport(Some(Rect::new(
            0, chrome_h, win_w as u32, content_h as u32,
        )));

        {
            let tab  = &mut self.tabs[self.active];
            let ctx  = RenderCtx {
                viewport_width:  win_w,
                viewport_height: content_h,
                scroll_y:        tab.scroll_y,
            };
            let base_url = tab.current_url().to_owned();
            let mut state = LayoutState::new(&ctx);
            // Seed live input state so form controls render correctly
            state.set_input_state(tab.input_values.clone(), tab.focused_input);
            state.layout_node(
                &mut self.window.canvas,
                &tc,
                &mut self.fonts,
                &mut tab.images,
                &base_url,
                &tab.dom,
                // Reserve SCROLLBAR_W on the right for the scrollbar
                win_w - SCROLLBAR_W - 4,
            );
            tab.link_areas     = state.link_areas;
            tab.input_areas    = state.input_areas;
            tab.button_areas   = state.button_areas;
            tab.content_height = state.content_height;
        }

        // Reset viewport before drawing the scrollbar so coordinates are
        // window-absolute (the content viewport is still active at this point).
        self.window.canvas.set_viewport(None);

        // ---- scrollbar ----
        if let Some((track_y, track_h, thumb_y, thumb_h)) =
            self.scrollbar_geometry(content_h)
        {
            let sx = win_w - SCROLLBAR_W;

            // Track
            let (tr, tg, tb) = SCROLLBAR_TRACK_COLOR;
            self.window.canvas.set_draw_color(Color::RGB(tr, tg, tb));
            let _ = self.window.canvas.fill_rect(Rect::new(
                sx, chrome_h + track_y, SCROLLBAR_W as u32, track_h as u32,
            ));

            // Thumb
            let dragging = self.scrollbar_drag.is_some();
            let (cr, cg, cb) = if dragging { SCROLLBAR_THUMB_HOVER } else { SCROLLBAR_THUMB_COLOR };
            self.window.canvas.set_draw_color(Color::RGB(cr, cg, cb));
            let _ = self.window.canvas.fill_rect(Rect::new(
                sx + 2,
                chrome_h + track_y + thumb_y,
                (SCROLLBAR_W - 4) as u32,
                thumb_h as u32,
            ));
        }

        // ---- chrome (tab bar + address bar) ----
        // (viewport already reset to None above)

        // Tab bar
        let titles: Vec<String> = self.tabs.iter().map(|t| t.title()).collect();
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
