// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::context_ext::DataProviderContextExt;

use super::big_int::BigInt;
use super::date_time::DateTime;
use super::protocol_config::ProtocolConfigs;
use super::validator_set::ValidatorSet;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct Epoch {
    pub epoch_id: u64,
    #[graphql(skip)]
    pub protocol_version: u64,
    pub reference_gas_price: Option<BigInt>,
    pub validator_set: Option<ValidatorSet>,
    pub start_timestamp: Option<DateTime>,
    pub end_timestamp: Option<DateTime>,
}

#[ComplexObject]
impl Epoch {
    async fn protocol_configs(&self, ctx: &Context<'_>) -> Result<Option<ProtocolConfigs>> {
        Ok(Some(
            ctx.data_provider()
                .fetch_protocol_config(Some(self.protocol_version))
                .await?,
        ))
    }
}
