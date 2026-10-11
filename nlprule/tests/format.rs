//! Tests for the self-describing header on tokenizer and rules binaries.
//!
//! `bincode` has no schema, so without the header a binary written by an older or newer engine
//! deserializes into shifted fields and silently produces wrong results. These tests pin the
//! header contract: the magic, the format version, and the engine version that wrote the file.

use std::convert::TryInto;

use nlprule::{Error, Rules, Tokenizer, RULES_FORMAT_VERSION, TOKENIZER_FORMAT_VERSION};

const TOKENIZER_MAGIC: &[u8; 8] = b"WMBTOK01";
const RULES_MAGIC: &[u8; 8] = b"WMBRUL01";

fn serialize_tokenizer() -> Vec<u8> {
    let mut bytes = Vec::new();
    Tokenizer::default()
        .to_writer(&mut bytes)
        .expect("serializing the default tokenizer");
    bytes
}

fn serialize_rules() -> Vec<u8> {
    let mut bytes = Vec::new();
    Rules::default()
        .to_writer(&mut bytes)
        .expect("serializing the default rules");
    bytes
}

fn expect_format_error<T>(result: Result<T, Error>) -> String {
    match result {
        Ok(_) => panic!("expected the binary to be rejected"),
        Err(Error::Format(message)) => message,
        Err(other) => panic!("expected Error::Format, got {:?}", other),
    }
}

#[test]
fn tokenizer_binary_round_trips() {
    let bytes = serialize_tokenizer();
    assert_eq!(&bytes[..8], TOKENIZER_MAGIC);
    assert_eq!(
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        TOKENIZER_FORMAT_VERSION
    );

    let loaded = Tokenizer::from_reader(bytes.as_slice()).expect("round trip must load");
    assert_eq!(loaded.rules().len(), 0);
    assert!(loaded.chunker().is_none());
}

#[test]
fn rules_binary_round_trips() {
    let bytes = serialize_rules();
    assert_eq!(&bytes[..8], RULES_MAGIC);
    assert_eq!(
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        RULES_FORMAT_VERSION
    );

    let loaded = Rules::from_reader(bytes.as_slice()).expect("round trip must load");
    assert_eq!(loaded.rules().len(), 0);
    assert!(loaded.builtins().is_empty());
}

#[test]
fn corrupted_magic_is_a_format_error() {
    let mut bytes = serialize_tokenizer();
    bytes[0] ^= 0xff;

    let message = expect_format_error(Tokenizer::from_reader(bytes.as_slice()));
    assert!(
        message.contains("not a writemorebetter tokenizer binary"),
        "unexpected message: {}",
        message
    );
}

#[test]
fn a_rules_binary_is_not_a_tokenizer_binary() {
    let bytes = serialize_rules();

    let message = expect_format_error(Tokenizer::from_reader(bytes.as_slice()));
    assert!(
        message.contains("not a writemorebetter tokenizer binary")
            && message.contains("found a rules binary instead"),
        "unexpected message: {}",
        message
    );
}

#[test]
fn a_tokenizer_binary_is_not_a_rules_binary() {
    let bytes = serialize_tokenizer();

    let message = expect_format_error(Rules::from_reader(bytes.as_slice()));
    assert!(
        message.contains("not a writemorebetter rules binary")
            && message.contains("found a tokenizer binary instead"),
        "unexpected message: {}",
        message
    );
}

#[test]
fn a_legacy_headerless_binary_is_rejected() {
    // The pre-header format was a bare bincode payload; its first bytes are a `Vec` length.
    let mut legacy = Vec::new();
    bincode::serialize_into(&mut legacy, &Tokenizer::default()).expect("serialize legacy payload");

    let message = expect_format_error(Tokenizer::from_reader(legacy.as_slice()));
    assert!(
        message.contains("not a writemorebetter tokenizer binary"),
        "unexpected message: {}",
        message
    );
}

#[test]
fn an_empty_file_is_rejected() {
    let message = expect_format_error(Tokenizer::from_reader(&[][..]));
    assert!(
        message.contains("not a writemorebetter tokenizer binary"),
        "unexpected message: {}",
        message
    );
}

#[test]
fn a_mismatched_format_version_is_a_format_error() {
    let mut bytes = serialize_tokenizer();
    let written = TOKENIZER_FORMAT_VERSION + 1;
    bytes[8..12].copy_from_slice(&written.to_le_bytes());

    let message = expect_format_error(Tokenizer::from_reader(bytes.as_slice()));
    assert!(
        message.contains(&TOKENIZER_FORMAT_VERSION.to_string())
            && message.contains(&written.to_string()),
        "message must name both versions: {}",
        message
    );
    assert!(
        message.contains(env!("CARGO_PKG_VERSION")),
        "message must name the engine version that wrote the file: {}",
        message
    );
}

#[test]
fn a_mismatched_rules_format_version_is_a_format_error() {
    let mut bytes = serialize_rules();
    let written = RULES_FORMAT_VERSION + 7;
    bytes[8..12].copy_from_slice(&written.to_le_bytes());

    let message = expect_format_error(Rules::from_reader(bytes.as_slice()));
    assert!(
        message.contains("writemorebetter rules binary format version")
            && message.contains(&written.to_string()),
        "unexpected message: {}",
        message
    );
}
