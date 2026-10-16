use std::fmt;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[non_exhaustive]
pub enum StyleError {
    InvalidOptions { message: String },
    InvalidBaseline { report: String, message: String },
    NeedsTokenDetails { report: String },
    MissingTable { report: String },
    InvalidTable { report: String, message: String },
}

impl StyleError {
    pub(crate) fn options(message: impl fmt::Display) -> Self {
        Self::InvalidOptions {
            message: message.to_string(),
        }
    }

    pub(crate) fn baseline(report: &str, message: impl fmt::Display) -> Self {
        Self::InvalidBaseline {
            report: report.to_owned(),
            message: message.to_string(),
        }
    }

    pub(crate) fn table(report: &str, message: impl fmt::Display) -> Self {
        Self::InvalidTable {
            report: report.to_owned(),
            message: message.to_string(),
        }
    }

    pub(crate) fn missing_table(report: &str) -> Self {
        Self::MissingTable {
            report: report.to_owned(),
        }
    }

    pub(crate) fn needs_token_details(report: &str) -> Self {
        Self::NeedsTokenDetails {
            report: report.to_owned(),
        }
    }
}

impl fmt::Display for StyleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions { message } => write!(f, "invalid options: {message}"),
            Self::InvalidBaseline { report, message } => {
                write!(f, "invalid {report} baseline: {message}")
            }
            Self::NeedsTokenDetails { report } => write!(
                f,
                "{report} needs token readings; analyze the document with include_token_details"
            ),
            Self::MissingTable { report } => write!(
                f,
                "{report} needs a caller-supplied word table (a JSON object or TSV of word and rating); none was given"
            ),
            Self::InvalidTable { report, message } => {
                write!(f, "invalid {report} table: {message}")
            }
        }
    }
}

impl std::error::Error for StyleError {}

pub(crate) fn percent_in_range(report: &str, name: &str, value: f64) -> Result<(), StyleError> {
    if value.is_finite() && (0.0..=100.0).contains(&value) {
        Ok(())
    } else {
        Err(StyleError::baseline(
            report,
            format!("{name} must be a percentage in 0..=100, got {value}"),
        ))
    }
}
