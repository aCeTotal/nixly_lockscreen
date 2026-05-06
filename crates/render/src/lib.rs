mod blur;
mod clock;
mod font;
mod output;
mod rain;
mod renderer;
mod ui;

pub use output::OutputId;
pub use renderer::{PromptInfo, PromptStatus, Renderer};

pub struct SurfaceTarget {
    pub raw_display: raw_window_handle::RawDisplayHandle,
    pub raw_window: raw_window_handle::RawWindowHandle,
    pub width: u32,
    pub height: u32,
}

pub struct Screenshot<'a> {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixels: &'a [u8],
}
