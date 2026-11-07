//! The picker: state, layout, interaction and daemon lifecycle.
use std::{
    cell::{Cell as StdCell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

use slint::{
    ComponentHandle, Image, LogicalSize, Model, ModelNotify, ModelRc, ModelTracker, Timer, VecModel,
    winit_030::{
        EventResult, WinitWindowAccessor,
        winit::{
            self,
            raw_window_handle::{HasWindowHandle, RawWindowHandle},
        },
    },
};

use crate::{
    Category, Cell, Line, Main, Tr, emojis,
    glyphs::Glyphs,
    instance,
    settings::{self, Settings},
    x11,
};

/// Values of the language setting in the order of the Preferences list: system default, English, then the translations.
fn language_choices() -> impl Iterator<Item = &'static str> {
    ["", "en"].into_iter().chain(emojis::language_codes())
}

/// The language of the session from the usual POSIX variables ("pt_BR.UTF-8", "fr:en"...).
fn system_language() -> &'static str {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        .unwrap_or_default();
    emojis::resolve_language(locale.split(':').next().unwrap_or(""))
}

/// Grace period after showing during which the WM may bounce focus before we trust a focus-out.
const FOCUS_GRACE: Duration = Duration::from_millis(300);
/// How often to check whether the previous window has regained focus, and how long to wait at most.
const PASTE_POLL: Duration = Duration::from_millis(25);
const PASTE_MAX_POLLS: u32 = 40;
/// Longest a non-resident FastMoji waits for the pasted-into application to read the clipboard (25 ms polls).
const EXIT_WAIT_MAX_POLLS: u32 = 80;
/// Copy-only mode (not resident): ignore clipboard reads right after copying (clipboard managers).
const HOLD_IGNORE_READS: Duration = Duration::from_secs(1);
const HOLD_GRACE: Duration = Duration::from_millis(400);
/// How long to keep an unpasted copy alive before giving up and exiting.
const HOLD_MAX: Duration = Duration::from_secs(300);
const CELL_PADDING: f32 = 14.0;
/// Image sizes used in `ui/main.slint`; images are rendered at exactly these (times the scale).
const CATEGORY_PX: f32 = 20.0;
const PREVIEW_PX: f32 = 36.0;

/// "Ctrl+Alt+." from a key press, or None for a lone modifier or a key the shortcut parser does not know.
/// `mods` is [ctrl, alt, shift, meta]; at least one is required (a bare key would steal normal typing).
fn shortcut_from_key(text: &str, mods: [bool; 4]) -> Option<String> {
    let mut chars = text.chars();
    let c = chars.next().filter(|_| chars.next().is_none())?;
    let key = match c {
        ' ' => "space".to_string(),
        '\t' => "Tab".into(),
        '\r' | '\n' => "Return".into(),
        // Slint reports F1..F12 as private-use characters.
        '\u{f704}'..='\u{f70f}' => format!("F{}", c as u32 - 0xf704 + 1),
        c if c.is_ascii_alphanumeric() => c.to_ascii_uppercase().to_string(),
        '.' | ',' | '-' => c.to_string(),
        _ => return None,
    };
    let mut parts: Vec<&str> = MODIFIER_NAMES
        .iter()
        .zip(mods)
        .filter(|(_, on)| *on)
        .map(|(n, _)| *n)
        .collect();
    if parts.is_empty() {
        return None;
    }
    parts.push(&key);
    Some(parts.join("+"))
}

const MODIFIER_NAMES: [&str; 4] = ["Ctrl", "Alt", "Shift", "Super"];

/// Live label while recording: "Ctrl + Alt + …", or None when nothing is held (the caller shows the translated prompt).
fn held_text(mods: [bool; 4]) -> Option<String> {
    let held: Vec<&str> = MODIFIER_NAMES
        .iter()
        .zip(mods)
        .filter(|(_, on)| *on)
        .map(|(n, _)| *n)
        .collect();
    (!held.is_empty()).then(|| format!("{} + …", held.join(" + ")))
}

fn debug(message: impl FnOnce() -> String) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("FASTMOJI_DEBUG").is_some()) {
        eprintln!(
            "[{:?}] {}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                % 100000,
            message()
        );
    }
}

enum HoldStep {
    Stop,
    Rebaseline,
    MarkRead,
    Wait,
    Exit(&'static str),
}

/// One tick of the copy-only wait (see `App::hold_clipboard`), as a pure decision.
/// `read_elapsed` is the time since the first read after the ignore period, if any.
fn hold_step(
    visible: bool,
    owns: bool,
    since_copy: Duration,
    reads: usize,
    baseline: usize,
    read_elapsed: Option<Duration>,
) -> HoldStep {
    if visible {
        return HoldStep::Stop;
    }
    if !owns {
        return HoldStep::Exit("the clipboard was replaced");
    }
    if since_copy < HOLD_IGNORE_READS {
        return HoldStep::Rebaseline;
    }
    if reads > baseline {
        // Let the application finish inserting before we go.
        return match read_elapsed {
            None => HoldStep::MarkRead,
            Some(at) if at >= HOLD_GRACE => HoldStep::Exit("the clipboard was pasted"),
            Some(_) => HoldStep::Wait,
        };
    }
    if since_copy >= HOLD_MAX {
        return HoldStep::Exit("nobody pasted it in time");
    }
    HoldStep::Wait
}

enum PasteStep {
    Wait,
    /// The WM did not hand focus back by itself: ask for it like a pager would.
    RestoreFocus,
    Paste,
    /// Timed out and the previous window is not in front: keep the emoji on the clipboard only.
    Skip,
}

/// One tick of the wait before pasting (see `App::paste_tick`), as a pure decision. `attempts` counts this tick;
/// `focus_ok` says whether the X input focus is inside the given window (only asked once it matters).
fn paste_step(
    previous: Option<u32>,
    active: Option<u32>,
    focus_ok: impl FnOnce(u32) -> bool,
    attempts: u32,
) -> PasteStep {
    let ready = match previous {
        // Give the application a moment to react to getting focus back.
        Some(window) => active == Some(window) && attempts >= 3 && focus_ok(window),
        None => attempts >= 6,
    };
    if ready || attempts >= PASTE_MAX_POLLS {
        // On timeout the previous window may be gone or the user went elsewhere: do not send Ctrl+V to
        // an unrelated window.
        return if previous.is_none() || active == previous {
            PasteStep::Paste
        } else {
            PasteStep::Skip
        };
    }
    if attempts == 8 && previous.is_some() {
        return PasteStep::RestoreFocus;
    }
    PasteStep::Wait
}

enum ExitStep {
    Stop,
    MarkRead,
    Wait,
    Exit(&'static str),
}

/// One tick of the wait after a paste (see `App::exit_after_clipboard_read`). `polls` counts this tick;
/// `read_at` is the poll on which the clipboard text was first seen read.
fn exit_step(visible: bool, polls: u32, read_at: Option<u32>, read: bool) -> ExitStep {
    if visible {
        return ExitStep::Stop;
    }
    match read_at {
        // Once read, give the application a moment to finish inserting before we disappear.
        Some(at) if polls >= at + 4 => ExitStep::Exit("the clipboard was read"),
        Some(_) => ExitStep::Wait,
        None if read => ExitStep::MarkRead,
        None if polls >= EXIT_WAIT_MAX_POLLS => ExitStep::Exit("the clipboard was never read"),
        None => ExitStep::Wait,
    }
}

thread_local! {
    static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) };
}

/// Run `f` on the UI thread with the app (used by the hotkey and socket threads).
fn post(f: impl FnOnce(&Rc<App>) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        APP.with(|app| {
            if let Some(app) = app.borrow().as_ref() {
                f(app);
            }
        })
    });
}

/// A timer callback that runs `f` on the app (timers outlive any borrow, so they look it up each tick).
fn tick(f: fn(&App)) -> impl FnMut() + 'static {
    move || {
        APP.with(|app| {
            if let Some(app) = app.borrow().as_ref() {
                f(app);
            }
        })
    }
}

enum Item {
    Title(String),
    Cells { start: usize, len: usize },
}

struct State {
    /// Only to read the translated titles (`Tr`).
    ui: slint::Weak<Main>,
    settings: Settings,
    /// Trimmed search text; empty means the category catalogue is shown.
    query: String,
    /// Emoji index per display position.
    display: Vec<usize>,
    items: Vec<Item>,
    /// Item (row) of each display position.
    row_of: Vec<usize>,
    category_row: [usize; emojis::CATEGORIES.len()],
    selected: usize,
    columns: usize,
    active: usize,
    saved_scroll: Option<f32>,
    last_width: f32,
}

impl State {
    fn cell_px(&self) -> f32 {
        self.settings.emoji_size as f32 + CELL_PADDING
    }

    fn recent_ids(&self) -> Vec<usize> {
        self.settings
            .recent
            .iter()
            .filter_map(|c| emojis::find_by_char(c))
            .collect()
    }

    fn push_cells(&mut self, ids: &[usize]) {
        for chunk in ids.chunks(self.columns.max(1)) {
            let start = self.display.len();
            self.display.extend_from_slice(chunk);
            self.row_of.extend(std::iter::repeat_n(self.items.len(), chunk.len()));
            self.items.push(Item::Cells {
                start,
                len: chunk.len(),
            });
        }
    }

    fn rebuild(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let tr = ui.global::<Tr>();
        self.display.clear();
        self.items.clear();
        self.row_of.clear();
        if self.query.is_empty() {
            for c in 0..emojis::CATEGORIES.len() {
                let ids: Vec<usize> = if c == 0 {
                    self.recent_ids()
                } else {
                    emojis::in_category(c).iter().map(|&i| i as usize).collect()
                };
                self.category_row[c] = self.items.len();
                if !ids.is_empty() {
                    self.items.push(Item::Title(tr.invoke_category(c as i32).to_string()));
                    self.push_cells(&ids);
                }
            }
        } else {
            let hits = emojis::search(&self.query);
            let query = self.query.as_str().into();
            let title = if hits.is_empty() {
                tr.invoke_no_matches(query)
            } else {
                tr.invoke_results(query)
            };
            self.items.push(Item::Title(title.to_string()));
            self.push_cells(&hits);
        }
        if self.selected >= self.display.len() {
            self.selected = 0;
        }
    }

    /// First emoji at or below `row`, used to keep the view anchored when the layout changes.
    fn anchor_at(&self, row: usize) -> Option<usize> {
        self.items.iter().skip(row).find_map(|i| match i {
            Item::Cells { start, .. } => Some(*start),
            Item::Title(_) => None,
        })
    }
}

/// Rows are built on demand, so only what `ListView` shows is shaped and decoded.
struct RowsModel {
    st: Rc<RefCell<State>>,
    glyphs: Rc<Glyphs>,
    notify: ModelNotify,
}

impl Model for RowsModel {
    type Data = Line;

    fn row_count(&self) -> usize {
        self.st.borrow().items.len()
    }

    fn row_data(&self, row: usize) -> Option<Line> {
        let st = self.st.borrow();
        Some(match st.items.get(row)? {
            Item::Title(title) => Line {
                title: title.as_str().into(),
                cells: ModelRc::default(),
            },
            Item::Cells { start, len } => {
                let cells: Vec<Cell> = (*start..start + len)
                    .map(|d| {
                        let ch = emojis::char_for(st.display[d], st.settings.skin_tone);
                        Cell {
                            img: self.glyphs.image(ch, st.settings.emoji_size as f32),
                            index: d as i32,
                        }
                    })
                    .collect();
                Line {
                    title: "".into(),
                    cells: ModelRc::new(VecModel::from(cells)),
                }
            }
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

struct App {
    ui: slint::Weak<Main>,
    st: Rc<RefCell<State>>,
    model: Rc<RowsModel>,
    glyphs: Rc<Glyphs>,
    /// X11 clipboard that knows when it is taken over (see `x11::ClipboardOwner`).
    x11_clipboard: Option<x11::ClipboardOwner>,
    /// Shared X connection for focus queries and paste.
    xconn: Option<x11::Session>,
    /// Fallback for non-X11 sessions.
    clipboard: RefCell<Option<arboard::Clipboard>>,
    paste_pending: StdCell<bool>,
    /// True once the selection was pasted into the previous window.
    pasted: StdCell<bool>,
    reads_before_paste: StdCell<usize>,
    /// Copy-only mode: when the clipboard was last set, and the read count that does not count.
    copied_at: StdCell<Instant>,
    hold_baseline: StdCell<usize>,
    hold_read_at: StdCell<Option<Instant>>,
    hold_timer: Timer,
    exit_polls: StdCell<u32>,
    exit_read_at: StdCell<Option<u32>>,
    exit_timer: Timer,
    hotkey: RefCell<Option<x11::Hotkey>>,
    shown_at: StdCell<Option<Instant>>,
    last_time: StdCell<u32>,
    ignore_scroll_until: StdCell<Instant>,
    focused: StdCell<bool>,
    focus_attempts: StdCell<u32>,
    focus_timer: Timer,
    blur_timer: Timer,
    /// The window that had focus before the picker opened; it should receive the paste.
    previous_window: StdCell<Option<u32>>,
    paste_attempts: StdCell<u32>,
    paste_timer: Timer,
    x11: bool,
}

impl App {
    fn active_window(&self) -> Option<u32> {
        self.xconn.as_ref()?.active_window().ok().flatten()
    }

    fn ui(&self) -> Main {
        self.ui.unwrap()
    }

    /// Rebuild rows after a state change. The state borrow must be released before notifying.
    fn relayout(&self) {
        self.st.borrow_mut().rebuild();
        self.model.notify.reset();
        self.sync();
    }

    /// Pick the UI and emoji-name language from the setting (or the system locale). Callers redraw afterwards.
    fn apply_language(&self) {
        let pref = self.st.borrow().settings.language.clone();
        let code = if pref.is_empty() {
            system_language()
        } else {
            emojis::resolve_language(&pref)
        };
        emojis::set_language(code);
        // Only fails when the UI was built without bundled translations; the source strings are English anyway.
        let _ = slint::select_bundled_translation(code);
    }

    fn set_categories(&self) {
        let categories: Vec<Category> = emojis::CATEGORIES
            .iter()
            .enumerate()
            .map(|(i, (_, icon))| Category {
                img: self.glyphs.image(icon, CATEGORY_PX),
                label: self.ui().global::<Tr>().invoke_category(i as i32),
            })
            .collect();
        self.ui().set_categories(ModelRc::new(VecModel::from(categories)));
    }

    /// Images are made for the current scale factor, which is only known once the window exists
    /// and changes when it moves to another monitor; redraw them all when it does.
    fn refresh_scale(&self) {
        if self.glyphs.set_scale(self.ui().window().scale_factor()) {
            self.set_categories();
            self.model.notify.reset();
            self.sync();
        }
    }

    /// Push derived state (selection, preview, settings) into the UI.
    fn sync(&self) {
        let ui = self.ui();
        let st = self.st.borrow();
        let tone = st.settings.skin_tone;
        ui.set_cell(st.cell_px());
        ui.set_emoji_px(st.settings.emoji_size as f32);
        ui.set_tone(tone as i32);
        ui.set_size_choice(
            settings::EMOJI_SIZES
                .iter()
                .position(|s| *s == st.settings.emoji_size)
                .unwrap_or(2) as i32,
        );
        ui.set_auto_paste(st.settings.auto_paste);
        ui.set_language_choice(language_choices().position(|c| c == st.settings.language).unwrap_or(0) as i32);
        ui.set_version(env!("CARGO_PKG_VERSION").into());
        ui.set_keep_resident(!self.x11 || st.settings.keep_resident);
        ui.set_resident_locked(!self.x11);
        ui.set_shortcut_label(st.settings.shortcut.as_str().into());
        ui.set_active_category(if st.query.is_empty() { st.active as i32 } else { -1 });
        match st.display.get(st.selected) {
            Some(&e) => {
                ui.set_selected_index(st.selected as i32);
                ui.set_preview_img(self.glyphs.image(emojis::char_for(e, tone), PREVIEW_PX));
                ui.set_preview_name(emojis::name(e).into());
            }
            None => {
                ui.set_selected_index(-1);
                ui.set_preview_img(Image::default());
                ui.set_preview_name("".into());
            }
        }
    }

    // ---- layout and scrolling -------------------------------------------------------------

    fn on_width(&self, width: f32, force: bool) {
        if width <= 0.0 {
            return;
        }
        self.refresh_scale();
        let ui = self.ui();
        let (anchor, cell) = {
            let mut st = self.st.borrow_mut();
            st.last_width = width;
            let cell = st.cell_px();
            let columns = (((width - 24.0) / cell) as usize).max(1);
            if columns == st.columns && !force && !st.items.is_empty() {
                return;
            }
            let row = (-ui.get_scroll_y() / cell).max(0.0) as usize;
            // A title row is anchored together with the first row below it.
            let on_title = matches!(st.items.get(row), Some(Item::Title(_)));
            let anchor = st.anchor_at(row).map(|a| (a, on_title));
            st.columns = columns;
            (anchor, cell)
        };
        self.relayout();
        if let Some((anchor, on_title)) = anchor {
            let row = self.st.borrow().row_of.get(anchor).copied();
            if let Some(row) = row {
                ui.set_scroll_y(-((row - on_title as usize) as f32 * cell));
            }
        }
    }

    fn on_scrolled(&self) {
        if Instant::now() < self.ignore_scroll_until.get() {
            return;
        }
        let ui = self.ui();
        let mut st = self.st.borrow_mut();
        if !st.query.is_empty() {
            return;
        }
        let row = ((-ui.get_scroll_y() + st.cell_px() * 0.5) / st.cell_px()).max(0.0) as usize;
        let active = st.category_row.iter().rposition(|&r| r <= row).unwrap_or(0);
        if active != st.active {
            st.active = active;
            ui.set_active_category(active as i32);
        }
    }

    fn ensure_selected_visible(&self) {
        let ui = self.ui();
        let target = {
            let st = self.st.borrow();
            let Some(&row) = st.row_of.get(st.selected) else { return };
            let (cell, view) = (st.cell_px(), ui.get_list_height());
            let (top, current) = (row as f32 * cell, -ui.get_scroll_y());
            if top < current {
                Some(-top)
            } else if top + cell > current + view {
                Some(-(top + cell - view))
            } else {
                None
            }
        };
        // No State borrow while Slint may call `scrolled` back.
        if let Some(y) = target {
            ui.set_scroll_y(y);
        }
    }

    fn jump_to_category(&self, category: usize) {
        let ui = self.ui();
        if !self.st.borrow().query.is_empty() {
            ui.set_search_text("".into());
            self.search("");
        }
        let y = {
            let mut st = self.st.borrow_mut();
            st.active = category;
            let row = st.category_row[category];
            if let Some(start) = st.anchor_at(row) {
                st.selected = start;
            }
            -(row as f32 * st.cell_px())
        };
        // Programmatic scrolling must not be reinterpreted by the scroll-spy.
        self.ignore_scroll_until
            .set(Instant::now() + Duration::from_millis(300));
        ui.set_scroll_y(y);
        self.sync();
    }

    // ---- search and selection -------------------------------------------------------------

    fn search(&self, text: &str) {
        let query = text.trim().to_string();
        let ui = self.ui();
        let restore = {
            let mut st = self.st.borrow_mut();
            let was_searching = !st.query.is_empty();
            if query.is_empty() && !was_searching {
                return;
            }
            if !was_searching {
                st.saved_scroll = Some(ui.get_scroll_y());
            }
            st.selected = 0;
            st.query = query.clone();
            if query.is_empty() { st.saved_scroll.take() } else { None }
        };
        self.relayout();
        ui.set_scroll_y(restore.unwrap_or(0.0));
        if restore.is_some() {
            self.on_scrolled();
        }
    }

    fn set_selected(&self, index: usize) {
        self.st.borrow_mut().selected = index;
        self.sync();
    }

    fn move_horizontal(&self, delta: i32) {
        let (len, current) = {
            let st = self.st.borrow();
            (st.display.len(), st.selected)
        };
        if len == 0 {
            return;
        }
        self.set_selected((current as i32 + delta).clamp(0, len as i32 - 1) as usize);
        self.ensure_selected_visible();
    }

    /// Returns false when there is no row in that direction (Up on the top row moves to the category bar).
    fn move_vertical(&self, direction: i32) -> bool {
        let target = {
            let st = self.st.borrow();
            let Some(&row) = st.row_of.get(st.selected) else {
                return false;
            };
            let Some(Item::Cells { start, .. }) = st.items.get(row) else {
                return false;
            };
            let column = st.selected - start;
            let mut next = row as i32 + direction;
            let mut found = None;
            while next >= 0 && (next as usize) < st.items.len() {
                if let Item::Cells { start, len } = &st.items[next as usize] {
                    found = Some(start + column.min(len - 1));
                    break;
                }
                next += direction;
            }
            found
        };
        let Some(index) = target else { return false };
        self.set_selected(index);
        self.ensure_selected_visible();
        true
    }

    /// Moves by one screenful of rows (keeping one row of overlap); stops at the first/last row.
    fn move_page(&self, direction: i32) {
        let rows = ((self.ui().get_list_height() / self.st.borrow().cell_px()) as i32 - 1).max(1);
        for _ in 0..rows {
            if !self.move_vertical(direction) {
                break;
            }
        }
    }

    fn cycle_category(&self, delta: i32) {
        let count = emojis::CATEGORIES.len() as i32;
        let active = self.st.borrow().active as i32;
        self.jump_to_category((active + delta).rem_euclid(count) as usize);
    }

    // ---- copy / paste ---------------------------------------------------------------------

    /// False when the text did not reach the clipboard; the caller must not paste then.
    fn copy(&self, text: &str) -> bool {
        if let Some(owner) = &self.x11_clipboard {
            let result = owner.set(text);
            if let Err(e) = &result {
                eprintln!("clipboard error: {e}");
            }
            self.copied_at.set(Instant::now());
            self.hold_read_at.set(None);
            return result.is_ok();
        }
        let mut clipboard = self.clipboard.borrow_mut();
        if clipboard.is_none() {
            *clipboard = arboard::Clipboard::new().ok();
        }
        match clipboard.as_mut().map(|c| c.set_text(text.to_string())) {
            Some(Ok(())) => true,
            Some(Err(e)) => {
                eprintln!("clipboard error: {e}");
                false
            }
            None => {
                eprintln!("clipboard unavailable");
                false
            }
        }
    }

    fn emoji_at(&self, display_index: usize) -> Option<&'static str> {
        let st = self.st.borrow();
        st.display
            .get(display_index)
            .map(|&e| emojis::char_for(e, st.settings.skin_tone))
    }

    /// Enter / click: copy, close and paste.
    fn select(&self, index: usize) {
        let Some(ch) = self.emoji_at(index) else { return };
        let paste = {
            let mut st = self.st.borrow_mut();
            st.settings.push_recent(ch);
            st.settings.save();
            st.settings.auto_paste
        };
        let copied = self.copy(ch);
        let will_paste = paste && self.x11 && copied;
        // Must be set before hide(): a non-resident FastMoji may only exit after the paste is sent.
        self.paste_pending.set(will_paste);
        self.hide();
        if will_paste {
            self.paste_when_ready();
        }
    }

    /// Paste once the window that was in front before the picker has focus again. Pasting on a
    /// fixed delay fails whenever the window manager is slower than the delay.
    fn paste_when_ready(&self) {
        self.paste_attempts.set(0);
        self.paste_timer
            .start(slint::TimerMode::Repeated, PASTE_POLL, tick(App::paste_tick));
    }

    fn paste_tick(&self) {
        let attempts = self.paste_attempts.get() + 1;
        self.paste_attempts.set(attempts);
        let previous = self.previous_window.get();
        let active = self.active_window();
        let focus_ok = |window| {
            self.xconn
                .as_ref()
                .is_none_or(|x| x.input_focus_inside(window).unwrap_or(true))
        };
        let step = paste_step(previous, active, focus_ok, attempts);
        if matches!(step, PasteStep::RestoreFocus)
            && let Some(window) = previous
        {
            debug(|| format!("restoring focus to {window:#x} (active {active:?})"));
            let _ = self.xconn.as_ref().map(|x| x.focus(window));
        }
        if matches!(step, PasteStep::Paste | PasteStep::Skip) {
            self.paste_timer.stop();
            if let Some(c) = &self.x11_clipboard {
                self.reads_before_paste.set(c.reads());
            }
            if matches!(step, PasteStep::Paste) {
                let result = match &self.xconn {
                    Some(x) => x.paste(active.is_some_and(|w| x.is_terminal(w))),
                    None => Err("no X connection".into()),
                };
                debug(|| format!("paste after {attempts} polls, active {active:?}, previous {previous:?}: {result:?}"));
                match result {
                    Ok(()) => self.pasted.set(true),
                    Err(e) => eprintln!("paste failed: {e}"),
                }
            } else {
                eprintln!(
                    "paste skipped: {previous:?} did not regain focus (active {active:?}); emoji is on the clipboard"
                );
            }
            self.paste_pending.set(false);
            self.maybe_exit();
        }
    }

    /// Whether the process stays alive after the picker closes.
    fn resident(&self) -> bool {
        // Without our own X11 clipboard (Wayland) the process has to stay to keep the clipboard.
        !self.x11 || self.st.borrow().settings.keep_resident
    }

    /// Non-resident mode: quit once the picker is closed and nothing is being pasted. After a
    /// successful paste the job is done and we exit right away. If the user only copied (auto-paste
    /// off), keep serving the clipboard until another
    /// application replaces it (`on_clipboard_lost`), otherwise the copy would be useless.
    fn maybe_exit(&self) {
        if self.resident() || self.paste_pending.get() || self.ui().window().is_visible() {
            return;
        }
        if self.x11_clipboard.as_ref().is_some_and(|c| c.owns()) {
            if self.pasted.get() {
                self.exit_after_clipboard_read();
            } else {
                self.hold_clipboard();
            }
            return;
        }
        debug(|| "not resident: exiting".into());
        let _ = slint::quit_event_loop();
    }

    /// Copy-only: the user will paste by hand. Stay until that paste has happened (the clipboard
    /// was read), another application replaces the clipboard, or `HOLD_MAX` has passed. Reads in
    /// the first moments after copying are clipboard managers saving a copy, not the user.
    fn hold_clipboard(&self) {
        if self.hold_timer.running() {
            return;
        }
        debug(|| "not resident: holding the clipboard until it is pasted".into());
        self.hold_read_at.set(None);
        self.hold_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(100),
            tick(App::hold_tick),
        );
    }

    fn hold_tick(&self) {
        let Some(clipboard) = &self.x11_clipboard else { return };
        let step = hold_step(
            self.ui().window().is_visible(),
            clipboard.owns(),
            self.copied_at.get().elapsed(),
            clipboard.reads(),
            self.hold_baseline.get(),
            self.hold_read_at.get().map(|at| at.elapsed()),
        );
        match step {
            // The picker is open again; the next hide() decides.
            HoldStep::Stop => self.hold_timer.stop(),
            HoldStep::Rebaseline => self.hold_baseline.set(clipboard.reads()),
            HoldStep::MarkRead => self.hold_read_at.set(Some(Instant::now())),
            HoldStep::Wait => {}
            HoldStep::Exit(why) => {
                debug(|| format!("not resident: exiting, {why}"));
                let _ = slint::quit_event_loop();
            }
        }
    }

    /// The target application reads the clipboard only after it receives Ctrl+V, so leaving
    /// immediately would paste nothing. Wait until it has fetched the text (plus a short grace
    /// period), but never longer than `EXIT_WAIT_MAX`.
    fn exit_after_clipboard_read(&self) {
        if self.exit_timer.running() {
            return;
        }
        self.exit_polls.set(0);
        self.exit_read_at.set(None);
        self.exit_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(25),
            tick(App::exit_tick),
        );
    }

    fn exit_tick(&self) {
        let visible = self.ui().window().is_visible();
        let polls = self.exit_polls.get() + 1;
        self.exit_polls.set(polls);
        let read = self
            .x11_clipboard
            .as_ref()
            .is_some_and(|c| c.reads() > self.reads_before_paste.get());
        match exit_step(visible, polls, self.exit_read_at.get(), read) {
            // The picker was opened again while waiting; the next hide() decides.
            ExitStep::Stop => self.exit_timer.stop(),
            ExitStep::MarkRead => self.exit_read_at.set(Some(polls)),
            ExitStep::Wait => {}
            ExitStep::Exit(why) => {
                debug(|| format!("not resident: exiting, {why} ({polls} polls)"));
                let _ = slint::quit_event_loop();
            }
        }
    }

    fn start_hotkey(&self) {
        if !self.x11 || self.hotkey.borrow().is_some() {
            return;
        }
        let shortcut = self.st.borrow().settings.shortcut.clone();
        match x11::spawn_hotkey(&shortcut, |time| post(move |app| app.hotkey_pressed(time))) {
            Ok(hotkey) => {
                *self.hotkey.borrow_mut() = Some(hotkey);
                self.ui().set_shortcut_error("".into());
            }
            Err(e) => {
                let ui = self.ui();
                let tr = ui.global::<Tr>();
                let reason = match e {
                    x11::HotkeyError::Taken => tr.invoke_shortcut_taken(shortcut.as_str().into()),
                    x11::HotkeyError::Other(text) => text.into(),
                };
                eprintln!("global shortcut unavailable: {reason}");
                self.ui().set_shortcut_error(tr.invoke_shortcut_unavailable(reason));
            }
        }
    }

    /// Switch the global shortcut; on failure the previous one is grabbed again and stays in the settings.
    fn change_shortcut(&self, text: &str) {
        let text = text.trim();
        if self.x11 && x11::parse_shortcut(text).is_none() {
            self.ui()
                .set_shortcut_error(self.ui().global::<Tr>().invoke_cannot_parse(format!("{text:?}").into()));
            return;
        }
        let old = std::mem::replace(&mut self.st.borrow_mut().settings.shortcut, text.to_string());
        if let Some(hotkey) = self.hotkey.borrow_mut().take() {
            hotkey.release();
        }
        if self.resident() {
            self.start_hotkey();
        }
        let error = self.ui().get_shortcut_error();
        if !error.is_empty() {
            self.st.borrow_mut().settings.shortcut = old;
            self.start_hotkey();
            self.ui().set_shortcut_error(error);
        }
        self.st.borrow().settings.save();
        self.sync();
    }

    fn accept_selection(&self) {
        let selected = self.st.borrow().selected;
        self.select(selected)
    }

    // ---- window lifecycle -----------------------------------------------------------------

    fn toggle(&self, time: u32) {
        // The grab swallows the current shortcut while recording: pressing it again means "keep it".
        if self.ui().get_recording_shortcut() {
            self.ui().set_recording_shortcut(false);
        } else if self.ui().window().is_visible() {
            self.hide()
        } else {
            self.open(time)
        }
    }

    /// The global shortcut only opens the picker or brings it back to the front; it never closes it
    /// (Esc, a click elsewhere or picking an emoji do). `--toggle` still toggles.
    fn hotkey_pressed(&self, time: u32) {
        if self.ui().get_recording_shortcut() {
            self.ui().set_recording_shortcut(false);
        } else if self.ui().window().is_visible() {
            self.last_time.set(time);
            self.activate(time);
            self.ui().invoke_focus_search();
        } else {
            self.open(time);
        }
    }

    fn open(&self, time: u32) {
        debug(|| format!("open time={time}"));
        // A paste still waiting for the previous window would hit whatever is active now.
        self.paste_timer.stop();
        self.paste_pending.set(false);
        if self.x11 && !self.ui().window().is_visible() {
            self.previous_window.set(self.active_window());
        }
        let ui = self.ui();
        self.pasted.set(false);
        {
            let mut st = self.st.borrow_mut();
            st.query.clear();
            st.selected = 0;
            st.active = 0;
            st.saved_scroll = None;
        }
        ui.set_search_text("".into());
        ui.set_settings_open(false);
        ui.set_about_open(false);
        ui.set_categories_focused(false);
        let (w, h) = {
            let st = self.st.borrow();
            (st.settings.width, st.settings.height)
        };
        ui.window().set_size(LogicalSize::new(w as f32, h as f32));
        // Placement must reach the WM before the map; moving after show() flashes at the WM's own spot.
        if self.x11
            && let Some((x, y)) = self.target_position(w, h)
        {
            if let (Some(id), Some(xconn)) = (self.window_id(), &self.xconn) {
                xconn.set_position_hint(id, x, y);
            }
            ui.window().set_position(slint::PhysicalPosition::new(x, y));
        }
        self.relayout();
        ui.set_scroll_y(0.0);
        self.shown_at.set(Some(Instant::now()));
        self.last_time.set(time);
        let _ = ui.show();
        self.refresh_scale();
        self.activate(time);
        ui.invoke_focus_search();
        self.keep_requesting_focus();
    }

    fn hide(&self) {
        debug(|| "hide".into());
        let ui = self.ui();
        ui.set_recording_shortcut(false);
        let window = ui.window();
        if window.is_visible() {
            let size = window.size().to_logical(window.scale_factor());
            let mut st = self.st.borrow_mut();
            let (w, h) = (size.width.round() as u32, size.height.round() as u32);
            if w >= settings::MIN_SIZE.0
                && h >= settings::MIN_SIZE.1
                && (w, h) != (st.settings.width, st.settings.height)
            {
                st.settings.width = w;
                st.settings.height = h;
                st.settings.save();
            }
        }
        let _ = ui.hide();
        self.maybe_exit();
    }

    /// Hand focus to the picker using the hotkey's timestamp (X11 only).
    fn activate(&self, time: u32) {
        if !self.x11 {
            return;
        }
        let Some(xconn) = &self.xconn else { return };
        if let Some(id) = self.window_id()
            && let Err(e) = xconn.activate(id, time)
        {
            eprintln!("could not activate window: {e}");
        }
    }

    /// X11 id of the picker window, once winit has created it.
    fn window_id(&self) -> Option<u32> {
        self.ui()
            .window()
            .with_winit_window(|w| match w.window_handle().ok()?.as_raw() {
                RawWindowHandle::Xlib(h) => Some(h.window as u32),
                RawWindowHandle::Xcb(h) => Some(h.window.get()),
                _ => None,
            })
            .flatten()
    }

    /// Top-left (physical px) that centers a `width` x `height` logical popup on the pointer, inside its monitor.
    fn target_position(&self, width: u32, height: u32) -> Option<(i32, i32)> {
        let xconn = self.xconn.as_ref()?;
        let ui = self.ui();
        let window = ui.window();
        let scale = window.scale_factor();
        let (w, h) = (
            (width as f32 * scale).round() as i32,
            (height as f32 * scale).round() as i32,
        );
        let (px, py) = xconn.pointer()?;
        // Monitors are known only once the window exists; before that use the whole screen.
        let monitors = window
            .with_winit_window(|win| {
                win.available_monitors()
                    .map(|m| {
                        (
                            m.position().x,
                            m.position().y,
                            m.size().width as i32,
                            m.size().height as i32,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let (mx, my, mw, mh) = monitors
            .into_iter()
            .find(|&(x, y, w, h)| (x..x + w).contains(&px) && (y..y + h).contains(&py))
            .or_else(|| xconn.screen_size().map(|(w, h)| (0, 0, w, h)))
            .unwrap_or((0, 0, i32::MAX, i32::MAX));
        Some((
            (px - w / 2).clamp(mx, mx + (mw - w).max(0)),
            (py - h / 2).clamp(my, my + (mh - h).max(0)),
        ))
    }

    /// The WM may ignore the first request; keep asking until the picker actually has focus.
    fn keep_requesting_focus(&self) {
        self.focused.set(false);
        self.focus_attempts.set(0);
        self.focus_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(100),
            tick(App::retry_focus),
        );
    }

    fn retry_focus(&self) {
        let attempts = self.focus_attempts.get() + 1;
        self.focus_attempts.set(attempts);
        if self.focused.get() || attempts > 6 || !self.ui().window().is_visible() {
            self.focus_timer.stop();
            return;
        }
        debug(|| format!("retry focus #{attempts}"));
        self.activate(self.last_time.get());
        self.ui().invoke_focus_search();
    }

    /// True when the X input focus is still inside the picker's window.
    fn focus_still_ours(&self) -> bool {
        let (Some(id), Some(x)) = (self.window_id(), &self.xconn) else {
            return false;
        };
        x.input_focus_inside(id).unwrap_or(false)
    }

    fn on_focus_lost(&self) {
        self.focused.set(false);
        if self.shown_at.get().is_some_and(|t| t.elapsed() < FOCUS_GRACE) {
            // The WM sometimes bounces focus right after mapping; ask again instead of hiding.
            self.activate(self.last_time.get());
        } else if self.focus_still_ours() {
            // Pressing or releasing a grabbed key sends the window a focus-out (mode Grab/Ungrab) although the X
            // input focus never moved; the hotkey thread's flag can arrive too late to tell, the server cannot.
            debug(|| "focus-out without a focus change (key grab): ignored".into());
        } else if self.ui().get_recording_shortcut() || self.hotkey.borrow().as_ref().is_some_and(|h| h.in_use()) {
            // Pressing or releasing the grabbed shortcut itself bounces the focus (X grab starts and ends), and the
            // hotkey thread's toggle arrives separately. Hide only if the focus really went elsewhere.
            self.blur_timer.start(
                slint::TimerMode::SingleShot,
                FOCUS_GRACE,
                tick(|app| {
                    if !app.focused.get() {
                        app.hide();
                    }
                }),
            );
        } else {
            self.hide();
        }
    }
}

pub fn run(show: bool, command: &str) {
    let ui = Main::new().expect("cannot create the window");
    let glyphs = Rc::new(Glyphs::new());
    let settings = Settings::load();
    // Match the shortcut the user has configured (see `settings.toml`).
    let keep_resident = settings.keep_resident;
    let st = Rc::new(RefCell::new(State {
        ui: ui.as_weak(),
        settings,
        query: String::new(),
        display: Vec::new(),
        items: Vec::new(),
        row_of: Vec::new(),
        category_row: [0; emojis::CATEGORIES.len()],
        selected: 0,
        columns: 1,
        active: 0,
        saved_scroll: None,
        last_width: 0.0,
    }));
    let model = Rc::new(RowsModel {
        st: st.clone(),
        glyphs: glyphs.clone(),
        notify: Default::default(),
    });
    let x11_session = x11::is_x11_session();
    let app = Rc::new(App {
        ui: ui.as_weak(),
        st,
        model: model.clone(),
        glyphs: glyphs.clone(),
        x11_clipboard: if x11_session {
            x11::ClipboardOwner::new(|| post(|app| app.maybe_exit()))
                .map_err(|e| eprintln!("X11 clipboard unavailable: {e}"))
                .ok()
        } else {
            None
        },
        xconn: if x11_session {
            x11::Session::new()
                .map_err(|e| eprintln!("X11 connection unavailable: {e}"))
                .ok()
        } else {
            None
        },
        clipboard: RefCell::new(if x11_session {
            None
        } else {
            arboard::Clipboard::new().ok()
        }),
        paste_pending: StdCell::new(false),
        pasted: StdCell::new(false),
        reads_before_paste: StdCell::new(0),
        copied_at: StdCell::new(Instant::now()),
        hold_baseline: StdCell::new(0),
        hold_read_at: StdCell::new(None),
        hold_timer: Timer::default(),
        exit_polls: StdCell::new(0),
        exit_read_at: StdCell::new(None),
        exit_timer: Timer::default(),
        hotkey: RefCell::new(None),
        shown_at: StdCell::new(None),
        last_time: StdCell::new(0),
        ignore_scroll_until: StdCell::new(Instant::now()),
        focused: StdCell::new(false),
        focus_attempts: StdCell::new(0),
        focus_timer: Timer::default(),
        blur_timer: Timer::default(),
        previous_window: StdCell::new(None),
        paste_attempts: StdCell::new(0),
        paste_timer: Timer::default(),
        x11: x11_session,
    });
    APP.with(|a| *a.borrow_mut() = Some(app.clone()));

    app.apply_language();
    ui.set_lines(ModelRc::from(model));
    app.set_categories();

    macro_rules! wire {
        ($on:ident, |$app:ident $(, $arg:ident)*| $body:expr) => {{
            let $app = app.clone();
            ui.$on(move |$($arg),*| $body);
        }};
    }
    wire!(on_search, |app, text| app.search(&text));
    wire!(on_width_changed, |app, w| app.on_width(w, false));
    wire!(on_scrolled, |app| app.on_scrolled());
    // The window opens under the pointer and Slint reports a synthetic move for it; ignore that burst.
    wire!(on_hovered, |app, i| {
        if app
            .shown_at
            .get()
            .is_none_or(|t| t.elapsed() > Duration::from_millis(400))
        {
            app.set_selected(i as usize);
        }
    });
    wire!(on_picked, |app, i| app.select(i as usize));
    wire!(on_category_clicked, |app, c| app.jump_to_category(c as usize));
    wire!(on_move_horizontal, |app, d| app.move_horizontal(d));
    wire!(on_move_vertical, |app, d| app.move_vertical(d));
    wire!(on_move_page, |app, d| app.move_page(d));
    wire!(on_accept_selection, |app| app.accept_selection());
    wire!(on_cycle_category, |app, d| app.cycle_category(d));
    wire!(on_dismiss, |app| app.hide());
    wire!(on_set_tone, |app, i| {
        {
            let mut st = app.st.borrow_mut();
            st.settings.skin_tone = i as usize;
            st.settings.save();
        }
        app.model.notify.reset();
        app.sync();
    });
    wire!(on_set_size, |app, i| {
        let width = {
            let mut st = app.st.borrow_mut();
            st.settings.emoji_size = settings::EMOJI_SIZES[(i as usize).min(4)];
            st.settings.save();
            st.last_width
        };
        app.on_width(width, true);
    });
    wire!(on_set_language, |app, i| {
        {
            let mut st = app.st.borrow_mut();
            st.settings.language = language_choices().nth(i as usize).unwrap_or_default().to_string();
            st.settings.save();
        }
        app.apply_language();
        app.set_categories();
        app.relayout();
    });
    wire!(on_set_auto_paste, |app, on| {
        let mut st = app.st.borrow_mut();
        st.settings.auto_paste = on;
        st.settings.save();
    });
    wire!(on_set_keep_resident, |app, on| {
        {
            let mut st = app.st.borrow_mut();
            st.settings.keep_resident = on;
            st.settings.save();
        }
        if on {
            app.start_hotkey();
        }
        app.sync();
    });
    ui.on_record_key({
        let app = app.clone();
        move |text, ctrl, alt, shift, meta| {
            let mut text = text.to_string();
            if app.x11
                && let Some(c) = text.chars().next().filter(|_| text.chars().count() == 1)
                && let Some(base) = x11::base_key(c)
            {
                text = base.to_string();
            }
            match shortcut_from_key(&text, [ctrl, alt, shift, meta]) {
                Some(shortcut) => {
                    app.change_shortcut(&shortcut);
                    true
                }
                None => false,
            }
        }
    });
    ui.on_held_text({
        let ui = ui.as_weak();
        move |ctrl, alt, shift, meta| match held_text([ctrl, alt, shift, meta]) {
            Some(text) => text.into(),
            None => ui.unwrap().global::<Tr>().invoke_press_keys(),
        }
    });
    ui.on_open_url(|url| {
        // Needs xdg-open on the host; inside Flatpak it is missing, so the click does nothing there yet.
        // Reaped on a thread, or every click would leave a zombie in this long-lived process.
        if let Ok(mut child) = std::process::Command::new("xdg-open").arg(url.as_str()).spawn() {
            std::thread::spawn(move || _ = child.wait());
        }
    });
    ui.on_quit(|| {
        let _ = slint::quit_event_loop();
    });

    let weak_app = Rc::downgrade(&app);
    ui.window().on_winit_window_event(move |_, event| {
        if let winit::event::WindowEvent::Focused(focused) = event
            && let Some(app) = weak_app.upgrade()
        {
            debug(|| format!("focused={focused}"));
            if *focused {
                app.focused.set(true);
            } else {
                app.on_focus_lost();
            }
        }
        EventResult::Propagate
    });

    if let Err(e) = instance::listen(|command| {
        post(move |app| match command.as_str() {
            "toggle" => app.toggle(0),
            "show" => app.open(0),
            _ => {}
        })
    }) {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            // Lost the start-up race to another instance: hand it our command and leave.
            instance::send(command);
            return;
        }
        // Without the socket a later `fastmoji` starts another copy, but a picker that works beats one that never shows.
        eprintln!("single-instance socket unavailable ({e}); running without single-instance mode");
    }
    if x11_session {
        // The global shortcut only exists while the process stays resident.
        if keep_resident {
            app.start_hotkey();
        }
    } else {
        eprintln!("Not an X11 session: bind `fastmoji --toggle` to a key in your desktop's keyboard settings.");
    }

    if show {
        app.open(0);
    }
    slint::run_event_loop_until_quit().expect("event loop failed");
}

#[cfg(test)]
mod tests {
    #[test]
    fn records_shortcuts() {
        use super::shortcut_from_key as key;
        assert_eq!(key(".", [true, true, false, false]).as_deref(), Some("Ctrl+Alt+."));
        assert_eq!(key("e", [true, false, true, false]).as_deref(), Some("Ctrl+Shift+E"));
        assert_eq!(
            key("\u{f708}", [false, false, false, true]).as_deref(),
            Some("Super+F5")
        );
        assert_eq!(
            super::held_text([true, true, false, false]).as_deref(),
            Some("Ctrl + Alt + …")
        );
        assert_eq!(super::held_text([false; 4]), None);
        assert_eq!(key("a", [false; 4]), None, "needs a modifier");
        assert_eq!(
            key("{", [true, true, false, false]),
            None,
            "layout symbols are resolved to the key before this"
        );
        // Everything the recorder produces must be accepted by the hotkey parser.
        for (text, mods) in [
            (".", [true, true, false, false]),
            ("7", [true, false, true, false]),
            ("\u{f70b}", [false, false, false, true]),
            (" ", [true, false, false, false]),
            ("\t", [true, true, false, false]),
        ] {
            let shortcut = key(text, mods).unwrap();
            assert!(
                crate::x11::parse_shortcut(&shortcut).is_some(),
                "{shortcut} not parseable"
            );
        }
        assert_eq!(key("\u{11}", [true, false, false, false]), None, "lone modifier");
    }

    use super::*;

    const SEC: Duration = Duration::from_secs(1);

    fn hold(visible: bool, owns: bool, since_copy: Duration, reads: usize, read_elapsed: Option<Duration>) -> HoldStep {
        hold_step(visible, owns, since_copy, reads, 0, read_elapsed)
    }

    #[test]
    fn hold_stops_when_the_picker_is_open_again() {
        assert!(matches!(hold(true, false, SEC * 9, 5, None), HoldStep::Stop));
    }

    #[test]
    fn hold_exits_when_the_clipboard_is_replaced() {
        assert!(matches!(hold(false, false, SEC, 0, None), HoldStep::Exit(_)));
    }

    #[test]
    fn hold_ignores_reads_right_after_copying() {
        assert!(matches!(
            hold(false, true, HOLD_IGNORE_READS / 2, 3, None),
            HoldStep::Rebaseline
        ));
    }

    #[test]
    fn hold_waits_for_a_late_read_then_a_grace_period() {
        assert!(matches!(hold(false, true, SEC * 2, 0, None), HoldStep::Wait));
        assert!(matches!(hold(false, true, SEC * 2, 1, None), HoldStep::MarkRead));
        assert!(matches!(
            hold(false, true, SEC * 3, 1, Some(HOLD_GRACE / 2)),
            HoldStep::Wait
        ));
        assert!(matches!(
            hold(false, true, SEC * 3, 1, Some(HOLD_GRACE)),
            HoldStep::Exit(_)
        ));
    }

    #[test]
    fn hold_gives_up_after_the_maximum() {
        assert!(matches!(hold(false, true, HOLD_MAX - SEC, 0, None), HoldStep::Wait));
        assert!(matches!(hold(false, true, HOLD_MAX, 0, None), HoldStep::Exit(_)));
    }

    const W: Option<u32> = Some(7);

    #[test]
    fn paste_waits_for_the_previous_window_and_its_focus() {
        assert!(matches!(paste_step(W, W, |_| true, 2), PasteStep::Wait), "too early");
        assert!(matches!(paste_step(W, W, |_| true, 3), PasteStep::Paste));
        assert!(
            matches!(paste_step(W, Some(1), |_| true, 3), PasteStep::Wait),
            "other window active"
        );
        assert!(
            matches!(paste_step(W, W, |_| false, 3), PasteStep::Wait),
            "active but no input focus yet"
        );
        assert!(matches!(paste_step(None, None, |_| true, 5), PasteStep::Wait));
        assert!(matches!(paste_step(None, None, |_| true, 6), PasteStep::Paste));
    }

    #[test]
    fn paste_asks_the_wm_for_focus_once_at_poll_eight() {
        assert!(matches!(paste_step(W, Some(1), |_| true, 8), PasteStep::RestoreFocus));
        assert!(matches!(paste_step(W, Some(1), |_| true, 9), PasteStep::Wait));
        assert!(
            matches!(paste_step(None, None, |_| true, 8), PasteStep::Paste),
            "nothing to restore"
        );
    }

    #[test]
    fn paste_times_out_without_hitting_another_window() {
        let max = PASTE_MAX_POLLS;
        assert!(matches!(paste_step(W, Some(1), |_| true, max - 1), PasteStep::Wait));
        assert!(matches!(paste_step(W, Some(1), |_| true, max), PasteStep::Skip));
        assert!(
            matches!(paste_step(W, W, |_| false, max), PasteStep::Paste),
            "right window, focus unconfirmed"
        );
    }

    #[test]
    fn exit_waits_for_the_read_plus_four_polls() {
        assert!(matches!(exit_step(false, 1, None, false), ExitStep::Wait));
        assert!(matches!(exit_step(false, 2, None, true), ExitStep::MarkRead));
        assert!(matches!(exit_step(false, 5, Some(2), true), ExitStep::Wait));
        assert!(matches!(exit_step(false, 6, Some(2), true), ExitStep::Exit(_)));
    }

    #[test]
    fn exit_gives_up_when_nothing_reads_the_clipboard() {
        assert!(matches!(
            exit_step(false, EXIT_WAIT_MAX_POLLS - 1, None, false),
            ExitStep::Wait
        ));
        assert!(matches!(
            exit_step(false, EXIT_WAIT_MAX_POLLS, None, false),
            ExitStep::Exit(_)
        ));
    }

    #[test]
    fn exit_never_quits_under_a_visible_picker() {
        assert!(matches!(
            exit_step(true, EXIT_WAIT_MAX_POLLS, None, false),
            ExitStep::Stop
        ));
        assert!(matches!(exit_step(true, 99, Some(1), true), ExitStep::Stop));
    }
}
