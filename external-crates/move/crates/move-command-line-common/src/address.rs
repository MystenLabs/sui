// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::parser::{parse_address_number, NumberFormat};
use anyhow::anyhow;
use move_core_types::account_address::AccountAddress;
use num_bigint::BigUint;
use std::{fmt, hash::Hash};

// Parsed Address, either a name or a numerical address
#[derive(Eq, PartialEq, Debug, Clone)]
pub enum ParsedAddress {
    Named(String),
    Numerical(NumericalAddress),
}

/// Numerical address represents non-named address values
/// or the assigned value of a named address
#[derive(Clone, Copy)]
pub struct NumericalAddress {
    /// the number for the address
    bytes: AccountAddress,
    /// The format (e.g. decimal or hex) for displaying the number
    format: NumberFormat,
}

impl ParsedAddress {
    pub fn into_account_address(
        self,
        mapping: &impl Fn(&str) -> Option<AccountAddress>,
    ) -> anyhow::Result<AccountAddress> {
        match self {
            Self::Named(n) => {
                mapping(n.as_str()).ok_or_else(|| anyhow!("Unbound named address: '{}'", n))
            }
            Self::Numerical(a) => Ok(a.into_inner()),
        }
    }
}

impl NumericalAddress {
    // bytes used for errors when an address is not known but is needed
    pub const DEFAULT_ERROR_ADDRESS: Self = NumericalAddress {
        bytes: AccountAddress::ONE,
        format: NumberFormat::Hex,
    };

    pub const fn new(bytes: [u8; AccountAddress::LENGTH], format: NumberFormat) -> Self {
        Self {
            bytes: AccountAddress::new(bytes),
            format,
        }
    }

    pub fn into_inner(self) -> AccountAddress {
        self.bytes
    }

    pub fn into_bytes(self) -> [u8; AccountAddress::LENGTH] {
        self.bytes.into_bytes()
    }

    pub fn parse_str(s: &str) -> Result<NumericalAddress, String> {
        match parse_address_number(s) {
            Some((n, format)) => Ok(NumericalAddress {
                bytes: AccountAddress::new(n),
                format,
            }),
            None =>
            // TODO the kind of error is in an unstable nightly API
            // But currently the only way this should fail is if the number is too long
            {
                Err(format!(
                    "Invalid address literal. The numeric value is too large. \
                    The maximum size is {} bytes",
                    AccountAddress::LENGTH,
                ))
            }
        }
    }
}

impl AsRef<[u8]> for NumericalAddress {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

impl fmt::Display for NumericalAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.format {
            NumberFormat::Decimal => {
                let n = BigUint::from_bytes_be(self.bytes.as_ref());
                write!(f, "{}", n)
            }
            NumberFormat::Hex => write!(f, "{:#X}", self),
        }
    }
}

impl fmt::Debug for NumericalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::UpperHex for NumericalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let encoded = hex::encode_upper(self.as_ref());
        let dropped = encoded
            .chars()
            .skip_while(|c| c == &'0')
            .collect::<String>();
        let prefix = if f.alternate() { "0x" } else { "" };
        if dropped.is_empty() {
            write!(f, "{}0", prefix)
        } else {
            write!(f, "{}{}", prefix, dropped)
        }
    }
}

impl PartialOrd for NumericalAddress {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for NumericalAddress {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let Self {
            bytes: self_bytes,
            format: _,
        } = self;
        let Self {
            bytes: other_bytes,
            format: _,
        } = other;
        self_bytes.cmp(other_bytes)
    }
}

impl PartialEq for NumericalAddress {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            bytes: self_bytes,
            format: _,
        } = self;
        let Self {
            bytes: other_bytes,
            format: _,
        } = other;
        self_bytes == other_bytes
    }
}
impl Eq for NumericalAddress {}

impl Hash for NumericalAddress {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Self {
            bytes: self_bytes,
            format: _,
        } = self;
        self_bytes.hash(state)
    }
}
