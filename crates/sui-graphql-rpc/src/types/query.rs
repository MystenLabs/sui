// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{connection::Connection, *};
use fastcrypto::encoding::{Base64, Encoding};
use sui_indexer::schema::transactions::gas_budget;
use sui_json_rpc::name_service::NameServiceConfig;
use sui_json_rpc_types::{DevInspectResults, DryRunTransactionBlockResponse};
use sui_sdk::SuiClient;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::transaction::{TransactionData, TransactionDataAPI, TransactionKind};

use super::{
    address::Address,
    checkpoint::{Checkpoint, CheckpointId},
    coin::Coin,
    coin_metadata::CoinMetadata,
    dry_run_result::DryRunResult,
    epoch::Epoch,
    event::{Event, EventFilter},
    object::{Object, ObjectFilter},
    owner::{ObjectOwner, Owner},
    protocol_config::ProtocolConfigs,
    sui_address::SuiAddress,
    sui_system_state_summary::SuiSystemStateSummary,
    transaction_block::{TransactionBlock, TransactionBlockFilter},
    transaction_meta::TransactionMeta,
};
use crate::{
    config::ServiceConfig, context_data::db_data_provider::PgManager, deserialize_tx_data,
    error::Error, mutation::Mutation,
};

pub(crate) struct Query;
pub(crate) type SuiGraphQLSchema = async_graphql::Schema<Query, Mutation, EmptySubscription>;

///[Object]
impl Query {
    /// First four bytes of the network's genesis checkpoint digest (uniquely identifies the
    /// network).
    async fn chain_identifier(&self, ctx: &Context<'_>) -> Result<String> {
        ctx.data_unchecked::<PgManager>()
            .fetch_chain_identifier()
            .await
            .extend()
    }

    /// Configuration for this RPC service
    async fn service_config(&self, ctx: &Context<'_>) -> Result<ServiceConfig> {
        ctx.data()
            .map_err(|_| Error::Internal("Unable to fetch service configuration.".to_string()))
            .cloned()
            .extend()
    }

    // availableRange - pending impl. on IndexerV2
    // coinMetadata

    /// Simulate running a transaction to inspect its effects without
    /// committing to them on-chain.
    ///
    /// `txBytes` either a `TransactionData` struct or a `TransactionKind`
    ///     struct, BCS-encoded and then Base64-encoded.  The expected
    ///     type is controlled by the presence or absence of `txMeta`: If
    ///     present, `txBytes` is assumed to be a `TransactionKind`, if
    ///     absent, then `TransactionData`.
    ///
    /// `txMeta` the data that is missing from a `TransactionKind` to make
    ///     a `TransactionData` (sender address and gas information).  All
    ///     its fields are nullable: `sender` defaults to `0x0`, if
    ///     `gasObjects` is not present, or is an empty list, it is
    ///     substituted with a mock Coin object, and `gasPrice` defaults to
    ///     the reference gas price.
    ///
    /// `skipChecks` optional flag to disable the usual verification
    ///     checks that prevent access to objects that are owned by
    ///     addresses other than the sender, and calling non-public,
    ///     non-entry functions.  Defaults to false.
    ///
    /// `epoch` the epoch to simulate executing the transaction in.
    ///     Defaults to the current epoch.
    async fn dry_run_transaction_block(
        &self,
        ctx: &Context<'_>,
        tx_bytes: String,
        tx_meta: Option<TransactionMeta>,
        skip_checks: Option<bool>,
        epoch: Option<u64>,
    ) -> Result<DryRunResult> {
        let skip_checks = skip_checks.unwrap_or(false);

        let sui_sdk_client: &Option<SuiClient> = ctx
            .data()
            .map_err(|_| Error::Internal("Unable to fetch Sui SDK client".to_string()))
            .extend()?;
        let sui_sdk_client = sui_sdk_client
            .as_ref()
            .ok_or_else(|| Error::Internal("Sui SDK client not initialized".to_string()))
            .extend()?;

        let exec_arg = if let Some(TransactionMeta {
            sender,
            gas_price,
            gas_objects, // TODO: How do we make this a mock coin when None?
        }) = tx_meta
        {
            // This implies `TransactionKind`
            let tx_kind = deserialize_tx_data::<TransactionKind>(tx_bytes)?;

            // Default is 0x0
            let sender_address = sender.unwrap_or_else(|| SuiAddress::ZERO).into();

            // Default is the reference gas price which is handled by the sdk internally
            let gas_price = gas_price
                .map(|x| x.to_u64())
                .transpose()
                // TODO: repr the error without debug? Current doesn't impl Display
                .map_err(|e| Error::Client(format!("`Unable to parse `gasPrice` to u64: {:?}", e)))
                .extend()?;

            // Default is the current epoch which is handled by the sdk internally
            let epoch = epoch.map(|x| x.into());

            DryRunExecArg::TransactionKindWithMeta {
                kind: tx_kind,
                sender: sender_address,
                gas_price,
                epoch,
                gas_objects: gas_objects.map(|x| x.iter().map(|x| x.into()).collect()),
            }
        } else {
            // This implies `TransactionData`
            let tx_data = deserialize_tx_data::<TransactionData>(tx_bytes)?;

            DryRunExecArg::TransactionData(tx_data)
        };

        // This determines if we use dry run or dev inspect when calling the FN API
        if skip_checks {
            let (tx_kind, sender_address, gas_price, epoch) =
                exec_arg.into_transaction_kind_with_meta();
            let dev_inspect_result = sui_sdk_client
                .read_api()
                .dev_inspect_transaction_block(
                    sender_address,
                    tx_kind,
                    gas_price.map(|x| x.into()),
                    epoch.map(|x| x.into()),
                )
                .await?;
            DryRunExecResult::DevInspect(dev_inspect_result)
        } else {
            let tx_data = exec_arg.into_transaction_data();
            let dry_run_result = sui_sdk_client
                .read_api()
                .dry_run_transaction_block(tx_data)
                .await?;
            DryRunExecResult::DryRun(dry_run_result)
        }
        .into_dry_run_result()
    }

    async fn owner(&self, address: SuiAddress) -> Option<ObjectOwner> {
        Some(ObjectOwner::Owner(Owner { address }))
    }

    async fn object(
        &self,
        ctx: &Context<'_>,
        address: SuiAddress,
        version: Option<u64>,
    ) -> Result<Option<Object>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_obj(address, version)
            .await
            .extend()
    }

    async fn address(&self, address: SuiAddress) -> Option<Address> {
        Some(Address { address })
    }

    async fn epoch(&self, ctx: &Context<'_>, id: Option<u64>) -> Result<Option<Epoch>> {
        if let Some(epoch_id) = id {
            ctx.data_unchecked::<PgManager>()
                .fetch_epoch(epoch_id)
                .await
                .extend()
        } else {
            Ok(Some(
                ctx.data_unchecked::<PgManager>()
                    .fetch_latest_epoch()
                    .await
                    .extend()?,
            ))
        }
    }

    async fn checkpoint(
        &self,
        ctx: &Context<'_>,
        id: Option<CheckpointId>,
    ) -> Result<Option<Checkpoint>> {
        if let Some(id) = id {
            match (&id.digest, &id.sequence_number) {
                (Some(_), Some(_)) => Err(Error::InvalidCheckpointQuery.extend()),
                _ => ctx
                    .data_unchecked::<PgManager>()
                    .fetch_checkpoint(id.digest.as_deref(), id.sequence_number)
                    .await
                    .extend(),
            }
        } else {
            Ok(Some(
                ctx.data_unchecked::<PgManager>()
                    .fetch_latest_checkpoint()
                    .await
                    .extend()?,
            ))
        }
    }

    async fn transaction_block(
        &self,
        ctx: &Context<'_>,
        digest: String,
    ) -> Result<Option<TransactionBlock>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_tx(&digest)
            .await
            .extend()
    }

    /// The coin objects that exist in the network.
    ///
    /// The type field is a string of the inner type of the coin by which to filter
    /// (e.g. `0x2::sui::SUI`). If no type is provided, it will default to `0x2::sui::SUI`.
    async fn coin_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        type_: Option<String>,
    ) -> Result<Option<Connection<String, Coin>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_coins(None, type_, first, after, last, before)
            .await
            .extend()
    }

    async fn checkpoint_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Checkpoint>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_checkpoints(first, after, last, before, None)
            .await
            .extend()
    }

    async fn transaction_block_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Option<Connection<String, TransactionBlock>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_txs(first, after, last, before, filter)
            .await
            .extend()
    }

    async fn event_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: EventFilter,
    ) -> Result<Option<Connection<String, Event>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_events(first, after, last, before, filter)
            .await
            .extend()
    }

    async fn object_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, Object>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_objs(first, after, last, before, filter)
            .await
            .extend()
    }

    async fn protocol_config(
        &self,
        ctx: &Context<'_>,
        protocol_version: Option<u64>,
    ) -> Result<ProtocolConfigs> {
        ctx.data_unchecked::<PgManager>()
            .fetch_protocol_configs(protocol_version)
            .await
            .extend()
    }

    /// Resolves the owner address of the provided domain name
    async fn resolve_name_service_address(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> Result<Option<Address>> {
        ctx.data_unchecked::<PgManager>()
            .resolve_name_service_address(ctx.data_unchecked::<NameServiceConfig>(), name)
            .await
            .extend()
    }

    async fn latest_sui_system_state(&self, ctx: &Context<'_>) -> Result<SuiSystemStateSummary> {
        ctx.data_unchecked::<PgManager>()
            .fetch_latest_sui_system_state()
            .await
            .extend()
    }

    async fn coin_metadata(
        &self,
        ctx: &Context<'_>,
        coin_type: String,
    ) -> Result<Option<CoinMetadata>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_coin_metadata(coin_type)
            .await
            .extend()
    }
}

// Intermediate enum to handle the two possible types of `tx_bytes` in `dry_run_transaction_block` usiing FN
enum DryRunExecArg {
    TransactionData(TransactionData),
    TransactionKindWithMeta {
        kind: TransactionKind,
        sender: NativeSuiAddress,
        gas_price: Option<u64>,
        epoch: Option<u64>,
        gas_objects: Option<Vec<NativeSuiAddress>>,
    },
}

impl DryRunExecArg {
    pub fn into_transaction_data(self) -> TransactionData {
        match self {
            DryRunExecArg::TransactionData(td) => return td,

            // We hit this scenario if the TransactionKind and meta are provided
            // and skip_checks is false.
            // This requires forming a full TransactionData object.
            DryRunExecArg::TransactionKindWithMeta {
                kind,
                sender,
                gas_price,
                epoch,
                gas_objects,
            } => {
                // TODO: We need to handle the gas data

                // We dont have enough info for these
                // let gas_payment: ObjectRef;
                // let gas_budget: u64;
                TransactionData::new_with_epoch(
                    kind,
                    sender,
                    gas_payment,
                    gas_budget,
                    gas_price,
                    epoch,
                )
            }
        }
    }

    pub fn into_transaction_kind_with_meta(
        self,
    ) -> (
        TransactionKind,
        /* Sender */ NativeSuiAddress,
        /* Gas price */ Option<u64>,
        /* Epoch */ Option<u64>,
    ) {
        match self {
            DryRunExecArg::TransactionData(td) => {
                let kind = td.kind();
                let sender = td.sender().into();
                let gas_price = td.gas_price();
                let epoch = td.expiration().into();
                (kind.clone(), sender, Some(gas_price), epoch)
            }
            DryRunExecArg::TransactionKindWithMeta {
                kind,
                sender,
                gas_price,
                epoch,
                ..
            } => (kind, sender, gas_price, epoch),
        }
    }
}

// Intermediate enum to handle the two possible return types of `dry_run_transaction_block` using FN
enum DryRunExecResult {
    DryRun(DryRunTransactionBlockResponse),
    DevInspect(DevInspectResults),
}

impl DryRunExecResult {
    pub fn into_dry_run_result(self) -> Result<DryRunResult> {
        match self {
            DryRunExecResult::DryRun(dr) => unimplemented!(),
            DryRunExecResult::DevInspect(_) => unimplemented!(),
        }
    }
}
