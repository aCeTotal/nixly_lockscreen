mod pam_ffi;

use std::time::{Duration, Instant};

const TIER1_FAILS: u32 = 5;
const TIER1_LOCKOUT: Duration = Duration::from_secs(30);
const TIER2_FAILS: u32 = 10;
const TIER2_LOCKOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Bad,
    LockedOut,
}

pub struct Authenticator {
    service: String,
    user: String,
    failed: u32,
    lockout_until: Option<Instant>,
}

impl Authenticator {
    pub fn new(service: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            user: user.into(),
            failed: 0,
            lockout_until: None,
        }
    }

    pub fn lockout_remaining(&self) -> Option<Duration> {
        let until = self.lockout_until?;
        let now = Instant::now();
        if until <= now {
            None
        } else {
            Some(until - now)
        }
    }

    pub fn is_locked_out(&self) -> bool {
        self.lockout_remaining().is_some()
    }

    pub fn failed_attempts(&self) -> u32 {
        self.failed
    }

    pub fn verify(&mut self, password: &str) -> Verdict {
        if self.is_locked_out() {
            return Verdict::LockedOut;
        }
        if pam_ffi::check(&self.service, &self.user, password) {
            self.failed = 0;
            self.lockout_until = None;
            Verdict::Ok
        } else {
            self.failed = self.failed.saturating_add(1);
            if self.failed >= TIER2_FAILS {
                self.lockout_until = Some(Instant::now() + TIER2_LOCKOUT);
            } else if self.failed >= TIER1_FAILS && self.failed % TIER1_FAILS == 0 {
                self.lockout_until = Some(Instant::now() + TIER1_LOCKOUT);
            }
            Verdict::Bad
        }
    }
}

pub fn current_user() -> Option<String> {
    if let Ok(u) = std::env::var("USER") {
        if !u.is_empty() {
            return Some(u);
        }
    }
    if let Ok(u) = std::env::var("LOGNAME") {
        if !u.is_empty() {
            return Some(u);
        }
    }
    let uid = unsafe { libc::getuid() };
    let mut buf = vec![0i8; 4096];
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    let r = unsafe {
        libc::getpwuid_r(
            uid,
            &mut pwd,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if r != 0 || result.is_null() {
        return None;
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) };
    cstr.to_str().ok().map(String::from)
}

pub fn default_service() -> String {
    std::env::var("NIXLY_LOCKSCREEN_PAM_SERVICE").unwrap_or_else(|_| "nixly-lockscreen".to_string())
}
