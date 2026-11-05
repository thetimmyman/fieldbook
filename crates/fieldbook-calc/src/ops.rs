//! Exact scaled-integer decimal and money arithmetic (AD-14, DT-04).
//!
//! Ordinary operations never round: a result that cannot be represented exactly in
//! [`Dec`] returns `Ok(Value::Unknown(Reason::Inexact))`, and a result whose magnitude
//! exceeds [`Dec::MAX`] returns `Err(CalcError::Overflow)`. Stage-aware division
//! ([`div_at_stage`]) rounds the exact rational quotient once at the named [`Stage`].

use rust_decimal::Decimal;

use crate::decimal::Dec;
use crate::error::CalcError;
use crate::money::Money;
use crate::rounding::{RoundingPolicy, Stage, round_quotient_at_stage};
use crate::value::{Reason, Value};

/// Maximum unsigned 96-bit mantissa of [`Decimal`] (`2^96 - 1`).
pub(crate) const MAX_MANTISSA: u128 = 79_228_162_514_264_337_593_543_950_335;

/// Unsigned 384-bit integer (`6 × 64` bits) in big-endian field order for exact
/// intermediate rational arithmetic without array indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct U384 {
    pub w5: u64,
    pub w4: u64,
    pub w3: u64,
    pub w2: u64,
    pub w1: u64,
    pub w0: u64,
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "intentionally extracts the low 64-bit word of a u128"
)]
const fn low_u64(v: u128) -> u64 {
    v as u64
}

impl U384 {
    pub const ZERO: Self = Self {
        w5: 0,
        w4: 0,
        w3: 0,
        w2: 0,
        w1: 0,
        w0: 0,
    };

    pub const fn from_u128(v: u128) -> Self {
        Self {
            w5: 0,
            w4: 0,
            w3: 0,
            w2: 0,
            w1: low_u64(v >> 64),
            w0: low_u64(v),
        }
    }

    pub const fn is_zero(self) -> bool {
        self.w5 == 0 && self.w4 == 0 && self.w3 == 0 && self.w2 == 0 && self.w1 == 0 && self.w0 == 0
    }

    pub const fn to_u128(self) -> Option<u128> {
        if self.w5 == 0 && self.w4 == 0 && self.w3 == 0 && self.w2 == 0 {
            Some((self.w1 as u128) << 64 | (self.w0 as u128))
        } else {
            None
        }
    }

    pub const fn add(self, rhs: Self) -> Self {
        let s0 = (self.w0 as u128) + (rhs.w0 as u128);
        let w0 = low_u64(s0);
        let c0 = s0 >> 64;

        let s1 = (self.w1 as u128) + (rhs.w1 as u128) + c0;
        let w1 = low_u64(s1);
        let c1 = s1 >> 64;

        let s2 = (self.w2 as u128) + (rhs.w2 as u128) + c1;
        let w2 = low_u64(s2);
        let c2 = s2 >> 64;

        let s3 = (self.w3 as u128) + (rhs.w3 as u128) + c2;
        let w3 = low_u64(s3);
        let c3 = s3 >> 64;

        let s4 = (self.w4 as u128) + (rhs.w4 as u128) + c3;
        let w4 = low_u64(s4);
        let c4 = s4 >> 64;

        let s5 = (self.w5 as u128) + (rhs.w5 as u128) + c4;
        let w5 = low_u64(s5);

        Self {
            w5,
            w4,
            w3,
            w2,
            w1,
            w0,
        }
    }

    pub const fn sub(self, rhs: Self) -> Self {
        let (w0, b0) = self.w0.overflowing_sub(rhs.w0);
        let (t1, b1a) = self.w1.overflowing_sub(rhs.w1);
        let (w1, b1b) = t1.overflowing_sub(b0 as u64);
        let b1 = b1a || b1b;

        let (t2, b2a) = self.w2.overflowing_sub(rhs.w2);
        let (w2, b2b) = t2.overflowing_sub(b1 as u64);
        let b2 = b2a || b2b;

        let (t3, b3a) = self.w3.overflowing_sub(rhs.w3);
        let (w3, b3b) = t3.overflowing_sub(b2 as u64);
        let b3 = b3a || b3b;

        let (t4, b4a) = self.w4.overflowing_sub(rhs.w4);
        let (w4, b4b) = t4.overflowing_sub(b3 as u64);
        let b4 = b4a || b4b;

        let (t5, _) = self.w5.overflowing_sub(rhs.w5);
        let (w5, _) = t5.overflowing_sub(b4 as u64);

        Self {
            w5,
            w4,
            w3,
            w2,
            w1,
            w0,
        }
    }

    pub const fn mul_u64(self, rhs: u64) -> Self {
        let m = rhs as u128;

        let p0 = (self.w0 as u128) * m;
        let w0 = low_u64(p0);
        let c0 = p0 >> 64;

        let p1 = (self.w1 as u128) * m + c0;
        let w1 = low_u64(p1);
        let c1 = p1 >> 64;

        let p2 = (self.w2 as u128) * m + c1;
        let w2 = low_u64(p2);
        let c2 = p2 >> 64;

        let p3 = (self.w3 as u128) * m + c2;
        let w3 = low_u64(p3);
        let c3 = p3 >> 64;

        let p4 = (self.w4 as u128) * m + c3;
        let w4 = low_u64(p4);
        let c4 = p4 >> 64;

        let p5 = (self.w5 as u128) * m + c4;
        let w5 = low_u64(p5);

        Self {
            w5,
            w4,
            w3,
            w2,
            w1,
            w0,
        }
    }

    const fn shift_left_word(self) -> Self {
        Self {
            w5: self.w4,
            w4: self.w3,
            w3: self.w2,
            w2: self.w1,
            w1: self.w0,
            w0: 0,
        }
    }

    pub const fn mul_u128(self, rhs: u128) -> Self {
        let lo = self.mul_u64(low_u64(rhs));
        let hi = self.mul_u64(low_u64(rhs >> 64)).shift_left_word();
        lo.add(hi)
    }

    pub fn mul_pow10(mut self, exp: u32) -> Self {
        for _ in 0..exp {
            self = self.mul_u64(10);
        }
        self
    }

    const fn bit(self, idx: u32) -> u64 {
        let word = match idx / 64 {
            0 => self.w0,
            1 => self.w1,
            2 => self.w2,
            3 => self.w3,
            4 => self.w4,
            _ => self.w5,
        };
        (word >> (idx % 64)) & 1
    }

    const fn set_bit(mut self, idx: u32) -> Self {
        let mask = 1u64 << (idx % 64);
        match idx / 64 {
            0 => self.w0 |= mask,
            1 => self.w1 |= mask,
            2 => self.w2 |= mask,
            3 => self.w3 |= mask,
            4 => self.w4 |= mask,
            _ => self.w5 |= mask,
        }
        self
    }

    const fn shl1_with_bit(self, bit: u64) -> Self {
        Self {
            w5: (self.w5 << 1) | (self.w4 >> 63),
            w4: (self.w4 << 1) | (self.w3 >> 63),
            w3: (self.w3 << 1) | (self.w2 >> 63),
            w2: (self.w2 << 1) | (self.w1 >> 63),
            w1: (self.w1 << 1) | (self.w0 >> 63),
            w0: (self.w0 << 1) | (bit & 1),
        }
    }

    pub fn div_rem(self, divisor: Self) -> (Self, Self) {
        let mut quot = Self::ZERO;
        let mut rem = Self::ZERO;
        for i in (0..384).rev() {
            rem = rem.shl1_with_bit(self.bit(i));
            if rem >= divisor {
                rem = rem.sub(divisor);
                quot = quot.set_bit(i);
            }
        }
        (quot, rem)
    }
}

const fn pow10_u128(exp: u32) -> u128 {
    let mut out = 1u128;
    let mut i = 0;
    while i < exp {
        out *= 10;
        i += 1;
    }
    out
}

/// Converts an exact rational `(-1)^negative * num / den` (with `den > 0`) into a [`Value<Dec>`]
/// without rounding, starting from preferred scale `pref_scale` (`0..=28`).
///
/// - If `num / den > MAX_MANTISSA`, returns `Err(CalcError::Overflow)`.
/// - If `num / den` cannot be represented exactly as `m * 10^(-s)` with `0 <= s <= 28` and
///   `0 <= m <= MAX_MANTISSA`, returns `Ok(Value::Unknown(Reason::Inexact))`.
pub(crate) fn exact_rational_to_dec(
    negative: bool,
    num: U384,
    den: U384,
    pref_scale: u32,
) -> Result<Value<Dec>, CalcError> {
    let mut s = pref_scale.min(RoundingPolicy::MAX_SCALE);
    if num.is_zero() {
        return Ok(Value::Known(Dec::from_decimal(
            Decimal::from_i128_with_scale(0, s),
        )));
    }

    let max_m = U384::from_u128(MAX_MANTISSA);
    if num > den.mul_u128(MAX_MANTISSA) {
        return Err(CalcError::Overflow);
    }

    let ten = U384::from_u128(10);
    let (mut q, mut r) = num.mul_pow10(s).div_rem(den);

    if r.is_zero() {
        while q > max_m && s > 0 {
            let (q_next, rem10) = q.div_rem(ten);
            if !rem10.is_zero() {
                return Ok(Value::Unknown(Reason::Inexact));
            }
            q = q_next;
            s -= 1;
        }
        if q > max_m {
            return Err(CalcError::Overflow);
        }
    } else {
        while !r.is_zero() && s < RoundingPolicy::MAX_SCALE {
            let (digit_q, next_r) = r.mul_u64(10).div_rem(den);
            q = q.mul_u64(10).add(digit_q);
            r = next_r;
            s += 1;
            if q > max_m {
                return Ok(Value::Unknown(Reason::Inexact));
            }
        }
        if !r.is_zero() || q > max_m {
            return Ok(Value::Unknown(Reason::Inexact));
        }
    }

    let q_u128 = q.to_u128().ok_or(CalcError::Overflow)?;
    let q_i128 = i128::try_from(q_u128).map_err(|_| CalcError::Overflow)?;
    let signed_m = if negative && q_i128 != 0 {
        -q_i128
    } else {
        q_i128
    };
    Ok(Value::Known(Dec::from_decimal(
        Decimal::from_i128_with_scale(signed_m, s),
    )))
}

fn add_signed_exact(a: Dec, b: Dec, negate_b: bool) -> Result<Value<Dec>, CalcError> {
    let s_a = a.scale();
    let s_b = b.scale();
    let s = s_a.max(s_b);

    let mag_a = U384::from_u128(a.mantissa().unsigned_abs()).mul_pow10(s - s_a);
    let mag_b = U384::from_u128(b.mantissa().unsigned_abs()).mul_pow10(s - s_b);

    let neg_a = a.is_negative();
    let neg_b = if b.is_zero() {
        false
    } else {
        b.is_negative() ^ negate_b
    };

    let (negative, total_mag) = if neg_a == neg_b {
        (neg_a, mag_a.add(mag_b))
    } else if mag_a >= mag_b {
        (neg_a, mag_a.sub(mag_b))
    } else {
        (neg_b, mag_b.sub(mag_a))
    };

    let den = U384::from_u128(1).mul_pow10(s);
    exact_rational_to_dec(negative, total_mag, den, s)
}

/// Exact checked addition of two [`Value<Dec>`] operands.
pub fn add_dec(a: Value<Dec>, b: Value<Dec>) -> Result<Value<Dec>, CalcError> {
    match (a, b) {
        (Value::Unknown(r), _) | (Value::Known(_), Value::Unknown(r)) => Ok(Value::Unknown(r)),
        (Value::Known(x), Value::Known(y)) => add_signed_exact(x, y, false),
    }
}

/// Exact checked subtraction of two [`Value<Dec>`] operands.
pub fn sub_dec(a: Value<Dec>, b: Value<Dec>) -> Result<Value<Dec>, CalcError> {
    match (a, b) {
        (Value::Unknown(r), _) | (Value::Known(_), Value::Unknown(r)) => Ok(Value::Unknown(r)),
        (Value::Known(x), Value::Known(y)) => add_signed_exact(x, y, true),
    }
}

/// Exact checked multiplication of two [`Value<Dec>`] operands.
///
/// Returns `Ok(Value::Unknown(Reason::Inexact))` on precision loss and
/// `Err(CalcError::Overflow)` when the exact product exceeds [`Dec::MAX`].
pub fn mul_dec(a: Value<Dec>, b: Value<Dec>) -> Result<Value<Dec>, CalcError> {
    let (x, y) = match (a, b) {
        (Value::Unknown(r), _) | (Value::Known(_), Value::Unknown(r)) => {
            return Ok(Value::Unknown(r));
        }
        (Value::Known(x), Value::Known(y)) => (x, y),
    };
    let mag_x = x.mantissa().unsigned_abs();
    let mag_y = y.mantissa().unsigned_abs();
    let s_x = x.scale();
    let s_y = y.scale();

    let int_x = mag_x / pow10_u128(s_x);
    let int_y = mag_y / pow10_u128(s_y);
    if (int_x << 32).checked_mul(int_y).is_none() {
        return Err(CalcError::Overflow);
    }

    let negative = x.is_negative() ^ y.is_negative();
    let num = U384::from_u128(mag_x).mul_u128(mag_y);
    let den = U384::from_u128(1).mul_pow10(s_x + s_y);
    exact_rational_to_dec(
        negative,
        num,
        den,
        (s_x + s_y).min(RoundingPolicy::MAX_SCALE),
    )
}

/// Exact checked division of two [`Value<Dec>`] operands.
///
/// - A zero divisor returns `Ok(Value::Unknown(Reason::ZeroDenominator))`.
/// - A non-terminating or >28-scale quotient returns `Ok(Value::Unknown(Reason::Inexact))`.
/// - A quotient exceeding [`Dec::MAX`] returns `Err(CalcError::Overflow)`.
pub fn div_dec(a: Value<Dec>, b: Value<Dec>) -> Result<Value<Dec>, CalcError> {
    let (x, y) = match (a, b) {
        (Value::Unknown(r), _) | (Value::Known(_), Value::Unknown(r)) => {
            return Ok(Value::Unknown(r));
        }
        (Value::Known(x), Value::Known(y)) => (x, y),
    };
    if y.is_zero() {
        return Ok(Value::Unknown(Reason::ZeroDenominator));
    }

    let negative = x.is_negative() ^ y.is_negative();
    let mag_x = x.mantissa().unsigned_abs();
    let mag_y = y.mantissa().unsigned_abs();
    let s_x = x.scale();
    let s_y = y.scale();

    let num = U384::from_u128(mag_x).mul_pow10(s_y);
    let den = U384::from_u128(mag_y).mul_pow10(s_x);
    exact_rational_to_dec(negative, num, den, s_x.saturating_sub(s_y))
}

/// Exact checked addition of two [`Value<Money>`] operands.
///
/// Mixing currencies returns `Err(CalcError::Mismatch)`.
pub fn add(a: Value<Money>, b: Value<Money>) -> Result<Value<Money>, CalcError> {
    let (x, y) = match (a, b) {
        (Value::Unknown(r), _) | (Value::Known(_), Value::Unknown(r)) => {
            return Ok(Value::Unknown(r));
        }
        (Value::Known(x), Value::Known(y)) => (x, y),
    };
    if x.currency != y.currency {
        return Err(CalcError::Mismatch);
    }
    let out = add_signed_exact(x.amount, y.amount, false)?;
    Ok(out.map(|amount| Money::new(amount, x.currency)))
}

/// Exact checked subtraction of two [`Value<Money>`] operands.
///
/// Mixing currencies returns `Err(CalcError::Mismatch)`.
pub fn sub(a: Value<Money>, b: Value<Money>) -> Result<Value<Money>, CalcError> {
    let (x, y) = match (a, b) {
        (Value::Unknown(r), _) | (Value::Known(_), Value::Unknown(r)) => {
            return Ok(Value::Unknown(r));
        }
        (Value::Known(x), Value::Known(y)) => (x, y),
    };
    if x.currency != y.currency {
        return Err(CalcError::Mismatch);
    }
    let out = add_signed_exact(x.amount, y.amount, true)?;
    Ok(out.map(|amount| Money::new(amount, x.currency)))
}

/// Exact checked multiplication of a [`Value<Money>`] amount by a [`Value<Dec>`] factor.
pub fn mul(a: Value<Money>, factor: Value<Dec>) -> Result<Value<Money>, CalcError> {
    let (m, f) = match (a, factor) {
        (Value::Unknown(r), _) | (Value::Known(_), Value::Unknown(r)) => {
            return Ok(Value::Unknown(r));
        }
        (Value::Known(m), Value::Known(f)) => (m, f),
    };
    let int_a = m.amount.mantissa().unsigned_abs() / pow10_u128(m.amount.scale());
    let int_b = f.mantissa().unsigned_abs() / pow10_u128(f.scale());
    if (int_a << 32).checked_mul(int_b).is_none() {
        return Err(CalcError::Overflow);
    }
    let out = mul_dec(Value::Known(m.amount), Value::Known(f))?;
    Ok(out.map(|amount| Money::new(amount, m.currency)))
}

/// Exact checked division of a [`Value<Money>`] amount by a [`Value<Dec>`] divisor.
///
/// A zero divisor returns `Ok(Value::Unknown(Reason::ZeroDenominator))`.
pub fn div(a: Value<Money>, divisor: Value<Dec>) -> Result<Value<Money>, CalcError> {
    let (m, d) = match (a, divisor) {
        (Value::Unknown(r), _) | (Value::Known(_), Value::Unknown(r)) => {
            return Ok(Value::Unknown(r));
        }
        (Value::Known(m), Value::Known(d)) => (m, d),
    };
    let out = div_dec(Value::Known(m.amount), Value::Known(d))?;
    Ok(out.map(|amount| Money::new(amount, m.currency)))
}

/// Exact checked percentage (`amount × percent / 100`), dividing by 100 before the
/// representability test.
pub fn percent_of(amount: Value<Money>, percent: Value<Dec>) -> Result<Value<Money>, CalcError> {
    let (m, p) = match (amount, percent) {
        (Value::Unknown(r), _) | (Value::Known(_), Value::Unknown(r)) => {
            return Ok(Value::Unknown(r));
        }
        (Value::Known(m), Value::Known(p)) => (m, p),
    };
    let negative = m.amount.is_negative() ^ p.is_negative();
    let mag_a = m.amount.mantissa().unsigned_abs();
    let mag_p = p.mantissa().unsigned_abs();
    let s_a = m.amount.scale();
    let s_p = p.scale();

    let num = U384::from_u128(mag_a).mul_u128(mag_p);
    let den = U384::from_u128(1).mul_pow10(s_a + s_p + 2);
    let out = exact_rational_to_dec(
        negative,
        num,
        den,
        (s_a + s_p).min(RoundingPolicy::MAX_SCALE),
    )?;
    Ok(out.map(|dec| Money::new(dec, m.currency)))
}

/// Stage-aware division of a [`Value<Money>`] amount by a [`Value<Dec>`] divisor.
///
/// Rounds the exact rational quotient once at `stage` using `policy` (`MidpointAwayFromZero`),
/// without pre-rounding to an intermediate [`Dec`].
pub fn div_at_stage(
    a: Value<Money>,
    divisor: Value<Dec>,
    stage: Stage,
    policy: &RoundingPolicy,
) -> Result<Value<Money>, CalcError> {
    if policy.scale() > RoundingPolicy::MAX_SCALE {
        return Err(CalcError::InvalidScale);
    }
    let (m, d) = match (a, divisor) {
        (Value::Unknown(r), _) | (Value::Known(_), Value::Unknown(r)) => {
            return Ok(Value::Unknown(r));
        }
        (Value::Known(m), Value::Known(d)) => (m, d),
    };
    if d.is_zero() {
        return Ok(Value::Unknown(Reason::ZeroDenominator));
    }

    let negative = m.amount.is_negative() ^ d.is_negative();
    let num_mag = m.amount.mantissa().unsigned_abs();
    let den_mag = d.mantissa().unsigned_abs();
    let s_a = m.amount.scale();
    let s_b = d.scale();
    let target_scale = policy.scale();

    let scaled_num = U384::from_u128(num_mag).mul_pow10(s_b + target_scale);
    let scaled_den = U384::from_u128(den_mag).mul_pow10(s_a);
    let (quot, rem) = scaled_num.div_rem(scaled_den);
    let out = round_quotient_at_stage(negative, quot, rem, scaled_den, stage, policy)?;
    Ok(out.map(|dec| Money::new(dec, m.currency)))
}

/// Scales `amount` by the exact positive integer ratio `num_factor / den_factor`
/// after cancelling common factors, without rounding.
pub(crate) fn scale_dec_by_ratio(
    amount: Dec,
    num_factor: u128,
    den_factor: u128,
) -> Result<Value<Dec>, CalcError> {
    let negative = amount.is_negative();
    let mag_a = amount.mantissa().unsigned_abs();
    let s_a = amount.scale();

    let g1 = gcd_u128(num_factor, den_factor);
    let nf = num_factor / g1;
    let df = den_factor / g1;

    let g2 = gcd_u128(mag_a, df);
    let reduced_mag = mag_a / g2;
    let reduced_df = df / g2;

    let num = U384::from_u128(reduced_mag).mul_u128(nf);
    let den = U384::from_u128(reduced_df).mul_pow10(s_a);
    exact_rational_to_dec(negative, num, den, s_a)
}

pub(crate) const fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    if a == 0 { 1 } else { a }
}
