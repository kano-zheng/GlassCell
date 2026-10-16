//! Notices for the info bar under the toolbar, in the spirit of Excel's message bar: what an
//! opened file carried that GridCraft handled differently.

use serde::Serialize;
use serde_json::Value as Json;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    Info,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Notice {
    pub text: String,
    pub kind: NoticeKind,
    /// Optional button: `(label, dialog name)`, opening that dialog.
    pub action: Option<(String, String)>,
}

/// The notice for a `file.open` result's `warnings`: the most important one (preserved macros
/// outrank, say, a skipped optional part), or `None` for a clean open (which clears a notice left
/// from the previous document). Any open-time warning can reach the user this way.
pub fn open_notice(result: &Json) -> Option<Notice> {
    let msgs: Vec<&str> = result.get("warnings")?.as_array()?.iter().filter_map(Json::as_str).collect();
    let first = *msgs.first()?;
    if msgs.iter().any(|m| m.to_lowercase().contains("macro") || m.contains("VBA")) {
        return Some(Notice {
            text: "This workbook contains VBA macros. GridCraft keeps them intact when you save, but does not run them.".into(),
            kind: NoticeKind::Warning,
            action: Some(("What this means".into(), "macros".into())),
        });
    }
    let text =
        if msgs.len() == 1 { first.to_string() } else { format!("{} items in this file were handled differently. First: {first}", msgs.len()) };
    Some(Notice { text, kind: NoticeKind::Info, action: None })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn macros_outrank_other_warnings_and_a_clean_open_clears() {
        assert_eq!(open_notice(&json!({})), None);
        assert_eq!(open_notice(&json!({"warnings": []})), None);
        let n = open_notice(&json!({"warnings": ["skipped a part", "VBA project preserved"]})).unwrap();
        assert_eq!(n.kind, NoticeKind::Warning);
        assert_eq!(n.action.as_ref().map(|a| a.1.as_str()), Some("macros"));
        let n = open_notice(&json!({"warnings": ["one", "two"]})).unwrap();
        assert_eq!((n.kind, n.text.as_str()), (NoticeKind::Info, "2 items in this file were handled differently. First: one"));
    }
}
