//! Values on the wire (ADR-0150 decision 2), read and written against their
//! declared types.
//!
//! JSON's own where JSON keeps a value exactly: absence is null, a bool a
//! boolean, an int a number within ±(2^53 − 1), text and an identity a
//! string, a reference the object's id, a set an array. Every other value is
//! a string in the model's notation: a decimal `"19.99"`, money `"USD
//! 19.99"`, a duration `"30 min"`, a date `"2026-10-04"`, a timestamp in RFC
//! 3339 at UTC, a state or an enumeration member by its name.

use serde_json::{Value as Json, json};

use crate::model::{Declared, Library};
use crate::value::{
    Decimal, Value, date_text, duration_text, minor_unit, parse_date, parse_duration,
    parse_timestamp, timestamp_text,
};
use crate::verdict::InputReason;

/// The safe range of a JSON number: every reader keeps it exact.
const SAFE_INT: i64 = (1 << 53) - 1;

/// A wire value that is not of its declared type, and why (ADR-0173
/// decision 9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotOfType(pub String, pub InputReason);

/// The most units a decimal or an amount holds at its scale: 18 digits,
/// which the store keeps in 64 bits (ADR-0143).
const MAX_UNITS: i128 = 999_999_999_999_999_999;

/// The most bytes a string value holds, in UTF-8 (ADR-0188 decision 1).
pub const TEXT_BYTES: usize = 64 * 1024;

/// A `decimal(p,s)`'s digits and places.
pub fn precision_and_scale(declared: &str) -> Option<(u32, u8)> {
    let (p, s) = declared
        .strip_prefix("decimal(")?
        .strip_suffix(')')?
        .split_once(',')?;
    Some((p.trim().parse().ok()?, s.trim().parse().ok()?))
}

fn scalar(text: &str) -> &str {
    text.trim_end_matches("[]")
}

/// A decimal held at a scale, refused where it has more places than the
/// scale, or more digits than its type holds: a `decimal(p,s)`'s `p`, an
/// amount's 18, which the store keeps in 64 bits.
fn held_at(d: Decimal, scale: u8, digits: u32, what: &str) -> Result<Decimal, NotOfType> {
    let range = || {
        NotOfType(
            format!("{what} has more than {digits} digits"),
            InputReason::OutOfRange,
        )
    };
    let held = d.rescale(scale).ok_or_else(range)?;
    if held != d {
        return Err(NotOfType(
            format!("{what} has more places than {scale}"),
            InputReason::Malformed,
        ));
    }
    if held.units().abs() > MAX_UNITS.min(10i128.pow(digits) - 1) {
        return Err(range());
    }
    Ok(held)
}

/// A wire value as a value of a declared type. A reference is read as an id
/// and checked against the store by the request, not here.
pub fn decode(
    v: &Json,
    declared: &Declared,
    many: bool,
    lib: &Library,
) -> Result<Value, NotOfType> {
    if v.is_null() {
        return Ok(Value::Absent);
    }
    if many {
        let Json::Array(items) = v else {
            return Err(NotOfType(
                format!("{v} is not a set"),
                InputReason::WrongType,
            ));
        };
        let mut out: Vec<Value> = Vec::new();
        for item in items {
            let v = decode(item, declared, false, lib)?;
            // a set never repeats an element (`DESIGN.md` §5.5)
            if out.contains(&v) {
                return Err(NotOfType(
                    format!("{item} is repeated in a set"),
                    InputReason::Malformed,
                ));
            }
            out.push(v);
        }
        return Ok(Value::Set(out));
    }
    let wrong = || {
        NotOfType(
            format!("{v} is not of its declared type"),
            InputReason::WrongType,
        )
    };
    let malformed = || {
        NotOfType(
            format!("{v} is not written as its type is"),
            InputReason::Malformed,
        )
    };
    match declared {
        Declared::Reference(_) => v
            .as_str()
            .map(|s| Value::Object(s.to_string()))
            .ok_or_else(wrong),
        Declared::Scalar(t) => {
            let t = scalar(t);
            if let Some(members) = lib.enumerations.get(t) {
                let s = v.as_str().ok_or_else(wrong)?;
                // a member by its name, or qualified as an example writes it
                let member = s.strip_prefix(&format!("{t}.")).unwrap_or(s);
                return if members.iter().any(|m| m == member) {
                    Ok(Value::Member {
                        enumeration: t.to_string(),
                        member: member.to_string(),
                    })
                } else {
                    Err(NotOfType(
                        format!("{s} is not a member of {t}"),
                        InputReason::WrongType,
                    ))
                };
            }
            match t {
                "bool" => v.as_bool().map(Value::Bool).ok_or_else(wrong),
                "int" | "counter" => {
                    let range =
                        || NotOfType(format!("{v} is past 64 bits"), InputReason::OutOfRange);
                    match v {
                        Json::Number(n) if n.is_f64() => Err(wrong()),
                        Json::Number(n) => n.as_i64().map(Value::Int).ok_or_else(range),
                        Json::String(s) => match s.parse() {
                            Ok(n) => Ok(Value::Int(n)),
                            Err(_)
                                if s.trim_start_matches('-')
                                    .chars()
                                    .all(|c| c.is_ascii_digit())
                                    && !s.is_empty() =>
                            {
                                Err(range())
                            }
                            Err(_) => Err(malformed()),
                        },
                        _ => Err(wrong()),
                    }
                }
                "timestamp" => parse_timestamp(v.as_str().ok_or_else(wrong)?)
                    .map(Value::Timestamp)
                    .ok_or_else(malformed),
                "date" => parse_date(v.as_str().ok_or_else(wrong)?)
                    .map(Value::Date)
                    .ok_or_else(malformed),
                "duration" => parse_duration(v.as_str().ok_or_else(wrong)?)
                    .map(Value::Duration)
                    .ok_or_else(malformed),
                // a file is its reference, never a name (ADR-0173 decision 11)
                "file" => file_of(v).map_err(|why| match why {
                    Some(why) => NotOfType(why, InputReason::Malformed),
                    None => wrong(),
                }),
                "string" | "text" | "identity" | "actor_kind" => {
                    let s = v.as_str().ok_or_else(wrong)?;
                    if s.contains('\0') {
                        return Err(NotOfType(
                            "a string never holds U+0000".to_string(),
                            InputReason::Malformed,
                        ));
                    }
                    // at most 64 KiB, where a title of 1.9 MB was stored
                    // (third production review, ADR-0188 decision 1)
                    if s.len() > TEXT_BYTES {
                        return Err(NotOfType(
                            format!("a string holds at most {TEXT_BYTES} bytes, not {}", s.len()),
                            InputReason::OutOfRange,
                        ));
                    }
                    Ok(Value::Text(s.to_string()))
                }
                t if t.starts_with("decimal(") => {
                    let (digits, scale) = precision_and_scale(t).ok_or_else(wrong)?;
                    let d = match v {
                        Json::String(s) => Decimal::parse(s).ok_or_else(malformed)?,
                        Json::Number(n) if n.is_f64() => return Err(wrong()),
                        Json::Number(n) => n.as_i64().map(Decimal::from_int).ok_or_else(|| {
                            NotOfType(format!("{v} is past 64 bits"), InputReason::OutOfRange)
                        })?,
                        _ => return Err(wrong()),
                    };
                    // more places than the scale is refused, never rounded,
                    // and more digits than `p`, as declaration-syntax.md §4
                    // says it holds (third production review, minor)
                    Ok(Value::Decimal(held_at(d, scale, digits, &v.to_string())?))
                }
                t if t.starts_with("money(") => {
                    let currency = t.trim_start_matches("money(").trim_end_matches(')');
                    let minor = minor_unit(currency).ok_or_else(|| {
                        NotOfType(format!("the currency {currency}"), InputReason::WrongType)
                    })?;
                    let s = v.as_str().ok_or_else(wrong)?;
                    let (ccy, amount) = s.split_once(' ').ok_or_else(malformed)?;
                    let d = Decimal::parse(amount).ok_or_else(malformed)?;
                    if ccy != currency {
                        return Err(NotOfType(
                            format!("{s} is not in {currency}"),
                            InputReason::WrongCurrency,
                        ));
                    }
                    Ok(Value::Money {
                        currency: currency.to_string(),
                        amount: held_at(d, minor, 18, s)?,
                    })
                }
                other => Err(NotOfType(
                    format!("the type {other}"),
                    InputReason::WrongType,
                )),
            }
        }
    }
}

/// A value as the wire carries it.
/// A file's wire form, `{"hash": "sha256:<hex>", "size": <bytes>,
/// "media_type": "<type>/<subtype>"}`, read as a value: none where it is not
/// an object, and why it is malformed where it is one (ADR-0173 decision 11).
pub fn file_of(v: &Json) -> Result<Value, Option<String>> {
    let o = v.as_object().ok_or(None)?;
    if let Some(k) = o
        .keys()
        .find(|k| !["hash", "size", "media_type"].contains(&k.as_str()))
    {
        return Err(Some(format!("a file has no {k}")));
    }
    let hash = o.get("hash").and_then(Json::as_str).unwrap_or_default();
    let hex = hash.strip_prefix("sha256:").unwrap_or_default();
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Some(format!(
            "a file's hash is sha256:<64 lowercase hex digits>, not {hash}"
        )));
    }
    let size = o
        .get("size")
        .and_then(Json::as_i64)
        .filter(|n| (0..=SAFE_INT).contains(n))
        .ok_or_else(|| Some("a file's size is a count of bytes".to_string()))?;
    let media_type = o
        .get("media_type")
        .and_then(Json::as_str)
        .unwrap_or_default();
    let token = |t: &str| {
        !t.is_empty()
            && t.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&b))
    };
    match media_type.split_once('/') {
        Some((kind, sub)) if token(kind) && token(sub) => {}
        _ => {
            return Err(Some(format!(
                "a file's media_type is <type>/<subtype>, not {media_type}"
            )));
        }
    }
    Ok(Value::File {
        hash: hash.to_string(),
        size,
        media_type: media_type.to_ascii_lowercase(),
    })
}

pub fn encode(v: &Value) -> Json {
    match v {
        Value::Absent => Json::Null,
        Value::Bool(b) => json!(b),
        // the least integer has no positive counterpart, so its magnitude
        // is taken unsigned rather than wrapped to a negative (F5)
        Value::Int(n) if n.unsigned_abs() <= SAFE_INT as u64 => json!(n),
        Value::Int(n) => json!(n.to_string()),
        Value::Decimal(d) => json!(d.to_string()),
        Value::Money { currency, amount } => json!(format!("{currency} {amount}")),
        Value::Duration(s) => json!(duration_text(*s)),
        Value::Timestamp(t) => json!(timestamp_text(*t)),
        Value::Date(d) => json!(date_text(*d)),
        Value::Text(s) | Value::Object(s) => json!(s),
        Value::State { name, .. } => json!(name),
        Value::Member { member, .. } => json!(member),
        Value::File {
            hash,
            size,
            media_type,
        } => json!({"hash": hash, "size": size, "media_type": media_type}),
        Value::Set(items) => Json::Array(items.iter().map(encode).collect()),
        Value::Row(fields) => {
            Json::Object(fields.iter().map(|(k, v)| (k.clone(), encode(v))).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> Library {
        let mut lib = Library::default();
        lib.enumerations
            .insert("Priority".into(), vec!["LOW".into(), "HIGH".into()]);
        lib
    }

    fn s(t: &str) -> Declared {
        Declared::Scalar(t.into())
    }

    #[test]
    fn each_type_reads_its_notation() {
        let l = lib();
        assert_eq!(
            decode(&json!(null), &s("int"), false, &l),
            Ok(Value::Absent)
        );
        assert_eq!(decode(&json!(3), &s("int"), false, &l), Ok(Value::Int(3)));
        assert_eq!(
            decode(&json!("HIGH"), &s("Priority"), false, &l).map(|v| encode(&v)),
            Ok(json!("HIGH"))
        );
        assert_eq!(
            decode(&json!("Priority.LOW"), &s("Priority"), false, &l).map(|v| encode(&v)),
            Ok(json!("LOW"))
        );
        assert!(decode(&json!("MID"), &s("Priority"), false, &l).is_err());
        assert_eq!(
            decode(&json!("19.9"), &s("decimal(8,2)"), false, &l).map(|v| encode(&v)),
            Ok(json!("19.90"))
        );
        assert!(decode(&json!("19.999"), &s("decimal(8,2)"), false, &l).is_err());
        assert_eq!(
            decode(&json!("USD 5"), &s("money(USD)"), false, &l).map(|v| encode(&v)),
            Ok(json!("USD 5.00"))
        );
        assert!(decode(&json!("SGD 5.00"), &s("money(USD)"), false, &l).is_err());
        assert_eq!(
            decode(&json!("90 min"), &s("duration"), false, &l),
            Ok(Value::Duration(5400))
        );
        assert_eq!(
            decode(&json!("2026-01-01T00:00Z"), &s("timestamp"), false, &l).map(|v| encode(&v)),
            Ok(json!("2026-01-01T00:00:00Z"))
        );
        assert!(decode(&json!("a\u{0}b"), &s("string"), false, &l).is_err());
        assert!(decode(&json!(1), &s("string"), false, &l).is_err());
        assert_eq!(
            decode(
                &json!(["a-1", "a-2"]),
                &Declared::Reference("A".into()),
                true,
                &l
            )
            .map(|v| encode(&v)),
            Ok(json!(["a-1", "a-2"]))
        );
    }

    /// Each value refused says why (ADR-0173 decision 9), and a value past
    /// what the store holds is refused at the boundary, not faulted in the
    /// store (D554): 18 digits of an amount or a decimal fit, 19 do not.
    #[test]
    fn a_value_refused_says_why() {
        let l = lib();
        let why = |v: Json, t: &str| decode(&v, &s(t), false, &l).err().map(|e| e.1);
        assert_eq!(why(json!("MID"), "Priority"), Some(InputReason::WrongType));
        assert_eq!(why(json!(1), "string"), Some(InputReason::WrongType));
        assert_eq!(
            why(json!(1.5), "decimal(8,2)"),
            Some(InputReason::WrongType)
        );
        assert_eq!(why(json!(1500), "money(USD)"), Some(InputReason::WrongType));
        assert_eq!(
            why(json!("19.999"), "decimal(8,2)"),
            Some(InputReason::Malformed)
        );
        assert_eq!(
            why(json!("2025-02-30"), "date"),
            Some(InputReason::Malformed)
        );
        assert_eq!(
            why(json!("soon"), "timestamp"),
            Some(InputReason::Malformed)
        );
        assert_eq!(why(json!("many"), "int"), Some(InputReason::Malformed));
        assert_eq!(
            why(json!("1500.00"), "money(USD)"),
            Some(InputReason::Malformed)
        );
        assert_eq!(
            why(json!("a\u{0}b"), "string"),
            Some(InputReason::Malformed)
        );
        assert_eq!(
            why(json!("SGD 5.00"), "money(USD)"),
            Some(InputReason::WrongCurrency)
        );
        assert_eq!(
            why(json!("99999999999999999999"), "int"),
            Some(InputReason::OutOfRange)
        );
        let set = |v: Json| {
            decode(&v, &Declared::Reference("A".into()), true, &l)
                .err()
                .map(|e| e.1)
        };
        assert_eq!(set(json!(["a-1", "a-2"])), None);
        assert_eq!(set(json!(["a-1", "a-1"])), Some(InputReason::Malformed));
        // the boundary: 18 digits at the amount's scale, then 19
        assert_eq!(why(json!("USD 9999999999999999.99"), "money(USD)"), None);
        assert_eq!(
            why(json!("USD 10000000000000000.00"), "money(USD)"),
            Some(InputReason::OutOfRange)
        );
        assert_eq!(why(json!("9999999999999999.99"), "decimal(18,2)"), None);
        assert_eq!(
            why(json!("10000000000000000.00"), "decimal(18,2)"),
            Some(InputReason::OutOfRange)
        );
    }

    /// A `decimal(p,s)` holds `p` digits, as declaration-syntax.md §4 says,
    /// where 18 were taken whatever its `p` (third production review,
    /// minor); an amount holds the store's 18.
    #[test]
    fn a_decimal_holds_its_precisions_digits() {
        let l = lib();
        let read = |v: Json, t: &str| decode(&v, &s(t), false, &l);
        assert!(read(json!("99.99"), "decimal(4,2)").is_ok());
        assert!(read(json!("-99.99"), "decimal(4,2)").is_ok());
        let past = read(json!("100.00"), "decimal(4,2)").unwrap_err();
        assert!(matches!(past.1, InputReason::OutOfRange), "{past:?}");
        assert!(read(json!("100"), "decimal(4,2)").is_err());
        assert!(read(json!("9999999999999999.99"), "decimal(18,2)").is_ok());
        assert_eq!(precision_and_scale("decimal(12,2)"), Some((12, 2)));
    }

    #[test]
    fn an_int_leaves_json_past_the_safe_range() {
        assert_eq!(encode(&Value::Int(SAFE_INT)), json!(SAFE_INT));
        assert_eq!(
            encode(&Value::Int(SAFE_INT + 1)),
            json!((SAFE_INT + 1).to_string())
        );
        // the least integer, which has no positive counterpart, past the
        // safe range as its negation would be (F5)
        assert_eq!(encode(&Value::Int(-SAFE_INT)), json!(-SAFE_INT));
        assert_eq!(encode(&Value::Int(i64::MIN)), json!("-9223372036854775808"));
        assert_eq!(
            crate::value::duration_text(i64::MIN),
            "-9223372036854775808 s"
        );
    }
}
