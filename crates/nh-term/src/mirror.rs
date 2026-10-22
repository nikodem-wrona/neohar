//! State that `alacritty_terminal` keeps but does not let anyone read.
//!
//! `Term` has no getter for its tab stops, its scroll region, the active charset, its title
//! stack and its keyboard mode stacks. `vte::ansi::Processor` keeps the last printed character
//! for `REP` and does not show it. A snapshot must hold all of them. So the engine feeds the
//! processor through [`Tee`], a `Handler` that passes every event to the terminal and records the
//! events that change this state in a [`Mirror`]. The mirror applies the rules that the terminal
//! applies. The exactness check of `nh-snapcheck` compares a restored engine with the engine that
//! continued, so a rule that the mirror gets wrong shows up as a difference.

use crate::history::HistoryTracker;
use alacritty_terminal::{
    event::EventListener,
    grid::Cursor,
    term::{Term, TermMode, cell::Cell},
    vte::ansi::{
        Attr, CharsetIndex, ClearMode, CursorShape, CursorStyle, Handler, Hyperlink, KeyboardModes,
        KeyboardModesApplyBehavior, LineClearMode, Mode, ModifyOtherKeys, NamedPrivateMode,
        PrivateMode, Rgb, ScpCharPath, ScpUpdateMode, StandardCharset, TabulationClearMode,
        cursor_icon::CursorIcon,
    },
};

/// The depth at which `alacritty_terminal` drops the oldest entry of a stack (its
/// `TITLE_STACK_MAX_DEPTH` and `KEYBOARD_MODE_STACK_MAX_DEPTH`).
pub(crate) const STACK_MAX: usize = 4096;

/// The distance between the tab stops of a new terminal.
const TAB_INTERVAL: usize = 8;

/// The mouse modes of xterm that exclude each other. The terminal knows 1000, 1002 and 1003.
const MOUSE_PRIVATE_MODES: [NamedPrivateMode; 3] = [
    NamedPrivateMode::ReportMouseClicks,
    NamedPrivateMode::ReportCellMouseMotion,
    NamedPrivateMode::ReportAllMouseMotion,
];

/// What the terminal does not show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mirror {
    /// One flag per column: a tab stop.
    pub tabs: Vec<bool>,
    /// The scroll region as lines `start..end` of the screen. `end` is not in the region.
    pub region: (i32, i32),
    pub active_charset: CharsetIndex,
    /// The title, as the terminal holds it. An empty title and no title are different here.
    pub title: Option<String>,
    pub title_stack: Vec<Option<String>>,
    /// The kitty keyboard stack of the screen that is on show.
    pub kitty: Vec<KeyboardModes>,
    /// The stack of the other screen. The two swap with the screens.
    pub kitty_inactive: Vec<KeyboardModes>,
    /// The character that `CSI n b` repeats.
    pub last_char: Option<char>,
    /// Mouse mode 9. The terminal ignores it.
    pub mouse_x10: bool,
    /// Mouse encoding 1015. The terminal ignores it.
    pub mouse_urxvt: bool,
    pub modify_other_keys: ModifyOtherKeys,
    /// The saved cursor of the primary screen while the alternate screen is on show, when it
    /// differs from the cursor of the primary screen. Only a resize in the alternate screen makes
    /// them differ. A snapshot reads the saved cursor and sets it to the cursor (the terminal
    /// offers no other way to reach the screen), and a restore cannot set it. The mirror keeps the
    /// value that was lost and writes it back when the screen goes off. A further resize in the
    /// alternate screen changes the saved cursor in ways that only the terminal knows, so it
    /// drops the value.
    pub primary_saved: Option<Cursor<Cell>>,
}

impl Mirror {
    /// The state of a new terminal of this size.
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            tabs: default_tabs(cols),
            region: (0, rows as i32),
            active_charset: CharsetIndex::G0,
            title: None,
            title_stack: Vec::new(),
            kitty: Vec::new(),
            kitty_inactive: Vec::new(),
            last_char: None,
            mouse_x10: false,
            mouse_urxvt: false,
            modify_other_keys: ModifyOtherKeys::Reset,
            primary_saved: None,
        }
    }

    /// A full reset (`ESC c`, `CSI ! p`). The last character stays: the processor keeps it.
    fn reset(&mut self, cols: usize, rows: usize) {
        let last_char = self.last_char;
        *self = Self::new(cols, rows);
        self.last_char = last_char;
    }

    /// The terminal changed size. It ignores a resize that changes nothing.
    pub fn resize(&mut self, from: (usize, usize), to: (usize, usize)) {
        if from == to {
            return;
        }
        self.primary_saved = None;
        if from.0 != to.0 {
            let mut index = self.tabs.len();
            self.tabs.resize_with(to.0, || {
                let stop = index.is_multiple_of(TAB_INTERVAL);
                index += 1;
                stop
            });
        }
        self.region = (0, to.1 as i32);
    }

    /// `CSI top ; bottom r`, with the checks and the clamps of the terminal.
    fn set_region(&mut self, top: usize, bottom: Option<usize>, rows: usize) {
        let bottom = bottom.unwrap_or(rows);
        if top >= bottom {
            return;
        }
        let rows = rows as i32;
        self.region = ((top as i32 - 1).min(rows), (bottom as i32).min(rows));
    }

    fn swap_screens(&mut self) {
        std::mem::swap(&mut self.kitty, &mut self.kitty_inactive);
    }
}

/// The tab stops of a new terminal of `cols` columns.
pub(crate) fn default_tabs(cols: usize) -> Vec<bool> {
    (0..cols).map(|i| i.is_multiple_of(TAB_INTERVAL)).collect()
}

/// A `Handler` that gives every event to the terminal and notes what the mirror needs.
///
/// It also counts the lines that left the screen at the events that make counting
/// impossible later: the alternate screen opens, the scrollback is cleared, the terminal resets.
/// The processor applies a synchronized update in one call, so only the events themselves show
/// where the counts must happen.
pub(crate) struct Tee<'a, T> {
    pub term: &'a mut Term<T>,
    pub mirror: &'a mut Mirror,
    pub history: &'a mut HistoryTracker,
}

impl<T: EventListener> Tee<'_, T> {
    fn column(&self) -> usize {
        self.term.grid().cursor.point.column.0
    }

    fn size(&self) -> (usize, usize) {
        use alacritty_terminal::grid::Dimensions;
        (self.term.columns(), self.term.screen_lines())
    }

    fn alt_screen(&self) -> bool {
        self.term.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Mode 9 and the modes 1000, 1002 and 1003 exclude each other, as in xterm.
    fn set_x10(&mut self, on: bool) {
        if on {
            for mode in MOUSE_PRIVATE_MODES {
                Handler::unset_private_mode(&mut *self.term, PrivateMode::Named(mode));
            }
        }
        self.mirror.mouse_x10 = on;
    }
}

/// Defines the methods of `Handler` that only pass the event on.
macro_rules! forward {
    ($(fn $name:ident(&mut self $(, $arg:ident: $ty:ty)*);)*) => {
        $(
            #[inline]
            fn $name(&mut self $(, $arg: $ty)*) {
                Handler::$name(&mut *self.term $(, $arg)*)
            }
        )*
    };
}

impl<T: EventListener> Handler for Tee<'_, T> {
    fn set_title(&mut self, title: Option<String>) {
        self.mirror.title.clone_from(&title);
        Handler::set_title(&mut *self.term, title);
    }

    #[inline]
    fn input(&mut self, c: char) {
        self.mirror.last_char = Some(c);
        Handler::input(&mut *self.term, c);
    }

    fn set_horizontal_tabstop(&mut self) {
        let column = self.column();
        if let Some(stop) = self.mirror.tabs.get_mut(column) {
            *stop = true;
        }
        Handler::set_horizontal_tabstop(&mut *self.term);
    }

    fn clear_tabs(&mut self, mode: TabulationClearMode) {
        match mode {
            TabulationClearMode::Current => {
                let column = self.column();
                if let Some(stop) = self.mirror.tabs.get_mut(column) {
                    *stop = false;
                }
            }
            TabulationClearMode::All => self.mirror.tabs.fill(false),
        }
        Handler::clear_tabs(&mut *self.term, mode);
    }

    fn reset_state(&mut self) {
        self.history.account(self.term);
        Handler::reset_state(&mut *self.term);
        let (cols, rows) = self.size();
        self.mirror.reset(cols, rows);
    }

    fn set_private_mode(&mut self, mode: PrivateMode) {
        match mode {
            PrivateMode::Named(NamedPrivateMode::SwapScreenAndSetRestoreCursor) => {
                if !self.alt_screen() {
                    // The history of the primary screen cannot be counted while the other
                    // screen is on show.
                    self.history.account(self.term);
                    self.mirror.swap_screens();
                    // Opening the screen sets the saved cursor to the cursor, as a mirror says.
                    self.mirror.primary_saved = None;
                }
            }
            PrivateMode::Named(NamedPrivateMode::ColumnMode) => {
                let rows = self.size().1;
                self.mirror.set_region(1, None, rows);
            }
            PrivateMode::Named(
                NamedPrivateMode::ReportMouseClicks
                | NamedPrivateMode::ReportCellMouseMotion
                | NamedPrivateMode::ReportAllMouseMotion,
            ) => self.mirror.mouse_x10 = false,
            PrivateMode::Unknown(9) => self.set_x10(true),
            PrivateMode::Unknown(1015) => self.mirror.mouse_urxvt = true,
            _ => {}
        }
        Handler::set_private_mode(&mut *self.term, mode);
    }

    fn unset_private_mode(&mut self, mode: PrivateMode) {
        let mut leaving = false;
        match mode {
            PrivateMode::Named(NamedPrivateMode::SwapScreenAndSetRestoreCursor) => {
                if self.alt_screen() {
                    self.mirror.swap_screens();
                    leaving = true;
                }
            }
            PrivateMode::Named(NamedPrivateMode::ColumnMode) => {
                let rows = self.size().1;
                self.mirror.set_region(1, None, rows);
            }
            PrivateMode::Unknown(9) => self.mirror.mouse_x10 = false,
            PrivateMode::Unknown(1015) => self.mirror.mouse_urxvt = false,
            _ => {}
        }
        Handler::unset_private_mode(&mut *self.term, mode);
        if leaving {
            // The primary screen is on show again. Count from its history, which a resize in the
            // alternate screen may have changed.
            self.history.account(self.term);
            if let Some(saved) = self.mirror.primary_saved.take() {
                // Its saved cursor can be written now.
                self.term.grid_mut().saved_cursor = saved;
            }
        }
    }

    fn set_scrolling_region(&mut self, top: usize, bottom: Option<usize>) {
        let rows = self.size().1;
        self.mirror.set_region(top, bottom, rows);
        Handler::set_scrolling_region(&mut *self.term, top, bottom);
    }

    fn set_active_charset(&mut self, index: CharsetIndex) {
        self.mirror.active_charset = index;
        Handler::set_active_charset(&mut *self.term, index);
    }

    fn push_title(&mut self) {
        if self.mirror.title_stack.len() >= STACK_MAX {
            self.mirror.title_stack.remove(0);
        }
        let title = self.mirror.title.clone();
        self.mirror.title_stack.push(title);
        Handler::push_title(&mut *self.term);
    }

    fn pop_title(&mut self) {
        if let Some(popped) = self.mirror.title_stack.pop() {
            self.mirror.title = popped;
        }
        Handler::pop_title(&mut *self.term);
    }

    fn push_keyboard_mode(&mut self, mode: KeyboardModes) {
        // At the maximum depth the terminal removes the oldest entry of its title stack, where it
        // means its keyboard stack (a bug of `alacritty_terminal` 0.26). It panics when the title
        // stack is empty. A title that we push first is the entry that goes, so that nothing
        // changes. The mirror does what the terminal does.
        if self.mirror.kitty.len() >= STACK_MAX {
            if self.mirror.title_stack.is_empty() {
                Handler::push_title(&mut *self.term);
            } else {
                self.mirror.title_stack.remove(0);
            }
        }
        self.mirror.kitty.push(mode);
        Handler::push_keyboard_mode(&mut *self.term, mode);
    }

    fn pop_keyboard_modes(&mut self, to_pop: u16) {
        let keep = self.mirror.kitty.len().saturating_sub(usize::from(to_pop));
        self.mirror.kitty.truncate(keep);
        Handler::pop_keyboard_modes(&mut *self.term, to_pop);
    }

    fn clear_screen(&mut self, mode: ClearMode) {
        if matches!(mode, ClearMode::Saved) {
            self.history.account(self.term);
        }
        Handler::clear_screen(&mut *self.term, mode);
    }

    fn set_modify_other_keys(&mut self, mode: ModifyOtherKeys) {
        self.mirror.modify_other_keys = mode;
        Handler::set_modify_other_keys(&mut *self.term, mode);
    }

    forward! {
        fn set_cursor_style(&mut self, style: Option<CursorStyle>);
        fn set_cursor_shape(&mut self, shape: CursorShape);
        fn goto(&mut self, line: i32, col: usize);
        fn goto_line(&mut self, line: i32);
        fn goto_col(&mut self, col: usize);
        fn insert_blank(&mut self, count: usize);
        fn move_up(&mut self, count: usize);
        fn move_down(&mut self, count: usize);
        fn identify_terminal(&mut self, intermediate: Option<char>);
        fn device_status(&mut self, arg: usize);
        fn move_forward(&mut self, col: usize);
        fn move_backward(&mut self, col: usize);
        fn move_down_and_cr(&mut self, row: usize);
        fn move_up_and_cr(&mut self, row: usize);
        fn put_tab(&mut self, count: u16);
        fn backspace(&mut self);
        fn carriage_return(&mut self);
        fn linefeed(&mut self);
        fn bell(&mut self);
        fn substitute(&mut self);
        fn newline(&mut self);
        fn scroll_up(&mut self, count: usize);
        fn scroll_down(&mut self, count: usize);
        fn insert_blank_lines(&mut self, count: usize);
        fn delete_lines(&mut self, count: usize);
        fn erase_chars(&mut self, count: usize);
        fn delete_chars(&mut self, count: usize);
        fn move_backward_tabs(&mut self, count: u16);
        fn move_forward_tabs(&mut self, count: u16);
        fn save_cursor_position(&mut self);
        fn restore_cursor_position(&mut self);
        fn clear_line(&mut self, mode: LineClearMode);
        fn set_tabs(&mut self, interval: u16);
        fn reverse_index(&mut self);
        fn terminal_attribute(&mut self, attr: Attr);
        fn set_mode(&mut self, mode: Mode);
        fn unset_mode(&mut self, mode: Mode);
        fn report_mode(&mut self, mode: Mode);
        fn report_private_mode(&mut self, mode: PrivateMode);
        fn set_keypad_application_mode(&mut self);
        fn unset_keypad_application_mode(&mut self);
        fn configure_charset(&mut self, index: CharsetIndex, charset: StandardCharset);
        fn set_color(&mut self, index: usize, color: Rgb);
        fn dynamic_color_sequence(&mut self, prefix: String, index: usize, terminator: &str);
        fn reset_color(&mut self, index: usize);
        fn clipboard_store(&mut self, clipboard: u8, base64: &[u8]);
        fn clipboard_load(&mut self, clipboard: u8, terminator: &str);
        fn decaln(&mut self);
        fn text_area_size_pixels(&mut self);
        fn text_area_size_chars(&mut self);
        fn set_hyperlink(&mut self, hyperlink: Option<Hyperlink>);
        fn set_mouse_cursor_icon(&mut self, icon: CursorIcon);
        fn report_keyboard_mode(&mut self);
        fn set_keyboard_mode(&mut self, mode: KeyboardModes, behavior: KeyboardModesApplyBehavior);
        fn report_modify_other_keys(&mut self);
        fn set_scp(&mut self, char_path: ScpCharPath, update_mode: ScpUpdateMode);
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    /// The names of the methods of `vte::ansi::Handler`, read from the source of the `vte` crate
    /// that the build uses.
    fn handler_methods() -> Vec<String> {
        let output = Command::new(env!("CARGO"))
            .args(["metadata", "--format-version", "1", "--locked", "--offline"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("cargo metadata runs");
        assert!(output.status.success(), "cargo metadata failed");
        let text = String::from_utf8(output.stdout).expect("metadata is UTF-8");
        // The metadata is JSON, and the vte entry holds `"name":"vte"` and its manifest path. A
        // JSON parser would be a new dependency, so read the one field with a search.
        let vte = text
            .match_indices("\"name\":\"vte\"")
            .map(|(i, _)| i)
            .find(|&i| text[i..].starts_with("\"name\":\"vte\",\"version\":\"0.15"))
            .expect("the vte crate is in the metadata");
        let manifest = text[vte..]
            .split("\"manifest_path\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("the vte entry has a manifest path");
        let source = std::fs::read_to_string(manifest.replace("Cargo.toml", "src/ansi.rs"))
            .expect("the source of vte is there");
        let start = source
            .find("pub trait Handler")
            .expect("the Handler trait is in ansi.rs");
        let body = &source[start..];
        let end = body.find("\n}\n").expect("the trait ends");
        body[..end]
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                let rest = line.strip_prefix("fn ")?;
                Some(rest.split('(').next()?.to_owned())
            })
            .collect()
    }

    /// `Tee` must define every method of `Handler`. A method that it leaves out falls back to the
    /// empty default, and the terminal never sees the event. This test fails when a new version of
    /// `vte` adds a method.
    #[test]
    fn the_tee_defines_every_method_of_the_handler_trait() {
        let source = include_str!("mirror.rs");
        let missing: Vec<_> = handler_methods()
            .into_iter()
            .filter(|name| !source.contains(&format!("fn {name}(&mut self")))
            .collect();
        assert!(
            missing.is_empty(),
            "Tee lacks these methods of vte::ansi::Handler: {missing:?}"
        );
        assert!(
            handler_methods().len() >= 70,
            "the method list is too short"
        );
    }

    #[test]
    fn the_region_follows_the_checks_of_the_terminal() {
        let mut m = super::Mirror::new(80, 24);
        m.set_region(5, Some(10), 24);
        assert_eq!(m.region, (4, 10));
        // top >= bottom is ignored.
        m.set_region(12, Some(12), 24);
        assert_eq!(m.region, (4, 10));
        // Values past the screen clamp to its height.
        m.set_region(30, Some(40), 24);
        assert_eq!(m.region, (24, 24));
        m.set_region(1, None, 24);
        assert_eq!(m.region, (0, 24));
    }

    #[test]
    fn tab_stops_follow_a_change_of_width() {
        let mut m = super::Mirror::new(10, 5);
        m.tabs[3] = true;
        m.tabs[8] = false;
        m.resize((10, 5), (20, 5));
        let stops: Vec<_> = (0..20).filter(|&i| m.tabs[i]).collect();
        assert_eq!(stops, [0, 3, 16]);
        m.resize((20, 5), (12, 5));
        assert_eq!(m.tabs.len(), 12);
        // No change of size changes nothing, not even the region.
        m.region = (1, 3);
        m.resize((12, 5), (12, 5));
        assert_eq!(m.region, (1, 3));
        m.resize((12, 5), (12, 7));
        assert_eq!(m.region, (0, 7));
    }
}
