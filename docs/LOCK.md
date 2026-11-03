# kestrel's lock screen (`kestrel lock`)

Status (2026-11-18): code and unit tests written. **Not tried on a compositor yet, not used on
the desktop.** hyprlock stays the lock screen until it has passed the rig steps below and the user
has switched it himself.

## What it is

A process of its own (`src/lock/`), like the prompt process: the shell crashing, freezing or
restarting has no effect on it, and it shares nothing with the shell but the colour seed in
state.toml and the wallpaper's file (both read only).

| Part | File | Does |
|---|---|---|
| Process | `src/lock/mod.rs` | asks for the lock, one lock surface per output, keys, clock, look, unlock and exit |
| State | `src/lock/state.rs` | typed input, attempts, the message shown, when it may unlock. Pure, unit tested |
| Password | `src/lock/secret.rs` | one fixed 511-byte buffer, never reallocated, wiped on clear, submit and drop |
| PAM | `src/lock/pam.rs` | `pam_start` / `pam_authenticate` / `pam_setcred(REFRESH)` / `pam_end`, the conversation |
| Session lock | `vendor/iced_layershell` patch 26 | ext-session-lock-v1 on SCTK 0.20's `session_lock` (vendor/README.md) |

### How locking works (ext-session-lock-v1)

Protocol: <https://wayland.app/protocols/ext-session-lock-v1>. Implemented as swaylock and
hyprlock do.

1. At start it asks the compositor for the lock, before reading anything else (the wallpaper
   comes later, off the UI thread).
2. For every output (and every output plugged in later) it makes a lock surface; the compositor
   sizes it to the output. An output that goes away loses its surface.
3. The compositor sends `locked` once every output shows a lock frame. Hyprland
   (src/managers/SessionLockManager.cpp) also sends it after 5 s if a client is slow, showing
   its own blank "lockdead" frame on outputs with nothing drawn.
4. From then on only the lock surfaces get input, until this process sends
   `unlock_and_destroy`. **If the process crashes or is killed, the session stays locked**:
   Hyprland only clears its locked state on `unlock_and_destroy` (src/protocols/SessionLock.cpp).
5. If the compositor refuses (another lock screen holds the session; Hyprland does this unless
   `misc:allow_session_lock_restore` is on) it sends `finished`: `kestrel lock` exits with an
   error and changes nothing.

Rules kept in the code, each because breaking it is a lockout or a crash of the lock:

- Unlocking needs a `pam::Accepted`, which only a successful `pam_authenticate` creates.
- Never unlock before `locked` (a protocol error). A password accepted earlier is held until
  `locked` arrives, then the session unlocks.
- Never exit before `locked`: leaving between the request and `locked` leaves Hyprland locked
  with nobody to unlock it.
- A lock surface is never committed without a buffer (Hyprland drops the client for that).
- The unlock is flushed to the compositor before the process ends.
- One `kestrel lock` per display: a second one finds the first's `flock` on
  `$XDG_RUNTIME_DIR/kestrel-lock-<display>.lock` and exits at once. The kernel drops the
  `flock` when the process dies, so a crash never leaves a stale lock behind.

### Input

Type the password; Enter checks it; Backspace deletes a character; Esc (or Ctrl+U, as swaylock)
clears the field. A wrong password shows "Wrong password", plus PAM's own words when it says
any (pam_faillock: "The account is locked due to 3 failed logins", "(9 minutes left to
unlock)"). The message stays until the next key, then you can try again. While a check runs,
typing waits; **Esc gives up on that check** (its answer is then ignored), so a PAM module
that never answers can never leave the screen unusable. A Caps Lock line shows while Caps Lock
is on (the compositor's own modifier state, vendor patch 26).

### Look

The wallpaper (the shell's screen-sized cached copy, else the file itself), scaled down to
480 px wide, Gaussian-blurred and drawn stretched over the screen, under `base` at 45 %. With
no wallpaper: plain `base`. A large clock (`text::HERO`, the bar's 24h/12h setting), the date,
the user's full name (from the password database, else the login name), the password field
(a `raised` pill with a dot per character) and two status lines of fixed height, so nothing
moves. Colours are the shell's scheme. Until the wallpaper is ready (a few tens of ms on a
cache hit) the first frame is plain `base`: the lock never waits for a file.

### Never blocking, no network

- PAM runs on a blocking thread (`tokio::task::spawn_blocking`): pam_unix waits about 2 s after
  a wrong password (its fail delay).
- The wallpaper is read, scaled and blurred on another blocking thread.
- The clock ticks once a second; a surface is redrawn only when something visible changed
  (`redraw_key`: once a minute when idle).
- Before locking it reads two small files (config, state.toml) and the password database
  entry, the same as the prompt process at start.
- No network, no D-Bus: nothing but the compositor, PAM and those files. (PAM's modules are the
  system's own.)

### The password in memory

- Kept in `Secret`: a buffer whose 511 bytes (Linux-PAM's `PAM_MAX_RESP_SIZE` less the NUL)
  are reserved once, so it is never reallocated (a reallocation would leave a copy behind).
  Deleting a character wipes its bytes; Enter moves the password into the check and wipes the
  field; the check's copy is wiped when it drops (`zeroize`).
- PAM gets one `calloc`'d copy, which Linux-PAM overwrites before freeing (`_pam_drop_reply`).
  The conversation answers the first password prompt once; any later prompt is refused, so a
  module that asks again cannot loop (swaylock guards the same way for pam_systemd_home).
- Never logged: `Secret`, the key messages and the lock state all print as `..`.
- Not covered (known): the compositor and libxkbcommon see every key; iced's key event
  carries the character briefly (a small inline string, not wiped); the process is not
  `mlock`ed, so a page could be swapped out.

## PAM service

`kestrel lock` uses the existing **`login`** service. No file is created or needed.

- hyprlock's own `/etc/pam.d/hyprlock` (from its package) is the single line
  `auth include login`; swaylock's is the same. So `login` is exactly their check.
- A service name with no file in /etc/pam.d falls back to `/etc/pam.d/other`, which on Arch is
  `pam_deny`: **a lock that can never be unlocked**. That is why it is not a new name.
- Calls: `pam_authenticate`, then on success `pam_setcred(PAM_REFRESH_CRED)` (as swaylock). No
  `pam_acct_mgmt` (neither swaylock nor hyprlock calls it), no session.
- pam_faillock (in Arch's system-auth), with its default faillock.conf: after 3 wrong passwords in 15 minutes the account is refused
  for 10 minutes, **everywhere** (sudo, TTY login too). PAM's message is shown. This is the same
  for hyprlock today. From root, `faillock --user <name> --reset` clears it.

A dedicated service, if the user ever wants one (say, to add pam_fprintd only for the lock):
create `/etc/pam.d/kestrel-lock` containing

```
auth include login
```

(root-owned, 0644), then change `pam::SERVICE` to `"kestrel-lock"` in the same commit. The
file must exist before a build using that name is installed. Not done: it is a change to
/etc/pam.d, design call.

## Testing it safely (nested rig only)

Everything below locks **only the nested test compositor**. Never run `kestrel lock` on the
desktop until all of it passes. If anything goes wrong, `tools/rig/rig.sh stop` ends the
nested compositor and the lock with it; the user's desktop is never locked.

1. Build with the test tools: `cargo build --release --features devtools`. In this build the
   rig's lock takes the password **`kestrel`** instead of PAM (`KESTREL_PREVIEW=lock`), so no
   real password is typed and no failure counts against the account.
2. `tools/rig/rig.sh start` (if not up), then `tools/rig/rig.sh lock`. It refuses if there is
   no nested display, or if the display it found is the desktop's. Log: `~/.cache/kestrel-rig/lock.log`
   (expect "session locked").
3. Look: `tools/rig/rig.sh shot /tmp/lock.png`. The rig's HOME has no wallpaper set, so the
   background is plain `base`; set `wallpaper = "…"` in `~/.cache/kestrel-rig/home/.config/kestrel/config.toml` to see the blur.
4. Keys (the rig's virtual keyboard; if Hyprland does not pass virtual keys to a lock surface,
   show the rig with `hyprctl dispatch togglespecialworkspace rig` and type on the
   keyboard into it):
   - `rig.sh type abc`, shot: three dots. `rig.sh type -k BackSpace`: two. `-k Escape`: none.
   - `rig.sh type wrong`, `-k Return`: "Checking…", then "Wrong password" (red edge). Type
     again: the message goes.
   - `-k Caps_Lock`: the Caps Lock line appears; again: it goes.
   - `rig.sh type kestrel`, `-k Return`: the lock ends, the process exits (log: "unlocked"),
     the rig's desktop is back.
5. Crash while locked: `rig.sh lock`, then `kill -9 $(cat ~/.cache/kestrel-rig/lock.pid)`. The
   rig must stay locked (Hyprland's blank lockdead screen). Then `rig.sh lock` again: the rig's
   config has `allow_session_lock_restore = true`, so the new lock takes over; unlock with
   `kestrel`.
6. A second lock: with one running, `rig.sh lock` again: the second exits at once ("a lock
   screen is already running"); the first keeps working.
7. Outputs: while locked, `rig.sh hypr output create headless KRIG2`: a lock surface appears on
   it (shot both). `rig.sh hypr output remove KRIG2`: no error in lock.log.
8. Timing: `KESTREL_TRACE=1 tools/rig/rig.sh lock`: the trace shows "session lock requested",
   the first "present" of each lock surface and "session locked"; time between them.
9. Real PAM, once the above passes, and only then: `KESTREL_LOCK_PAM=1 tools/rig/rig.sh lock`
   (or a build without devtools). **Every wrong password counts towards pam_faillock's 3**
   (a 10-minute lockout of the user's account everywhere), so type it right, once.

## How hypridle would start it

Not changed: hypridle's config is the user's. When the user decides to switch, in
`~/.config/hypr/hypridle.conf`:

```
general {
    lock_cmd = kestrel lock                 # was: pidof hyprlock || hyprlock
    before_sleep_cmd = loginctl lock-session
    after_sleep_cmd = hyprctl dispatch dpms on
}
```

- `loginctl lock-session` (what the power menu's Lock and the `before_sleep_cmd` do) makes
  logind emit `Lock`; hypridle answers by running `lock_cmd`. No `pidof` check is needed:
  `kestrel lock` is single-instance by itself (the `flock` above), and `pidof kestrel` would
  match the shell.
- Before sleep, hypridle holds logind's sleep delay inhibitor while `before_sleep_cmd` runs;
  the lock asks for the session lock first thing, before reading any file.
- Before relying on it, set `misc:allow_session_lock_restore = true` in the user's Hyprland config
  (the Lua config's `misc` table), so that if `kestrel lock` ever crashes, a new lock screen
  (kestrel's or hyprlock) can be started from a TTY to take over: Ctrl+Alt+F3, log in, then
  `WAYLAND_DISPLAY=wayland-1 XDG_RUNTIME_DIR=/run/user/$(id -u) hyprlock` (the display name as
  `ls /run/user/$(id -u)` shows). Without that option the only way out of a dead lock is
  ending the Hyprland session. Keep hyprlock installed as that recovery.

## Not done / open

- Never run on a compositor yet: everything above "Testing" is code and unit tests only.
- No avatar (optional; would read `~/.face` or AccountsService's icon).
- No fingerprint (pam_fprintd would need its own PAM service and a conversation that waits for
  a finger while also taking a password, as hyprlock's fingerprint path does).
- No `mlock` of the password page.
- The clock ticks every second (redraws only once a minute); a timer aligned to the minute
  would wake less, but monotonic timers stop during suspend, so it would show the old time
  after resume.
- The greeter (replacing ly, Phase 14's other half) is not started.
