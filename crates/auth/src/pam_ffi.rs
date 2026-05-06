use std::ffi::{c_char, c_int, c_void, CString};
use std::ptr;

#[allow(non_camel_case_types)]
pub type pam_handle_t = c_void;

const PAM_SUCCESS: c_int = 0;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_CONV_ERR: c_int = 19;
const PAM_BUF_ERR: c_int = 5;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

#[repr(C)]
struct PamConv {
    conv: extern "C" fn(c_int, *mut *const PamMessage, *mut *mut PamResponse, *mut c_void) -> c_int,
    appdata_ptr: *mut c_void,
}

extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conv: *const PamConv,
        pamh: *mut *mut pam_handle_t,
    ) -> c_int;
    fn pam_end(pamh: *mut pam_handle_t, status: c_int) -> c_int;
    fn pam_authenticate(pamh: *mut pam_handle_t, flags: c_int) -> c_int;
    fn pam_acct_mgmt(pamh: *mut pam_handle_t, flags: c_int) -> c_int;
}

extern "C" fn conv_fn(
    num_msg: c_int,
    msg: *mut *const PamMessage,
    resp: *mut *mut PamResponse,
    appdata: *mut c_void,
) -> c_int {
    if num_msg <= 0 || msg.is_null() || resp.is_null() || appdata.is_null() {
        return PAM_CONV_ERR;
    }
    unsafe {
        let pwd = &*(appdata as *const CString);
        let arr =
            libc::calloc(num_msg as usize, std::mem::size_of::<PamResponse>()) as *mut PamResponse;
        if arr.is_null() {
            return PAM_BUF_ERR;
        }
        for i in 0..(num_msg as isize) {
            let m_ptr = *msg.offset(i);
            if m_ptr.is_null() {
                continue;
            }
            let m = &*m_ptr;
            let entry = &mut *arr.offset(i);
            match m.msg_style {
                PAM_PROMPT_ECHO_OFF | PAM_PROMPT_ECHO_ON => {
                    entry.resp = libc::strdup(pwd.as_ptr());
                }
                _ => {
                    entry.resp = ptr::null_mut();
                }
            }
            entry.resp_retcode = 0;
        }
        *resp = arr;
    }
    PAM_SUCCESS
}

pub fn check(service: &str, user: &str, password: &str) -> bool {
    let Ok(svc) = CString::new(service) else { return false; };
    let Ok(usr) = CString::new(user) else { return false; };
    let Ok(pwd) = CString::new(password) else { return false; };

    let conv = PamConv {
        conv: conv_fn,
        appdata_ptr: &pwd as *const CString as *mut c_void,
    };

    let mut pamh: *mut pam_handle_t = ptr::null_mut();
    let r = unsafe { pam_start(svc.as_ptr(), usr.as_ptr(), &conv, &mut pamh) };
    if r != PAM_SUCCESS {
        log::error!("pam_start failed: {}", r);
        return false;
    }

    let auth_r = unsafe { pam_authenticate(pamh, 0) };
    let acct_r = if auth_r == PAM_SUCCESS {
        unsafe { pam_acct_mgmt(pamh, 0) }
    } else {
        -1
    };
    unsafe {
        pam_end(pamh, auth_r);
    }
    drop(pwd);

    if auth_r != PAM_SUCCESS {
        log::warn!("pam_authenticate: {}", auth_r);
        return false;
    }
    if acct_r != PAM_SUCCESS {
        log::warn!("pam_acct_mgmt: {}", acct_r);
        return false;
    }
    true
}
