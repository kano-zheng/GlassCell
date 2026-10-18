//! Keyboard shortcuts from the engine's command catalog (`CommandInfo::shortcut`, such as
//! `Cmd+Shift+T` or `F2`), matched against browser key events. Pure Rust: tested natively.
//!
//! `Cmd` is the platform's command key: ⌘ on a Mac, Ctrl elsewhere. `Ctrl` is Ctrl itself on a
//! Mac; elsewhere it is the same key as `Cmd`, and a `Cmd` shortcut wins when both want it.

use gridcraft_engine::CommandInfo;

/// A shortcut: the modifiers it needs (exactly these) and its key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// A lowercase character, or the name of a key as `KeyboardEvent.key` has it.
    pub key: String,
}

/// What a key press was, as `KeyboardEvent` reports it.
#[derive(Clone, Debug, Default)]
pub struct Press {
    pub meta: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
    pub code: String,
}

/// Parses `Cmd+Shift+T`, `Ctrl+Space`, `F2`, `Delete`.
pub fn parse(text: &str) -> Option<Chord> {
    if text.is_empty() {
        return None;
    }
    let mut chord = Chord { cmd: false, ctrl: false, alt: false, shift: false, key: String::new() };
    let parts: Vec<&str> = text.split('+').collect();
    // A key that is itself `+` leaves an empty part at the end ("Ctrl+Shift++").
    let mut i = 0;
    while i < parts.len() {
        let part = parts[i];
        let last = i + 1 >= parts.len() || (i + 2 == parts.len() && parts[i + 1].is_empty());
        if last {
            chord.key = if part.is_empty() { "+".into() } else { key_name(part) };
            break;
        }
        match part {
            "Cmd" => chord.cmd = true,
            "Ctrl" => chord.ctrl = true,
            "Alt" => chord.alt = true,
            "Shift" => chord.shift = true,
            _ => return None,
        }
        i += 1;
    }
    (!chord.key.is_empty()).then_some(chord)
}

fn key_name(k: &str) -> String {
    match k {
        "Left" => "ArrowLeft".into(),
        "Right" => "ArrowRight".into(),
        "Up" => "ArrowUp".into(),
        "Down" => "ArrowDown".into(),
        "Space" => " ".into(),
        k if k.chars().count() == 1 => k.to_lowercase(),
        k => k.into(),
    }
}

impl Chord {
    /// The modifiers as (primary, ctrl-too, alt, shift) on this platform: the primary key is ⌘
    /// on a Mac and Ctrl elsewhere.
    fn needs(&self, mac: bool) -> (bool, bool, bool, bool) {
        if mac { (self.cmd, self.ctrl, self.alt, self.shift) } else { (self.cmd || self.ctrl, false, self.alt, self.shift) }
    }

    pub fn matches(&self, p: &Press, mac: bool) -> bool {
        let pressed = if mac { (p.meta, p.ctrl, p.alt, p.shift) } else { (p.ctrl, false, p.alt, p.shift) };
        self.needs(mac) == pressed && key_matches(&self.key, &p.key, &p.code)
    }

    /// Whether this shortcut can only come from a deliberate chord: modifiers, or a function key.
    /// (Plain keys are the grid's own: typing, arrows, Enter, Delete.)
    pub fn is_command(&self) -> bool {
        self.cmd || self.ctrl || self.alt || (self.shift && self.key == " ") || is_function_key(&self.key)
    }
}

fn is_function_key(k: &str) -> bool {
    k.strip_prefix('F').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// A key matches by what it types, or by where it is (a digit or letter key with Shift typed
/// something else: Ctrl+Shift+8 types `*`).
fn key_matches(chord: &str, key: &str, code: &str) -> bool {
    let typed = if key.chars().count() == 1 { key.to_lowercase() } else { key.to_string() };
    if typed == chord {
        return true;
    }
    let mut chars = chord.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return false };
    match c {
        'a'..='z' => code.strip_prefix("Key").is_some_and(|k| k.eq_ignore_ascii_case(chord)),
        '0'..='9' => code.strip_prefix("Digit") == Some(chord),
        '=' => code == "Equal",
        '-' => code == "Minus",
        '`' => code == "Backquote",
        '<' => code == "Comma",
        '>' => code == "Period",
        '%' => code == "Digit5",
        _ => false,
    }
}

/// Commands that stay with the browser's own events (the clipboard) or that don't fit a page.
fn skipped(id: &str) -> bool {
    matches!(id, "edit.copy" | "edit.cut" | "edit.paste" | "edit.pasteSpecial" | "file.new" | "file.close" | "file.print") || id == "edit.beginEdit"
}

/// Every deliberate shortcut of the catalog, `Cmd` ones first.
pub fn table(commands: &[CommandInfo]) -> Vec<(Chord, &'static str)> {
    let mut out: Vec<(Chord, &'static str)> = commands
        .iter()
        .filter(|c| !skipped(c.id))
        .filter_map(|c| Some((parse(c.shortcut?)?, c.id)))
        .filter(|(chord, _)| chord.is_command())
        .collect();
    out.sort_by_key(|(chord, _)| !chord.cmd);
    out
}

/// The command a key press runs.
pub fn lookup(table: &[(Chord, &'static str)], press: &Press, mac: bool) -> Option<&'static str> {
    table.iter().find(|(chord, _)| chord.matches(press, mac)).map(|(_, id)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridcraft_engine::Session;

    fn press(key: &str, code: &str, meta: bool, ctrl: bool, alt: bool, shift: bool) -> Press {
        Press { meta, ctrl, alt, shift, key: key.into(), code: code.into() }
    }

    #[test]
    fn shortcuts_parse() {
        let c = parse("Cmd+Shift+T").expect("parses");
        assert!(c.cmd && c.shift && !c.ctrl && !c.alt);
        assert_eq!(c.key, "t");
        assert_eq!(parse("F2").expect("parses").key, "F2");
        assert_eq!(parse("Ctrl+Space").expect("parses").key, " ");
        assert_eq!(parse("Alt+Shift+Left").expect("parses").key, "ArrowLeft");
        assert_eq!(parse("Ctrl+Shift+=").expect("parses").key, "=");
        assert_eq!(parse("Ctrl+Shift+%").expect("parses").key, "%");
        assert_eq!(parse("Ctrl+`").expect("parses").key, "`");
        assert_eq!(parse("Cmd+-").expect("parses").key, "-");
        assert_eq!(parse("Ctrl+Shift++").expect("parses").key, "+");
        assert_eq!(parse("Bogus+X"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn the_primary_key_is_command_on_a_mac_and_ctrl_elsewhere() {
        let bold = parse("Cmd+B").expect("parses");
        assert!(bold.matches(&press("b", "KeyB", true, false, false, false), true));
        assert!(!bold.matches(&press("b", "KeyB", false, true, false, false), true), "Ctrl+B is not ⌘B on a Mac");
        assert!(bold.matches(&press("b", "KeyB", false, true, false, false), false));
        assert!(!bold.matches(&press("b", "KeyB", true, false, false, false), false));
        assert!(!bold.matches(&press("b", "KeyB", true, false, false, true), true), "Shift too is another chord");
    }

    #[test]
    fn keys_match_by_what_they_type_or_where_they_are() {
        let region = parse("Ctrl+Shift+8").expect("parses");
        // Shift+8 types `*`: the key's place decides.
        assert!(region.matches(&press("*", "Digit8", false, true, false, true), true));
        let percent = parse("Ctrl+Shift+%").expect("parses");
        assert!(percent.matches(&press("%", "Digit5", false, true, false, true), true));
        let bigger = parse("Cmd+Shift+>").expect("parses");
        assert!(bigger.matches(&press(">", "Period", true, false, false, true), true));
        // A capital letter (Caps Lock or Shift) is the same key.
        let sum = parse("Cmd+Shift+T").expect("parses");
        assert!(sum.matches(&press("T", "KeyT", true, false, false, true), true));
    }

    #[test]
    fn the_catalog_gives_a_table_that_finds_commands() {
        let session = Session::new();
        let table = table(&session.commands());
        assert!(table.len() > 30);
        let find = |p: Press, mac| lookup(&table, &p, mac);
        assert_eq!(find(press("b", "KeyB", true, false, false, false), true), Some("home.bold"));
        assert_eq!(find(press("b", "KeyB", false, true, false, false), false), Some("home.bold"));
        assert_eq!(find(press("z", "KeyZ", true, false, false, false), true), Some("edit.undo"));
        assert_eq!(find(press("F9", "F9", false, false, false, false), true), Some("formulas.calculateNow"));
        assert_eq!(find(press(" ", "Space", false, false, false, true), true), Some("selection.row"));
        // The grid's own keys and the clipboard are not in it.
        assert_eq!(find(press("Delete", "Delete", false, false, false, false), true), None);
        assert_eq!(find(press("c", "KeyC", true, false, false, false), true), None);
        assert_eq!(find(press("a", "KeyA", false, false, false, false), true), None, "typing is not a shortcut");
        assert_eq!(find(press("F2", "F2", false, false, false, false), true), None);
    }

    #[test]
    fn on_a_pc_a_command_shortcut_beats_a_ctrl_one_on_the_same_key() {
        // Cmd+E aligns, Ctrl+E is Flash Fill: with one Ctrl key the first wins.
        let session = Session::new();
        let table = table(&session.commands());
        let e = press("e", "KeyE", false, true, false, false);
        assert_eq!(lookup(&table, &e, false), Some("home.alignCenter"));
        assert_eq!(lookup(&table, &press("e", "KeyE", false, true, false, false), true), Some("edit.flashFill"));
    }
}
