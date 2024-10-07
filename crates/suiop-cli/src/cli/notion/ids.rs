// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;
use std::fmt::Error;

pub trait Identifier: Display {
    fn value(&self) -> &str;
}
/// Meant to be a helpful trait allowing anything that can be
/// identified by the type specified in `ById`.
pub trait AsIdentifier<ById: Identifier> {
    fn as_id(&self) -> &ById;
}

impl<T> AsIdentifier<T> for T
where
    T: Identifier,
{
    fn as_id(&self) -> &T {
        self
    }
}

impl<T> AsIdentifier<T> for &T
where
    T: Identifier,
{
    fn as_id(&self) -> &T {
        self
    }
}

macro_rules! identifer {
    ($name:ident) => {
        #[derive(serde::Serialize, serde::Deserialize, Debug, Eq, PartialEq, Hash, Clone)]
        #[serde(transparent)]
        pub struct $name(String);

        impl Identifier for $name {
            fn value(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl std::str::FromStr for $name {
            type Err = Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok($name(s.to_string()))
            }
        }
    };
}

identifer!(DatabaseId);
identifer!(PageId);
identifer!(BlockId);
identifer!(UserId);
identifer!(PropertyId);

impl From<PageId> for BlockId {
    fn from(page_id: PageId) -> Self {
        BlockId(page_id.0)
    }
}
