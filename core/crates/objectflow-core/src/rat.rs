//! An exact rational over 128-bit integers, its arithmetic checked: what a
//! metric's value is computed in before it is rounded once (ADR-0146).

use std::cmp::Ordering;

use crate::value::Decimal;

/// A fraction in lowest terms, its denominator positive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rat {
    n: i128,
    d: i128,
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    i128::try_from(a.max(1)).unwrap_or(1)
}

impl Rat {
    /// `n / d`, or none for a zero denominator or one past 128 bits.
    pub fn new(n: i128, d: i128) -> Option<Rat> {
        if d == 0 {
            return None;
        }
        let (n, d) = if d < 0 {
            (n.checked_neg()?, d.checked_neg()?)
        } else {
            (n, d)
        };
        let g = gcd(n, d);
        Some(Rat { n: n / g, d: d / g })
    }

    pub fn int(n: i128) -> Rat {
        Rat { n, d: 1 }
    }

    pub fn is_integer(&self) -> bool {
        self.d == 1
    }

    /// A decimal's exact value.
    pub fn of_decimal(d: &Decimal) -> Option<Rat> {
        Rat::new(d.units(), 10i128.checked_pow(u32::from(d.scale()))?)
    }

    pub fn checked_add(self, o: Rat) -> Option<Rat> {
        let n = self
            .n
            .checked_mul(o.d)?
            .checked_add(o.n.checked_mul(self.d)?)?;
        Rat::new(n, self.d.checked_mul(o.d)?)
    }

    pub fn checked_sub(self, o: Rat) -> Option<Rat> {
        self.checked_add(Rat {
            n: o.n.checked_neg()?,
            d: o.d,
        })
    }

    pub fn checked_mul(self, o: Rat) -> Option<Rat> {
        Rat::new(self.n.checked_mul(o.n)?, self.d.checked_mul(o.d)?)
    }

    /// The quotient, none for a zero divisor.
    pub fn checked_div(self, o: Rat) -> Option<Rat> {
        Rat::new(self.n.checked_mul(o.d)?, self.d.checked_mul(o.n)?)
    }

    /// Toward zero, as `int / int` discards its remainder (§8.3).
    pub fn trunc(self) -> i128 {
        self.n / self.d
    }

    /// The least integer at least this one.
    pub fn ceil(self) -> i128 {
        -(-self.n).div_euclid(self.d)
    }

    /// Rounded half to even to so many decimal places, as units of that
    /// scale (`declaration-syntax.md` §8.3, ADR-0128).
    pub fn half_even(self, places: u8) -> Option<i128> {
        let scale = 10i128.checked_pow(u32::from(places))?;
        let scaled = self.n.checked_mul(scale)?;
        let floor = scaled.div_euclid(self.d);
        let rest = scaled.rem_euclid(self.d);
        let twice = rest.checked_mul(2)?;
        Some(match twice.cmp(&self.d) {
            Ordering::Greater => floor + 1,
            Ordering::Equal if floor.rem_euclid(2) == 1 => floor + 1,
            _ => floor,
        })
    }
}

/// The 256-bit product of two 128-bit magnitudes, its high and low halves.
fn wide_mul(a: u128, b: u128) -> (u128, u128) {
    let half = u128::from(u64::MAX);
    let (a1, a0) = (a >> 64, a & half);
    let (b1, b0) = (b >> 64, b & half);
    let (p00, p01, p10, p11) = (a0 * b0, a0 * b1, a1 * b0, a1 * b1);
    let mid = (p00 >> 64) + (p01 & half) + (p10 & half);
    let low = (p00 & half) | (mid << 64);
    let high = p11 + (p01 >> 64) + (p10 >> 64) + (mid >> 64);
    (high, low)
}

impl Ord for Rat {
    /// Exact: by sign, then by the cross products in 256 bits, where two
    /// whose products passed 128 bits were compared as floats (the audit's
    /// values A4).
    fn cmp(&self, o: &Rat) -> Ordering {
        match (self.n.signum(), o.n.signum()) {
            (a, b) if a != b => a.cmp(&b),
            (0, _) => Ordering::Equal,
            (sign, _) => {
                let x = wide_mul(self.n.unsigned_abs(), o.d.unsigned_abs());
                let y = wide_mul(o.n.unsigned_abs(), self.d.unsigned_abs());
                if sign > 0 { x.cmp(&y) } else { y.cmp(&x) }
            }
        }
    }
}

impl PartialOrd for Rat {
    fn partial_cmp(&self, o: &Rat) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Two values whose cross products pass 128 bits compare exactly, where
    /// they compared as floats and two neighbours were equal (the audit's
    /// values A4).
    #[test]
    fn rationals_compare_exactly_past_128_bits() {
        let m = i128::MAX;
        let a = Rat::new(m - 1, m).unwrap();
        let b = Rat::new(m - 2, m - 1).unwrap();
        assert_eq!(a.cmp(&b), Ordering::Greater);
        let (na, nb) = (Rat::new(1 - m, m).unwrap(), Rat::new(2 - m, m - 1).unwrap());
        assert_eq!(na.cmp(&nb), Ordering::Less);
        assert_eq!(Rat::int(0).cmp(&Rat::int(0)), Ordering::Equal);
        assert_eq!(wide_mul(u128::MAX, u128::MAX), (u128::MAX - 1, 1));
    }

    #[test]
    fn half_even_rounds_a_tie_to_the_even_neighbour() {
        let r = |n, d| Rat::new(n, d).unwrap();
        assert_eq!(r(125, 1000).half_even(2), Some(12));
        assert_eq!(r(375, 1000).half_even(2), Some(38));
        assert_eq!(r(-125, 1000).half_even(2), Some(-12));
        assert_eq!(r(1, 3).half_even(3), Some(333));
        assert_eq!(r(5, 2).half_even(0), Some(2));
        assert_eq!(r(7, 2).half_even(0), Some(4));
    }

    #[test]
    fn arithmetic_is_exact_and_checked() {
        let r = |n, d| Rat::new(n, d).unwrap();
        assert_eq!(r(1, 3).checked_add(r(1, 6)), Some(r(1, 2)));
        assert_eq!(r(7, 2).trunc(), 3);
        assert_eq!(r(-7, 2).trunc(), -3);
        assert_eq!(r(7, 2).ceil(), 4);
        assert_eq!(r(1, 2).checked_div(Rat::int(0)), None);
        assert_eq!(Rat::int(i128::MAX).checked_add(Rat::int(1)), None);
        assert!(r(1, 3) < r(1, 2));
        assert!(r(-1, 2) < r(1, 3));
    }
}
