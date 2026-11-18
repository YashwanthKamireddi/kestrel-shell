//! Checking the password through PAM, the system's own login rules.
//!
//! As swaylock (pam.c) and hyprlock (src/auth/Pam.cpp) do: `pam_start` for the user this
//! process runs as, `pam_authenticate`, and on success `pam_setcred(PAM_REFRESH_CRED)` (as
//! swaylock). The conversation answers the first password prompt with the typed password, once;
//! any later prompt is refused, so a module that asks again (pam_systemd_home retries, as
//! swaylock notes) cannot loop. PAM's own messages (pam_faillock's "account is locked ... left
//! to unlock") are kept and shown.
//!
//! Service: `login`. On Arch, hyprlock's `/etc/pam.d/hyprlock` is `auth include login` and
//! swaylock's is the same, so this is exactly their check, with no new file in /etc/pam.d. A
//! name with no file there would fall to `/etc/pam.d/other`, which denies everything: a lock
//! that can never be unlocked. See docs/LOCK.md for a dedicated `kestrel-lock` service.
//!
//! `authenticate` blocks (pam_unix waits about two seconds after a wrong password) and is only
//! ever called off the UI thread. The password reaches PAM in one malloc'd copy, which
//! Linux-PAM wipes before freeing (`_pam_drop_reply`); kestrel's copy is the caller's `Secret`,
//! wiped when dropped.

use super::secret::Secret;
use std::ffi::{CStr, CString, c_char, c_int, c_void};

/// The PAM service whose rules check the password (see the module comment).
pub const SERVICE: &str = "login";

/// Proof that PAM accepted the password. Only this module can make one, and unlocking takes
/// one, so nothing but a successful authentication can unlock the session.
#[derive(Clone, Debug)]
pub struct Accepted(());

impl Accepted {
    #[cfg(test)]
    pub fn for_test() -> Self {
        Accepted(())
    }
}

/// What one attempt came to.
#[derive(Clone, Debug)]
pub enum Verdict {
    Accepted(Accepted),
    /// Wrong password (or the account refused); PAM's own messages, if it said any.
    Rejected(Vec<String>),
    /// PAM could not check at all (a broken configuration, a module error).
    Failed(String),
}

// Linux-PAM's ABI (security/_pam_types.h).
const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 19;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;
const PAM_REFRESH_CRED: c_int = 0x0010;
const PAM_MAX_NUM_MSG: c_int = 32;
/// Results that mean "not this password" rather than "PAM is broken".
const PAM_PERM_DENIED: c_int = 6;
const PAM_AUTH_ERR: c_int = 7;
const PAM_USER_UNKNOWN: c_int = 10;
const PAM_MAXTRIES: c_int = 11;

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

type ConvFn = unsafe extern "C" fn(
    c_int,
    *mut *const PamMessage,
    *mut *mut PamResponse,
    *mut c_void,
) -> c_int;

#[repr(C)]
struct PamConv {
    conv: ConvFn,
    appdata_ptr: *mut c_void,
}

#[repr(C)]
struct PamHandle {
    _private: [u8; 0],
}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conv: *const PamConv,
        pamh: *mut *mut PamHandle,
    ) -> c_int;
    fn pam_authenticate(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_setcred(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_end(pamh: *mut PamHandle, status: c_int) -> c_int;
    fn pam_strerror(pamh: *mut PamHandle, errnum: c_int) -> *const c_char;
}

/// The conversation's state for one attempt.
struct Conversation {
    /// Given to the first password prompt, then gone (wiped on drop).
    password: Option<Secret>,
    /// What PAM said to show (errors and information).
    said: Vec<String>,
}

/// PAM asks; we answer. Never unwinds into C.
unsafe extern "C" fn converse(
    num_msg: c_int,
    msg: *mut *const PamMessage,
    resp: *mut *mut PamResponse,
    appdata: *mut c_void,
) -> c_int {
    std::panic::catch_unwind(|| {
        // Safety: PAM passes the pointer we gave pam_start, alive for the whole transaction,
        // and the message array it owns, `num_msg` long.
        unsafe { converse_inner(num_msg, msg, resp, appdata.cast::<Conversation>()) }
    })
    .unwrap_or(PAM_CONV_ERR)
}

/// # Safety
/// `msg` points to `num_msg` message pointers, `resp` is writable, `conv` is live.
unsafe fn converse_inner(
    num_msg: c_int,
    msg: *mut *const PamMessage,
    resp: *mut *mut PamResponse,
    conv: *mut Conversation,
) -> c_int {
    if num_msg <= 0
        || num_msg > PAM_MAX_NUM_MSG
        || msg.is_null()
        || resp.is_null()
        || conv.is_null()
    {
        return PAM_CONV_ERR;
    }
    let n = num_msg as usize;
    // Safety: calloc for PAM to free; zeroed so unanswered entries are NULL.
    let replies =
        unsafe { libc::calloc(n, std::mem::size_of::<PamResponse>()) }.cast::<PamResponse>();
    if replies.is_null() {
        return PAM_BUF_ERR;
    }
    // Safety: the caller's guarantee.
    let conv = unsafe { &mut *conv };
    for i in 0..n {
        // Safety: Linux-PAM passes an array of `n` message pointers.
        let m = unsafe { &**msg.add(i) };
        let text = if m.msg.is_null() {
            String::new()
        } else {
            // Safety: a NUL-terminated string PAM owns.
            unsafe { CStr::from_ptr(m.msg) }
                .to_string_lossy()
                .into_owned()
        };
        match m.msg_style {
            PAM_PROMPT_ECHO_OFF | PAM_PROMPT_ECHO_ON => {
                let Some(password) = conv.password.take() else {
                    // Asked a second time: refuse rather than answer again.
                    // Safety: frees what was filled so far, then the array.
                    unsafe { free_replies(replies, n) };
                    return PAM_CONV_ERR;
                };
                let bytes = password.bytes();
                // Safety: room for the bytes and a NUL, zeroed.
                let p = unsafe { libc::calloc(bytes.len() + 1, 1) }.cast::<u8>();
                if p.is_null() {
                    // Safety: as above.
                    unsafe { free_replies(replies, n) };
                    return PAM_BUF_ERR;
                }
                // Safety: `p` has bytes.len() + 1 bytes; the last stays NUL.
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
                    (*replies.add(i)).resp = p.cast::<c_char>();
                }
                // `password` is wiped as it drops here.
            }
            PAM_ERROR_MSG | PAM_TEXT_INFO => {
                let t = text.trim();
                if !t.is_empty() {
                    conv.said.push(t.to_string());
                }
            }
            _ => {
                // Binary prompts and anything unknown: not something a lock screen answers.
                // Safety: as above.
                unsafe { free_replies(replies, n) };
                return PAM_CONV_ERR;
            }
        }
    }
    // Safety: `resp` is writable (checked non-null).
    unsafe { *resp = replies };
    PAM_SUCCESS
}

/// Frees a reply array, wiping any answer in it first.
///
/// # Safety
/// `replies` came from calloc with `n` entries, each `resp` NULL or from calloc.
unsafe fn free_replies(replies: *mut PamResponse, n: usize) {
    for i in 0..n {
        // Safety: within the array.
        let r = unsafe { &mut *replies.add(i) };
        if !r.resp.is_null() {
            // Safety: a NUL-terminated string we made.
            let len = unsafe { libc::strlen(r.resp) };
            // Safety: wipes then frees our own allocation.
            unsafe {
                std::ptr::write_bytes(r.resp, 0, len);
                libc::free(r.resp.cast());
            }
            r.resp = std::ptr::null_mut();
        }
    }
    // Safety: the array itself.
    unsafe { libc::free(replies.cast()) };
}

/// Checks `password` for `user` with PAM. Blocks: call it off the UI thread.
pub fn authenticate(user: &str, password: Secret) -> Verdict {
    let (Ok(service), Ok(user)) = (CString::new(SERVICE), CString::new(user)) else {
        return Verdict::Failed("Bad user name".into());
    };
    let mut conversation = Box::new(Conversation {
        password: Some(password),
        said: Vec::new(),
    });
    let conv = PamConv {
        conv: converse,
        appdata_ptr: (&mut *conversation as *mut Conversation).cast(),
    };
    let mut handle: *mut PamHandle = std::ptr::null_mut();
    // Safety: valid C strings; `conv` and `conversation` outlive the transaction (pam_end
    // below, before either drops).
    let started = unsafe { pam_start(service.as_ptr(), user.as_ptr(), &conv, &mut handle) };
    if started != PAM_SUCCESS || handle.is_null() {
        return Verdict::Failed(format!("PAM could not start ({started})"));
    }
    // Safety: a live handle.
    let status = unsafe { pam_authenticate(handle, 0) };
    if status == PAM_SUCCESS {
        // Safety: as above. Its result does not decide anything (swaylock ignores it too).
        unsafe { pam_setcred(handle, PAM_REFRESH_CRED) };
    }
    // Safety: a live handle.
    let why = unsafe { pam_strerror(handle, status) };
    let why = if why.is_null() {
        format!("PAM error {status}")
    } else {
        // Safety: a static string from Linux-PAM.
        unsafe { CStr::from_ptr(why) }
            .to_string_lossy()
            .into_owned()
    };
    // Safety: ends the transaction; the handle is not used after.
    unsafe { pam_end(handle, status) };
    // The password is gone (given to PAM) or wiped here if PAM never asked.
    let Conversation { said, .. } = *conversation;
    verdict(status, said, why)
}

/// The meaning of PAM's answer.
fn verdict(status: c_int, said: Vec<String>, why: String) -> Verdict {
    match status {
        PAM_SUCCESS => Verdict::Accepted(Accepted(())),
        PAM_AUTH_ERR | PAM_PERM_DENIED | PAM_USER_UNKNOWN | PAM_MAXTRIES => Verdict::Rejected(said),
        _ => Verdict::Failed(said.into_iter().next().unwrap_or(why)),
    }
}

/// The login name of the user this process runs as, and their full name if the account has
/// one. From the password database (`getpwuid_r`), not the environment.
pub fn current_user() -> Option<(String, Option<String>)> {
    // Safety: plain libc calls with a buffer we own and size.
    unsafe {
        let uid = libc::getuid();
        let mut buf = vec![0 as c_char; 4096];
        let mut pw: libc::passwd = std::mem::zeroed();
        let mut out: *mut libc::passwd = std::ptr::null_mut();
        let rc = libc::getpwuid_r(uid, &mut pw, buf.as_mut_ptr(), buf.len(), &mut out);
        if rc != 0 || out.is_null() || pw.pw_name.is_null() {
            return None;
        }
        let name = CStr::from_ptr(pw.pw_name).to_string_lossy().into_owned();
        let full = if pw.pw_gecos.is_null() {
            None
        } else {
            let g = CStr::from_ptr(pw.pw_gecos).to_string_lossy();
            // GECOS: "Full Name,room,phone,...".
            g.split(',')
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
        };
        Some((name, full))
    }
}

/// devtools builds only: a stand-in for PAM that accepts "kestrel" and nothing else, after
/// the same short wait, so the lock screen can be tried in a test compositor without typing a
/// real password (and without counting failures against the real account).
#[cfg(feature = "devtools")]
pub fn preview(password: Secret) -> Verdict {
    std::thread::sleep(std::time::Duration::from_millis(600));
    if password.bytes() == b"kestrel" {
        Verdict::Accepted(Accepted(()))
    } else {
        Verdict::Rejected(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(s: &str) -> Secret {
        let mut p = Secret::new();
        for c in s.chars() {
            p.push(c);
        }
        p
    }

    /// Runs the conversation function on made-up messages, as PAM would.
    fn run(
        password: Option<&str>,
        msgs: &[(c_int, &str)],
    ) -> (c_int, Vec<Option<Vec<u8>>>, Vec<String>) {
        let texts: Vec<CString> = msgs
            .iter()
            .map(|(_, t)| CString::new(*t).unwrap())
            .collect();
        let structs: Vec<PamMessage> = msgs
            .iter()
            .zip(&texts)
            .map(|((style, _), t)| PamMessage {
                msg_style: *style,
                msg: t.as_ptr(),
            })
            .collect();
        let mut ptrs: Vec<*const PamMessage> = structs.iter().map(|m| m as *const _).collect();
        let mut conv = Conversation {
            password: password.map(secret),
            said: Vec::new(),
        };
        let mut resp: *mut PamResponse = std::ptr::null_mut();
        // Safety: the same shapes PAM passes.
        let rc = unsafe {
            converse(
                msgs.len() as c_int,
                ptrs.as_mut_ptr(),
                &mut resp,
                (&mut conv as *mut Conversation).cast(),
            )
        };
        let mut answers = Vec::new();
        if rc == PAM_SUCCESS {
            for i in 0..msgs.len() {
                // Safety: the array converse made.
                let r = unsafe { &*resp.add(i) };
                answers.push(if r.resp.is_null() {
                    None
                } else {
                    // Safety: a NUL-terminated string converse made.
                    Some(unsafe { CStr::from_ptr(r.resp) }.to_bytes().to_vec())
                });
            }
            // Safety: frees as PAM would.
            unsafe { free_replies(resp, msgs.len()) };
        }
        (rc, answers, conv.said)
    }

    #[test]
    fn answers_the_password_prompt() {
        let (rc, answers, said) = run(Some("pa ss€"), &[(PAM_PROMPT_ECHO_OFF, "Password: ")]);
        assert_eq!(rc, PAM_SUCCESS);
        assert_eq!(answers, vec![Some("pa ss€".as_bytes().to_vec())]);
        assert!(said.is_empty());
    }

    #[test]
    fn answers_only_once() {
        // Two prompts in one call: the second is refused.
        let (rc, _, _) = run(
            Some("x"),
            &[
                (PAM_PROMPT_ECHO_OFF, "Password: "),
                (PAM_PROMPT_ECHO_OFF, "Again: "),
            ],
        );
        assert_eq!(rc, PAM_CONV_ERR);
        // No password left at all: refused.
        let (rc, _, _) = run(None, &[(PAM_PROMPT_ECHO_OFF, "Password: ")]);
        assert_eq!(rc, PAM_CONV_ERR);
    }

    #[test]
    fn keeps_what_pam_says() {
        let (rc, answers, said) = run(
            Some("x"),
            &[
                (
                    PAM_ERROR_MSG,
                    "The account is locked due to 3 failed logins.",
                ),
                (PAM_TEXT_INFO, "(9 minutes left to unlock)"),
            ],
        );
        assert_eq!(rc, PAM_SUCCESS);
        assert_eq!(answers, vec![None, None]);
        assert_eq!(
            said,
            vec![
                "The account is locked due to 3 failed logins.".to_string(),
                "(9 minutes left to unlock)".to_string()
            ]
        );
    }

    #[test]
    fn refuses_what_it_cannot_answer() {
        let (rc, _, _) = run(Some("x"), &[(5, "binary")]);
        assert_eq!(rc, PAM_CONV_ERR);
        let (rc, _, _) = run(Some("x"), &[]);
        assert_eq!(rc, PAM_CONV_ERR);
    }

    #[test]
    fn reads_pam_results() {
        assert!(matches!(
            verdict(PAM_SUCCESS, vec![], String::new()),
            Verdict::Accepted(_)
        ));
        assert!(matches!(
            verdict(PAM_AUTH_ERR, vec!["locked".into()], String::new()),
            Verdict::Rejected(s) if s == ["locked"]
        ));
        assert!(matches!(
            verdict(PAM_MAXTRIES, vec![], String::new()),
            Verdict::Rejected(_)
        ));
        assert!(matches!(
            verdict(PAM_CONV_ERR, vec![], "Conversation error".into()),
            Verdict::Failed(s) if s == "Conversation error"
        ));
    }

    #[test]
    fn finds_the_current_user() {
        let (name, _) = current_user().expect("this process's user");
        assert!(!name.is_empty());
    }
}
