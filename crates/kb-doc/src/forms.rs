//! AcroForm filling (architecture §6, task 3A.3): list the fields of a PDF and write values.

use std::collections::HashMap;

use pdfium_render::prelude::*;
use serde::Serialize;

use crate::DocError;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Field {
    /// 0-based page.
    pub page: usize,
    pub name: String,
    /// `text`, `checkbox`, `radio`, `combo`, `list`, `button`, `signature`, `other`.
    pub kind: &'static str,
    pub value: String,
    pub checked: Option<bool>,
}

fn kind_of(f: &PdfFormField) -> &'static str {
    match f.field_type() {
        PdfFormFieldType::Text => "text",
        PdfFormFieldType::Checkbox => "checkbox",
        PdfFormFieldType::RadioButton => "radio",
        PdfFormFieldType::ComboBox => "combo",
        PdfFormFieldType::ListBox => "list",
        PdfFormFieldType::PushButton => "button",
        PdfFormFieldType::Signature => "signature",
        _ => "other",
    }
}

fn err(e: impl std::fmt::Display) -> DocError {
    DocError::Render(e.to_string())
}

pub fn list_fields(pdfium: &Pdfium, data: &[u8]) -> Result<Vec<Field>, DocError> {
    let doc = pdfium
        .load_pdf_from_byte_slice(data, None)
        .map_err(|e| DocError::Open(e.to_string()))?;
    let mut out = Vec::new();
    for (pi, page) in doc.pages().iter().enumerate() {
        for ann in page.annotations().iter() {
            let Some(field) = ann.as_form_field() else {
                continue;
            };
            let value = field
                .as_text_field()
                .and_then(|t| t.value())
                .or_else(|| field.as_combo_box_field().and_then(|c| c.value()))
                .unwrap_or_default();
            let checked = field
                .as_checkbox_field()
                .and_then(|c| c.is_checked().ok())
                .or_else(|| {
                    field
                        .as_radio_button_field()
                        .and_then(|r| r.is_checked().ok())
                });
            out.push(Field {
                page: pi,
                name: field.name().unwrap_or_default(),
                kind: kind_of(field),
                value,
                checked,
            });
        }
    }
    Ok(out)
}

/// Sets text fields (`value`) and checkboxes (`"true"` / `"false"` / `"on"` / `"off"`) by field
/// name and returns the new document. Unknown names are an error (typos must not pass silently).
pub fn fill(
    pdfium: &Pdfium,
    data: &[u8],
    values: &HashMap<String, String>,
) -> Result<Vec<u8>, DocError> {
    let doc = pdfium
        .load_pdf_from_byte_slice(data, None)
        .map_err(|e| DocError::Open(e.to_string()))?;
    let mut done: Vec<&str> = Vec::new();
    for page in doc.pages().iter() {
        for mut ann in page.annotations().iter() {
            let Some(field) = ann.as_form_field_mut() else {
                continue;
            };
            let name = field.name().unwrap_or_default();
            let Some(v) = values.get(&name) else {
                continue;
            };
            if let Some(t) = field.as_text_field_mut() {
                t.set_value(v).map_err(err)?;
            } else if let Some(c) = field.as_checkbox_field_mut() {
                c.set_checked(matches!(
                    v.to_ascii_lowercase().as_str(),
                    "true" | "on" | "1" | "yes"
                ))
                .map_err(err)?;
            } else {
                return Err(DocError::Render(format!("field `{name}` cannot be filled")));
            }
            if let Some(k) = values.keys().find(|k| **k == name) {
                done.push(k);
            }
        }
    }
    if let Some(missing) = values.keys().find(|k| !done.contains(&k.as_str())) {
        return Err(DocError::Render(format!(
            "no fillable field named `{missing}`"
        )));
    }
    doc.save_to_bytes().map_err(err)
}
