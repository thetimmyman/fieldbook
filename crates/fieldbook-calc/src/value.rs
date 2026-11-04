//! Values that may be `Known` or `Unknown` with a typed `Reason`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Why a calculated value is `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// Division by a zero denominator.
    ZeroDenominator,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDenominator => f.write_str("zero denominator"),
        }
    }
}

/// A calculation value that is either `Known(T)` or `Unknown(Reason)`.
///
/// Every operation on an `Unknown` value returns that `Unknown` unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Value<T> {
    /// A known value.
    Known(T),
    /// An unknown value with its reason.
    Unknown(Reason),
}

impl<T> Value<T> {
    /// Returns `true` when the value is `Known`.
    #[must_use]
    pub const fn is_known(&self) -> bool {
        matches!(self, Self::Known(_))
    }

    /// Returns `true` when the value is `Unknown`.
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
    }

    /// Converts from `&Value<T>` to `Value<&T>`.
    #[must_use]
    pub const fn as_ref(&self) -> Value<&T> {
        match self {
            Self::Known(v) => Value::Known(v),
            Self::Unknown(r) => Value::Unknown(*r),
        }
    }

    /// Extracts the known value, if present.
    #[must_use]
    pub fn known(self) -> Option<T> {
        match self {
            Self::Known(v) => Some(v),
            Self::Unknown(_) => None,
        }
    }

    /// Extracts the unknown reason, if present.
    #[must_use]
    pub const fn reason(&self) -> Option<Reason> {
        match self {
            Self::Known(_) => None,
            Self::Unknown(r) => Some(*r),
        }
    }

    /// Maps a `Value<T>` to `Value<U>` by applying `f` to a `Known` value,
    /// leaving `Unknown` unchanged.
    #[must_use]
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Value<U> {
        match self {
            Self::Known(v) => Value::Known(f(v)),
            Self::Unknown(r) => Value::Unknown(r),
        }
    }

    /// Chains a computation that itself returns `Value<U>`, leaving `Unknown` unchanged.
    #[must_use]
    pub fn and_then<U, F: FnOnce(T) -> Value<U>>(self, f: F) -> Value<U> {
        match self {
            Self::Known(v) => f(v),
            Self::Unknown(r) => Value::Unknown(r),
        }
    }

    /// Maps a fallible operation over a `Known` value, propagating `Unknown` unchanged.
    pub fn try_map<U, E, F: FnOnce(T) -> Result<U, E>>(self, f: F) -> Result<Value<U>, E> {
        match self {
            Self::Known(v) => Ok(Value::Known(f(v)?)),
            Self::Unknown(r) => Ok(Value::Unknown(r)),
        }
    }

    /// Chains a fallible operation that returns `Result<Value<U>, E>`,
    /// propagating `Unknown` unchanged.
    pub fn try_and_then<U, E, F: FnOnce(T) -> Result<Value<U>, E>>(
        self,
        f: F,
    ) -> Result<Value<U>, E> {
        match self {
            Self::Known(v) => f(v),
            Self::Unknown(r) => Ok(Value::Unknown(r)),
        }
    }

    /// Combines two `Value`s with `f`, returning the first `Unknown` unchanged if either is unknown.
    #[must_use]
    pub fn zip_with<U, V, F: FnOnce(T, U) -> V>(self, other: Value<U>, f: F) -> Value<V> {
        match (self, other) {
            (Self::Known(a), Value::Known(b)) => Value::Known(f(a, b)),
            (Self::Unknown(r), _) | (Self::Known(_), Value::Unknown(r)) => Value::Unknown(r),
        }
    }

    /// Combines two `Value`s with a fallible function `f`, returning the first `Unknown` unchanged.
    pub fn try_zip_with<U, V, E, F: FnOnce(T, U) -> Result<V, E>>(
        self,
        other: Value<U>,
        f: F,
    ) -> Result<Value<V>, E> {
        match (self, other) {
            (Self::Known(a), Value::Known(b)) => Ok(Value::Known(f(a, b)?)),
            (Self::Unknown(r), _) | (Self::Known(_), Value::Unknown(r)) => Ok(Value::Unknown(r)),
        }
    }
}

impl<T: fmt::Display> fmt::Display for Value<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known(v) => fmt::Display::fmt(v, f),
            Self::Unknown(r) => write!(f, "n/a: {r}"),
        }
    }
}
