#[test]
fn print_adapter_selection() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
        flags: wgpu::InstanceFlags::default(),
        dx12_shader_compiler: Default::default(),
        gles_minor_version: Default::default(),
    });
    for a in instance.enumerate_adapters(wgpu::Backends::VULKAN | wgpu::Backends::GL) {
        let i = a.get_info();
        println!("available: {:?} {} ({:?})", i.device_type, i.name, i.backend);
    }
    for pref in [wgpu::PowerPreference::LowPower, wgpu::PowerPreference::HighPerformance] {
        let picked = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: pref,
            compatible_surface: None,
            force_fallback_adapter: false,
        }));
        match picked {
            Some(a) => println!("{:?} picks: {}", pref, a.get_info().name),
            None => println!("{:?} picks: none", pref),
        }
    }
}
