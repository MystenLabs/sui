// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Client-side utilities for interacting with a Sui network from Rust.
//!
//! The JSON-RPC client this crate used to provide was removed together with
//! the fullnode JSON-RPC service. What remains is the wallet and configuration
//! layer shared by the Sui CLI and the tooling in this repository:
//!
//! * [`wallet_context::WalletContext`] - the keystore-backed wallet used to
//!   sign and execute transactions against a network over gRPC.
//! * [`sui_client_config::SuiClientConfig`] - the persisted client
//!   configuration (`client.yaml`) and its named network environments.
//! * [`verify_personal_message_signature`] - verification of personal message
//!   signatures through a fullnode.
//!
//! For a general-purpose Rust SDK, use the
//! [`sui-rust-sdk`](https://github.com/MystenLabs/sui-rust-sdk) crate.

pub use sui_crypto;
pub use sui_rpc;
pub use sui_sdk_types;
pub use sui_types as types;

pub mod digests;
pub mod error;
pub mod sui_client_config;
pub mod verify_personal_message_signature;
pub mod wallet_context;

pub const SUI_COIN_TYPE: &str = "0x2::sui::SUI";
pub const SUI_LOCAL_NETWORK_URL: &str = "http://127.0.0.1:9000";
pub const SUI_LOCAL_NETWORK_URL_0: &str = "http://0.0.0.0:9000";
pub const SUI_LOCAL_NETWORK_GAS_URL: &str = "http://127.0.0.1:5003/v2/gas";
pub const SUI_DEVNET_URL: &str = "https://fullnode.devnet.sui.io:443";
pub const SUI_TESTNET_URL: &str = "https://fullnode.testnet.sui.io:443";
pub const SUI_MAINNET_URL: &str = "https://fullnode.mainnet.sui.io:443";
