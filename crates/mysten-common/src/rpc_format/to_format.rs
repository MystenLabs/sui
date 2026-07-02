// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use crate::rpc_format::Format;
use crate::rpc_format::Meter;
use crate::rpc_format::MeterError;

/// Render `self` into any [`Format`] sink. Designed for use by code that wants to expose a
/// statically typed Rust value (e.g. each field of `ProtocolConfig`) through one of the
/// `Format`-implementing wire types (`serde_json::Value`, `prost_types::Value`, etc.) without
/// having to enumerate the destination type at the call site.
///
/// Integer types wider than 32 bits are rendered through [`Format::string`] rather than
/// [`Format::number`] because `Format::number` is defined to take `u32`. This also matches the
/// convention used elsewhere in the codebase to dodge the IEEE-754 precision loss that bites
/// JavaScript clients consuming JSON numbers above 2^53.
pub trait ToFormat {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError>;
}

impl ToFormat for bool {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        F::bool(meter, *self)
    }
}

impl ToFormat for u8 {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        F::number(meter, u32::from(*self))
    }
}

impl ToFormat for u16 {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        F::number(meter, u32::from(*self))
    }
}

impl ToFormat for u32 {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        F::number(meter, *self)
    }
}

impl ToFormat for u64 {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        F::string(meter, self.to_string())
    }
}

impl ToFormat for u128 {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        F::string(meter, self.to_string())
    }
}

impl ToFormat for usize {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        // Treat `usize` as potentially wider than 32 bits — it's `u64` on every target we ship.
        F::string(meter, self.to_string())
    }
}

impl ToFormat for String {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        F::string(meter, self.clone())
    }
}

impl ToFormat for &str {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        F::string(meter, (*self).to_owned())
    }
}

impl<T: ToFormat> ToFormat for Option<T> {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        match self {
            Some(inner) => inner.to_format(meter),
            None => F::null(meter),
        }
    }
}

impl<T: ToFormat> ToFormat for Vec<T> {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        let mut out = F::Vec::default();
        // Scope the nested meter so it's dropped before we reuse `meter` to finalize the vec.
        {
            let mut nested = meter.nest()?;
            for item in self {
                let elem = item.to_format::<F, _>(&mut nested)?;
                F::vec_push_element(&mut nested, &mut out, elem)?;
            }
        }
        F::vec(meter, out)
    }
}

impl<T: ToFormat> ToFormat for [T] {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        let mut out = F::Vec::default();
        {
            let mut nested = meter.nest()?;
            for item in self {
                let elem = item.to_format::<F, _>(&mut nested)?;
                F::vec_push_element(&mut nested, &mut out, elem)?;
            }
        }
        F::vec(meter, out)
    }
}

impl<T: ToFormat> ToFormat for BTreeSet<T> {
    fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
        let mut out = F::Vec::default();
        {
            let mut nested = meter.nest()?;
            for item in self {
                let elem = item.to_format::<F, _>(&mut nested)?;
                F::vec_push_element(&mut nested, &mut out, elem)?;
            }
        }
        F::vec(meter, out)
    }
}

macro_rules! impl_to_format_for_tuple {
    ($($name:ident),+) => {
        impl<$($name: ToFormat),+> ToFormat for ($($name,)+) {
            fn to_format<F: Format, M: Meter>(&self, meter: &mut M) -> Result<F, MeterError> {
                #[allow(non_snake_case)]
                let ($($name,)+) = self;
                let mut out = F::Vec::default();
                {
                    let mut nested = meter.nest()?;
                    $(
                        let elem = $name.to_format::<F, _>(&mut nested)?;
                        F::vec_push_element(&mut nested, &mut out, elem)?;
                    )+
                }
                F::vec(meter, out)
            }
        }
    };
}

impl_to_format_for_tuple!(T1, T2);
impl_to_format_for_tuple!(T1, T2, T3);
impl_to_format_for_tuple!(T1, T2, T3, T4);
