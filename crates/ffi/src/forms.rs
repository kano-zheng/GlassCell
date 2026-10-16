//! The engine's form dialogs for Swift: a `FormSession` holds one open form (its values, and
//! fields that follow them), and gives OK its command and parameters.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gridcraft_engine::forms::{self, Field, Form};
use serde_json::{Value as Json, json};

use crate::{Engine, FfiError, engine_error, guard};

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FormOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FormField {
    Text { key: String, label: String },
    Password { key: String, label: String },
    Number { key: String, label: String },
    Choice { key: String, label: String, options: Vec<FormOption> },
    Check { key: String, label: String },
    Note { text: String },
}

impl From<&Field> for FormField {
    fn from(f: &Field) -> Self {
        match f {
            Field::Text { key, label } => FormField::Text { key: (*key).into(), label: (*label).into() },
            Field::Password { key, label } => FormField::Password { key: (*key).into(), label: (*label).into() },
            Field::Number { key, label } => FormField::Number { key: (*key).into(), label: (*label).into() },
            Field::Choice { key, label, options } => FormField::Choice {
                key: (*key).into(),
                label: (*label).into(),
                options: options.iter().map(|(v, l)| FormOption { value: (*v).into(), label: (*l).into() }).collect(),
            },
            Field::Check { key, label } => FormField::Check { key: (*key).into(), label: (*label).into() },
            Field::Note(text) => FormField::Note { text: text.clone() },
        }
    }
}

/// One open form dialog.
#[derive(uniffi::Object)]
pub struct FormSession {
    form: Mutex<Form>,
}

impl FormSession {
    fn form(&self) -> MutexGuard<'_, Form> {
        self.form.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[uniffi::export]
impl FormSession {
    pub fn name(&self) -> String {
        self.form().name.clone()
    }
    pub fn title(&self) -> String {
        self.form().title.clone()
    }
    /// The command OK runs, with `params()`.
    pub fn command(&self) -> String {
        self.form().command.to_string()
    }
    pub fn fields(&self) -> Vec<FormField> {
        self.form().fields.iter().map(FormField::from).collect()
    }
    /// A value as text (numbers formatted, empty when unset).
    pub fn text(&self, key: String) -> String {
        match self.form().values.get(&key) {
            Some(Json::String(s)) => s.clone(),
            Some(Json::Number(n)) => n.to_string(),
            Some(Json::Bool(b)) => b.to_string(),
            _ => String::new(),
        }
    }
    pub fn flag(&self, key: String) -> bool {
        self.form().values.get(&key).and_then(Json::as_bool).unwrap_or(false)
    }
    pub fn set_text(&self, key: String, value: String) -> Result<(), FfiError> {
        guard(|| {
            forms::set_value(&mut self.form(), &key, json!(value));
            Ok(())
        })
    }
    /// A number field's value; text that isn't a number is kept as text (the command says so).
    pub fn set_number(&self, key: String, value: String) -> Result<(), FfiError> {
        guard(|| {
            let v = value.trim().parse::<f64>().ok().filter(|n| n.is_finite()).map_or(json!(value), |n| json!(n));
            forms::set_value(&mut self.form(), &key, v);
            Ok(())
        })
    }
    pub fn set_flag(&self, key: String, value: bool) -> Result<(), FfiError> {
        guard(|| {
            forms::set_value(&mut self.form(), &key, json!(value));
            Ok(())
        })
    }
    /// The command's parameters for the values now (JSON).
    pub fn params(&self) -> Result<String, FfiError> {
        guard(|| {
            let f = self.form();
            Ok(forms::build_params(&f.name, &f.values).to_string())
        })
    }
}

#[uniffi::export]
impl Engine {
    /// The form for dialog `name` (a `UiRequest::Dialog`) with its JSON `params`, or `None` when
    /// it isn't a plain form.
    pub fn open_form(&self, name: String, params: String) -> Result<Option<Arc<FormSession>>, FfiError> {
        guard(|| {
            let params: Json = if params.trim().is_empty() { json!({}) } else { serde_json::from_str(&params).map_err(engine_error)? };
            let mut s = self.session();
            Ok(forms::open(&mut s, &name, &params).map(|form| Arc::new(FormSession { form: Mutex::new(form) })))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_opens_edits_and_gives_its_command() {
        let e = Engine::new();
        e.execute("file.new".into(), String::new()).unwrap();
        let f = e.open_form("rowHeight".into(), "{}".into()).unwrap().unwrap();
        assert_eq!(f.command(), "home.rowHeight");
        assert!(matches!(f.fields().first(), Some(FormField::Number { key, .. }) if key == "height"));
        f.set_number("height".into(), "31".into()).unwrap();
        let params = f.params().unwrap();
        e.execute(f.command(), params).unwrap();
        assert!(e.open_form("find".into(), String::new()).unwrap().is_none());
        assert!(e.open_form("rowHeight".into(), "{oops".into()).is_err());
        let dv = e.open_form("dataValidation".into(), String::new()).unwrap().unwrap();
        dv.set_text("type".into(), "whole".into()).unwrap();
        assert!(dv.fields().iter().any(|x| matches!(x, FormField::Choice { key, .. } if key == "operator")));
    }
}
