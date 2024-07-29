// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;

use crate::{crypto::DefaultHash, digests::Digest};
use fastcrypto::hash::HashFunction;
use serde::{Deserialize, Serialize};
pub use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

/// Models the set of protocol versions supported by a validator.
/// The `sui-node` binary will always use the SYSTEM_DEFAULT constant, but for testing we need
/// to be able to inject arbitrary versions into SuiNode.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct SupportedProtocolVersions {
    pub min: ProtocolVersion,
    pub max: ProtocolVersion,
}

impl SupportedProtocolVersions {
    pub const SYSTEM_DEFAULT: Self = Self {
        min: ProtocolVersion::MIN,
        max: ProtocolVersion::MAX,
    };

    /// Use by VersionedProtocolMessage implementors to describe in which range of versions a
    /// message variant is supported.
    pub fn new_for_message(min: u64, max: u64) -> Self {
        let min = ProtocolVersion::new(min);
        let max = ProtocolVersion::new(max);
        Self { min, max }
    }

    pub fn new_for_testing(min: u64, max: u64) -> Self {
        let min = min.into();
        let max = max.into();
        Self { min, max }
    }

    pub fn is_version_supported(&self, v: ProtocolVersion) -> bool {
        v.as_u64() >= self.min.as_u64() && v.as_u64() <= self.max.as_u64()
    }

    pub fn as_range(&self) -> RangeInclusive<u64> {
        self.min.as_u64()..=self.max.as_u64()
    }

    pub fn truncate_below(self, v: ProtocolVersion) -> Self {
        let min = std::cmp::max(self.min, v);
        Self { min, max: self.max }
    }
}

/// Models the set of protocol versions supported by a validator.
/// The `sui-node` binary will always use the SYSTEM_DEFAULT constant, but for testing we need
/// to be able to inject arbitrary versions into SuiNode.
#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
pub struct SupportedProtocolVersionsWithHashes {
    pub versions: Vec<(ProtocolVersion, Digest)>,
}

impl SupportedProtocolVersionsWithHashes {
    pub fn get_version_digest(&self, v: ProtocolVersion) -> Option<Digest> {
        self.versions
            .iter()
            .find(|(version, _)| *version == v)
            .map(|(_, digest)| *digest)
    }

    // Ideally this would be in sui-protocol-config, but sui-types depends on sui-protocol-config,
    // so it would introduce a circular dependency.
    fn protocol_config_digest(config: &ProtocolConfig) -> Digest {
        let mut digest = DefaultHash::default();
        bcs::serialize_into(&mut digest, &config).expect("serialization cannot fail");
        Digest::new(digest.finalize().into())
    }

    pub fn from_supported_versions(supported: SupportedProtocolVersions, chain: Chain) -> Self {
        Self {
            versions: supported
                .as_range()
                .map(|v| {
                    (
                        v.into(),
                        Self::protocol_config_digest(&ProtocolConfig::get_for_version(
                            v.into(),
                            chain,
                        )),
                    )
                })
                .collect(),
        }
    }
}
