use sdl2::{Sdl, VideoSubsystem};
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::render::renderer::RenderCtx;

pub const DEFAULT_W:   u32 = 1024;
pub const DEFAULT_H:   u32 = 768;
pub const SCROLL_STEP: i32 = 40;

/// Owns the SDL canvas and provides a helper to build a `RenderCtx`.
pub struct AppWindow {
    pub canvas: Canvas<Window>,
}

impl AppWindow {
    pub fn new(sdl: &Sdl, title: &str) -> Result<Self, String> {
        let video: VideoSubsystem = sdl.video()?;
        let window = video
            .window(title, DEFAULT_W, DEFAULT_H)
            .position_centered()
            .resizable()
            .build()
            .map_err(|e| e.to_string())?;

        let canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .map_err(|e| e.to_string())?;

        Ok(AppWindow { canvas })
    }

    /// Build a `RenderCtx` for the current window size and scroll position.
    pub fn make_ctx(&self, scroll_y: i32) -> RenderCtx {
        let (w, h) = self.canvas.output_size()
            .map(|(w, h)| (w as i32, h as i32))
            .unwrap_or((DEFAULT_W as i32, DEFAULT_H as i32));
        RenderCtx {
            viewport_width:  w,
            viewport_height: h,
            scroll_y,
        }
    }
}
