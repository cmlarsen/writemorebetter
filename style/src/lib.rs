//! Whole-document prose analysis, separate from grammar suggestions and LGPL rule data.
//!
//! The caller supplies paragraphs and one already-loaded tokenizer. Reports consume the
//! resulting [`Document`] without retokenizing. Baselines belong to the caller.

pub mod document;
pub mod reports;

pub use document::{analyze_document, Document, Paragraph, TokenPosition};
