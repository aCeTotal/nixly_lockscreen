use anyhow::{Context, Result};

use smithay_client_toolkit::{
    delegate_registry, delegate_shm,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};
use wayland_client::{
    globals::GlobalList,
    protocol::{wl_output, wl_shm},
    Connection, Dispatch, EventQueue, QueueHandle, WEnum,
};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::{self, ZwlrScreencopyManagerV1},
};

pub struct CapturedOutput {
    pub output: wl_output::WlOutput,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub shm_format: wl_shm::Format,
    pub pixels: Vec<u8>,
}

pub fn capture_all(
    conn: &Connection,
    globals: &GlobalList,
    outputs: &[wl_output::WlOutput],
) -> Result<Vec<CapturedOutput>> {
    let mut event_queue: EventQueue<ScreencopyState> = conn.new_event_queue();
    let qh = event_queue.handle();

    let shm = Shm::bind(globals, &qh).map_err(|e| anyhow::anyhow!("wl_shm: {e}"))?;
    let manager: ZwlrScreencopyManagerV1 = globals
        .bind(&qh, 1..=3, ())
        .map_err(|e| anyhow::anyhow!("zwlr_screencopy_manager_v1: {e}"))?;

    let mut state = ScreencopyState {
        registry_state: RegistryState::new(globals),
        shm,
        slots: Vec::with_capacity(outputs.len()),
    };

    for (idx, output) in outputs.iter().enumerate() {
        let frame = manager.capture_output(0, output, &qh, FrameUserData { idx });
        state.slots.push(Slot {
            output: output.clone(),
            frame,
            width: 0,
            height: 0,
            stride: 0,
            format: wl_shm::Format::Argb8888,
            pool: None,
            buffer: None,
            buffer_done: false,
            ready: false,
            failed: false,
            copy_sent: false,
        });
    }

    while !state.all_settled() {
        event_queue.blocking_dispatch(&mut state)?;
        state.maybe_send_copies()?;
    }

    let mut out = Vec::with_capacity(state.slots.len());
    for mut slot in state.slots {
        slot.frame.destroy();
        if slot.failed {
            log::warn!("screencopy failed for output {:?}", slot.output);
            continue;
        }
        let mut pool = slot.pool.take().context("pool missing")?;
        let buffer = slot.buffer.take().context("buffer missing")?;
        let pixels = buffer
            .canvas(&mut pool)
            .context("canvas")?
            .to_vec();
        out.push(CapturedOutput {
            output: slot.output,
            width: slot.width,
            height: slot.height,
            stride: slot.stride,
            shm_format: slot.format,
            pixels,
        });
    }
    Ok(out)
}

struct Slot {
    output: wl_output::WlOutput,
    frame: ZwlrScreencopyFrameV1,
    width: u32,
    height: u32,
    stride: u32,
    format: wl_shm::Format,
    pool: Option<SlotPool>,
    buffer: Option<Buffer>,
    buffer_done: bool,
    ready: bool,
    failed: bool,
    copy_sent: bool,
}

struct ScreencopyState {
    registry_state: RegistryState,
    shm: Shm,
    slots: Vec<Slot>,
}

impl ScreencopyState {
    fn all_settled(&self) -> bool {
        self.slots.iter().all(|s| s.ready || s.failed)
    }

    fn maybe_send_copies(&mut self) -> Result<()> {
        for slot in self.slots.iter_mut() {
            if slot.buffer_done && !slot.copy_sent && !slot.failed {
                let pool_size = (slot.stride as usize) * (slot.height as usize);
                let mut pool = SlotPool::new(pool_size.max(4), &self.shm)?;
                let (buffer, _canvas) = pool.create_buffer(
                    slot.width as i32,
                    slot.height as i32,
                    slot.stride as i32,
                    slot.format,
                )?;
                slot.frame.copy(buffer.wl_buffer());
                slot.pool = Some(pool);
                slot.buffer = Some(buffer);
                slot.copy_sent = true;
            }
        }
        Ok(())
    }
}

pub struct FrameUserData {
    idx: usize,
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for ScreencopyState {
    fn event(
        _: &mut Self,
        _: &ZwlrScreencopyManagerV1,
        _: zwlr_screencopy_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, FrameUserData> for ScreencopyState {
    fn event(
        state: &mut Self,
        _: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        meta: &FrameUserData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let slot = match state.slots.get_mut(meta.idx) {
            Some(s) => s,
            None => return,
        };
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                if let WEnum::Value(f) = format {
                    slot.format = f;
                    slot.width = width;
                    slot.height = height;
                    slot.stride = stride;
                }
            }
            zwlr_screencopy_frame_v1::Event::BufferDone => {
                slot.buffer_done = true;
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                slot.ready = true;
            }
            zwlr_screencopy_frame_v1::Event::Failed => {
                slot.failed = true;
            }
            _ => {}
        }
    }
}

impl ShmHandler for ScreencopyState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for ScreencopyState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![];
}

delegate_shm!(ScreencopyState);
delegate_registry!(ScreencopyState);

pub fn shm_format_to_render_pixels(captured: &CapturedOutput) -> Vec<u8> {
    let CapturedOutput {
        width,
        height,
        stride,
        shm_format,
        pixels,
        ..
    } = captured;
    let w = *width as usize;
    let h = *height as usize;
    let stride = *stride as usize;
    let mut out = vec![0u8; w * h * 4];
    match shm_format {
        // Memory order B,G,R,A (or X). Already wgpu BGRA8.
        wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => {
            for y in 0..h {
                let src = &pixels[y * stride..y * stride + w * 4];
                let dst = &mut out[y * w * 4..y * w * 4 + w * 4];
                dst.copy_from_slice(src);
            }
        }
        // Memory order R,G,B,A (or X). Swap R<->B.
        wl_shm::Format::Abgr8888 | wl_shm::Format::Xbgr8888 => {
            for y in 0..h {
                let src = &pixels[y * stride..y * stride + w * 4];
                let dst = &mut out[y * w * 4..y * w * 4 + w * 4];
                for x in 0..w {
                    let s = &src[x * 4..x * 4 + 4];
                    let d = &mut dst[x * 4..x * 4 + 4];
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = 255;
                }
            }
        }
        other => {
            log::warn!("unsupported shm format {:?}, copying raw", other);
            for y in 0..h {
                let src = &pixels[y * stride..y * stride + w * 4];
                let dst = &mut out[y * w * 4..y * w * 4 + w * 4];
                dst.copy_from_slice(src);
            }
        }
    }
    out
}
