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

/// The safe range of a JSON number: every reader keeps it exact.
const SAFE_INT: i64 = (1 << 53) - 1;

/// A wire value that is not of its declared type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotOfType(pub String);

fn scalar(text: &str) -> &str {
    text.trim_end_matches("[]")
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
            return Err(NotOfType(format!("{v} is not a set")));
        };
        let mut out = Vec::new();
        for item in items {
            out.push(decode(item, declared, false, lib)?);
        }
        return Ok(Value::Set(out));
    }
    let wrong = || NotOfType(format!("{v} is not of its declared type"));
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
                    Err(NotOfType(format!("{s} is not a member of {t}")))
                };
            }
            match t {
                "bool" => v.as_bool().map(Value::Bool).ok_or_else(wrong),
                "int" | "counter" => match v {
                    Json::Number(n) => n.as_i64().map(Value::Int).ok_or_else(wrong),
                    Json::String(s) => s.parse().map(Value::Int).map_err(|_| wrong()),
                    _ => Err(wrong()),
                },
                "timestamp" => v
                    .as_str()
                    .and_then(parse_timestamp)
                    .map(Value::Timestamp)
                    .ok_or_else(wrong),
                "date" => v
                    .as_str()
                    .and_then(parse_date)
                    .map(Value::Date)
                    .ok_or_else(wrong),
                "duration" => v
                    .as_str()
                    .and_then(parse_duration)
                    .map(Value::Duration)
                    .ok_or_else(wrong),
                "string" | "text" | "identity" | "actor_kind" | "file" => {
                    let s = v.as_str().ok_or_else(wrong)?;
                    if s.contains('\0') {
                        return Err(NotOfType("a string never holds U+0000".to_string()));
                    }
                    Ok(Value::Text(s.to_string()))
                }
                t if t.starts_with("decimal(") => {
                    let scale = t
                        .trim_end_matches(')')
                        .rsplit(',')
                        .next()
                        .and_then(|s| s.trim().parse::<u8>().ok())
                        .ok_or_else(wrong)?;
                    let d = match v {
                        Json::String(s) => Decimal::parse(s),
                        Json::Number(n) => n.as_i64().map(Decimal::from_int),
                        _ => None,
                    }
                    .ok_or_else(wrong)?;
                    // more places than the scale is refused, never rounded
                    let held = d.rescale(scale).ok_or_else(wrong)?;
                    if held != d {
                        return Err(NotOfType(format!("{v} has more places than {t}")));
                    }
                    Ok(Value::Decimal(held))
                }
                t if t.starts_with("money(") => {
                    let currency = t.trim_start_matches("money(").trim_end_matches(')');
                    let minor = minor_unit(currency)
                        .ok_or_else(|| NotOfType(format!("the currency {currency}")))?;
                    let s = v.as_str().ok_or_else(wrong)?;
                    let (ccy, amount) = s.split_once(' ').ok_or_else(wrong)?;
                    if ccy != currency {
                        return Err(NotOfType(format!("{s} is not in {currency}")));
                    }
                    let d = Decimal::parse(amount).ok_or_else(wrong)?;
                    let held = d.rescale(minor).ok_or_else(wrong)?;
                    if held != d {
                        return Err(NotOfType(format!(
                            "{s} has more places than {currency}'s minor unit"
                        )));
                    }
                    Ok(Value::Money {
                        currency: currency.to_string(),
                        amount: held,
                    })
                }
                other => Err(NotOfType(format!("the type {other}"))),
            }
        }
    }
}

/// A value as the wire carries it.
pub fn encode(v: &Value) -> Json {
    match v {
        Value::Absent => Json::Null,
        Value::Bool(b) => json!(b),
        Value::Int(n) if n.abs() <= SAFE_INT => json!(n),
        Value::Int(n) => json!(n.to_string()),
        Value::Decimal(d) => json!(d.to_string()),
        Value::Money { currency, amount } => json!(format!("{currency} {amount}")),
        Value::Duration(s) => json!(duration_text(*s)),
        Value::Timestamp(t) => json!(timestamp_text(*t)),
        Value::Date(d) => json!(date_text(*d)),
        Value::Text(s) | Value::Object(s) => json!(s),
        Value::State { name, .. } => json!(name),
        Value::Member { member, .. } => json!(member),
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

    #[test]
    fn an_int_leaves_json_past_the_safe_range() {
        assert_eq!(encode(&Value::Int(SAFE_INT)), json!(SAFE_INT));
        assert_eq!(
            encode(&Value::Int(SAFE_INT + 1)),
            json!((SAFE_INT + 1).to_string())
        );
    }
}
