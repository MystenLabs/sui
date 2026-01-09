// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use sui_name_service::Domain as NativeDomain;

use crate::api::scalars::impl_string_input;

/// Wrap SuiNS domain type to expose as a string scalar in GraphQL.
#[derive(Serialize, Deserialize)]
pub(crate) struct Domain(NativeDomain);

impl_string_input!(Domain);

impl Deref for Domain {
    type Target = NativeDomain;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for Domain {
    type Err = <NativeDomain as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Domain(NativeDomain::from_str(s)?))
    }
}

impl From<NativeDomain> for Domain {
    fn from(value: NativeDomain) -> Self {
        Domain(value)
    }
}
