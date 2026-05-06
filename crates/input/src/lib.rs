use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use evdev::{Device, KeyCode};

const RESCAN_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct GamepadActivity {
    last_unix_ms: Arc<AtomicI64>,
}

impl GamepadActivity {
    pub fn spawn() -> Self {
        let stamp = Arc::new(AtomicI64::new(0));
        let stamp_for_thread = stamp.clone();
        thread::Builder::new()
            .name("gamepad-scanner".into())
            .spawn(move || scanner_loop(stamp_for_thread))
            .expect("spawn gamepad scanner");
        Self { last_unix_ms: stamp }
    }

    pub fn last_unix_ms(&self) -> i64 {
        self.last_unix_ms.load(Ordering::Relaxed)
    }

    pub fn active_within(&self, window: Duration) -> bool {
        let last = self.last_unix_ms.load(Ordering::Relaxed);
        if last <= 0 {
            return false;
        }
        let now = unix_now_ms();
        now > 0 && now - last < window.as_millis() as i64
    }
}

fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn is_gamepad(dev: &Device) -> bool {
    let keys = match dev.supported_keys() {
        Some(k) => k,
        None => return false,
    };
    keys.contains(KeyCode::BTN_SOUTH)
        || keys.contains(KeyCode::BTN_TRIGGER)
        || keys.contains(KeyCode::BTN_START)
}

fn scanner_loop(stamp: Arc<AtomicI64>) {
    let mut handles: HashMap<PathBuf, JoinHandle<()>> = HashMap::new();
    loop {
        let entries = match std::fs::read_dir("/dev/input") {
            Ok(it) => it,
            Err(e) => {
                log::warn!("gamepad: read /dev/input: {}", e);
                thread::sleep(RESCAN_INTERVAL);
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if !name.starts_with("event") {
                continue;
            }
            if handles.get(&path).map(|h| !h.is_finished()).unwrap_or(false) {
                continue;
            }
            handles.remove(&path);

            let dev = match Device::open(&path) {
                Ok(d) => d,
                Err(e) => {
                    log::trace!("open {:?}: {}", path, e);
                    continue;
                }
            };
            if !is_gamepad(&dev) {
                continue;
            }
            log::info!(
                "gamepad: {} ({})",
                path.display(),
                dev.name().unwrap_or("?"),
            );
            let stamp_clone = stamp.clone();
            let path_clone = path.clone();
            let handle = thread::Builder::new()
                .name(format!("gamepad:{}", name))
                .spawn(move || reader_loop(dev, path_clone, stamp_clone))
                .expect("spawn gamepad reader");
            handles.insert(path, handle);
        }
        thread::sleep(RESCAN_INTERVAL);
    }
}

fn reader_loop(mut dev: Device, path: PathBuf, stamp: Arc<AtomicI64>) {
    loop {
        match dev.fetch_events() {
            Ok(events) => {
                let mut hit = false;
                for _ev in events {
                    hit = true;
                }
                if hit {
                    stamp.store(unix_now_ms(), Ordering::Relaxed);
                }
            }
            Err(e) => {
                log::info!("gamepad {} closed: {}", path.display(), e);
                return;
            }
        }
    }
}
