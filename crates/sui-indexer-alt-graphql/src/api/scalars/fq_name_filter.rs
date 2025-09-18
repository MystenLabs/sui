// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, str::FromStr};

use sui_types::parse_sui_fq_name;

use crate::api::{
    scalars::{impl_string_input, module_filter::ModuleFilter, sui_address::SuiAddress},
    types::transaction::filter::Error,
};

/// GraphQL scalar containing a filter on fully-qualified names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FqNameFilter {
    /// Filter the module member by the package or module it's from.
    Module(ModuleFilter),

    /// Exact match on the module member.
    FqName(SuiAddress, String, String),
}
impl_string_input!(FqNameFilter);

impl FromStr for FqNameFilter {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        if let Ok((module, name)) = parse_sui_fq_name(s) {
            Ok(FqNameFilter::FqName(
                SuiAddress::from(*module.address()),
                module.name().to_string(),
                name,
            ))
        } else if let Ok(filter) = ModuleFilter::from_str(s) {
            Ok(FqNameFilter::Module(filter))
        } else {
            Err(Error::InvalidFormat("package[::module[::function]]"))
        }
    }
}

impl fmt::Display for FqNameFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FqNameFilter::Module(m) => write!(f, "{m}"),
            FqNameFilter::FqName(p, m, n) => write!(f, "{p}::{m}::{n}"),
        }
    }
}
