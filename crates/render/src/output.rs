#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputId(pub(crate) usize);

pub(crate) struct OutputState {
    pub surface: wgpu::Surface<'static>,
    pub format: wgpu::TextureFormat,
    pub alpha_mode: wgpu::CompositeAlphaMode,
    pub present_mode: wgpu::PresentMode,
    pub width: u32,
    pub height: u32,
}
