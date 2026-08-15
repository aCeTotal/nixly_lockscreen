use crate::blur::MipLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputId(pub(crate) usize);

pub(crate) struct OutputState {
    pub surface: wgpu::Surface<'static>,
    pub format: wgpu::TextureFormat,
    pub alpha_mode: wgpu::CompositeAlphaMode,
    pub width: u32,
    pub height: u32,
    pub bg_view: wgpu::TextureView,
    pub bg_width: u32,
    pub bg_height: u32,
    pub mip_chain: Vec<MipLevel>,
    pub blurred_offset: Option<f32>,
    pub _bg_texture: wgpu::Texture,
}
