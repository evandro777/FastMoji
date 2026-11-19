//! X11 integration: global hotkey (XGrabKey), paste (XTEST Ctrl+V) and focus hand-over to the WM.
use std::{
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{
            Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask,
            GetKeyboardMappingReply, GrabMode, InputFocus, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, ModMask, PropMode,
            SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, SelectionRequestEvent, WindowClass,
        },
        xtest::ConnectionExt as _,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const XK_SHIFT_L: u32 = 0xffe1;
const XK_CONTROL_L: u32 = 0xffe3;
const XK_V: u32 = 0x76;

/// True when the picker runs against an X server (and so can grab keys and inject input).
/// A Wayland socket in the environment means the toolkit would pick Wayland, even if the session
/// type variable did not make it into a sandbox.
pub fn is_x11_session() -> bool {
    let wayland = std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland")
        || std::env::var_os("WAYLAND_DISPLAY").is_some();
    !wayland && std::env::var_os("DISPLAY").is_some()
}

/// Parse "Ctrl+Alt+E" style shortcuts into (modifiers, keysym).
pub fn parse_shortcut(text: &str) -> Option<(ModMask, u32)> {
    let mut mods = ModMask::from(0u16);
    let mut key = None;
    for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "primary" => mods |= ModMask::CONTROL,
            "alt" => mods |= ModMask::M1,
            "shift" => mods |= ModMask::SHIFT,
            "super" | "meta" => mods |= ModMask::M4,
            "period" => key = Some(0x2e),
            "comma" => key = Some(0x2c),
            "space" => key = Some(0x20),
            "minus" => key = Some(0x2d),
            "tab" => key = Some(0xff09),
            "return" | "enter" => key = Some(0xff0d),
            f if f
                .strip_prefix('f')
                .and_then(|n| n.parse::<u32>().ok())
                .is_some_and(|n| (1..=12).contains(&n)) =>
            {
                key = Some(0xffbe + f[1..].parse::<u32>().unwrap() - 1);
            }
            other => {
                let mut chars = other.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if c.is_ascii_graphic() => key = Some(c as u32),
                    _ => return None,
                }
            }
        }
    }
    let key = key?;
    (mods != ModMask::from(0u16)).then_some((mods, key))
}

/// The whole keyboard map and the first keycode it covers.
fn keyboard_map(conn: &impl Connection) -> Result<(u8, GetKeyboardMappingReply)> {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    Ok((min, conn.get_keyboard_mapping(min, max - min + 1)?.reply()?))
}

/// Keycodes for `keysyms`, from one fetch of the keyboard map.
fn keycodes(conn: &impl Connection, keysyms: &[u32]) -> Result<Vec<u8>> {
    let (min, map) = keyboard_map(conn)?;
    let per = map.keysyms_per_keycode as usize;
    let chunks = || map.keysyms.chunks(per);
    keysyms
        .iter()
        .map(|keysym| {
            // Prefer a key that produces the symbol unshifted in the first layout group.
            chunks()
                .position(|c| c.first() == Some(keysym))
                .or_else(|| chunks().position(|c| c.contains(keysym)))
                .map(|i| min + i as u8)
                .ok_or_else(|| format!("no key for keysym {keysym:#x}").into())
        })
        .collect()
}

/// The unshifted character on the physical key that produces `c` in the current layout (`{` -> `7` on layouts
/// where AltGr+7 gives `{`; Ctrl+Alt is AltGr there). Lets the recorder name keys independently of Ctrl/Alt/Shift.
pub fn base_key(c: char) -> Option<char> {
    let (conn, _) = x11rb::connect(None).ok()?;
    let (_, map) = keyboard_map(&conn).ok()?;
    let per = map.keysyms_per_keycode as usize;
    let keysym = c as u32;
    let first = map.keysyms.chunks(per).find(|k| k.contains(&keysym))?[0];
    char::from_u32(first).filter(char::is_ascii_graphic)
}

/// Presses closer together than this are key auto-repeat (the shortcut held down), not new presses.
const REPEAT_MS: u32 = 100;

/// True when a press at `time` (server ms, wraps) is auto-repeat: X sends each repeat as a release and a press with
/// the same timestamp (the first one only after the ~500 ms repeat delay), so that is the main test; pressing again
/// within `REPEAT_MS` of the last press is the fallback.
fn is_repeat(last_press: Option<u32>, last_release: Option<u32>, time: u32) -> bool {
    last_release == Some(time) || last_press.is_some_and(|t| time.wrapping_sub(t) < REPEAT_MS)
}

/// Ask the server to stop sending release+press pairs while a key is held (XKB "detectable auto-repeat").
/// Applies to this connection only. False when XKB is unavailable, and `is_repeat` is used instead.
fn detectable_auto_repeat(conn: &RustConnection) -> bool {
    use x11rb::protocol::xkb::{ConnectionExt as _, ID, PerClientFlag};
    let flag = PerClientFlag::DETECTABLE_AUTO_REPEAT;
    let enable = || -> Result<bool> {
        conn.xkb_use_extension(1, 0)?.reply()?;
        let reply = conn
            .xkb_per_client_flags(
                ID::USE_CORE_KBD.into(),
                flag,
                flag,
                0u32.into(),
                0u32.into(),
                0u32.into(),
            )?
            .reply()?;
        Ok(reply.value & flag != PerClientFlag::from(0u32))
    };
    enable().unwrap_or(false)
}

/// Why a shortcut could not be grabbed; `app.rs` turns it into translated text.
pub enum HotkeyError {
    /// Another client holds the combination.
    Taken,
    Other(String),
}

/// A grabbed shortcut. `release` frees the key combination and stops the listener thread.
pub struct Hotkey {
    conn: Arc<RustConnection>,
    root: u32,
    /// Our own window: an event sent to it with an empty mask reaches this client, waking the listener.
    wake: u32,
    /// Wall-clock ms until which the shortcut counts as in use: `u64::MAX` while the key is down, and a short
    /// grace after its release. Releasing a grabbed key ends the X grab, which bounces the keyboard focus.
    busy_until: Arc<AtomicU64>,
}

/// Focus bounce after the grab ends is over by then.
const RELEASE_GRACE_MS: u64 = 400;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

impl Hotkey {
    /// True while the shortcut is held, and just after it was let go.
    pub fn in_use(&self) -> bool {
        now_ms() < self.busy_until.load(Ordering::SeqCst)
    }

    pub fn release(&self) {
        let _ = self.conn.ungrab_key(0, self.root, ModMask::ANY);
        let quit = ClientMessageEvent::new(32, self.wake, AtomEnum::CARDINAL, [0; 5]);
        let _ = self.conn.send_event(false, self.wake, EventMask::NO_EVENT, quit);
        let _ = self.conn.flush();
    }
}

/// Grab the shortcut on its own thread (blocking on the X socket, no polling) and call
/// `on_press` with the event timestamp. Fails with `HotkeyError::Taken` if the combination is in use.
pub fn spawn_hotkey(
    shortcut: &str,
    on_press: impl Fn(u32) + Send + 'static,
) -> std::result::Result<Hotkey, HotkeyError> {
    let other = |e: &dyn std::fmt::Display| HotkeyError::Other(e.to_string());
    let (mods, keysym) =
        parse_shortcut(shortcut).ok_or_else(|| HotkeyError::Other(format!("cannot parse shortcut {shortcut:?}")))?;
    let (conn, screen) = x11rb::connect(None).map_err(|e| other(&e))?;
    let root = conn.setup().roots[screen].root;
    let code = keycodes(&conn, &[keysym]).map_err(|e| other(&*e))?[0];
    // Num Lock and Caps Lock must not defeat the shortcut.
    for extra in [
        ModMask::from(0u16),
        ModMask::LOCK,
        ModMask::M2,
        ModMask::LOCK | ModMask::M2,
    ] {
        let cookie = conn
            .grab_key(true, root, mods | extra, code, GrabMode::ASYNC, GrabMode::ASYNC)
            .map_err(|e| other(&e))?;
        cookie.check().map_err(|_| HotkeyError::Taken)?;
    }
    let wake = conn.generate_id().map_err(|e| other(&e))?;
    conn.create_window(
        0,
        wake,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new(),
    )
    .map_err(|e| other(&e))?;
    conn.flush().map_err(|e| other(&e))?;
    let detectable = detectable_auto_repeat(&conn);
    let conn = Arc::new(conn);
    let listener = conn.clone();
    let busy_until = Arc::new(AtomicU64::new(0));
    let busy = busy_until.clone();
    std::thread::spawn(move || {
        let debug = std::env::var_os("FASTMOJI_DEBUG").is_some();
        let (mut last, mut released, mut down) = (None::<u32>, None::<u32>, false);
        while let Ok(event) = listener.wait_for_event() {
            match event {
                Event::KeyPress(ev) => {
                    // Detectable: a held key sends one press and no releases, so "already down" is a repeat.
                    let repeat = if detectable {
                        down
                    } else {
                        is_repeat(last, released, ev.time)
                    };
                    if debug {
                        eprintln!("hotkey press time={} detectable={detectable} repeat={repeat}", ev.time);
                    }
                    busy.store(u64::MAX, Ordering::SeqCst);
                    if !repeat {
                        on_press(ev.time);
                    }
                    (last, down) = (Some(ev.time), true);
                }
                Event::KeyRelease(ev) => {
                    if debug {
                        eprintln!("hotkey release time={}", ev.time);
                    }
                    busy.store(now_ms() + RELEASE_GRACE_MS, Ordering::SeqCst);
                    (released, down) = (Some(ev.time), false);
                }
                Event::ClientMessage(_) => break,
                _ => {}
            }
        }
    });
    Ok(Hotkey {
        conn,
        root,
        wake,
        busy_until,
    })
}

/// Terminals that paste with Ctrl+Shift+V (plain Ctrl+V reaches the shell as a control character).
const TERMINAL_CLASSES: [&str; 13] = [
    "tilix",
    "ghostty",
    "st-256color",
    "term",
    "konsole",
    "kitty",
    "alacritty",
    "wezterm",
    "foot",
    "ptyxis",
    "guake",
    "yakuake",
    "sakura",
];

/// Does a `WM_CLASS` value (instance and class, any case) belong to a terminal emulator?
fn is_terminal_class(class: &str) -> bool {
    let class = class.to_ascii_lowercase();
    TERMINAL_CLASSES.iter().any(|t| class.contains(t))
}

/// One long-lived connection for what the UI thread asks of the X server (focus queries, paste).
/// Connecting per call cost a handshake each, up to 80 of them during a paste's one second wait.
pub struct Session {
    conn: RustConnection,
    root: u32,
    active: Atom,
    user_time: Atom,
}

impl Session {
    pub fn new() -> Result<Self> {
        let (conn, screen) = x11rb::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let (active, user_time) = (
            conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?,
            conn.intern_atom(false, b"_NET_WM_USER_TIME")?,
        );
        let (active, user_time) = (active.reply()?.atom, user_time.reply()?.atom);
        Ok(Self {
            conn,
            root,
            active,
            user_time,
        })
    }

    /// The window manager's currently active top-level window.
    pub fn active_window(&self) -> Result<Option<u32>> {
        let reply = self
            .conn
            .get_property(false, self.root, self.active, AtomEnum::WINDOW, 0, 1)?
            .reply()?;
        Ok(reply.value32().and_then(|mut v| v.next()).filter(|w| *w != 0))
    }

    /// True when the X input focus is `window` or one of its descendants. `_NET_ACTIVE_WINDOW` can
    /// change slightly before the real keyboard focus does, and a key sent in between is lost.
    pub fn input_focus_inside(&self, window: u32) -> Result<bool> {
        let mut current = self.conn.get_input_focus()?.reply()?.focus;
        for _ in 0..16 {
            if current == window {
                return Ok(true);
            }
            if current == self.root || current == 0 {
                break;
            }
            current = self.conn.query_tree(current)?.reply()?.parent;
        }
        Ok(false)
    }

    /// Ask the WM to focus `window` the way a pager would (not subject to focus-stealing prevention).
    pub fn focus(&self, window: u32) -> Result<()> {
        self.request_active(window, [2, 0, 0, 0, 0])
    }

    fn request_active(&self, window: u32, data: [u32; 5]) -> Result<()> {
        let message = ClientMessageEvent::new(32, window, self.active, data);
        self.conn.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            message,
        )?;
        self.conn.flush()?;
        Ok(())
    }

    /// True when `window` is a terminal emulator, judged by its `WM_CLASS`.
    pub fn is_terminal(&self, window: u32) -> bool {
        let check = || -> Result<bool> {
            let reply = self
                .conn
                .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)?
                .reply()?;
            Ok(is_terminal_class(&String::from_utf8_lossy(&reply.value)))
        };
        check().unwrap_or(false)
    }

    /// Send Ctrl+V (Ctrl+Shift+V when `terminal`) to whatever window has focus.
    pub fn paste(&self, terminal: bool) -> Result<()> {
        // Looked up per paste, not cached: the user may change keyboard layout while we run.
        let codes = keycodes(&self.conn, &[XK_CONTROL_L, XK_SHIFT_L, XK_V])?;
        let (ctrl, shift, v) = (codes[0], codes[1], codes[2]);
        let mut keys = vec![(KEY_PRESS_EVENT, ctrl)];
        if terminal {
            keys.push((KEY_PRESS_EVENT, shift));
        }
        keys.push((KEY_PRESS_EVENT, v));
        keys.push((KEY_RELEASE_EVENT, v));
        if terminal {
            keys.push((KEY_RELEASE_EVENT, shift));
        }
        keys.push((KEY_RELEASE_EVENT, ctrl));
        for (kind, code) in keys {
            self.conn.xtest_fake_input(kind, code, 0, self.root, 0, 0, 0)?;
            self.conn.flush()?;
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        // Round-trip so the server has processed everything before we move on.
        self.conn.get_input_focus()?.reply()?;
        Ok(())
    }

    /// Tells the WM where to map `window` (USPosition/PPosition in WM_NORMAL_HINTS). A WM places a new window
    /// itself unless the position is a hint set before mapping; moving it afterwards shows it at the WM's spot first.
    pub fn set_position_hint(&self, window: u32, x: i32, y: i32) {
        // WM_SIZE_HINTS is 18 CARD32; keep what the toolkit already put there.
        let mut hints: Vec<u32> = self
            .conn
            .get_property(false, window, AtomEnum::WM_NORMAL_HINTS, AtomEnum::WM_SIZE_HINTS, 0, 18)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(Iterator::collect))
            .unwrap_or_default();
        hints.resize(18, 0);
        hints[0] |= 1 | 4; // USPosition | PPosition
        hints[1] = x as u32;
        hints[2] = y as u32;
        let _ = self.conn.change_property32(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NORMAL_HINTS,
            AtomEnum::WM_SIZE_HINTS,
            &hints,
        );
        let _ = self.conn.flush();
    }

    /// The mouse pointer in root coordinates.
    pub fn pointer(&self) -> Option<(i32, i32)> {
        let p = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
        Some((i32::from(p.root_x), i32::from(p.root_y)))
    }

    /// Size of the whole screen, the bound to use before winit knows the monitors.
    pub fn screen_size(&self) -> Option<(i32, i32)> {
        let g = self.conn.get_geometry(self.root).ok()?.reply().ok()?;
        Some((i32::from(g.width), i32::from(g.height)))
    }

    /// Ask the window manager to focus `window`, using the hotkey's timestamp so Muffin does not treat
    /// the request as focus stealing.
    pub fn activate(&self, window: u32, time: u32) -> Result<()> {
        self.conn
            .change_property32(PropMode::REPLACE, window, self.user_time, AtomEnum::CARDINAL, &[time])?;
        self.conn.set_input_focus(InputFocus::PARENT, window, time)?;
        self.request_active(window, [1, time, 0, 0, 0])
    }
}

struct ClipboardAtoms {
    clipboard: Atom,
    targets: Atom,
    utf8: Atom,
    text: Atom,
    plain: Atom,
    plain_utf8: Atom,
}

/// Owns the CLIPBOARD selection and serves it from a background thread. Unlike a generic crate this
/// tells us exactly when another application has taken the clipboard over (`on_lost`), which is
/// what lets a non-resident FastMoji exit at the right moment without losing what was copied.
pub struct ClipboardOwner {
    conn: Arc<RustConnection>,
    window: u32,
    atoms: Arc<ClipboardAtoms>,
    text: Arc<Mutex<String>>,
    owned: Arc<AtomicBool>,
    /// How many times an application has fetched the actual text (not just the target list).
    reads: Arc<AtomicUsize>,
}

impl ClipboardOwner {
    pub fn new(on_lost: impl Fn() + Send + 'static) -> Result<Self> {
        let (conn, screen) = x11rb::connect(None)?;
        let conn = Arc::new(conn);
        let root = conn.setup().roots[screen].root;
        let window = conn.generate_id()?;
        conn.create_window(
            0,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )?;
        let atom = |name: &[u8]| -> Result<Atom> { Ok(conn.intern_atom(false, name)?.reply()?.atom) };
        let atoms = Arc::new(ClipboardAtoms {
            clipboard: atom(b"CLIPBOARD")?,
            targets: atom(b"TARGETS")?,
            utf8: atom(b"UTF8_STRING")?,
            text: atom(b"TEXT")?,
            plain: atom(b"text/plain")?,
            plain_utf8: atom(b"text/plain;charset=utf-8")?,
        });
        conn.flush()?;
        let owner = Self {
            conn: conn.clone(),
            window,
            atoms: atoms.clone(),
            text: Arc::new(Mutex::new(String::new())),
            owned: Arc::new(AtomicBool::new(false)),
            reads: Arc::new(AtomicUsize::new(0)),
        };
        let (text, owned, reads) = (owner.text.clone(), owner.owned.clone(), owner.reads.clone());
        std::thread::spawn(move || {
            while let Ok(event) = conn.wait_for_event() {
                match event {
                    Event::SelectionRequest(request) => {
                        let text = text.lock().map(|t| t.clone()).unwrap_or_default();
                        match serve(&conn, &atoms, &request, &text) {
                            Ok(true) => {
                                reads.fetch_add(1, Ordering::SeqCst);
                            }
                            Ok(false) => {}
                            Err(e) => eprintln!("clipboard request failed: {e}"),
                        }
                    }
                    Event::SelectionClear(_) => {
                        owned.store(false, Ordering::SeqCst);
                        on_lost();
                    }
                    _ => {}
                }
            }
        });
        Ok(owner)
    }

    pub fn set(&self, text: &str) -> Result<()> {
        *self.text.lock().map_err(|_| "clipboard lock poisoned")? = text.to_string();
        self.conn.set_selection_owner(self.window, self.atoms.clipboard, 0u32)?;
        self.conn.flush()?;
        let owner = self.conn.get_selection_owner(self.atoms.clipboard)?.reply()?.owner;
        self.owned.store(owner == self.window, Ordering::SeqCst);
        if owner == self.window {
            Ok(())
        } else {
            Err("could not take ownership of the clipboard".into())
        }
    }

    /// Number of times the text itself has been read by other applications so far.
    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }

    /// True while our content is still the clipboard.
    pub fn owns(&self) -> bool {
        self.owned.load(Ordering::SeqCst)
    }
}

/// The text as Latin-1 bytes, or None if any character (such as an emoji) cannot be expressed.
fn latin1(text: &str) -> Option<Vec<u8>> {
    text.chars().map(|c| u8::try_from(c as u32).ok()).collect()
}

/// Answer one request. Returns true when the clipboard text itself was delivered.
fn serve(conn: &RustConnection, atoms: &ClipboardAtoms, request: &SelectionRequestEvent, text: &str) -> Result<bool> {
    let property = if request.property == 0 {
        request.target
    } else {
        request.property
    };
    let target = request.target;
    let mut answered = true;
    let mut delivered_text = false;
    if request.selection != atoms.clipboard {
        answered = false;
    } else if target == atoms.targets {
        let list = [
            atoms.targets,
            atoms.utf8,
            atoms.plain_utf8,
            atoms.plain,
            atoms.text,
            AtomEnum::STRING.into(),
        ];
        conn.change_property32(PropMode::REPLACE, request.requestor, property, AtomEnum::ATOM, &list)?;
    } else if target == atoms.utf8 || target == atoms.plain_utf8 || target == atoms.plain || target == atoms.text {
        let kind = if target == atoms.plain_utf8 || target == atoms.plain {
            target
        } else {
            atoms.utf8
        };
        conn.change_property8(PropMode::REPLACE, request.requestor, property, kind, text.as_bytes())?;
        delivered_text = true;
    } else if target == Atom::from(AtomEnum::STRING) {
        // Legacy Latin-1 clients. Refusing (instead of sending '?') lets them ask for UTF8_STRING.
        match latin1(text) {
            Some(bytes) => {
                conn.change_property8(PropMode::REPLACE, request.requestor, property, AtomEnum::STRING, &bytes)?;
                delivered_text = true;
            }
            None => answered = false,
        }
    } else {
        answered = false;
    }
    let notify = SelectionNotifyEvent {
        response_type: SELECTION_NOTIFY_EVENT,
        sequence: 0,
        time: request.time,
        requestor: request.requestor,
        selection: request.selection,
        target,
        property: if answered { property } else { 0 },
    };
    conn.send_event(false, request.requestor, EventMask::NO_EVENT, notify)?;
    conn.flush()?;
    Ok(delivered_text && answered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_terminal_classes() {
        assert!(is_terminal_class("gnome-terminal-server\0Gnome-terminal\0"));
        assert!(is_terminal_class("xterm\0XTerm\0"));
        assert!(is_terminal_class("konsole\0konsole\0"));
        for class in [
            "tilix\0Tilix\0",
            "ghostty\0com.mitchellh.ghostty\0",
            "st-256color\0st-256color\0",
        ] {
            assert!(is_terminal_class(class), "{class:?}");
        }
        assert!(!is_terminal_class("xed\0Xed\0"));
        assert!(!is_terminal_class("firefox\0firefox\0"));
    }

    #[test]
    fn string_target_is_refused_for_non_latin1() {
        assert_eq!(latin1("café"), Some(vec![b'c', b'a', b'f', 0xe9]));
        assert_eq!(latin1("a😀"), None);
    }

    #[test]
    fn held_key_repeats_are_ignored() {
        assert!(!is_repeat(None, None, 5), "first press");
        assert!(is_repeat(Some(1000), None, 1030), "auto-repeat");
        assert!(
            !is_repeat(Some(1000), Some(1200), 1000 + REPEAT_MS),
            "a real second press"
        );
        assert!(is_repeat(Some(u32::MAX - 10), None, 20), "across the timestamp wrap");
        assert!(
            is_repeat(Some(1000), Some(1500), 1500),
            "first repeat after the ~500 ms delay"
        );
    }

    #[test]
    fn default_shortcut_parses() {
        let (mods, key) = parse_shortcut(&crate::settings::Settings::default().shortcut).unwrap();
        assert_eq!((mods, key), (ModMask::CONTROL | ModMask::M1, 0x2e));
    }

    #[test]
    fn parses_shortcuts() {
        let (mods, key) = parse_shortcut("Ctrl+Alt+E").unwrap();
        assert_eq!(mods, ModMask::CONTROL | ModMask::M1);
        assert_eq!(key, 'e' as u32);
        assert_eq!(parse_shortcut("Ctrl+Alt+period").unwrap().1, 0x2e);
        assert_eq!(parse_shortcut("Super+F2").unwrap(), (ModMask::M4, 0xffbf));
        assert_eq!(parse_shortcut("Ctrl+Alt+Tab").unwrap().1, 0xff09);
        assert_eq!(parse_shortcut("Ctrl+Return").unwrap().1, 0xff0d);
        assert!(parse_shortcut("Ctrl+F13").is_none());
        assert!(parse_shortcut("E").is_none(), "needs a modifier");
        assert!(parse_shortcut("Ctrl+Alt").is_none(), "needs a key");
    }
}
