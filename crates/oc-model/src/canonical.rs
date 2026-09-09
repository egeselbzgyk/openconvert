//! Canonical JSON serialisation of the IR (D13.3, ARCHITECTURE §4.3).
//!
//! Keys sorted · `ir_version` first · arrays in document order · strings NFC · no NaN/Inf
//! (a serialisation error, never a silent `null`) · `f32` printed with two decimals at
//! serialisation only, so geometry keeps full precision in memory (RT B5).
//!
//! Why a bespoke serialiser rather than `serde_json`: `serde_json` turns a non-finite
//! float into `null`. Silent `null` where a coordinate belongs is precisely the failure
//! ARCHITECTURE §4.3 rules out, and no formatter hook can distinguish it from a real
//! `None` after the fact. Key ordering also cannot be imposed by a streaming formatter,
//! so the value tree is built first and written second.

use std::collections::BTreeMap;
use std::fmt::{Display, Write as _};

use serde::{ser, Serialize};
use unicode_normalization::UnicodeNormalization;

/// The key that is emitted before all others, whatever its sort position.
const IR_VERSION_KEY: &str = "ir_version";

/// Decimal places used when printing an `f32`. Geometry is `f32` throughout the IR and
/// 0.01 pt is the documented serialisation precision (ARCHITECTURE §4.3).
const F32_DECIMALS: usize = 2;

/// Why a value could not be written as canonical JSON.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CanonError {
    /// A NaN or an infinity reached serialisation. Never written as `null`.
    #[error("canonical JSON cannot contain a non-finite float (NaN or infinity)")]
    NonFinite,
    /// A map was keyed by something that is not a string, so the keys cannot be sorted
    /// into a stable order.
    #[error("canonical JSON map keys must be strings")]
    KeyNotString,
    /// An error raised by the value's own `Serialize` implementation.
    #[error("{0}")]
    Message(String),
}

impl ser::Error for CanonError {
    fn custom<T: Display>(msg: T) -> Self {
        CanonError::Message(msg.to_string())
    }
}

/// Serialise a value as canonical JSON: compact, UTF-8, no BOM, no trailing newline.
///
/// Compact rather than indented because the point of the canonical form is byte-identical
/// output for the same input (D13.8); presentation is a separate concern.
pub fn to_canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<String, CanonError> {
    let tree = value.serialize(CanonSerializer)?;
    let mut out = String::new();
    write_value(&tree, &mut out);
    Ok(out)
}

// ---------------------------------------------------------------------------
// The canonical value tree: sorted, NFC, and finite by construction.
// ---------------------------------------------------------------------------

enum Canon {
    Null,
    Bool(bool),
    /// Already formatted. Keeping the text rather than the number is what makes the
    /// writer total and the output byte-stable.
    Number(String),
    Str(String),
    Array(Vec<Canon>),
    Object(BTreeMap<String, Canon>),
}

fn write_value(value: &Canon, out: &mut String) {
    match value {
        Canon::Null => out.push_str("null"),
        Canon::Bool(true) => out.push_str("true"),
        Canon::Bool(false) => out.push_str("false"),
        Canon::Number(text) => out.push_str(text),
        Canon::Str(text) => write_string(text, out),
        Canon::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Canon::Object(members) => {
            out.push('{');
            let mut first = true;
            if let Some(version) = members.get(IR_VERSION_KEY) {
                write_member(IR_VERSION_KEY, version, &mut first, out);
            }
            for (key, member) in members {
                if key != IR_VERSION_KEY {
                    write_member(key, member, &mut first, out);
                }
            }
            out.push('}');
        }
    }
}

fn write_member(key: &str, value: &Canon, first: &mut bool, out: &mut String) {
    if !*first {
        out.push(',');
    }
    *first = false;
    write_string(key, out);
    out.push(':');
    write_value(value, out);
}

/// Write a JSON string. Non-ASCII characters are written literally: the document is UTF-8
/// and already NFC, so escaping them would only make the output larger and less readable.
fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            control if control.is_control() => {
                // `write!` into a `String` is infallible; the formatter has nowhere to fail.
                let _ = write!(out, "\\u{:04x}", u32::from(control));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

/// Collapse `-0.0` to `0.0` so the sign of a zero cannot make two equal values print
/// differently.
fn without_negative_zero<F: PartialEq + Default + Copy>(value: F) -> F {
    if value == F::default() {
        F::default()
    } else {
        value
    }
}

fn format_f32(value: f32) -> Result<Canon, CanonError> {
    if !value.is_finite() {
        return Err(CanonError::NonFinite);
    }
    Ok(Canon::Number(format!(
        "{:.*}",
        F32_DECIMALS,
        without_negative_zero(value)
    )))
}

fn format_f64(value: f64) -> Result<Canon, CanonError> {
    if !value.is_finite() {
        return Err(CanonError::NonFinite);
    }
    // `to_string` gives the shortest text that round-trips, so it is both exact and stable.
    let mut text = without_negative_zero(value).to_string();
    if !text.contains(['.', 'e', 'E']) {
        text.push_str(".0");
    }
    Ok(Canon::Number(text))
}

fn nfc(text: &str) -> String {
    text.nfc().collect()
}

// ---------------------------------------------------------------------------
// The serialiser
// ---------------------------------------------------------------------------

struct CanonSerializer;

/// Collects a sequence, a tuple, or a tuple struct.
struct SeqBuilder {
    items: Vec<Canon>,
}

/// Collects a tuple variant, which is written as `{"Variant": [..]}`.
struct TupleVariantBuilder {
    variant: &'static str,
    items: Vec<Canon>,
}

/// Collects a map or a struct. `pending_key` holds the key between the two calls serde
/// makes for a map entry.
struct MapBuilder {
    members: BTreeMap<String, Canon>,
    pending_key: Option<String>,
}

/// Collects a struct variant, which is written as `{"Variant": {..}}`.
struct StructVariantBuilder {
    variant: &'static str,
    members: BTreeMap<String, Canon>,
}

impl ser::Serializer for CanonSerializer {
    type Ok = Canon;
    type Error = CanonError;
    type SerializeSeq = SeqBuilder;
    type SerializeTuple = SeqBuilder;
    type SerializeTupleStruct = SeqBuilder;
    type SerializeTupleVariant = TupleVariantBuilder;
    type SerializeMap = MapBuilder;
    type SerializeStruct = MapBuilder;
    type SerializeStructVariant = StructVariantBuilder;

    fn serialize_bool(self, v: bool) -> Result<Canon, CanonError> {
        Ok(Canon::Bool(v))
    }

    fn serialize_i8(self, v: i8) -> Result<Canon, CanonError> {
        Ok(Canon::Number(v.to_string()))
    }

    fn serialize_i16(self, v: i16) -> Result<Canon, CanonError> {
        Ok(Canon::Number(v.to_string()))
    }

    fn serialize_i32(self, v: i32) -> Result<Canon, CanonError> {
        Ok(Canon::Number(v.to_string()))
    }

    fn serialize_i64(self, v: i64) -> Result<Canon, CanonError> {
        Ok(Canon::Number(v.to_string()))
    }

    fn serialize_i128(self, v: i128) -> Result<Canon, CanonError> {
        Ok(Canon::Number(v.to_string()))
    }

    fn serialize_u8(self, v: u8) -> Result<Canon, CanonError> {
        Ok(Canon::Number(v.to_string()))
    }

    fn serialize_u16(self, v: u16) -> Result<Canon, CanonError> {
        Ok(Canon::Number(v.to_string()))
    }

    fn serialize_u32(self, v: u32) -> Result<Canon, CanonError> {
        Ok(Canon::Number(v.to_string()))
    }

    fn serialize_u64(self, v: u64) -> Result<Canon, CanonError> {
        Ok(Canon::Number(v.to_string()))
    }

    fn serialize_u128(self, v: u128) -> Result<Canon, CanonError> {
        Ok(Canon::Number(v.to_string()))
    }

    fn serialize_f32(self, v: f32) -> Result<Canon, CanonError> {
        format_f32(v)
    }

    fn serialize_f64(self, v: f64) -> Result<Canon, CanonError> {
        format_f64(v)
    }

    fn serialize_char(self, v: char) -> Result<Canon, CanonError> {
        Ok(Canon::Str(nfc(v.encode_utf8(&mut [0u8; 4]))))
    }

    fn serialize_str(self, v: &str) -> Result<Canon, CanonError> {
        Ok(Canon::Str(nfc(v)))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Canon, CanonError> {
        Ok(Canon::Array(
            v.iter()
                .map(|byte| Canon::Number(byte.to_string()))
                .collect(),
        ))
    }

    fn serialize_none(self) -> Result<Canon, CanonError> {
        Ok(Canon::Null)
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Canon, CanonError> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Canon, CanonError> {
        Ok(Canon::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Canon, CanonError> {
        Ok(Canon::Null)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<Canon, CanonError> {
        Ok(Canon::Str(nfc(variant)))
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Canon, CanonError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Canon, CanonError> {
        let mut members = BTreeMap::new();
        members.insert(nfc(variant), value.serialize(CanonSerializer)?);
        Ok(Canon::Object(members))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<SeqBuilder, CanonError> {
        Ok(SeqBuilder {
            items: Vec::with_capacity(len.unwrap_or_default()),
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<SeqBuilder, CanonError> {
        Ok(SeqBuilder {
            items: Vec::with_capacity(len),
        })
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<SeqBuilder, CanonError> {
        Ok(SeqBuilder {
            items: Vec::with_capacity(len),
        })
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<TupleVariantBuilder, CanonError> {
        Ok(TupleVariantBuilder {
            variant,
            items: Vec::with_capacity(len),
        })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<MapBuilder, CanonError> {
        Ok(MapBuilder {
            members: BTreeMap::new(),
            pending_key: None,
        })
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<MapBuilder, CanonError> {
        Ok(MapBuilder {
            members: BTreeMap::new(),
            pending_key: None,
        })
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<StructVariantBuilder, CanonError> {
        Ok(StructVariantBuilder {
            variant,
            members: BTreeMap::new(),
        })
    }
}

impl ser::SerializeSeq for SeqBuilder {
    type Ok = Canon;
    type Error = CanonError;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CanonError> {
        self.items.push(value.serialize(CanonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Canon, CanonError> {
        Ok(Canon::Array(self.items))
    }
}

impl ser::SerializeTuple for SeqBuilder {
    type Ok = Canon;
    type Error = CanonError;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CanonError> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Canon, CanonError> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleStruct for SeqBuilder {
    type Ok = Canon;
    type Error = CanonError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CanonError> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Canon, CanonError> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleVariant for TupleVariantBuilder {
    type Ok = Canon;
    type Error = CanonError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CanonError> {
        self.items.push(value.serialize(CanonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Canon, CanonError> {
        let mut members = BTreeMap::new();
        members.insert(nfc(self.variant), Canon::Array(self.items));
        Ok(Canon::Object(members))
    }
}

impl ser::SerializeMap for MapBuilder {
    type Ok = Canon;
    type Error = CanonError;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), CanonError> {
        self.pending_key = Some(key.serialize(KeySerializer)?);
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CanonError> {
        let key = self.pending_key.take().ok_or(CanonError::KeyNotString)?;
        self.members.insert(key, value.serialize(CanonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Canon, CanonError> {
        Ok(Canon::Object(self.members))
    }
}

impl ser::SerializeStruct for MapBuilder {
    type Ok = Canon;
    type Error = CanonError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), CanonError> {
        self.members
            .insert(nfc(key), value.serialize(CanonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Canon, CanonError> {
        Ok(Canon::Object(self.members))
    }
}

impl ser::SerializeStructVariant for StructVariantBuilder {
    type Ok = Canon;
    type Error = CanonError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), CanonError> {
        self.members
            .insert(nfc(key), value.serialize(CanonSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Canon, CanonError> {
        let mut outer = BTreeMap::new();
        outer.insert(nfc(self.variant), Canon::Object(self.members));
        Ok(Canon::Object(outer))
    }
}

// ---------------------------------------------------------------------------
// Map keys
// ---------------------------------------------------------------------------

/// Serialises a map key to the string it must be for the keys to have a sort order.
/// Integers and unit variants are accepted because they have one obvious rendering;
/// everything else is [`CanonError::KeyNotString`].
struct KeySerializer;

macro_rules! key_from_integer {
    ($($method:ident($ty:ty)),* $(,)?) => {
        $(fn $method(self, v: $ty) -> Result<String, CanonError> { Ok(v.to_string()) })*
    };
}

macro_rules! key_rejects {
    ($($method:ident($($arg:ty),*)),* $(,)?) => {
        $(fn $method(self $(, _: $arg)*) -> Result<String, CanonError> {
            Err(CanonError::KeyNotString)
        })*
    };
}

impl ser::Serializer for KeySerializer {
    type Ok = String;
    type Error = CanonError;
    type SerializeSeq = ser::Impossible<String, CanonError>;
    type SerializeTuple = ser::Impossible<String, CanonError>;
    type SerializeTupleStruct = ser::Impossible<String, CanonError>;
    type SerializeTupleVariant = ser::Impossible<String, CanonError>;
    type SerializeMap = ser::Impossible<String, CanonError>;
    type SerializeStruct = ser::Impossible<String, CanonError>;
    type SerializeStructVariant = ser::Impossible<String, CanonError>;

    key_from_integer!(
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_i128(i128),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_u128(u128),
    );

    key_rejects!(
        serialize_bool(bool),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_bytes(&[u8]),
        serialize_none(),
        serialize_unit(),
        serialize_unit_struct(&'static str),
    );

    fn serialize_char(self, v: char) -> Result<String, CanonError> {
        Ok(nfc(v.encode_utf8(&mut [0u8; 4])))
    }

    fn serialize_str(self, v: &str) -> Result<String, CanonError> {
        Ok(nfc(v))
    }

    fn serialize_some<T: Serialize + ?Sized>(self, _value: &T) -> Result<String, CanonError> {
        Err(CanonError::KeyNotString)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<String, CanonError> {
        Ok(nfc(variant))
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<String, CanonError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<String, CanonError> {
        Err(CanonError::KeyNotString)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, CanonError> {
        Err(CanonError::KeyNotString)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, CanonError> {
        Err(CanonError::KeyNotString)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, CanonError> {
        Err(CanonError::KeyNotString)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, CanonError> {
        Err(CanonError::KeyNotString)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, CanonError> {
        Err(CanonError::KeyNotString)
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, CanonError> {
        Err(CanonError::KeyNotString)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, CanonError> {
        Err(CanonError::KeyNotString)
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Test names are exactly the ones
// listed in the Phase 0 "Tests to write FIRST" table, rows 0.3 and 0.4.
// ---------------------------------------------------------------------------

#[cfg(test)]
#[derive(serde::Serialize)]
struct Nested {
    zebra: u8,
    caption: &'static str,
    alpha: bool,
}

#[cfg(test)]
#[derive(serde::Serialize)]
struct SnapshotFixture {
    /// Sorts after `bbox` and `alpha`, so a plain sort would not put it first.
    ir_version: u32,
    zebra: Nested,
    bbox: crate::geom::Rect,
    /// Decomposed on purpose: `e` + U+0301 must be emitted as the single NFC character.
    title: &'static str,
    alpha: Vec<i32>,
    confidence: f32,
    ratio: f64,
    missing: Option<u8>,
}

#[test]
fn canonical_json_sorts_keys_and_rounds_geometry() {
    use crate::canonical::to_canonical_json;
    use crate::geom::Rect;

    let fixture = SnapshotFixture {
        ir_version: crate::IR_VERSION,
        zebra: Nested {
            zebra: 3,
            caption: "figure 1",
            alpha: true,
        },
        bbox: Rect {
            x0: 1.234_567,
            y0: 2.345_678,
            x1: 3.456_789,
            y1: 4.567_891,
        },
        title: "cafe\u{0301}",
        alpha: vec![3, 1, 2],
        confidence: 0.95,
        ratio: 0.126,
        missing: None,
    };

    let json = to_canonical_json(&fixture).expect("the fixture is finite and serialisable");

    // Geometry is printed with two decimals (ARCHITECTURE §4.3).
    assert!(json.contains(r#""x0":1.23"#), "{json}");
    assert!(json.contains(r#""y1":4.57"#), "{json}");

    // `ir_version` comes first even though `alpha` and `bbox` sort before it.
    assert!(json.starts_with(r#"{"ir_version":1,"alpha":"#), "{json}");

    // Remaining keys are sorted, at every level.
    assert!(
        json.contains(r#""zebra":{"alpha":true,"caption":"figure 1","zebra":3}"#),
        "{json}"
    );

    // Arrays keep document order; they are not sorted.
    assert!(json.contains(r#""alpha":[3,1,2]"#), "{json}");

    // Strings are NFC: the decomposed input is emitted as one composed character.
    assert!(json.contains("\"title\":\"caf\u{e9}\""), "{json}");

    // The whole shape is the regression artefact.
    insta::assert_snapshot!(json);
}

#[test]
fn canonical_json_rejects_nan() {
    use crate::canonical::{to_canonical_json, CanonError};
    use crate::geom::Rect;

    assert!(matches!(
        to_canonical_json(&f32::NAN),
        Err(CanonError::NonFinite)
    ));
    assert!(matches!(
        to_canonical_json(&f32::INFINITY),
        Err(CanonError::NonFinite)
    ));
    assert!(matches!(
        to_canonical_json(&f64::NEG_INFINITY),
        Err(CanonError::NonFinite)
    ));

    // A non-finite coordinate buried inside a struct is rejected too: the point is that
    // NaN can never reach the output as a silent `null` (ARCHITECTURE §4.3).
    let poisoned = Rect {
        x0: 0.0,
        y0: 0.0,
        x1: f32::NAN,
        y1: 1.0,
    };
    assert!(matches!(
        to_canonical_json(&poisoned),
        Err(CanonError::NonFinite)
    ));

    // Finite values of the same types still serialise.
    assert_eq!(
        to_canonical_json(&0.5f32).expect("0.5 is finite"),
        "0.50".to_owned()
    );
}
