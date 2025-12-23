// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Liquidity pool detection and decoding module.
//!
//! This module identifies Move objects belonging to known DEX protocols
//! and extracts token type information from generic type parameters.

use move_core_types::language_storage::{StructTag, TypeTag};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use sui_types::base_types::ObjectID;
use sui_types::digests::TransactionDigest;
use sui_types::object::Object;

/// Known DEX protocol identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DexProtocol {
    Cetus,
    Turbos,
    SuiSwap,
    DeepBook,
    Kriya,
    Aftermath,
    Unknown,
}

impl DexProtocol {
    /// Returns the protocol name as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            DexProtocol::Cetus => "cetus",
            DexProtocol::Turbos => "turbos",
            DexProtocol::SuiSwap => "suiswap",
            DexProtocol::DeepBook => "deepbook",
            DexProtocol::Kriya => "kriya",
            DexProtocol::Aftermath => "aftermath",
            DexProtocol::Unknown => "unknown",
        }
    }

    /// Check if this protocol matches a regex pattern
    pub fn matches_pattern(&self, pattern: &Regex) -> bool {
        pattern.is_match(self.as_str())
    }
}

impl std::fmt::Display for DexProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Known DEX pool type patterns for detection
struct DexPattern {
    protocol: DexProtocol,
    /// Module name pattern to match
    module_pattern: &'static str,
    /// Struct name pattern to match
    struct_pattern: &'static str,
}

/// Static list of known DEX patterns
static DEX_PATTERNS: LazyLock<Vec<DexPattern>> = LazyLock::new(|| {
    vec![
        // Cetus Protocol pools
        DexPattern {
            protocol: DexProtocol::Cetus,
            module_pattern: "pool",
            struct_pattern: "Pool",
        },
        DexPattern {
            protocol: DexProtocol::Cetus,
            module_pattern: "clmm_pool",
            struct_pattern: "Pool",
        },
        // Turbos Finance pools
        DexPattern {
            protocol: DexProtocol::Turbos,
            module_pattern: "pool",
            struct_pattern: "Pool",
        },
        // SuiSwap pools
        DexPattern {
            protocol: DexProtocol::SuiSwap,
            module_pattern: "pool",
            struct_pattern: "Pool",
        },
        DexPattern {
            protocol: DexProtocol::SuiSwap,
            module_pattern: "swap",
            struct_pattern: "Pool",
        },
        // DeepBook orderbook
        DexPattern {
            protocol: DexProtocol::DeepBook,
            module_pattern: "clob",
            struct_pattern: "Pool",
        },
        DexPattern {
            protocol: DexProtocol::DeepBook,
            module_pattern: "clob_v2",
            struct_pattern: "Pool",
        },
        DexPattern {
            protocol: DexProtocol::DeepBook,
            module_pattern: "pool",
            struct_pattern: "Pool",
        },
        // Kriya DEX pools
        DexPattern {
            protocol: DexProtocol::Kriya,
            module_pattern: "spot_dex",
            struct_pattern: "Pool",
        },
        // Aftermath Finance pools
        DexPattern {
            protocol: DexProtocol::Aftermath,
            module_pattern: "pool",
            struct_pattern: "Pool",
        },
        DexPattern {
            protocol: DexProtocol::Aftermath,
            module_pattern: "amm",
            struct_pattern: "Pool",
        },
    ]
});

/// Known DEX package addresses on mainnet
static KNOWN_DEX_ADDRESSES: LazyLock<std::collections::HashMap<String, DexProtocol>> =
    LazyLock::new(|| {
        let mut map = std::collections::HashMap::new();
        // Cetus mainnet packages
        map.insert(
            "0x1eabed72c53feb3805120a081dc15963c204dc8d091542592abaf7a35689b2fb".to_string(),
            DexProtocol::Cetus,
        );
        map.insert(
            "0x714a63a0dba6da4f017b42d5d0fb78867f18bcde904868e51d951a5a6f5b7f57".to_string(),
            DexProtocol::Cetus,
        );
        // Turbos mainnet packages
        map.insert(
            "0x91bfbc386a41afcfd9b2533058d7e915a1d3829089cc268ff4333d54d6339ca1".to_string(),
            DexProtocol::Turbos,
        );
        // DeepBook mainnet packages
        map.insert(
            "0x000000000000000000000000000000000000000000000000000000000000dee9".to_string(),
            DexProtocol::DeepBook,
        );
        map.insert(
            "0xdee9".to_string(),
            DexProtocol::DeepBook,
        );
        // Kriya mainnet packages
        map.insert(
            "0xa0eba10b173538c8fecca1dff298e488402cc9ff374f8a12ca7758eebe830b66".to_string(),
            DexProtocol::Kriya,
        );
        // Aftermath mainnet packages
        map.insert(
            "0xefe8b36d5b2e43728cc323298626b83177803521d195cfb11e15b910e892fddf".to_string(),
            DexProtocol::Aftermath,
        );
        map
    });

/// Represents decoded state of a liquidity pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityPoolState {
    /// The pool object ID
    pub pool_id: ObjectID,
    /// The protocol this pool belongs to
    pub protocol: DexProtocol,
    /// Full type string of the pool (e.g., "0x...::pool::Pool<T1, T2>")
    pub pool_type: String,
    /// Token types extracted from generic parameters
    pub token_types: Vec<String>,
    /// Transaction digest that caused this update
    pub digest: TransactionDigest,
    /// Object version after this update
    pub version: u64,
    /// Raw BCS bytes of the object contents (optional, for client-side parsing)
    pub raw_bytes: Option<Vec<u8>>,
}

impl LiquidityPoolState {
    /// Create a new LiquidityPoolState from an object and transaction digest
    pub fn from_object(
        object: &Object,
        digest: TransactionDigest,
        include_raw_bytes: bool,
    ) -> Option<Self> {
        let move_obj = object.data.try_as_move()?;
        let struct_tag: StructTag = move_obj.type_().clone().into();

        let protocol = detect_protocol(&struct_tag)?;
        let token_types = extract_token_types(&struct_tag);
        let pool_type = format_struct_tag(&struct_tag);

        Some(LiquidityPoolState {
            pool_id: object.id(),
            protocol,
            pool_type,
            token_types,
            digest,
            version: object.version().value(),
            raw_bytes: if include_raw_bytes {
                Some(move_obj.contents().to_vec())
            } else {
                None
            },
        })
    }
}

/// Detect if a struct tag belongs to a known DEX protocol
pub fn detect_protocol(struct_tag: &StructTag) -> Option<DexProtocol> {
    let address = struct_tag.address.to_hex_literal();

    // First check if address is a known DEX package
    if let Some(protocol) = KNOWN_DEX_ADDRESSES.get(&address) {
        return Some(*protocol);
    }

    // Check against module/struct patterns
    let module_name = struct_tag.module.as_str();
    let struct_name = struct_tag.name.as_str();

    for pattern in DEX_PATTERNS.iter() {
        if module_name.contains(pattern.module_pattern)
            && struct_name == pattern.struct_pattern
        {
            return Some(pattern.protocol);
        }
    }

    None
}

/// Check if an object is a liquidity pool from a known DEX
pub fn is_liquidity_pool(object: &Object) -> bool {
    if let Some(move_obj) = object.data.try_as_move() {
        let struct_tag: StructTag = move_obj.type_().clone().into();
        detect_protocol(&struct_tag).is_some()
    } else {
        false
    }
}

/// Extract token type strings from a pool's generic type parameters
fn extract_token_types(struct_tag: &StructTag) -> Vec<String> {
    struct_tag
        .type_params
        .iter()
        .map(format_type_tag)
        .collect()
}

/// Format a TypeTag as a human-readable string
fn format_type_tag(type_tag: &TypeTag) -> String {
    match type_tag {
        TypeTag::Bool => "bool".to_string(),
        TypeTag::U8 => "u8".to_string(),
        TypeTag::U16 => "u16".to_string(),
        TypeTag::U32 => "u32".to_string(),
        TypeTag::U64 => "u64".to_string(),
        TypeTag::U128 => "u128".to_string(),
        TypeTag::U256 => "u256".to_string(),
        TypeTag::Address => "address".to_string(),
        TypeTag::Signer => "signer".to_string(),
        TypeTag::Vector(inner) => format!("vector<{}>", format_type_tag(inner)),
        TypeTag::Struct(s) => format_struct_tag(s),
    }
}

/// Format a StructTag as a human-readable string
fn format_struct_tag(struct_tag: &StructTag) -> String {
    let base = format!(
        "{}::{}::{}",
        struct_tag.address.to_hex_literal(),
        struct_tag.module,
        struct_tag.name
    );

    if struct_tag.type_params.is_empty() {
        base
    } else {
        let params: Vec<String> = struct_tag.type_params.iter().map(format_type_tag).collect();
        format!("{}<{}>", base, params.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_core_types::account_address::AccountAddress;
    use move_core_types::ident_str;

    #[test]
    fn test_detect_cetus_pool() {
        let struct_tag = StructTag {
            address: AccountAddress::from_hex_literal(
                "0x1eabed72c53feb3805120a081dc15963c204dc8d091542592abaf7a35689b2fb",
            )
            .unwrap(),
            module: ident_str!("pool").to_owned(),
            name: ident_str!("Pool").to_owned(),
            type_params: vec![],
        };

        let protocol = detect_protocol(&struct_tag);
        assert_eq!(protocol, Some(DexProtocol::Cetus));
    }

    #[test]
    fn test_detect_deepbook_pool() {
        let struct_tag = StructTag {
            address: AccountAddress::from_hex_literal("0xdee9").unwrap(),
            module: ident_str!("clob_v2").to_owned(),
            name: ident_str!("Pool").to_owned(),
            type_params: vec![],
        };

        let protocol = detect_protocol(&struct_tag);
        assert_eq!(protocol, Some(DexProtocol::DeepBook));
    }

    #[test]
    fn test_extract_token_types() {
        let sui_type = TypeTag::Struct(Box::new(StructTag {
            address: AccountAddress::from_hex_literal("0x2").unwrap(),
            module: ident_str!("sui").to_owned(),
            name: ident_str!("SUI").to_owned(),
            type_params: vec![],
        }));

        let usdc_type = TypeTag::Struct(Box::new(StructTag {
            address: AccountAddress::from_hex_literal("0xdba").unwrap(),
            module: ident_str!("usdc").to_owned(),
            name: ident_str!("USDC").to_owned(),
            type_params: vec![],
        }));

        let struct_tag = StructTag {
            address: AccountAddress::from_hex_literal(
                "0x1eabed72c53feb3805120a081dc15963c204dc8d091542592abaf7a35689b2fb",
            )
            .unwrap(),
            module: ident_str!("pool").to_owned(),
            name: ident_str!("Pool").to_owned(),
            type_params: vec![sui_type, usdc_type],
        };

        let tokens = extract_token_types(&struct_tag);
        assert_eq!(tokens.len(), 2);
        assert!(tokens[0].contains("sui::SUI"));
        assert!(tokens[1].contains("usdc::USDC"));
    }

    #[test]
    fn test_protocol_display() {
        assert_eq!(DexProtocol::Cetus.as_str(), "cetus");
        assert_eq!(DexProtocol::Turbos.as_str(), "turbos");
        assert_eq!(DexProtocol::DeepBook.as_str(), "deepbook");
    }

    #[test]
    fn test_protocol_pattern_matching() {
        let pattern = Regex::new("cetus|turbos").unwrap();
        assert!(DexProtocol::Cetus.matches_pattern(&pattern));
        assert!(DexProtocol::Turbos.matches_pattern(&pattern));
        assert!(!DexProtocol::DeepBook.matches_pattern(&pattern));
    }
}
