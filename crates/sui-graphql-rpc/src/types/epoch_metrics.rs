// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use async_graphql::*;

/// Rewards and storage fund metrics for an epoch that is not the current epoch.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct EpochMetrics {
    /// The total number of checkpoints in this epoch.
    pub(crate) total_checkpoints: Option<BigInt>,
    /// The total amount of gas fees (in MIST) that were paid in this epoch.
    pub(crate) total_gas_fees: Option<BigInt>,
    /// The total number of stake rewards that were generated in this epoch.
    pub(crate) total_stake_rewards: Option<BigInt>,
    /// The total number of stake subsidies in this epoch.
    pub(crate) total_stake_subsidies: Option<BigInt>,
    /// The storage fund available in this epoch.
    /// This fund is used to redistribute storage fees from past transactions
    /// to future validators.
    pub(crate) fund_size: Option<BigInt>,
    /// The difference between the fund inflow and outflow, representing
    /// the net amount of storage fees accumulated in this epoch.
    pub(crate) net_inflow: Option<BigInt>,
    /// The amount of storage fees paid for transactions executed during the epoch.
    pub(crate) fund_inflow: Option<BigInt>,
    /// The amount of storage fee rebates paid to users
    /// who deleted the data associated with past transactions.
    pub(crate) fund_outflow: Option<BigInt>,
}
