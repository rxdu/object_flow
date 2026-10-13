//! Values, as `declaration-syntax.md` §8.1 to §8.3 give them.
//!
//! Absence and unknown are one value, [`Value::Absent`], as in the reference
//! runner: reading an absent optional attribute yields unknown, and any
//! operator with an unknown operand yields unknown, but `is null`, `and` and
//! `or` (§8.2). A decimal is the core's own fixed point, an `i128` and a scale,
//! with no crate (ADR-0143); arithmetic on it is checked, and a result that
//! cannot be held is unknown rather than wrong.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

/// A decimal number, `units / 10^scale`, exact. Two decimals are equal when
/// their numbers are, whatever their scales: `1.50 == 1.5`.
#[derive(Clone, Copy, Debug)]
pub struct Decimal {
    units: i128,
    scale: u8,
}

/// The most places a decimal keeps: 38 digits fit an `i128`.
const MAX_SCALE: u8 = 38;

fn pow10(n: u8) -> Option<i128> {
    10_i128.checked_pow(u32::from(n))
}

/// `a / b` rounded half to even, `b` not zero.
fn div_half_even(a: i128, b: i128) -> Option<i128> {
    if b == 0 {
        return None;
    }
    let q = a.checked_div(b)?;
    let r = a.checked_rem(b)?;
    if r == 0 {
        return Some(q);
    }
    let twice = r.checked_abs()?.checked_mul(2)?;
    let denom = b.checked_abs()?;
    let negative = (a < 0) != (b < 0);
    let away = match twice.cmp(&denom) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => q % 2 != 0,
    };
    if !away {
        Some(q)
    } else if negative {
        q.checked_sub(1)
    } else {
        q.checked_add(1)
    }
}

impl Decimal {
    /// A decimal of `units` at `scale` places.
    pub fn new(units: i128, scale: u8) -> Option<Decimal> {
        (scale <= MAX_SCALE).then_some(Decimal { units, scale })
    }

    /// An integer as a decimal of no places.
    pub fn from_int(n: i64) -> Decimal {
        Decimal {
            units: i128::from(n),
            scale: 0,
        }
    }

    /// A decimal written as `-12.50`, keeping the places it is written with.
    pub fn parse(text: &str) -> Option<Decimal> {
        let text = text.trim();
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !fraction.bytes().all(|b| b.is_ascii_digit())
            || (digits.contains('.') && fraction.is_empty())
        {
            return None;
        }
        let scale = u8::try_from(fraction.len()).ok()?;
        let units: i128 = format!("{whole}{fraction}").parse().ok()?;
        Decimal::new(if negative { -units } else { units }, scale)
    }

    pub fn units(&self) -> i128 {
        self.units
    }

    pub fn scale(&self) -> u8 {
        self.scale
    }

    /// The same number at `scale` places, rounded half to even where places
    /// are dropped (§8.3).
    pub fn rescale(&self, scale: u8) -> Option<Decimal> {
        match scale.cmp(&self.scale) {
            Ordering::Equal => Some(*self),
            Ordering::Greater => {
                let f = pow10(scale - self.scale)?;
                Decimal::new(self.units.checked_mul(f)?, scale)
            }
            Ordering::Less => {
                let f = pow10(self.scale - scale)?;
                Decimal::new(div_half_even(self.units, f)?, scale)
            }
        }
    }

    /// Both at the larger of their scales, which loses nothing.
    fn aligned(&self, other: &Decimal) -> Option<(i128, i128, u8)> {
        let scale = self.scale.max(other.scale);
        Some((
            self.rescale(scale)?.units,
            other.rescale(scale)?.units,
            scale,
        ))
    }

    pub fn checked_add(&self, other: &Decimal) -> Option<Decimal> {
        let (a, b, s) = self.aligned(other)?;
        Decimal::new(a.checked_add(b)?, s)
    }

    pub fn checked_sub(&self, other: &Decimal) -> Option<Decimal> {
        let (a, b, s) = self.aligned(other)?;
        Decimal::new(a.checked_sub(b)?, s)
    }

    /// The exact product, its scale the sum of the scales (§8.3).
    pub fn checked_mul(&self, other: &Decimal) -> Option<Decimal> {
        Decimal::new(
            self.units.checked_mul(other.units)?,
            self.scale.checked_add(other.scale)?,
        )
    }

    /// The quotient at `scale` places, rounded half to even; `None` for a zero
    /// divisor, which §8.2 reads as unknown.
    pub fn checked_div(&self, other: &Decimal, scale: u8) -> Option<Decimal> {
        if other.units == 0 {
            return None;
        }
        // self / other = (a / 10^sa) / (b / 10^sb); at `scale` places that is
        // a * 10^(scale + sb - sa) / b
        let shift = i16::from(scale) + i16::from(other.scale) - i16::from(self.scale);
        let (num, den) = if shift >= 0 {
            (
                self.units.checked_mul(pow10(u8::try_from(shift).ok()?)?)?,
                other.units,
            )
        } else {
            (
                self.units,
                other
                    .units
                    .checked_mul(pow10(u8::try_from(-shift).ok()?)?)?,
            )
        };
        Decimal::new(div_half_even(num, den)?, scale)
    }

    pub fn checked_neg(&self) -> Option<Decimal> {
        Decimal::new(self.units.checked_neg()?, self.scale)
    }

    pub fn is_zero(&self) -> bool {
        self.units == 0
    }

    /// The number as an `i64` if it is whole and fits.
    pub fn to_int(&self) -> Option<i64> {
        let whole = self.rescale(0)?;
        (whole.rescale(self.scale)? == *self)
            .then(|| i64::try_from(whole.units).ok())
            .flatten()
    }
}

impl PartialEq for Decimal {
    fn eq(&self, other: &Decimal) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Decimal {}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Decimal) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Decimal) -> Ordering {
        match self.aligned(other) {
            Some((a, b, _)) => a.cmp(&b),
            // past 38 places neither fits; compare by sign, then by the
            // larger scale's rounding, which no declared decimal reaches
            None => self.units.signum().cmp(&other.units.signum()),
        }
    }
}

impl fmt::Display for Decimal {
    /// Written exactly, with every place its scale keeps: `19.90`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.units < 0 { "-" } else { "" };
        let digits = self.units.unsigned_abs().to_string();
        let places = usize::from(self.scale);
        if places == 0 {
            return write!(f, "{sign}{digits}");
        }
        let padded = format!("{digits:0>width$}", width = places + 1);
        let (whole, fraction) = padded.split_at(padded.len() - places);
        write!(f, "{sign}{whole}.{fraction}")
    }
}

/// The places a currency's minor unit has (ISO 4217), the scale its money
/// keeps (§8.3); the currencies the reference runner knows.
pub fn minor_unit(currency: &str) -> Option<u8> {
    Some(match currency {
        "USD" | "EUR" | "GBP" | "SGD" | "AUD" | "CAD" | "CHF" | "CNY" | "HKD" | "NZD" | "MYR"
        | "INR" | "THB" => 2,
        "JPY" | "KRW" => 0,
        "BHD" | "KWD" | "OMR" | "JOD" | "TND" => 3,
        _ => return None,
    })
}

/// A duration's units, in seconds: a closed set with no months or years
/// (§8.3, Units).
pub fn unit_seconds(unit: &str) -> Option<i64> {
    Some(match unit {
        "s" => 1,
        "min" => 60,
        "h" => 3600,
        "day" | "days" => 86_400,
        "week" | "weeks" => 604_800,
        _ => return None,
    })
}

/// A duration written as a count and a unit, `2 days`, in seconds.
pub fn parse_duration(text: &str) -> Option<i64> {
    let (n, unit) = text.trim().split_once(' ')?;
    n.parse::<i64>()
        .ok()?
        .checked_mul(unit_seconds(unit.trim())?)
}

/// Seconds written in their largest whole unit, `90 min`, as the model
/// writes a duration (ADR-0150 decision 2).
pub fn duration_text(seconds: i64) -> String {
    const UNITS: [(&str, &str, i64); 5] = [
        ("week", "weeks", 604_800),
        ("day", "days", 86_400),
        ("h", "h", 3600),
        ("min", "min", 60),
        ("s", "s", 1),
    ];
    for (one, many, size) in UNITS {
        if seconds % size == 0 && (seconds != 0 || size == 1) {
            let n = seconds / size;
            return format!("{n} {}", if n.unsigned_abs() == 1 { one } else { many });
        }
    }
    format!("{seconds} s")
}

/// Days since 1970-01-01 of a civil date (proleptic Gregorian).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date of days since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    // d is 1..=31 and m 1..=12 by construction
    (
        y,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

/// A calendar day written `2026-10-04`, in days since 1970-01-01.
/// A date written `YYYY-MM-DD` exactly, its year 0001 to 9999, as RFC 3339
/// writes one: a space, a sign, a one-digit month or a year past 9999 was
/// read and stored (second production review, minor).
pub fn parse_date(text: &str) -> Option<i64> {
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    let mut parts = text.splitn(3, '-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if !digits(y, 4) || !digits(m, 2) || !digits(d, 2) {
        return None;
    }
    let y: i64 = y.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    let d: u32 = d.parse().ok()?;
    if y < 1 || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let days = days_from_civil(y, m, d);
    (civil_from_days(days) == (y, m, d)).then_some(days)
}

/// The calendar a date or a time is written in, 0001-01-01 to
/// 9999-12-31T23:59:59Z, as the wire reads one: a value past it is written
/// as text no read takes back, which an effect wrote, making its object and
/// its type unreadable for good (fifth production review BRICK, ADR-0190).
pub const FIRST_DAY: i64 = -719_162;
pub const LAST_DAY: i64 = 2_932_896;
pub const FIRST_SECOND: i64 = FIRST_DAY * 86_400;
pub const LAST_SECOND: i64 = LAST_DAY * 86_400 + 86_399;

/// Whether a day, in days since 1970-01-01, is in the calendar.
pub fn in_calendar(day: i64) -> bool {
    (FIRST_DAY..=LAST_DAY).contains(&day)
}

pub fn date_text(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// An instant written in RFC 3339 at UTC, `2026-10-04T09:30:00Z` or with
/// `+00:00`, or to the minute as examples write it, in seconds since 1970;
/// each field its digits exactly, the date as [`parse_date`] reads it.
pub fn parse_timestamp(text: &str) -> Option<i64> {
    let body = text
        .strip_suffix('Z')
        .or_else(|| text.strip_suffix("+00:00"))?;
    let (date, time) = body.split_once('T')?;
    let days = parse_date(date)?;
    let two = |s: &str| -> Option<i64> {
        (s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let mut hms = time.split(':');
    let h: i64 = two(hms.next()?)?;
    let mi: i64 = two(hms.next()?)?;
    let s: i64 = match hms.next() {
        Some(s) => two(s)?,
        None => 0,
    };
    if hms.next().is_some()
        || !(0..24).contains(&h)
        || !(0..60).contains(&mi)
        || !(0..60).contains(&s)
    {
        return None;
    }
    days.checked_mul(86_400)?
        .checked_add(h * 3600 + mi * 60 + s)
}

pub fn timestamp_text(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    format!(
        "{}T{:02}:{:02}:{:02}Z",
        date_text(days),
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// A value of the expression language (§8.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Absent or unknown, one value (§8.2).
    Absent,
    Bool(bool),
    Int(i64),
    Decimal(Decimal),
    /// An amount at its currency's minor unit.
    Money {
        currency: String,
        amount: Decimal,
    },
    /// Whole seconds.
    Duration(i64),
    /// Seconds since 1970, UTC.
    Timestamp(i64),
    /// Days since 1970-01-01.
    Date(i64),
    Text(String),
    /// A state of a type's machine: the type, and the state's name.
    State {
        type_: String,
        name: String,
    },
    /// A member of an enumeration.
    Member {
        enumeration: String,
        member: String,
    },
    /// An object, by its id.
    Object(String),
    /// A file: a content-addressed reference to bytes the store never
    /// holds, by their hash, their size and their media type (`DESIGN.md`
    /// §5.2, ADR-0173 decision 11).
    File {
        hash: String,
        size: i64,
        media_type: String,
    },
    /// A set, in the order its elements were added.
    Set(Vec<Value>),
    /// A row of an object's own flow data: a span of `intervals` (§6.9).
    /// Its fields are shared, so a metric copies a row by its reference
    /// and not field by field; a row is never changed once built.
    Row(std::sync::Arc<BTreeMap<String, Value>>),
}

impl Value {
    /// What kind of value it is, as a fault names it, never the value
    /// itself, which may be a person's (sixth production review, ADR-0192
    /// decision 7).
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Absent => "no value",
            Value::Bool(_) => "a boolean",
            Value::Int(_) => "an int",
            Value::Decimal(_) => "a decimal",
            Value::Money { .. } => "an amount of money",
            Value::Duration(_) => "a duration",
            Value::Timestamp(_) => "a timestamp",
            Value::Date(_) => "a date",
            Value::Text(_) => "a text",
            Value::State { .. } => "a state",
            Value::Member { .. } => "a member of an enumeration",
            Value::Object(_) => "an object",
            Value::File { .. } => "a file",
            Value::Set(_) => "a set",
            Value::Row(_) => "a row",
        }
    }

    /// A row of the fields given.
    pub fn row(fields: BTreeMap<String, Value>) -> Value {
        Value::Row(std::sync::Arc::new(fields))
    }

    pub fn is_absent(&self) -> bool {
        matches!(self, Value::Absent)
    }

    /// The value as a number, for comparing an int with a decimal.
    fn number(&self) -> Option<Decimal> {
        match self {
            Value::Int(n) => Some(Decimal::from_int(*n)),
            Value::Decimal(d) => Some(*d),
            _ => None,
        }
    }

    /// How two values of one type order, or `None` if they do not compare
    /// (§8.3: comparison needs both sides of one type).
    pub fn compare(&self, other: &Value) -> Option<Ordering> {
        match (self, other) {
            (
                Value::Money {
                    currency: a,
                    amount: x,
                },
                Value::Money {
                    currency: b,
                    amount: y,
                },
            ) => (a == b).then(|| x.cmp(y)),
            (Value::Duration(a), Value::Duration(b))
            | (Value::Timestamp(a), Value::Timestamp(b))
            | (Value::Date(a), Value::Date(b)) => Some(a.cmp(b)),
            (Value::Text(a), Value::Text(b)) => Some(a.cmp(b)),
            (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
            _ => match (self.number(), other.number()) {
                (Some(a), Some(b)) => Some(a.cmp(&b)),
                _ => None,
            },
        }
    }

    /// Equality as `==` reads it: numbers by value, sets by their elements in
    /// order, anything else by identity of kind and content.
    pub fn equals(&self, other: &Value) -> bool {
        match (self.number(), other.number()) {
            (Some(a), Some(b)) => a == b,
            _ => self == other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Decimal {
        Decimal::parse(text).unwrap()
    }

    #[test]
    fn a_decimal_parses_and_writes_back_exactly() {
        for text in ["0", "19.99", "-0.05", "12000.00", "0.001", "-7"] {
            assert_eq!(d(text).to_string(), text);
        }
        for bad in ["", "-", "1.", ".5", "1.2.3", "1e3", "--1", " "] {
            assert!(Decimal::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn rounding_drops_places_half_to_even() {
        let cases = [
            ("2.5", 0, "2"),
            ("3.5", 0, "4"),
            ("-2.5", 0, "-2"),
            ("-3.5", 0, "-4"),
            ("2.51", 0, "3"),
            ("0.125", 2, "0.12"),
            ("0.135", 2, "0.14"),
            ("1.2", 3, "1.200"),
        ];
        for (from, scale, want) in cases {
            assert_eq!(
                d(from).rescale(scale).unwrap().to_string(),
                want,
                "{from} at {scale}"
            );
        }
    }

    #[test]
    fn arithmetic_follows_the_scale_rules() {
        assert_eq!(
            d("1.10").checked_add(&d("2.05")).unwrap().to_string(),
            "3.15"
        );
        assert_eq!(
            d("1.5").checked_sub(&d("2.25")).unwrap().to_string(),
            "-0.75"
        );
        // a product's scale is the sum of the scales (§8.3)
        assert_eq!(
            d("1.5").checked_mul(&d("2.25")).unwrap().to_string(),
            "3.375"
        );
        // a quotient kept at the left operand's scale, half to even
        assert_eq!(
            d("10.00").checked_div(&d("3"), 2).unwrap().to_string(),
            "3.33"
        );
        assert_eq!(
            d("0.05").checked_div(&d("2"), 2).unwrap().to_string(),
            "0.02"
        );
        assert_eq!(
            d("0.07").checked_div(&d("2"), 2).unwrap().to_string(),
            "0.04"
        );
        // dividing by zero is unknown, not an error (§8.2)
        assert!(d("1").checked_div(&d("0"), 2).is_none());
        assert!(d("1.00").checked_div(&d("0.00"), 2).is_none());
    }

    #[test]
    fn arithmetic_that_cannot_be_held_is_refused() {
        let big = Decimal::new(i128::MAX, 0).unwrap();
        assert!(big.checked_add(&d("1")).is_none());
        assert!(big.checked_mul(&d("2")).is_none());
        assert!(Decimal::new(1, 39).is_none());
    }

    #[test]
    fn decimals_order_across_scales() {
        assert_eq!(d("1.50").cmp(&d("1.5")), Ordering::Equal);
        assert!(d("1.49") < d("1.5"));
        assert!(d("-0.01") < d("0"));
    }

    #[test]
    fn a_whole_decimal_is_an_int() {
        assert_eq!(d("3.00").to_int(), Some(3));
        assert_eq!(d("3.10").to_int(), None);
    }

    #[test]
    fn durations_read_and_write_in_the_models_units() {
        assert_eq!(parse_duration("90 min"), Some(5400));
        assert_eq!(parse_duration("1 week"), Some(604_800));
        assert_eq!(parse_duration("2 months"), None);
        assert_eq!(duration_text(5400), "90 min");
        assert_eq!(duration_text(3600), "1 h");
        assert_eq!(duration_text(172_800), "2 days");
        assert_eq!(duration_text(0), "0 s");
        assert_eq!(duration_text(-3600), "-1 h");
    }

    #[test]
    fn timestamps_and_dates_round_trip_at_utc() {
        for text in [
            "1970-01-01T00:00:00Z",
            "2026-10-04T09:30:00Z",
            "2000-02-29T23:59:59Z",
            "1969-12-31T23:59:59Z",
        ] {
            assert_eq!(timestamp_text(parse_timestamp(text).unwrap()), text);
        }
        assert_eq!(
            parse_timestamp("2026-01-01T00:00Z"),
            parse_timestamp("2026-01-01T00:00:00Z")
        );
        assert_eq!(parse_timestamp("2026-01-01T00:00:00Z"), Some(1_767_225_600));
        assert!(parse_timestamp("2026-02-30T00:00:00Z").is_none());
        assert!(parse_timestamp("2026-01-01T24:00:00Z").is_none());
        assert!(parse_timestamp("2026-01-01T00:00:00").is_none());
        assert_eq!(date_text(parse_date("2024-02-29").unwrap()), "2024-02-29");
        assert!(parse_date("2023-02-29").is_none());
    }

    /// A date and a timestamp are read as RFC 3339 writes them, each field
    /// its digits exactly and the year 0001 to 9999: a space, a sign, a
    /// one-digit field, a year past 9999 or the year 0 was read and stored
    /// (second production review, minor).
    #[test]
    fn dates_and_timestamps_are_read_exactly() {
        for good in ["0001-01-01", "9999-12-31", "2024-02-29"] {
            assert!(parse_date(good).is_some(), "{good}");
        }
        for bad in [
            "0000-01-01",
            "10000-01-01",
            "2026-2-3",
            "2026-02-3",
            " 2026-10-01",
            "2026-10-01 ",
            "+2026-10-01",
            "-2026-10-01",
            "20261001",
            "2026-10-01T00:00:00Z",
        ] {
            assert!(parse_date(bad).is_none(), "{bad}");
        }
        for good in [
            "9999-12-31T23:59:59Z",
            "2026-10-04T09:30Z",
            "2026-10-04T09:30:00+00:00",
        ] {
            assert!(parse_timestamp(good).is_some(), "{good}");
        }
        for bad in [
            "10000-01-01T00:00:00Z",
            "2026-10-04T9:30:00Z",
            "2026-10-04T09:30:0Z",
            " 2026-10-04T09:30:00Z",
            "2026-10-04T09:30:00+0000",
            "2026-10-04T09:30:00+08:00",
            "2026-10-04T09:30:00.123Z",
            "2026-10-04t09:30:00Z",
        ] {
            assert!(parse_timestamp(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn comparison_needs_one_type() {
        assert_eq!(
            Value::Int(2).compare(&Value::Decimal(d("1.50"))),
            Some(Ordering::Greater)
        );
        assert!(Value::Int(1).equals(&Value::Decimal(d("1.00"))));
        let usd = |a: &str| Value::Money {
            currency: "USD".into(),
            amount: d(a),
        };
        let sgd = Value::Money {
            currency: "SGD".into(),
            amount: d("1.00"),
        };
        assert_eq!(usd("1.00").compare(&usd("2.00")), Some(Ordering::Less));
        assert_eq!(usd("1.00").compare(&sgd), None);
        assert_eq!(Value::Text("a".into()).compare(&Value::Int(1)), None);
    }

    #[test]
    fn minor_units_are_the_currencys() {
        assert_eq!(minor_unit("USD"), Some(2));
        assert_eq!(minor_unit("JPY"), Some(0));
        assert_eq!(minor_unit("BHD"), Some(3));
        assert_eq!(minor_unit("XXX"), None);
    }
}
