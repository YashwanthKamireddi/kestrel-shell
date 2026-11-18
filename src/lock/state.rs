//! What the lock screen knows and does, apart from drawing and from PAM: typed input, the
//! attempt in progress, the message to show and whether it may unlock. Pure, so every path is
//! tested (tests below); the process (`super`) carries out the `Effect`s it returns.
//!
//! The one rule that matters: `Effect::Unlock` is returned only with PAM's `Accepted` proof,
//! and only once the compositor has said the session is locked (unlocking earlier is a
//! protocol error, and leaving before `locked` leaves the session locked for good).

use super::pam::{Accepted, Verdict};
use super::secret::Secret;

/// A key, as the lock screen understands it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// Text typed (one key: a character, or a few from a compose sequence).
    Text(Chars),
    Backspace,
    /// Enter: check the password.
    Submit,
    /// Esc (or Ctrl+U, as swaylock): clear the field; while checking, give up on that attempt.
    Clear,
}

/// Up to four characters, inline (never a heap string that would outlive the key unwiped).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Chars {
    buf: [char; 4],
    len: u8,
}

impl Chars {
    pub fn new(s: &str) -> Option<Self> {
        let mut c = Chars {
            buf: ['\0'; 4],
            len: 0,
        };
        for ch in s.chars() {
            if usize::from(c.len) == c.buf.len() {
                return None;
            }
            c.buf[usize::from(c.len)] = ch;
            c.len += 1;
        }
        (c.len > 0).then_some(c)
    }

    fn iter(&self) -> impl Iterator<Item = char> + '_ {
        self.buf[..usize::from(self.len)].iter().copied()
    }
}

/// Keys never show what was typed, even in a debug print.
impl std::fmt::Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Input::Text(_) => f.write_str("Text(..)"),
            Input::Backspace => f.write_str("Backspace"),
            Input::Submit => f.write_str("Submit"),
            Input::Clear => f.write_str("Clear"),
        }
    }
}

/// Where the session lock stands with the compositor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session {
    /// Asked for; the compositor has not confirmed yet.
    Asked,
    Locked,
}

/// What is shown under the field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    None,
    Checking,
    /// The password was wrong; PAM's own words when it gave any (a locked account).
    Wrong(Option<String>),
    /// PAM could not check (its message).
    Failed(String),
}

/// What the process must do next.
#[derive(Debug)]
pub enum Effect {
    None,
    /// Check this password (attempt number to match the answer).
    Check(u64, Secret),
    /// Unlock the session and end.
    Unlock(Accepted),
}

#[derive(Debug)]
pub struct Lock {
    pub input: Secret,
    pub status: Status,
    pub caps_lock: bool,
    session: Session,
    /// The attempt being checked, if any.
    checking: Option<u64>,
    attempts: u64,
    /// Accepted before the compositor confirmed the lock: unlock as soon as it does.
    accepted: Option<Accepted>,
}

impl Default for Lock {
    fn default() -> Self {
        Self {
            input: Secret::new(),
            status: Status::None,
            caps_lock: false,
            session: Session::Asked,
            checking: None,
            attempts: 0,
            accepted: None,
        }
    }
}

impl Lock {
    pub fn session(&self) -> Session {
        self.session
    }

    pub fn is_checking(&self) -> bool {
        self.checking.is_some()
    }

    pub fn key(&mut self, input: Input) -> Effect {
        if self.accepted.is_some() {
            return Effect::None;
        }
        if self.checking.is_some() {
            // Typing waits for the answer; Esc gives up on it (a PAM module that hangs can
            // never leave the screen unusable). Its answer is then ignored.
            if input == Input::Clear {
                self.checking = None;
                self.status = Status::None;
            }
            return Effect::None;
        }
        match input {
            Input::Text(chars) => {
                for c in chars.iter() {
                    self.input.push(c);
                }
                self.status = Status::None;
                Effect::None
            }
            Input::Backspace => {
                self.input.pop();
                self.status = Status::None;
                Effect::None
            }
            Input::Clear => {
                self.input.clear();
                self.status = Status::None;
                Effect::None
            }
            Input::Submit => {
                if self.input.is_empty() {
                    return Effect::None;
                }
                self.attempts += 1;
                self.checking = Some(self.attempts);
                self.status = Status::Checking;
                Effect::Check(self.attempts, self.input.take())
            }
        }
    }

    /// PAM's answer to attempt `n`.
    pub fn verdict(&mut self, n: u64, v: Verdict) -> Effect {
        if self.checking != Some(n) {
            // An attempt given up on (Esc): its answer means nothing now.
            return Effect::None;
        }
        self.checking = None;
        match v {
            Verdict::Accepted(proof) => {
                self.status = Status::None;
                match self.session {
                    Session::Locked => Effect::Unlock(proof),
                    Session::Asked => {
                        self.accepted = Some(proof);
                        Effect::None
                    }
                }
            }
            Verdict::Rejected(said) => {
                self.status = Status::Wrong(if said.is_empty() {
                    None
                } else {
                    Some(said.join(" "))
                });
                Effect::None
            }
            Verdict::Failed(why) => {
                self.status = Status::Failed(why);
                Effect::None
            }
        }
    }

    /// The compositor confirmed the lock.
    pub fn locked(&mut self) -> Effect {
        self.session = Session::Locked;
        match self.accepted.take() {
            Some(proof) => Effect::Unlock(proof),
            None => Effect::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Input {
        Input::Text(Chars::new(s).unwrap())
    }

    fn typed(l: &mut Lock, s: &str) {
        for c in s.chars() {
            let mut b = [0u8; 4];
            l.key(text(c.encode_utf8(&mut b)));
        }
    }

    fn locked() -> Lock {
        let mut l = Lock::default();
        assert!(matches!(l.locked(), Effect::None));
        l
    }

    fn submit(l: &mut Lock) -> u64 {
        match l.key(Input::Submit) {
            Effect::Check(n, _) => n,
            e => panic!("expected a check, got {e:?}"),
        }
    }

    #[test]
    fn enter_checks_what_was_typed_and_empties_the_field() {
        let mut l = locked();
        typed(&mut l, "hunter2");
        match l.key(Input::Submit) {
            Effect::Check(1, p) => assert_eq!(p.bytes(), b"hunter2"),
            e => panic!("{e:?}"),
        }
        assert!(l.input.is_empty());
        assert_eq!(l.status, Status::Checking);
    }

    #[test]
    fn empty_enter_does_nothing() {
        let mut l = locked();
        assert!(matches!(l.key(Input::Submit), Effect::None));
        assert_eq!(l.status, Status::None);
    }

    #[test]
    fn esc_clears_and_backspace_deletes() {
        let mut l = locked();
        typed(&mut l, "abc");
        l.key(Input::Backspace);
        assert_eq!(l.input.bytes(), b"ab");
        l.key(Input::Clear);
        assert!(l.input.is_empty());
    }

    #[test]
    fn wrong_password_shows_why_then_retries() {
        let mut l = locked();
        typed(&mut l, "nope");
        let n = submit(&mut l);
        assert!(matches!(
            l.verdict(n, Verdict::Rejected(vec![])),
            Effect::None
        ));
        assert_eq!(l.status, Status::Wrong(None));
        // Typing again clears the message; a second attempt is a new number.
        typed(&mut l, "x");
        assert_eq!(l.status, Status::None);
        assert_eq!(submit(&mut l), n + 1);
    }

    #[test]
    fn pam_messages_are_shown() {
        let mut l = locked();
        typed(&mut l, "x");
        let n = submit(&mut l);
        l.verdict(
            n,
            Verdict::Rejected(vec![
                "The account is locked.".into(),
                "(9 minutes left)".into(),
            ]),
        );
        assert_eq!(
            l.status,
            Status::Wrong(Some("The account is locked. (9 minutes left)".into()))
        );
        typed(&mut l, "y");
        let n = submit(&mut l);
        l.verdict(n, Verdict::Failed("Module is unknown".into()));
        assert_eq!(l.status, Status::Failed("Module is unknown".into()));
    }

    #[test]
    fn right_password_unlocks() {
        let mut l = locked();
        typed(&mut l, "right");
        let n = submit(&mut l);
        assert!(matches!(
            l.verdict(n, Verdict::Accepted(Accepted::for_test())),
            Effect::Unlock(_)
        ));
    }

    #[test]
    fn never_unlocks_before_the_compositor_has_locked() {
        let mut l = Lock::default();
        typed(&mut l, "right");
        let n = submit(&mut l);
        assert!(matches!(
            l.verdict(n, Verdict::Accepted(Accepted::for_test())),
            Effect::None
        ));
        // Nothing typed afterwards can do anything.
        typed(&mut l, "more");
        assert!(matches!(l.key(Input::Submit), Effect::None));
        // The confirmation unlocks.
        assert!(matches!(l.locked(), Effect::Unlock(_)));
    }

    #[test]
    fn typing_waits_while_checking() {
        let mut l = locked();
        typed(&mut l, "a");
        submit(&mut l);
        typed(&mut l, "b");
        assert!(l.input.is_empty());
        assert!(matches!(l.key(Input::Submit), Effect::None));
    }

    #[test]
    fn esc_gives_up_on_a_check_and_its_answer_is_ignored() {
        let mut l = locked();
        typed(&mut l, "a");
        let n = submit(&mut l);
        l.key(Input::Clear);
        assert!(!l.is_checking());
        assert_eq!(l.status, Status::None);
        // A late "accepted" for the abandoned attempt does not unlock.
        assert!(matches!(
            l.verdict(n, Verdict::Accepted(Accepted::for_test())),
            Effect::None
        ));
        // A new attempt works as usual.
        typed(&mut l, "b");
        let m = submit(&mut l);
        assert_ne!(m, n);
        assert!(matches!(
            l.verdict(m, Verdict::Accepted(Accepted::for_test())),
            Effect::Unlock(_)
        ));
    }

    #[test]
    fn keys_never_print_what_was_typed() {
        assert_eq!(format!("{:?}", text("p")), "Text(..)");
        let mut l = locked();
        typed(&mut l, "secret");
        assert!(!format!("{l:?}").contains("secret"));
    }

    #[test]
    fn chars_hold_at_most_four() {
        assert!(Chars::new("").is_none());
        assert!(Chars::new("abcd").is_some());
        assert!(Chars::new("abcde").is_none());
    }
}
