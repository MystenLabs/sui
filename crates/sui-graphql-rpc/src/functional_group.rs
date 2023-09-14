// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

/// Logical Groups categorise APIs exposed by GraphQL.  Groups can be enabled or disabled based on
/// settings in the RPC's TOML configuration file.
#[derive(Deserialize, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum FunctionalGroup {
    /// Statistics about how the network was running (TPS, top packages, APY, etc)
    Analytics,

    /// Coin metadata, per-address coin and balance information.
    Coins,

    /// Querying an object's dynamic fields.
    DynamicFields,

    /// SuiNS name and reverse name look-up.
    NameServer,

    /// Struct and function signatures, and popular packages.
    Packages,

    /// Transaction and Event subscriptions.
    Subscriptions,

    /// Information about the system that changes from epoch to epoch (protocol config, committee,
    /// reference gas price).
    SystemState,
}
