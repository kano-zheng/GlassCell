//! Pasting with a system clipboard: whether the text there is newer than GridCraft's own copy,
//! and Excel's rule that a cut pastes once.
//!
//! The frontend reads and writes the system clipboard; [`Session::host_clipboard`] remembers
//! what it held around GridCraft's copies and decides what a paste pastes.
//!
//! [`Session::host_clipboard`]: crate::Session::host_clipboard

/// What a paste should paste.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Paste {
    /// GridCraft's own clipboard: `edit.paste` without text.
    Internal,
    /// This system clipboard text (with its HTML flavour, when the frontend can read it).
    Text(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostClipboard {
    /// The system clipboard's text when GridCraft last copied without publishing to it (a
    /// control-channel or MCP `edit.copy`/`edit.cut`). While the system clipboard still holds
    /// this text, GridCraft's own copy is the newer one.
    text_at_copy: Option<String>,
    /// The system clipboard's text right after GridCraft's last cut (the published text, or the
    /// text noted for an unpublished cut); `None` after a copy, or once the clipboard is seen
    /// holding other text.
    cut_text: Option<String>,
}

impl HostClipboard {
    /// GridCraft put `text` on the system clipboard for a copy (or a cut).
    pub fn published(&mut self, cut: bool, text: &str) {
        self.text_at_copy = None;
        self.cut_text = cut.then(|| normalize(text));
    }

    /// GridCraft copied (or cut) without touching the system clipboard, which holds `system`.
    pub fn unpublished(&mut self, cut: bool, system: Option<&str>) {
        self.text_at_copy = system.map(normalize).filter(|t| !t.is_empty());
        self.cut_text = if cut { self.text_at_copy.clone() } else { None };
    }

    /// Pasting `text` from the system clipboard (Cmd/Ctrl+V); `internal` = GridCraft's own
    /// clipboard holds something.
    ///
    /// Excel pastes a cut once: after GridCraft's cut has been pasted (which empties its own
    /// clipboard), the text that came with it is not pasted again over the moved block; the
    /// empty internal clipboard is pasted instead, which reports "Nothing to paste.". Other text
    /// means the clipboard has moved on, so the cut's text is forgotten.
    pub fn paste_text(&mut self, text: String, internal: bool) -> Paste {
        if same_text(&text, self.cut_text.as_deref()) {
            if !internal {
                return Paste::Internal;
            }
        } else {
            self.cut_text = None;
        }
        Paste::Text(text)
    }

    /// The Paste command (menu, toolbar): pastes the system clipboard's text (`system`) as
    /// Cmd/Ctrl+V does. With no text there, or the text that was there when GridCraft last
    /// copied without publishing, GridCraft's own clipboard is pasted.
    pub fn paste_command(&mut self, system: Option<&str>, internal: bool) -> Paste {
        match system.map(normalize).filter(|t| !t.is_empty()) {
            Some(text) if !internal || self.text_at_copy.as_ref() != Some(&text) => self.paste_text(text, internal),
            _ => Paste::Internal,
        }
    }
}

/// Line endings as text pasted into the grid has them.
fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Whether two clipboard texts are the same copy: trailing line breaks don't count, as in
/// `edit.paste`, and CRLF matches LF.
fn same_text(a: &str, b: Option<&str>) -> bool {
    fn chars(t: &str) -> impl Iterator<Item = char> + '_ {
        t.trim_end_matches(['\r', '\n']).chars().filter(|c| *c != '\r')
    }
    b.is_some_and(|b| chars(a).eq(chars(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_published_cut_pastes_once() {
        let mut h = HostClipboard::default();
        h.published(true, "a\tb\r\n");
        // First paste: GridCraft's clipboard still holds the cut, so the text is pasted.
        assert_eq!(h.paste_text("a\tb\n".into(), true), Paste::Text("a\tb\n".into()));
        // The cut was pasted (internal clipboard emptied): the same text pastes "nothing".
        assert_eq!(h.paste_text("a\tb".into(), false), Paste::Internal);
        // Other text: the clipboard moved on.
        assert_eq!(h.paste_text("x".into(), false), Paste::Text("x".into()));
        assert_eq!(h.paste_text("a\tb".into(), false), Paste::Text("a\tb".into()));
    }

    #[test]
    fn an_unpublished_copy_wins_while_the_clipboard_is_unchanged() {
        let mut h = HostClipboard::default();
        h.unpublished(false, Some("older"));
        assert_eq!(h.paste_command(Some("older"), true), Paste::Internal);
        assert_eq!(h.paste_command(Some("newer"), true), Paste::Text("newer".into()));
        assert_eq!(h.paste_command(None, true), Paste::Internal);
        assert_eq!(h.paste_command(Some(""), false), Paste::Internal);
        // Without an internal copy, whatever the system clipboard has is pasted.
        assert_eq!(h.paste_command(Some("older"), false), Paste::Text("older".into()));
    }
}
