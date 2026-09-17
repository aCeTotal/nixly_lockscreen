use std::ptr::NonNull;
use std::time::Instant;

use anyhow::{Context, Result};
use auth::{Authenticator, Verdict};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use render::{OutputId, PromptInfo, PromptStatus, Renderer, SurfaceTarget};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_session_lock,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    session_lock::{
        SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface,
        SessionLockSurfaceConfigure,
    },
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
    Connection, Proxy, QueueHandle,
};

const LOCKGUARD_SOCK: &str = "/run/nixly-lockguard.sock";

fn connect_lockguard() -> Option<std::os::unix::net::UnixStream> {
    match std::os::unix::net::UnixStream::connect(LOCKGUARD_SOCK) {
        Ok(s) => {
            log::info!("lockguard connected ({LOCKGUARD_SOCK})");
            Some(s)
        }
        Err(e) => {
            log::warn!(
                "lockguard unavailable ({e}); TTY/sysrq lockdown DISABLED. \
                 Enable services.nixly-lockscreen in NixOS to engage lockdown."
            );
            None
        }
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let no_auth = std::env::var("NIXLY_LOCKSCREEN_NO_AUTH")
        .map(|v| v == "1")
        .unwrap_or(false);

    let _guard_sock = if DEMO_MODE {
        log::warn!("DEMO_MODE: skipping lockguard, auth, prompt UI. ESC to exit.");
        None
    } else if no_auth {
        log::info!("no-auth mode (autologin): skipping lockguard; any input unlocks");
        None
    } else {
        connect_lockguard()
    };

    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)
        .map_err(|_| anyhow::anyhow!("wl_compositor missing"))?;
    let session_lock_state = SessionLockState::new(&globals, &qh);

    let user = auth::current_user().unwrap_or_else(|| "root".to_string());
    let service = auth::default_service();
    log::info!("auth: user={} service={}", user, service);
    let authenticator = Authenticator::new(service, user.clone());

    let mut state = State {
        renderer: None,
        lock_surfaces: Vec::new(),
        keyboard: None,
        pointer: None,
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        session_lock_state,
        session_lock: None,
        qh: qh.clone(),
        conn: conn.clone(),
        finished: false,
        exit: false,
        authenticator,
        prompt_buf: String::new(),
        prompt_status: None,
        last_input: Instant::now(),
        unlock_pending: false,
        awake: false,
        cursor_output: None,
        last_auto_attempt: None,
        no_auth,
    };

    event_queue.roundtrip(&mut state)?;

    log::info!("discovered {} outputs", state.output_state.outputs().count());

    let session_lock = state
        .session_lock_state
        .lock(&qh)
        .map_err(|_| anyhow::anyhow!("compositor lacks ext-session-lock-v1"))?;
    state.session_lock = Some(session_lock);

    let mut renderer = Renderer::new();
    renderer.set_username(user);
    state.renderer = Some(renderer);

    state.create_lock_surfaces();

    while !state.exit {
        event_queue.blocking_dispatch(&mut state)?;
        if state.unlock_pending
            && state
                .renderer
                .as_ref()
                .map(|r| r.unlock_done())
                .unwrap_or(true)
        {
            state.exit = true;
        }
    }

    state.renderer = None;

    if let Some(lock) = state.session_lock.take() {
        lock.unlock();
        conn.roundtrip()?;
    }
    log::info!("locker exited cleanly");
    Ok(())
}

struct LockSurface {
    surface: SessionLockSurface,
    output: wl_output::WlOutput,
    width: u32,
    height: u32,
    output_id: Option<OutputId>,
    pending_render: bool,
    presented: bool,
}

struct State {
    renderer: Option<Renderer>,
    lock_surfaces: Vec<LockSurface>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,

    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,

    #[allow(dead_code)]
    session_lock_state: SessionLockState,
    session_lock: Option<SessionLock>,

    qh: QueueHandle<Self>,
    conn: Connection,
    finished: bool,
    exit: bool,

    authenticator: Authenticator,
    prompt_buf: String,
    prompt_status: Option<PromptStatus>,
    last_input: Instant,
    unlock_pending: bool,
    awake: bool,
    cursor_output: Option<OutputId>,
    last_auto_attempt: Option<String>,
    no_auth: bool,
}

const MAX_PASSWORD_LEN: usize = 128;
const PROMPT_TIMEOUT_S: f32 = 15.0;
const FAIL_FLASH_S: f32 = 1.2;
const IDLE_TO_BLACK_S: f32 = 15.0;
const AUTO_VERIFY_DEBOUNCE_MS: u128 = 1000;
const AUTO_VERIFY_MIN_LEN: usize = 4;

const DEMO_MODE: bool = false;

impl State {
    fn create_lock_surfaces(&mut self) {
        let Some(lock) = self.session_lock.as_ref() else {
            return;
        };
        let outputs: Vec<_> = self.output_state.outputs().collect();
        for output in outputs {
            if self.lock_surfaces.iter().any(|s| s.output == output) {
                continue;
            }
            let wl_surface = self.compositor.create_surface(&self.qh);
            let lock_surface = lock.create_lock_surface(wl_surface, &output, &self.qh);
            self.lock_surfaces.push(LockSurface {
                surface: lock_surface,
                output,
                width: 0,
                height: 0,
                output_id: None,
                pending_render: false,
                presented: false,
            });
        }
    }

    fn ensure_renderer_output(&mut self, idx: usize) -> Result<()> {
        let surf = &self.lock_surfaces[idx];
        if surf.output_id.is_some() || surf.width == 0 || surf.height == 0 {
            return Ok(());
        }
        let renderer = self
            .renderer
            .as_mut()
            .context("renderer not initialised")?;

        let raw_display = make_display_handle(&self.conn)?;
        let raw_window = make_window_handle(surf.surface.wl_surface())?;
        let target = SurfaceTarget {
            raw_display,
            raw_window,
            width: surf.width,
            height: surf.height,
        };
        let id = renderer.add_output(target)?;
        self.lock_surfaces[idx].output_id = Some(id);
        Ok(())
    }

    // The idle screen is a static black frame, so the callback loop only
    // runs while something animates. Any input kicks it back to life.
    fn needs_frames(&self) -> bool {
        self.unlock_pending || (self.awake && self.prompt_status.is_some())
    }

    fn render_surface(&mut self, idx: usize) {
        self.tick_prompt();

        // Request the frame callback without committing: an empty commit before
        // the first buffer is a session-lock protocol error (null_buffer). The
        // wgpu present below commits the surface and carries the request along.
        if self.needs_frames() && !self.lock_surfaces[idx].pending_render {
            let wl = self.lock_surfaces[idx].surface.wl_surface().clone();
            wl.frame(&self.qh, wl.clone());
            self.lock_surfaces[idx].pending_render = true;
        }

        let id = self.lock_surfaces[idx].output_id;
        let presented = match (self.renderer.as_mut(), id) {
            (Some(renderer), Some(id)) => match renderer.render(&id) {
                Ok(()) => true,
                Err(e) => {
                    log::warn!("render error: {e}");
                    false
                }
            },
            _ => false,
        };
        if presented {
            self.lock_surfaces[idx].presented = true;
        } else if self.lock_surfaces[idx].pending_render && self.lock_surfaces[idx].presented {
            // The frame request above only reaches the compositor with a
            // commit. If nothing was presented this pass, the request is
            // orphaned, no callback ever fires and the render loop dies —
            // the lockscreen freezes until some other path happens to
            // commit (the recurring slow/fast cycle). A bare commit
            // latches the request so the loop retries next vblank; only
            // legal once a buffer has been attached.
            self.lock_surfaces[idx].surface.wl_surface().commit();
        } else {
            // No buffer attached yet: a bare commit would be a session-
            // lock protocol error. Drop the flag so the next configure or
            // render attempt re-requests the callback.
            self.lock_surfaces[idx].pending_render = false;
        }
    }

    fn tick_prompt(&mut self) {
        if DEMO_MODE {
            if let Some(r) = self.renderer.as_mut() {
                r.set_prompt(None);
            }
            return;
        }
        let now = Instant::now();
        let elapsed_since_input = now.duration_since(self.last_input).as_secs_f32();

        if self.awake && elapsed_since_input > IDLE_TO_BLACK_S {
            self.awake = false;
            self.clear_prompt();
        }

        if let Some(status) = self.prompt_status {
            let timed_out = elapsed_since_input > PROMPT_TIMEOUT_S;
            let recover_from_fail = matches!(status, PromptStatus::Failed)
                && elapsed_since_input > FAIL_FLASH_S;
            let lockout_done = matches!(status, PromptStatus::LockedOut)
                && !self.authenticator.is_locked_out();

            if timed_out {
                self.clear_prompt();
            } else if recover_from_fail {
                self.prompt_buf.clear();
                self.last_auto_attempt = None;
                self.prompt_status = Some(PromptStatus::Typing);
            } else if lockout_done {
                self.prompt_buf.clear();
                self.last_auto_attempt = None;
                self.prompt_status = Some(PromptStatus::Typing);
            }
        }

        self.maybe_auto_verify(now);

        self.update_renderer_prompt();
    }

    fn maybe_auto_verify(&mut self, now: Instant) {
        if self.unlock_pending || !self.awake {
            return;
        }
        if self.authenticator.is_locked_out() {
            return;
        }
        if !matches!(self.prompt_status, Some(PromptStatus::Typing)) {
            return;
        }
        let len = self.prompt_buf.chars().count();
        if len < AUTO_VERIFY_MIN_LEN {
            return;
        }
        let idle_ms = now.duration_since(self.last_input).as_millis();
        if idle_ms < AUTO_VERIFY_DEBOUNCE_MS {
            return;
        }
        if let Some(prev) = &self.last_auto_attempt {
            if prev == &self.prompt_buf {
                return;
            }
        }
        self.auto_try_auth();
    }

    fn auto_try_auth(&mut self) {
        if self.prompt_buf.is_empty() {
            return;
        }
        let buf = self.prompt_buf.clone();
        self.prompt_status = Some(PromptStatus::Authing);
        self.update_renderer_prompt();
        log::debug!("auto-verify ({} chars)", buf.chars().count());

        let verdict = self.authenticator.verify(&buf);
        self.last_auto_attempt = Some(buf);
        match verdict {
            Verdict::Ok => {
                log::info!("auto-auth ok");
                self.prompt_buf.clear();
                self.last_auto_attempt = None;
                self.unlock_pending = true;
                if let Some(r) = self.renderer.as_mut() {
                    r.start_unlock();
                }
                self.request_all_frames();
            }
            Verdict::Bad => {
                self.prompt_status = Some(PromptStatus::Failed);
            }
            Verdict::LockedOut => {
                self.prompt_buf.clear();
                self.last_auto_attempt = None;
                self.prompt_status = Some(PromptStatus::LockedOut);
            }
        }
    }

    // Any input goes straight to the password prompt.
    fn handle_input_activity(&mut self) {
        self.last_input = Instant::now();
        if self.no_auth {
            if !self.unlock_pending {
                log::info!("no-auth mode: input received, unlocking");
                self.unlock_pending = true;
                if let Some(r) = self.renderer.as_mut() {
                    r.start_unlock();
                }
            }
            return;
        }
        self.awake = true;
        if self.prompt_status.is_none() {
            self.prompt_status = Some(if self.authenticator.is_locked_out() {
                PromptStatus::LockedOut
            } else {
                PromptStatus::Typing
            });
        }
    }

    fn update_renderer_prompt(&mut self) {
        let prompt = if self.awake {
            self.prompt_status.map(|status| PromptInfo {
                chars: self.prompt_buf.chars().count() as u32,
                status,
            })
        } else {
            None
        };
        let prompt_target = self.cursor_output.or_else(|| {
            self.lock_surfaces
                .iter()
                .find_map(|s| s.output_id)
        });
        if let Some(r) = self.renderer.as_mut() {
            r.set_prompt(prompt);
            r.set_prompt_output(prompt_target);
        }
    }

    fn clear_prompt(&mut self) {
        self.prompt_buf.clear();
        self.prompt_status = None;
        self.last_auto_attempt = None;
    }

    // Restart the callback loop after input if anything needs drawing.
    fn kick_frames(&mut self) {
        if self.needs_frames() {
            self.request_all_frames();
        }
    }

    fn request_all_frames(&mut self) {
        for s in &mut self.lock_surfaces {
            // Never commit a surface that has not presented a buffer yet.
            if s.pending_render || !s.presented {
                continue;
            }
            let wl = s.surface.wl_surface().clone();
            wl.frame(&self.qh, wl.clone());
            wl.commit();
            s.pending_render = true;
        }
    }

    fn handle_keypress(&mut self, event: KeyEvent) {
        if self.unlock_pending {
            return;
        }
        if DEMO_MODE {
            if event.keysym == Keysym::Escape {
                log::info!("DEMO_MODE: ESC pressed, exiting");
                self.unlock_pending = true;
                if let Some(r) = self.renderer.as_mut() {
                    r.start_unlock();
                }
                self.request_all_frames();
            }
            return;
        }
        self.handle_input_activity();

        if self.authenticator.is_locked_out() {
            self.prompt_status = Some(PromptStatus::LockedOut);
            self.prompt_buf.clear();
            return;
        }

        match event.keysym {
            Keysym::Escape => {
                self.awake = false;
                self.clear_prompt();
                return;
            }
            Keysym::BackSpace => {
                self.prompt_buf.pop();
                self.prompt_status = Some(PromptStatus::Typing);
                return;
            }
            Keysym::Return | Keysym::KP_Enter => {
                self.try_auth();
                return;
            }
            _ => {}
        }

        if let Some(text) = event.utf8 {
            for ch in text.chars() {
                if !ch.is_control() && self.prompt_buf.len() < MAX_PASSWORD_LEN {
                    self.prompt_buf.push(ch);
                }
            }
            self.prompt_status = Some(PromptStatus::Typing);
        }
    }

    fn try_auth(&mut self) {
        if self.prompt_buf.is_empty() {
            return;
        }
        self.prompt_status = Some(PromptStatus::Authing);
        self.update_renderer_prompt();
        log::info!("auth: verifying ({} chars)", self.prompt_buf.chars().count());

        let verdict = self.authenticator.verify(&self.prompt_buf);
        self.prompt_buf.clear();
        match verdict {
            Verdict::Ok => {
                log::info!("auth ok, fading");
                self.unlock_pending = true;
                if let Some(r) = self.renderer.as_mut() {
                    r.start_unlock();
                }
                self.request_all_frames();
            }
            Verdict::Bad => {
                self.prompt_status = Some(PromptStatus::Failed);
            }
            Verdict::LockedOut => {
                self.prompt_status = Some(PromptStatus::LockedOut);
            }
        }
    }

}

fn make_display_handle(conn: &Connection) -> Result<RawDisplayHandle> {
    let ptr = conn.backend().display_ptr() as *mut std::ffi::c_void;
    let nn = NonNull::new(ptr).context("null display ptr")?;
    Ok(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(nn)))
}

fn make_window_handle(surface: &wl_surface::WlSurface) -> Result<RawWindowHandle> {
    let ptr = surface.id().as_ptr() as *mut std::ffi::c_void;
    let nn = NonNull::new(ptr).context("null surface ptr")?;
    Ok(RawWindowHandle::Wayland(WaylandWindowHandle::new(nn)))
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wayland_client::protocol::wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        let idx = self
            .lock_surfaces
            .iter()
            .position(|s| s.surface.wl_surface() == surface);
        // Render on every frame callback: the lockscreen runs at whatever
        // refresh rate the compositor drives the output at (max — nixlytile
        // forces the panel's top mode for the whole lock).
        if let Some(idx) = idx {
            self.lock_surfaces[idx].pending_render = false;
            self.render_surface(idx);
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        if self.session_lock.is_some() {
            self.create_lock_surfaces();
        }
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let kb = self
                .seat_state
                .get_keyboard(qh, &seat, None)
                .expect("get keyboard");
            self.keyboard = Some(kb);
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            let p = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("get pointer");
            self.pointer = Some(p);
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(kb) = self.keyboard.take() {
                kb.release();
            }
        }
        if capability == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        log::debug!("key {:?}", event.keysym);
        self.handle_keypress(event);
        self.kick_frames();
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        _: u32,
    ) {
    }
}

impl SessionLockHandler for State {
    fn locked(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        log::info!("session locked");
    }
    fn finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        log::warn!("session lock finished by compositor");
        self.finished = true;
        self.exit = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        session_lock_surface: SessionLockSurface,
        configure: SessionLockSurfaceConfigure,
        _serial: u32,
    ) {
        let (w, h) = configure.new_size;
        log::info!("configure surface size {}x{}", w, h);
        let w = w.max(1);
        let h = h.max(1);
        let idx = self
            .lock_surfaces
            .iter()
            .position(|s| s.surface.wl_surface() == session_lock_surface.wl_surface());
        let Some(idx) = idx else {
            return;
        };
        let resized = self.lock_surfaces[idx].width != w || self.lock_surfaces[idx].height != h;
        self.lock_surfaces[idx].width = w;
        self.lock_surfaces[idx].height = h;

        if let Err(e) = self.ensure_renderer_output(idx) {
            log::error!("ensure renderer: {e}");
            return;
        }
        if resized {
            if let (Some(r), Some(id)) = (
                self.renderer.as_mut(),
                self.lock_surfaces[idx].output_id.as_ref(),
            ) {
                r.resize(id, w, h);
            }
        }
        self.render_surface(idx);
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let mut real_input = false;
        for e in events {
            match e.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    if let Some(id) = self
                        .lock_surfaces
                        .iter()
                        .find(|s| s.surface.wl_surface() == &e.surface)
                        .and_then(|s| s.output_id)
                    {
                        self.cursor_output = Some(id);
                    }
                }
                _ => {}
            }
            if matches!(
                e.kind,
                PointerEventKind::Motion { .. }
                    | PointerEventKind::Press { .. }
                    | PointerEventKind::Release { .. }
                    | PointerEventKind::Axis { .. }
            ) {
                real_input = true;
            }
        }
        if real_input {
            self.handle_input_activity();
            self.kick_frames();
        }
    }
}

delegate_compositor!(State);
delegate_output!(State);
delegate_seat!(State);
delegate_keyboard!(State);
delegate_pointer!(State);
delegate_session_lock!(State);
delegate_registry!(State);
