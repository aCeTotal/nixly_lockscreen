use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};

const SOCK_PATH: &str = "/run/nixly-lockguard.sock";
const SYSRQ_PATH: &str = "/proc/sys/kernel/sysrq";
const TTY0: &str = "/dev/tty0";

const VT_LOCKSWITCH: libc::c_ulong = 0x560B;
const VT_UNLOCKSWITCH: libc::c_ulong = 0x560C;

fn vt_lock(lock: bool) -> Result<()> {
    let f = File::open(TTY0).with_context(|| format!("open {TTY0}"))?;
    let cmd = if lock { VT_LOCKSWITCH } else { VT_UNLOCKSWITCH };
    let r = unsafe { libc::ioctl(f.as_raw_fd(), cmd) };
    if r != 0 {
        return Err(std::io::Error::last_os_error()).context(if lock {
            "VT_LOCKSWITCH"
        } else {
            "VT_UNLOCKSWITCH"
        });
    }
    Ok(())
}

fn sysrq_read() -> String {
    std::fs::read_to_string(SYSRQ_PATH).unwrap_or_default()
}

fn sysrq_write(val: &str) -> Result<()> {
    let mut f = OpenOptions::new().write(true).open(SYSRQ_PATH)?;
    f.write_all(val.as_bytes())?;
    Ok(())
}

#[derive(Clone, Copy)]
struct PeerCred {
    uid: libc::uid_t,
    pid: libc::pid_t,
}

fn peer_cred(s: &UnixStream) -> Result<PeerCred> {
    let mut ucred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let r = unsafe {
        libc::getsockopt(
            s.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut ucred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if r != 0 {
        return Err(std::io::Error::last_os_error()).context("SO_PEERCRED");
    }
    Ok(PeerCred {
        uid: ucred.uid,
        pid: ucred.pid,
    })
}

static STOP_FLAG: AtomicBool = AtomicBool::new(false);

extern "C" fn signal_handler(_sig: libc::c_int) {
    STOP_FLAG.store(true, Ordering::SeqCst);
}

fn install_signal_handlers() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = signal_handler as *const () as usize;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGHUP, &sa, std::ptr::null_mut());
    }
}

struct Guard {
    prev_sysrq: String,
    armed: bool,
}

impl Guard {
    fn arm() -> Result<Self> {
        let prev_sysrq = sysrq_read();
        sysrq_write("0\n").context("disable sysrq")?;
        if let Err(e) = vt_lock(true) {
            log::warn!("VT lock failed: {e}");
            let _ = sysrq_write(prev_sysrq.trim());
            return Err(e);
        }
        log::info!("guard armed (prev sysrq={})", prev_sysrq.trim());
        Ok(Self {
            prev_sysrq,
            armed: true,
        })
    }

    fn disarm(&mut self) {
        if !self.armed {
            return;
        }
        if let Err(e) = vt_lock(false) {
            log::warn!("VT unlock failed: {e}");
        }
        let val = self.prev_sysrq.trim();
        let val = if val.is_empty() { "1" } else { val };
        if let Err(e) = sysrq_write(val) {
            log::warn!("sysrq restore failed: {e}");
        }
        self.armed = false;
        log::info!("guard disarmed");
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.disarm();
    }
}

fn serve_session(mut conn: UnixStream) -> Result<()> {
    let cred = peer_cred(&conn)?;
    log::info!("client connected uid={} pid={}", cred.uid, cred.pid);

    let mut guard = Guard::arm()?;

    let mut buf = [0u8; 64];
    loop {
        match conn.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }

    guard.disarm();
    log::info!("client disconnected");
    Ok(())
}

fn set_sock_perms(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o666);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if unsafe { libc::geteuid() } != 0 {
        anyhow::bail!("nixly-lockguard must run as root");
    }

    let path = Path::new(SOCK_PATH);
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).with_context(|| format!("bind {SOCK_PATH}"))?;
    set_sock_perms(path)?;
    log::info!("listening on {SOCK_PATH}");

    install_signal_handlers();

    listener.set_nonblocking(true).ok();
    let mut current: Option<std::thread::JoinHandle<()>> = None;
    let busy = Arc::new(AtomicBool::new(false));

    while !STOP_FLAG.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((conn, _)) => {
                if busy.swap(true, Ordering::SeqCst) {
                    log::warn!("rejected concurrent client");
                    drop(conn);
                    continue;
                }
                let busy_c = busy.clone();
                current = Some(std::thread::spawn(move || {
                    if let Err(e) = serve_session(conn) {
                        log::warn!("session error: {e}");
                    }
                    busy_c.store(false, Ordering::SeqCst);
                }));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(80));
            }
            Err(e) => {
                log::warn!("accept error: {e}");
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }

    log::info!("shutting down");
    if let Some(h) = current.take() {
        let _ = h.join();
    }
    let _ = std::fs::remove_file(path);
    Ok(())
}
