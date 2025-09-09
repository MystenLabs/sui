// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use std::{fmt, str::FromStr};
use sui_types::{parse_sui_address, parse_sui_module_id};

use crate::api::scalars::{impl_string_input, sui_address::SuiAddress};

/// GraphQL scalar containing a filter on modules. The filter can be one of:
///
/// - A package address: `0x2`,
/// - A module: `0x2::coin`,
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ModuleFilter {
    /// Filter by package address
    Package(SuiAddress),
    /// Filter by module (package and module name)
    Module(SuiAddress, String),
}

#[derive(thiserror::Error, Debug)]
#[error("Invalid filter format, expected: package[::module]")]
pub(crate) struct ModuleFilterError;

impl ModuleFilter {
    pub(crate) fn package(&self) -> Option<SuiAddress> {
        match self {
            ModuleFilter::Package(p) => Some(*p),
            ModuleFilter::Module(p, _) => Some(*p),
        }
    }

    pub(crate) fn module(&self) -> Option<&str> {
        match self {
            ModuleFilter::Package(_) => None,
            ModuleFilter::Module(_, m) => Some(m.as_str()),
        }
    }
}

impl_string_input!(ModuleFilter);

impl FromStr for ModuleFilter {
    type Err = ModuleFilterError;

    fn from_str(s: &str) -> Result<Self, ModuleFilterError> {
        if let Ok(module) = parse_sui_module_id(s) {
            // Try as a module ID
            Ok(ModuleFilter::Module(
                SuiAddress::from(*module.address()),
                module.name().to_string(),
            ))
        } else if let Ok(package) = parse_sui_address(s) {
            // Then try as just a package address
            Ok(ModuleFilter::Package(package.into()))
        } else {
            Err(ModuleFilterError)
        }
    }
}

impl fmt::Display for ModuleFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModuleFilter::Package(p) => write!(f, "{p}"),
            ModuleFilter::Module(p, m) => write!(f, "{p}::{m}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::*;

    #[test]
    fn test_valid_module_filters() {
        let inputs: Vec<_> = ["0x2", "0x2::coin", "0x3::staking_pool", "0x1::string"]
            .into_iter()
            .map(|i| ModuleFilter::from_str(i).unwrap().to_string())
            .collect();

        assert_snapshot!(&inputs.join("\n"), @r###"
        0x0000000000000000000000000000000000000000000000000000000000000002
        0x0000000000000000000000000000000000000000000000000000000000000002::coin
        0x0000000000000000000000000000000000000000000000000000000000000003::staking_pool
        0x0000000000000000000000000000000000000000000000000000000000000001::string
        "###);
    }

    #[test]
    fn test_invalid_module_filters() {
        for invalid_module_filter in [
            "u8",
            "address",
            "bool",
            "0x2::coin::Coin",
            "0x2::coin::Coin<0x2::sui::SUI>",
            "vector<u256>",
            "vector<0x3::staking_pool::StakedSui>",
            "not_a_real_address",
            "0x1:missing::colon",
            "0x2::trailing::",
            "0x3::mismatched::bra<0x4::ke::ts",
            "vector",
        ] {
            assert!(ModuleFilter::from_str(invalid_module_filter).is_err());
        }
    }
}
