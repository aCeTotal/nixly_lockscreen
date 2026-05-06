use std::process::{Child, Command};
use std::time::Duration;

use anyhow::{Context, Result};
use input::GamepadActivity;
use smithay_client_toolkit::{
    delegate_registry, delegate_seat,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::wl_seat::WlSeat,
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::{self, ExtIdleNotifierV1},
};

const DEFAULT_TIMEOUT_MS: u32 = 180_000;
const GAMEPAD_GRACE: Duration = Duration::from_secs(15);

fn fullscreen_window_active() -> bool {
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        if let Ok(out) = Command::new("hyprctl").args(["activewindow", "-j"]).output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                if text.contains("\"fullscreen\":2")
                    || text.contains("\"fullscreenMode\":2")
                    || text.contains("\"fullscreen\": 2")
                    || text.contains("\"fullscreenMode\": 2")
                {
                    return true;
                }
            }
        }
    }
    false
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let timeout_ms = std::env::var("NIXLY_IDLE_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_TIMEOUT_MS);
    let lock_cmd =
        std::env::var("NIXLY_LOCK_CMD").unwrap_or_else(|_| "nixly-lockscreen".to_string());

    log::info!("idle: timeout {} ms, lock cmd: {}", timeout_ms, lock_cmd);

    let gamepad = GamepadActivity::spawn();

    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init::<State>(&conn)?;
    let qh = event_queue.handle();

    let mut state = State {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        notifier: None,
        notification: None,
        seat: None,
        timeout_ms,
        idle: false,
        child: None,
        lock_cmd,
        gamepad,
    };

    event_queue.roundtrip(&mut state)?;

    state.notifier = Some(
        globals
            .bind::<ExtIdleNotifierV1, _, _>(&qh, 1..=2, ())
            .context("ext_idle_notifier_v1 not advertised")?,
    );
    let seat = state
        .seat_state
        .seats()
        .next()
        .context("no wl_seat")?;
    state.seat = Some(seat.clone());
    state.create_notification(&qh);

    loop {
        event_queue.blocking_dispatch(&mut state)?;
        state.poll_child();
    }
}

struct State {
    registry_state: RegistryState,
    seat_state: SeatState,
    notifier: Option<ExtIdleNotifierV1>,
    notification: Option<ExtIdleNotificationV1>,
    seat: Option<WlSeat>,
    timeout_ms: u32,
    idle: bool,
    child: Option<Child>,
    lock_cmd: String,
    gamepad: GamepadActivity,
}

impl State {
    fn create_notification(&mut self, qh: &QueueHandle<Self>) {
        let (Some(notifier), Some(seat)) = (self.notifier.as_ref(), self.seat.as_ref()) else {
            return;
        };
        if self.notification.is_some() {
            return;
        }
        let notif = notifier.get_idle_notification(self.timeout_ms, seat, qh, ());
        self.notification = Some(notif);
        log::info!("idle notification created ({}ms)", self.timeout_ms);
    }

    fn reset_notification(&mut self, qh: &QueueHandle<Self>) {
        if let Some(n) = self.notification.take() {
            n.destroy();
        }
        self.idle = false;
        self.create_notification(qh);
    }

    fn on_idled(&mut self, qh: &QueueHandle<Self>) {
        if self.gamepad.active_within(GAMEPAD_GRACE) {
            log::info!("idle: suppressed (gamepad active)");
            self.reset_notification(qh);
            return;
        }
        if fullscreen_window_active() {
            log::info!("idle: suppressed (fullscreen window)");
            self.reset_notification(qh);
            return;
        }
        self.idle = true;
        self.spawn_lock();
    }

    fn spawn_lock(&mut self) {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.child = None;
                }
                Ok(None) => {
                    log::info!("lockscreen already running, skipping spawn");
                    return;
                }
                Err(e) => {
                    log::warn!("try_wait: {}", e);
                    self.child = None;
                }
            }
        }
        match Command::new(&self.lock_cmd).spawn() {
            Ok(child) => {
                log::info!("spawned {} pid={}", self.lock_cmd, child.id());
                self.child = Some(child);
            }
            Err(e) => log::error!("spawn {}: {}", self.lock_cmd, e),
        }
    }

    fn poll_child(&mut self) {
        if let Some(child) = self.child.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                log::info!("lockscreen exited: {}", status);
                self.child = None;
            }
        }
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: Capability,
    ) {
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl Dispatch<ExtIdleNotifierV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotifierV1,
        _: ext_idle_notifier_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtIdleNotificationV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            ext_idle_notification_v1::Event::Idled => {
                log::info!("idled");
                state.on_idled(qh);
            }
            ext_idle_notification_v1::Event::Resumed => {
                log::info!("resumed");
                state.idle = false;
            }
            _ => {}
        }
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![SeatState];
}

delegate_seat!(State);
delegate_registry!(State);
