use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::ttf::Sdl2TtfContext;

use crate::dom::node::Node;
use crate::net::resolve_url;
use crate::render::font::FontCache;
use crate::render::image::ImageCache;
use crate::render::layout::LayoutState;
use crate::render::layout::state::LinkArea;
use crate::render::renderer::RenderCtx;
use crate::ui::searchbar::{SearchBar, BAR_HEIGHT};
use crate::ui::tabbar::{TabBar, TAB_BAR_HEIGHT};
use crate::window::window::{AppWindow, DEFAULT_H};

use super::loader::load_dom;

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
}

impl Tab {
    fn new(url: &str) -> Option<Self> {
        let (resolved, dom) = load_dom(url)?;
        Some(Tab {
            dom,
            scroll_y:   0,
            images:     ImageCache::new(),
            history:    History::new(&resolved),
            link_areas: Vec::new(),
        })
    }

    // ---- accessors ----

    pub fn current_url(&self) -> &str { self.history.current() }

    pub fn can_back(&self)    -> bool { self.history.can_back() }
    pub fn can_forward(&self) -> bool { self.history.can_forward() }

    /// Short display title — just the hostname (or full path for file:// URLs).
    pub fn title(&self) -> String {
        let url = self.history.current();
        if let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
            let host = rest.split('/').next().unwrap_or(rest);
            host.to_owned()
        } else if let Some(rest) = url.strip_prefix("file://") {
            // Use just the filename
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
        if let Some((final_url, new_dom)) = load_dom(url) {
            self.dom      = new_dom;
            self.scroll_y = 0;
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
        if let Some((final_url, new_dom)) = load_dom(url) {
            self.dom      = new_dom;
            self.scroll_y = 0;
            // Update the history entry in place so the URL bar stays accurate
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
            tabs:      vec![tab],
            active:    0,
            bar:       SearchBar::new(&bar_url),
            need_draw: true,
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
        if let Some(href) = self.tab().link_at(x, content_y) {
            let href = href.to_owned();
            self.navigate(&href);
        }
    }

    // ---- scrolling ----

    pub fn scroll_by(&mut self, delta: i32) {
        self.tab_mut().scroll_by(delta);
        self.need_draw = true;
    }

    pub fn scroll_to_top(&mut self) {
        self.tab_mut().scroll_to_top();
        self.need_draw = true;
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
            state.layout_node(
                &mut self.window.canvas,
                &tc,
                &mut self.fonts,
                &mut tab.images,
                &base_url,
                &tab.dom,
                win_w - 16,
            );
            tab.link_areas = state.link_areas;
        }

        // ---- chrome (tab bar + address bar) ----
        self.window.canvas.set_viewport(None);

        // Tab bar
        let titles: Vec<String> = self.tabs.iter().map(|t| t.title()).collect();
        TabBar::draw(
            &mut self.window.canvas,
            &tc,
            &mut self.fonts,
            win_w,
            &titles,
            self.active,
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
