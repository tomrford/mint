use crate::abi::Abi;
use crate::integers::{parse_c_unsigned, split_integer_suffix};

/// C11 integer constant types, in conversion-rank order. All supported
/// profiles have 32-bit long and 64-bit long long; C28x has 16-bit int.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Int,
    Long,
    LongLong,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Integer {
    value: i128,
    rank: Rank,
    unsigned: bool,
}

impl Integer {
    pub fn value(self) -> i128 {
        self.value
    }

    fn width(self, abi: Abi) -> u32 {
        match self.rank {
            Rank::Int => abi.int_bits(),
            Rank::Long => 32,
            Rank::LongLong => 64,
        }
    }

    fn checked(self, value: i128, abi: Abi) -> Result<Self, &'static str> {
        let width = self.width(abi);
        let value = if self.unsigned {
            value.rem_euclid(1i128 << width)
        } else {
            let limit = 1i128 << (width - 1);
            if !(-limit..limit).contains(&value) {
                return Err("signed integer overflow in shape expression");
            }
            value
        };
        Ok(Self { value, ..self })
    }

    pub fn enumerator(value: i128, abi: Abi) -> Result<Self, &'static str> {
        Self {
            value: 0,
            rank: Rank::Int,
            unsigned: false,
        }
        .checked(value, abi)
        .map_err(|_| "enumerator must fit the ABI's signed int (C11)")
    }

    pub fn literal(text: &str, abi: Abi) -> Result<Self, String> {
        let value = parse_c_unsigned(text)?;
        let (body, suffix) = split_integer_suffix(text)?;
        let suffix = suffix.to_ascii_lowercase();
        let first = if suffix.contains("ll") {
            Rank::LongLong
        } else if suffix.contains('l') {
            Rank::Long
        } else {
            Rank::Int
        };
        let explicitly_unsigned = suffix.contains('u');
        let decimal = !body.starts_with('0') || body == "0";
        for rank in [Rank::Int, Rank::Long, Rank::LongLong] {
            if rank < first {
                continue;
            }
            for unsigned in [false, true] {
                if explicitly_unsigned && !unsigned || !explicitly_unsigned && decimal && unsigned {
                    continue;
                }
                let ty = Self {
                    value: 0,
                    rank,
                    unsigned,
                };
                let max = (1u128 << (ty.width(abi) - u32::from(!unsigned))) - 1;
                if value <= max {
                    return Ok(Self {
                        value: value as i128,
                        ..ty
                    });
                }
            }
        }
        Err(format!(
            "integer literal '{text}' does not fit a C11 integer type"
        ))
    }

    pub fn negate(self, abi: Abi) -> Result<Self, &'static str> {
        self.checked(-self.value, abi)
    }

    pub fn binary(self, rhs: Self, op: char, abi: Abi) -> Result<Self, &'static str> {
        // C11 6.3.1.8: usual arithmetic conversions. Rank matters even
        // where two signed types have the same width.
        let (high, low) = if self.rank >= rhs.rank {
            (self, rhs)
        } else {
            (rhs, self)
        };
        let unsigned = high.unsigned || low.unsigned && high.width(abi) <= low.width(abi);
        let ty = Self { unsigned, ..high };
        let a = ty.checked(self.value, abi)?.value;
        let b = ty.checked(rhs.value, abi)?.value;
        if matches!(op, '/' | '%') {
            if b == 0 {
                return Err("division or modulo by zero");
            }
            if !unsigned && a == -(1i128 << (ty.width(abi) - 1)) && b == -1 {
                return Err("signed integer overflow in shape expression");
            }
        }
        // u64 products fit u128; signed 64-bit products fit i128. Rust's
        // intermediate width is an implementation detail, not C semantics.
        let value = match op {
            '+' => a + b,
            '-' => a - b,
            '*' if unsigned => ((a as u128 * b as u128) % (1u128 << ty.width(abi))) as i128,
            '*' => a * b,
            '/' => a / b,
            '%' => a % b,
            _ => unreachable!(),
        };
        ty.checked(value, abi)
    }
}
