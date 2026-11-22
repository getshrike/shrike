//! Diagnostics are structured, deterministic records. They are data, not logs.

use serde::{Deserialize, Serialize};

use crate::span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl Diagnostic {
    #[must_use]
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            message: message.into(),
            file: None,
            span: None,
        }
    }

    #[must_use]
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Warning,
            message: message.into(),
            file: None,
            span: None,
        }
    }

    #[must_use]
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Info,
            message: message.into(),
            file: None,
            span: None,
        }
    }

    #[must_use]
    pub fn in_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    #[must_use]
    pub fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}
