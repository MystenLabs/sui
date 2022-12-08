// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Note: u7 in a u8 is uleb-compatible, and any usage of this should be aware
/// that this field maybe updated to be uleb64 in the future
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, schemars::JsonSchema,
)]
pub struct ChainId(u8);

impl ChainId {
    pub const MAINNET: Self = Self(1);
    pub const TESTING: Self = Self(127);

    /// Create a `ChainId`.
    ///
    /// Current valid range for a ChainId is: [1, 127].
    pub fn new(id: u8) -> Option<Self> {
        const VALID_RANGE: std::ops::RangeInclusive<u8> = 1..=127;

        VALID_RANGE.contains(&id).then_some(Self(id))
    }

    /// Returns the value as a primitive type.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl<'de> serde::Deserialize<'de> for ChainId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(rename = "ChainId")]
        struct Value(u8);

        let value = Value::deserialize(deserializer)?.0;
        ChainId::new(value)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid chain id: {value}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_ids() {
        let mainnet = 1;
        let mainnet_chain_id = ChainId::new(mainnet).unwrap();
        assert_eq!(mainnet_chain_id, ChainId::MAINNET);
        assert_eq!(ChainId::MAINNET.get(), 1);

        let testing = 127;
        let testing_chain_id = ChainId::new(testing).unwrap();
        assert_eq!(testing_chain_id, ChainId::TESTING);
        assert_eq!(ChainId::TESTING.get(), 127);

        assert_ne!(ChainId::TESTING, ChainId::MAINNET);
    }

    #[test]
    fn valid_and_invalid_values() {
        assert!(ChainId::new(0).is_none());

        for i in 1..=127 {
            let chain_id = ChainId::new(i).unwrap();
            assert_eq!(chain_id.get(), i);
        }

        for i in 128..=u8::MAX {
            assert!(ChainId::new(i).is_none());
        }
    }
}
