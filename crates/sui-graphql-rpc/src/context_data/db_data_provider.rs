// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::{Limits, DEFAULT_SERVER_DB_POOL_SIZE},
    error::Error,
    types::{
        address::{Address, AddressTransactionBlockRelationship},
        balance::Balance,
        base64::Base64,
        big_int::BigInt,
        checkpoint::Checkpoint,
        coin::Coin,
        coin_metadata::CoinMetadata,
        committee_member::CommitteeMember,
        date_time::DateTime,
        digest::Digest,
        dynamic_field::{DynamicField, DynamicFieldName},
        end_of_epoch_data::EndOfEpochData,
        epoch::Epoch,
        event::{Event, EventFilter},
        gas::{GasCostSummary, GasInput},
        move_module::MoveModule,
        move_object::MoveObject,
        move_package::MovePackage,
        move_type::MoveType,
        object::{Object, ObjectFilter},
        protocol_config::{ProtocolConfigAttr, ProtocolConfigFeatureFlag, ProtocolConfigs},
        safe_mode::SafeMode,
        stake::StakedSui,
        stake_subsidy::StakeSubsidy,
        storage_fund::StorageFund,
        sui_address::SuiAddress,
        sui_system_state_summary::SuiSystemStateSummary,
        system_parameters::SystemParameters,
        transaction_block::{TransactionBlock, TransactionBlockFilter},
        transaction_block_effects::TransactionBlockEffects,
        transaction_block_kind::{
            AuthenticatorStateUpdate, ChangeEpochTransaction, ConsensusCommitPrologueTransaction,
            EndOfEpochTransaction, GenesisTransaction, ProgrammableTransaction,
            RandomnessStateUpdate, TransactionBlockKind,
        },
        transaction_signature::TransactionSignature,
        validator::Validator,
        validator_credentials::ValidatorCredentials,
        validator_set::ValidatorSet,
    },
};
use async_graphql::connection::{Connection, Edge};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl};
use move_core_types::language_storage::StructTag;
use std::str::FromStr;
use sui_indexer::{
    apis::GovernanceReadApiV2,
    indexer_reader::IndexerReader,
    models_v2::{
        checkpoints::StoredCheckpoint, epoch::StoredEpochInfo, objects::StoredObject,
        transactions::StoredTransaction,
    },
    schema_v2::transactions,
    types_v2::OwnerType,
    PgConnectionPoolConfig,
};
use sui_json_rpc::{
    coin_api::{parse_to_struct_tag, parse_to_type_tag},
    name_service::{Domain, NameRecord, NameServiceConfig},
};
use sui_json_rpc_types::{
    EventFilter as RpcEventFilter, ProtocolConfigResponse, Stake as RpcStakedSui,
};
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_types::{
    base_types::SuiAddress as NativeSuiAddress,
    base_types::{MoveObjectType, ObjectID},
    coin::{CoinMetadata as NativeCoinMetadata, TreasuryCap},
    digests::ChainIdentifier,
    digests::TransactionDigest,
    dynamic_field::{DynamicFieldType, Field},
    event::EventID,
    gas_coin::{GAS, TOTAL_SUPPLY_SUI},
    governance::StakedSui as NativeStakedSui,
    messages_checkpoint::{
        CheckpointCommitment, CheckpointDigest, EndOfEpochData as NativeEndOfEpochData,
    },
    object::Object as NativeObject,
    sui_system_state::sui_system_state_summary::{
        SuiSystemStateSummary as NativeSuiSystemStateSummary, SuiValidatorSummary,
    },
    transaction::{
        GenesisObject, SenderSignedData, TransactionDataAPI, TransactionExpiration, TransactionKind,
    },
    Identifier, TypeTag,
};

use super::db_backend::GenericQueryBuilder;

#[cfg(feature = "pg_backend")]
use super::pg_backend::{PgQueryExecutor, QueryBuilder};

#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum DbValidationError {
    #[error("Invalid checkpoint combination. 'before' or 'after' checkpoint cannot be used with 'at' checkpoint")]
    InvalidCheckpointCombination,
    #[error("Before checkpoint must be greater than after checkpoint")]
    InvalidCheckpointOrder,
    #[error("Filtering objects by package::module::type is not currently supported")]
    UnsupportedPMT,
    #[error("Filtering objects by object keys is not currently supported")]
    UnsupportedObjectKeys,
    #[error("Requires package and module")]
    RequiresPackageAndModule,
    #[error("Requires package")]
    RequiresPackage,
    #[error("'first' can only be used with 'after")]
    FirstAfter,
    #[error("'last' can only be used with 'before'")]
    LastBefore,
    #[error("Pagination is currently disabled on balances")]
    PaginationDisabledOnBalances,
    #[error("Invalid owner type. Must be Address or Object")]
    InvalidOwnerType,
    #[error("Query cost exceeded - cost: {0}, limit: {1}")]
    QueryCostExceeded(u64, u64),
    #[error("Page size exceeded - requested: {0}, limit: {1}")]
    PageSizeExceeded(u64, u64),
}

pub(crate) struct PgManager {
    pub inner: IndexerReader,
    pub limits: Limits,
}

impl PgManager {
    pub(crate) fn new(inner: IndexerReader, limits: Limits) -> Self {
        Self { inner, limits }
    }

    /// Create a new underlying reader, which is used by this type as well as other data providers.
    pub(crate) fn reader(db_url: impl Into<String>) -> Result<IndexerReader, Error> {
        Self::reader_with_config(db_url, DEFAULT_SERVER_DB_POOL_SIZE)
    }

    pub(crate) fn reader_with_config(
        db_url: impl Into<String>,
        pool_size: u32,
    ) -> Result<IndexerReader, Error> {
        let mut config = PgConnectionPoolConfig::default();
        config.set_pool_size(pool_size);
        IndexerReader::new_with_config(db_url, config)
            .map_err(|e| Error::Internal(format!("Failed to create reader: {e}")))
    }
}

/// Implement methods to query db and return StoredData
impl PgManager {
    async fn get_tx(&self, digest: Vec<u8>) -> Result<Option<StoredTransaction>, Error> {
        self.run_query_async_with_cost(
            move || Ok(QueryBuilder::get_tx_by_digest(digest.clone())),
            |query| move |conn| query.get_result::<StoredTransaction>(conn).optional(),
        )
        .await
    }

    async fn get_obj(
        &self,
        address: Vec<u8>,
        version: Option<i64>,
    ) -> Result<Option<StoredObject>, Error> {
        self.run_query_async_with_cost(
            move || Ok(QueryBuilder::get_obj(address.clone(), version)),
            |query| move |conn| query.get_result::<StoredObject>(conn).optional(),
        )
        .await
    }

    async fn get_obj_by_type(&self, object_type: String) -> Result<Option<StoredObject>, Error> {
        self.run_query_async_with_cost(
            move || Ok(QueryBuilder::get_obj_by_type(object_type.clone())),
            |query| move |conn| query.get_result::<StoredObject>(conn).optional(),
        )
        .await
    }

    pub async fn get_epoch(&self, epoch_id: Option<i64>) -> Result<Option<StoredEpochInfo>, Error> {
        let query_fn = move || {
            Ok(match epoch_id {
                Some(epoch_id) => QueryBuilder::get_epoch(epoch_id),
                None => QueryBuilder::get_latest_epoch(),
            })
        };

        self.run_query_async_with_cost(query_fn, |query| {
            move |conn| query.get_result::<StoredEpochInfo>(conn).optional()
        })
        .await
    }

    async fn get_checkpoint(
        &self,
        digest: Option<Vec<u8>>,
        sequence_number: Option<i64>,
    ) -> Result<Option<StoredCheckpoint>, Error> {
        let query = move || {
            Ok(match (digest.clone(), sequence_number) {
                (Some(digest), None) => QueryBuilder::get_checkpoint_by_digest(digest),
                (None, Some(sequence_number)) => {
                    QueryBuilder::get_checkpoint_by_sequence_number(sequence_number)
                }
                (Some(_), Some(_)) => {
                    return Err(Error::InvalidCheckpointQuery);
                }
                _ => QueryBuilder::get_latest_checkpoint(),
            })
        };

        self.run_query_async_with_cost(query, |query| {
            move |conn| query.get_result::<StoredCheckpoint>(conn).optional()
        })
        .await
    }

    async fn get_chain_identifier(&self) -> Result<ChainIdentifier, Error> {
        let result = self
            .get_checkpoint(None, Some(0))
            .await?
            .ok_or_else(|| Error::Internal("Genesis checkpoint cannot be found".to_string()))?;

        let digest = CheckpointDigest::try_from(result.checkpoint_digest).map_err(|e| {
            Error::Internal(format!(
                "Failed to convert checkpoint digest to CheckpointDigest. Error: {e}",
            ))
        })?;
        Ok(ChainIdentifier::from(digest))
    }

    /// Fetches the coins owned by the address and filters them by the given coin type.
    /// If no address is given, it fetches all available coin objects matching the coin type.
    async fn multi_get_coins(
        &self,
        address: Option<Vec<u8>>,
        coin_type: String,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<(Vec<StoredObject>, bool)>, Error> {
        let limit = self.validate_page_limit(first, last)?;
        let descending_order = last.is_some();
        let cursor = after
            .or(before)
            .map(|cursor| self.parse_obj_cursor(&cursor))
            .transpose()?;
        let coin_type = parse_to_type_tag(Some(coin_type))
            .map_err(|e| Error::InvalidCoinType(e.to_string()))?
            .to_canonical_string(/* with_prefix */ true);
        let result: Option<Vec<StoredObject>> = self
            .run_query_async_with_cost(
                move || {
                    Ok(QueryBuilder::multi_get_coins(
                        cursor.clone(),
                        descending_order,
                        limit,
                        address.clone(),
                        coin_type.clone(),
                    ))
                },
                |query| move |conn| query.load(conn).optional(),
            )
            .await?;

        result
            .map(|mut stored_objs| {
                let has_next_page = stored_objs.len() as i64 > limit;
                if has_next_page {
                    stored_objs.pop();
                }

                Ok((stored_objs, has_next_page))
            })
            .transpose()
    }

    async fn get_balance(
        &self,
        address: Vec<u8>,
        coin_type: String,
    ) -> Result<Option<(Option<i64>, Option<i64>, Option<String>)>, Error> {
        self.run_query_async_with_cost(
            move || {
                Ok(QueryBuilder::get_balance(
                    address.clone(),
                    coin_type.clone(),
                ))
            },
            |query| move |conn| query.get_result(conn).optional(),
        )
        .await
    }

    async fn multi_get_balances(
        &self,
        address: Vec<u8>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Vec<(Option<i64>, Option<i64>, Option<String>)>>, Error> {
        // Todo (wlmyng): paginating on balances does not really make sense
        // We'll always need to calculate all balances first
        if first.is_some() || after.is_some() || last.is_some() || before.is_some() {
            return Err(DbValidationError::PaginationDisabledOnBalances.into());
        }

        self.run_query_async_with_cost(
            move || Ok(QueryBuilder::multi_get_balances(address.clone())),
            |query| move |conn| query.load(conn).optional(),
        )
        .await
    }

    async fn multi_get_txs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Option<(Vec<StoredTransaction>, bool)>, Error> {
        let limit = self.validate_page_limit(first, last)?;
        let descending_order = last.is_some();
        let cursor = after
            .or(before)
            .map(|cursor| self.parse_tx_cursor(&cursor))
            .transpose()?;

        let mut after_tx_seq_num: Option<i64> = None;
        let mut before_tx_seq_num: Option<i64> = None;
        if let Some(filter) = &filter {
            if let Some(checkpoint) = filter.after_checkpoint {
                let subquery = transactions::dsl::transactions
                    .filter(transactions::dsl::checkpoint_sequence_number.eq(checkpoint as i64))
                    .order(transactions::dsl::tx_sequence_number.asc())
                    .select(transactions::dsl::tx_sequence_number)
                    .limit(1)
                    .into_boxed();

                after_tx_seq_num = self
                    .run_query_async(|conn| subquery.get_result::<i64>(conn).optional())
                    .await?;

                // Return early if we cannot find txs after the specified checkpoint
                if after_tx_seq_num.is_none() {
                    return Ok(None);
                }
            }

            if let Some(checkpoint) = filter.before_checkpoint {
                let subquery = transactions::dsl::transactions
                    .filter(transactions::dsl::checkpoint_sequence_number.eq(checkpoint as i64))
                    .order(transactions::dsl::tx_sequence_number.desc())
                    .select(transactions::dsl::tx_sequence_number)
                    .into_boxed();

                before_tx_seq_num = self
                    .run_query_async(|conn| subquery.get_result::<i64>(conn).optional())
                    .await?;

                // Return early if we cannot find tx before the specified checkpoint
                if before_tx_seq_num.is_none() {
                    return Ok(None);
                }
            }
        }

        let query = move || {
            QueryBuilder::multi_get_txs(
                cursor,
                descending_order,
                limit,
                filter.clone(),
                after_tx_seq_num,
                before_tx_seq_num,
            )
        };

        let result: Option<Vec<StoredTransaction>> = self
            .run_query_async_with_cost(query, |query| move |conn| query.load(conn).optional())
            .await?;

        result
            .map(|mut stored_txs| {
                let has_next_page = stored_txs.len() as i64 > limit;
                if has_next_page {
                    stored_txs.pop();
                }

                Ok((stored_txs, has_next_page))
            })
            .transpose()
    }

    async fn multi_get_checkpoints(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        epoch: Option<u64>,
    ) -> Result<Option<(Vec<StoredCheckpoint>, bool)>, Error> {
        let limit = self.validate_page_limit(first, last)?;
        let descending_order = last.is_some();
        let cursor = after
            .or(before)
            .map(|cursor| self.parse_checkpoint_cursor(&cursor))
            .transpose()?;

        let result: Option<Vec<StoredCheckpoint>> = self
            .run_query_async_with_cost(
                move || {
                    Ok(QueryBuilder::multi_get_checkpoints(
                        cursor,
                        descending_order,
                        limit,
                        epoch.map(|e| e as i64),
                    ))
                },
                |query| move |conn| query.load(conn).optional(),
            )
            .await?;

        result
            .map(|mut stored_checkpoints| {
                let has_next_page = stored_checkpoints.len() as i64 > limit;
                if has_next_page {
                    stored_checkpoints.pop();
                }

                Ok((stored_checkpoints, has_next_page))
            })
            .transpose()
    }

    async fn multi_get_objs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
        owner_type: Option<OwnerType>,
    ) -> Result<Option<(Vec<StoredObject>, bool)>, Error> {
        let limit = self.validate_page_limit(first, last)?;
        let descending_order = last.is_some();
        let cursor = after
            .or(before)
            .map(|cursor| self.parse_obj_cursor(&cursor))
            .transpose()?;

        let query = move || {
            QueryBuilder::multi_get_objs(
                cursor.clone(),
                descending_order,
                limit,
                filter.clone(),
                owner_type,
            )
        };

        let result: Option<Vec<StoredObject>> = self
            .run_query_async_with_cost(query, |query| move |conn| query.load(conn).optional())
            .await?;
        result
            .map(|mut stored_objs| {
                let has_next_page = stored_objs.len() as i64 > limit;
                if has_next_page {
                    stored_objs.pop();
                }

                Ok((stored_objs, has_next_page))
            })
            .transpose()
    }
}

/// Implement methods to be used by graphql resolvers
impl PgManager {
    pub(crate) fn parse_tx_cursor(&self, cursor: &str) -> Result<i64, Error> {
        let tx_sequence_number = cursor
            .parse::<i64>()
            .map_err(|_| Error::InvalidCursor("tx".to_string()))?;
        Ok(tx_sequence_number)
    }

    pub(crate) fn parse_obj_cursor(&self, cursor: &str) -> Result<Vec<u8>, Error> {
        Ok(SuiAddress::from_str(cursor)
            .map_err(|e| Error::InvalidCursor(e.to_string()))?
            .into_vec())
    }

    pub(crate) fn parse_checkpoint_cursor(&self, cursor: &str) -> Result<i64, Error> {
        let sequence_number = cursor
            .parse::<i64>()
            .map_err(|_| Error::InvalidCursor("checkpoint".to_string()))?;
        Ok(sequence_number)
    }

    pub(crate) fn parse_event_cursor(&self, cursor: String) -> Result<EventID, Error> {
        EventID::try_from(cursor).map_err(|_| Error::InvalidCursor("event".to_string()))
    }

    pub(crate) fn validate_package_dependencies(
        &self,
        package: Option<&SuiAddress>,
        module: Option<&String>,
        function: Option<&String>,
    ) -> Result<(), Error> {
        if function.is_some() {
            if package.is_none() || module.is_none() {
                return Err(DbValidationError::RequiresPackageAndModule.into());
            }
        } else if module.is_some() && package.is_none() {
            return Err(DbValidationError::RequiresPackage.into());
        }
        Ok(())
    }

    pub(crate) fn validate_tx_block_filter(
        &self,
        filter: &TransactionBlockFilter,
    ) -> Result<(), Error> {
        if filter.at_checkpoint.is_some()
            && (filter.before_checkpoint.is_some() || filter.after_checkpoint.is_some())
        {
            return Err(DbValidationError::InvalidCheckpointCombination.into());
        }
        if let (Some(before), Some(after)) = (filter.before_checkpoint, filter.after_checkpoint) {
            if before <= after {
                return Err(DbValidationError::InvalidCheckpointOrder.into());
            }
        }
        self.validate_package_dependencies(
            filter.package.as_ref(),
            filter.module.as_ref(),
            filter.function.as_ref(),
        )?;
        Ok(())
    }

    pub(crate) fn validate_obj_filter(&self, filter: &ObjectFilter) -> Result<(), Error> {
        if filter.package.is_some() || filter.module.is_some() || filter.ty.is_some() {
            return Err(DbValidationError::UnsupportedPMT.into());
        }
        if filter.object_keys.is_some() {
            return Err(DbValidationError::UnsupportedObjectKeys.into());
        }

        Ok(())
    }

    pub(crate) fn validate_page_limit(
        &self,
        first: Option<u64>,
        last: Option<u64>,
    ) -> Result<i64, Error> {
        if let Some(f) = first {
            if f > self.limits.max_page_size {
                return Err(
                    DbValidationError::PageSizeExceeded(f, self.limits.max_page_size).into(),
                );
            }
        }

        if let Some(l) = last {
            if l > self.limits.max_page_size {
                return Err(
                    DbValidationError::PageSizeExceeded(l, self.limits.max_page_size).into(),
                );
            }
        }

        // TODO (wlmyng): even though we do not allow passing in both first and last,
        // per the cursor connection specs, if both are provided, from the response,
        // we need to take the first F from the left and then take the last L from the right.
        Ok(first.or(last).unwrap_or(self.limits.default_page_size) as i64)
    }

    pub(crate) async fn fetch_tx(&self, digest: &str) -> Result<Option<TransactionBlock>, Error> {
        let digest = Digest::from_str(digest)?.into_vec();

        self.get_tx(digest)
            .await?
            .map(TransactionBlock::try_from)
            .transpose()
    }

    pub(crate) async fn fetch_latest_epoch(&self) -> Result<Epoch, Error> {
        let result = self
            .get_epoch(None)
            .await?
            .ok_or_else(|| Error::Internal("Latest epoch not found".to_string()))?;

        Epoch::try_from(result)
    }

    // To be used in scenarios where epoch may not exist, such as when epoch_id is provided by caller
    pub(crate) async fn fetch_epoch(&self, epoch_id: u64) -> Result<Option<Epoch>, Error> {
        let epoch_id = i64::try_from(epoch_id)
            .map_err(|_| Error::Internal("Failed to convert epoch id to i64".to_string()))?;
        self.get_epoch(Some(epoch_id))
            .await?
            .map(Epoch::try_from)
            .transpose()
    }

    // To be used in scenarios where epoch is expected to exist
    // For example, epoch of a transaction or checkpoint
    pub(crate) async fn fetch_epoch_strict(&self, epoch_id: u64) -> Result<Epoch, Error> {
        let result = self.fetch_epoch(epoch_id).await?;
        match result {
            Some(epoch) => Ok(epoch),
            None => Err(Error::Internal(format!("Epoch {} not found", epoch_id))),
        }
    }

    pub(crate) async fn fetch_latest_checkpoint(&self) -> Result<Checkpoint, Error> {
        let stored_checkpoint = self.get_checkpoint(None, None).await?;
        match stored_checkpoint {
            Some(stored_checkpoint) => Ok(Checkpoint::try_from(stored_checkpoint)?),
            None => Err(Error::Internal("Latest checkpoint not found".to_string())),
        }
    }

    pub(crate) async fn fetch_checkpoint(
        &self,
        digest: Option<&str>,
        sequence_number: Option<u64>,
    ) -> Result<Option<Checkpoint>, Error> {
        let stored_checkpoint = self
            .get_checkpoint(
                digest
                    .map(|digest| Digest::from_str(digest).map(|digest| digest.into_vec()))
                    .transpose()?,
                sequence_number.map(|sequence_number| sequence_number as i64),
            )
            .await?;
        stored_checkpoint.map(Checkpoint::try_from).transpose()
    }

    pub(crate) async fn fetch_chain_identifier(&self) -> Result<String, Error> {
        let result = self.get_chain_identifier().await?;
        Ok(result.to_string())
    }

    pub(crate) async fn fetch_txs_for_address(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        mut filter: Option<TransactionBlockFilter>,
        // TODO: Do we really need this when filter seems to be able to do the same?
        address_relation: (SuiAddress, AddressTransactionBlockRelationship),
    ) -> Result<Option<Connection<String, TransactionBlock>>, Error> {
        let (address, relation) = address_relation;
        if filter.is_none() {
            filter = Some(TransactionBlockFilter::default());
        }
        let address = Some(address);
        // Override filter with relation
        // TODO: is this the desired behavior?
        filter = filter.map(|mut f| {
            match relation {
                AddressTransactionBlockRelationship::Sign => f.sign_address = address,
                AddressTransactionBlockRelationship::Sent => f.sent_address = address,
                AddressTransactionBlockRelationship::Recv => f.recv_address = address,
                AddressTransactionBlockRelationship::Paid => f.paid_address = address,
            };
            f
        });

        self.fetch_txs(first, after, last, before, filter).await
    }

    pub(crate) async fn fetch_txs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Option<Connection<String, TransactionBlock>>, Error> {
        validate_cursor_pagination(&first, &after, &last, &before)?;
        if let Some(filter) = &filter {
            self.validate_tx_block_filter(filter)?;
        }

        let transactions = self
            .multi_get_txs(first, after, last, before, filter)
            .await?;

        if let Some((stored_txs, has_next_page)) = transactions {
            let mut connection = Connection::new(false, has_next_page);
            connection
                .edges
                .extend(stored_txs.into_iter().filter_map(|stored_tx| {
                    let cursor = stored_tx.tx_sequence_number.to_string();
                    TransactionBlock::try_from(stored_tx)
                        .map_err(|e| eprintln!("Error converting transaction: {:?}", e))
                        .ok()
                        .map(|tx| Edge::new(cursor, tx))
                }));
            Ok(Some(connection))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn fetch_txs_by_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> Result<Option<Vec<TransactionBlock>>, Error> {
        let tx_block_filter = TransactionBlockFilter {
            package: None,
            module: None,
            function: None,
            kind: None,
            after_checkpoint: None,
            at_checkpoint: None,
            before_checkpoint: None,
            sign_address: None,
            sent_address: None,
            recv_address: None,
            paid_address: None,
            input_object: None,
            changed_object: None,
            transaction_ids: Some(digests.iter().map(|x| x.to_string()).collect::<Vec<_>>()),
        };
        let txs = self
            .multi_get_txs(None, None, None, None, Some(tx_block_filter))
            .await?;

        let Some((txs, _)) = txs else {
            return Ok(None);
        };

        Ok(Some(
            txs.into_iter()
                .map(TransactionBlock::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }

    pub(crate) async fn fetch_obj(
        &self,
        address: SuiAddress,
        version: Option<u64>,
    ) -> Result<Option<Object>, Error> {
        let address = address.into_vec();
        let version = version.map(|v| v as i64);

        let stored_obj = self.get_obj(address, version).await?;
        stored_obj.map(Object::try_from).transpose()
    }

    pub(crate) async fn fetch_move_obj(
        &self,
        address: SuiAddress,
        version: Option<u64>,
    ) -> Result<Option<MoveObject>, Error> {
        let Some(object) = self.fetch_obj(address, version).await? else {
            return Ok(None);
        };

        Ok(Some(MoveObject::try_from(&object).map_err(|_| {
            Error::Internal(format!("{address} is not an object"))
        })?))
    }

    pub(crate) async fn fetch_move_package(
        &self,
        address: SuiAddress,
        version: Option<u64>,
    ) -> Result<Option<MovePackage>, Error> {
        let Some(object) = self.fetch_obj(address, version).await? else {
            return Ok(None);
        };

        Ok(Some(MovePackage::try_from(&object).map_err(|_| {
            Error::Internal(format!("{address} is not a package"))
        })?))
    }

    pub(crate) async fn fetch_move_module(
        &self,
        address: SuiAddress,
        name: &str,
    ) -> Result<Option<MoveModule>, Error> {
        // Fetch the latest version of a package (this means that when we fetch a module from the
        // system framework via one of its types, we will always get the latest version of that
        // module).
        let Some(package) = self.fetch_move_package(address, None).await? else {
            return Ok(None);
        };

        package.module_impl(name)
    }

    pub(crate) async fn fetch_owned_objs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
        owner: SuiAddress,
    ) -> Result<Option<Connection<String, Object>>, Error> {
        let filter = filter
            .map(|mut f| {
                f.owner = Some(owner);
                f
            })
            .unwrap_or_else(|| ObjectFilter {
                owner: Some(owner),
                ..Default::default()
            });
        self.fetch_objs(first, after, last, before, Some(filter))
            .await
    }

    pub(crate) async fn fetch_objs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, Object>>, Error> {
        validate_cursor_pagination(&first, &after, &last, &before)?;
        if let Some(filter) = &filter {
            self.validate_obj_filter(filter)?;
        }
        let objects = self
            .multi_get_objs(first, after, last, before, filter, None)
            .await?;

        if let Some((stored_objs, has_next_page)) = objects {
            let mut connection = Connection::new(false, has_next_page);
            connection
                .edges
                .extend(stored_objs.into_iter().filter_map(|stored_obj| {
                    Object::try_from(stored_obj)
                        .map_err(|e| eprintln!("Error converting object: {:?}", e))
                        .ok()
                        .map(|obj| Edge::new(obj.address.to_string(), obj))
                }));
            Ok(Some(connection))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn fetch_checkpoints(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        epoch: Option<u64>,
    ) -> Result<Option<Connection<String, Checkpoint>>, Error> {
        validate_cursor_pagination(&first, &after, &last, &before)?;
        let checkpoints = self
            .multi_get_checkpoints(first, after, last, before, epoch)
            .await?;

        if let Some((stored_checkpoints, has_next_page)) = checkpoints {
            let mut connection = Connection::new(false, has_next_page);
            connection
                .edges
                .extend(
                    stored_checkpoints
                        .into_iter()
                        .filter_map(|stored_checkpoint| {
                            let cursor = stored_checkpoint.sequence_number.to_string();
                            Checkpoint::try_from(stored_checkpoint)
                                .map_err(|e| eprintln!("Error converting checkpoint: {:?}", e))
                                .ok()
                                .map(|checkpoint| Edge::new(cursor, checkpoint))
                        }),
                );
            Ok(Some(connection))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn fetch_balance(
        &self,
        address: SuiAddress,
        coin_type: Option<String>,
    ) -> Result<Option<Balance>, Error> {
        let address = address.into_vec();
        let coin_type = parse_to_type_tag(coin_type.clone())
            .map_err(|e| Error::InvalidCoinType(e.to_string()))?;
        let result = self
            .get_balance(
                address,
                coin_type.to_canonical_string(/* with_prefix */ true),
            )
            .await?;

        match result {
            None | Some((None, None, None)) => Ok(None),

            Some((Some(balance), Some(count), Some(_coin_type))) => Ok(Some(Balance {
                coin_object_count: Some(count as u64),
                total_balance: Some(BigInt::from(balance)),
                coin_type: Some(MoveType::new(coin_type)),
            })),

            _ => Err(Error::Internal(
                "Expected fields are missing on balance calculation".to_string(),
            )),
        }
    }

    pub(crate) async fn fetch_balances(
        &self,
        address: SuiAddress,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Balance>>, Error> {
        let address = address.into_vec();
        let Some(balances) = self
            .multi_get_balances(address, first, after, last, before)
            .await?
        else {
            return Ok(None);
        };

        let mut connection = Connection::new(false, false);
        for (balance, count, coin_type) in balances {
            let (Some(balance), Some(count), Some(coin_type)) = (balance, count, coin_type) else {
                return Err(Error::Internal(
                    "Expected fields are missing on balance calculation".to_string(),
                ));
            };

            let coin_tag = TypeTag::from_str(&coin_type)
                .map_err(|e| Error::Internal(format!("Error parsing type '{coin_type}': {e}")))?;

            connection.edges.push(Edge::new(
                coin_type.clone(),
                Balance {
                    coin_object_count: Some(count as u64),
                    total_balance: Some(BigInt::from(balance)),
                    coin_type: Some(MoveType::new(coin_tag)),
                },
            ));
        }

        Ok(Some(connection))
    }

    /// Fetches all coins owned by the given address that match the given coin type.
    /// If no address is given, then it will fetch all coin objects of the given type.
    /// If no coin type is provided, it will use the default gas coin (SUI).
    pub(crate) async fn fetch_coins(
        &self,
        address: Option<SuiAddress>,
        coin_type: Option<String>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Coin>>, Error> {
        let address = address.map(|addr| addr.into_vec());
        let coin_type = coin_type.unwrap_or_else(|| {
            GAS::type_().to_canonical_string(/* with_prefix */ true)
        });

        let coins = self
            .multi_get_coins(address, coin_type, first, after, last, before)
            .await?;

        let Some((stored_objs, has_next_page)) = coins else {
            return Ok(None);
        };

        let mut connection = Connection::new(false, has_next_page);
        for stored_obj in stored_objs {
            let object = Object::try_from(stored_obj)?;

            let move_object = MoveObject::try_from(&object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {} to be a coin, but it's not an object",
                    object.address,
                ))
            })?;

            let coin_object = Coin::try_from(&move_object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {} to be a coin, but it is not",
                    object.address,
                ))
            })?;

            let cursor = move_object
                .native
                .id()
                .to_canonical_string(/* with_prefix */ true);

            connection.edges.push(Edge::new(cursor, coin_object));
        }

        Ok(Some(connection))
    }

    pub(crate) async fn resolve_name_service_address(
        &self,
        name_service_config: &NameServiceConfig,
        name: String,
    ) -> Result<Option<Address>, Error> {
        let domain = name.parse::<Domain>()?;

        let record_id = name_service_config.record_field_id(&domain);

        let field_record_object = match self.inner.get_object_in_blocking_task(record_id).await? {
            Some(o) => o,
            None => return Ok(None),
        };

        let record = field_record_object
            .to_rust::<Field<Domain, NameRecord>>()
            .ok_or_else(|| Error::Internal(format!("Malformed Object {record_id}")))?
            .value;

        Ok(record.target_address.map(|address| Address {
            address: SuiAddress::from_array(address.to_inner()),
        }))
    }

    pub(crate) async fn default_name_service_name(
        &self,
        name_service_config: &NameServiceConfig,
        address: SuiAddress,
    ) -> Result<Option<String>, Error> {
        let reverse_record_id =
            name_service_config.reverse_record_field_id(NativeSuiAddress::from(address));

        let field_reverse_record_object = match self
            .inner
            .get_object_in_blocking_task(reverse_record_id)
            .await?
        {
            Some(o) => o,
            None => return Ok(None),
        };

        let domain = field_reverse_record_object
            .to_rust::<Field<SuiAddress, Domain>>()
            .ok_or_else(|| Error::Internal(format!("Malformed Object {reverse_record_id}")))?
            .value;

        Ok(Some(domain.to_string()))
    }

    pub(crate) async fn fetch_latest_sui_system_state(
        &self,
    ) -> Result<SuiSystemStateSummary, Error> {
        let result = self
            .inner
            .spawn_blocking(|this| this.get_latest_sui_system_state())
            .await?;
        SuiSystemStateSummary::try_from(result)
    }

    pub(crate) async fn fetch_protocol_configs(
        &self,
        protocol_version: Option<u64>,
    ) -> Result<ProtocolConfigs, Error> {
        let chain = self.get_chain_identifier().await?.chain();
        let version: ProtocolVersion = if let Some(version) = protocol_version {
            (version).into()
        } else {
            (self.fetch_latest_epoch().await?.protocol_version).into()
        };

        let cfg = ProtocolConfig::get_for_version_if_supported(version, chain)
            .ok_or(Error::ProtocolVersionUnsupported(
                ProtocolVersion::MIN.as_u64(),
                ProtocolVersion::MAX.as_u64(),
            ))
            .map(ProtocolConfigResponse::from)?;

        Ok(ProtocolConfigs {
            configs: cfg
                .attributes
                .into_iter()
                .map(|(k, v)| ProtocolConfigAttr {
                    key: k,
                    // TODO:  what to return when value is None? nothing?
                    // TODO: do we want to return type info separately?
                    value: match v {
                        Some(q) => format!("{:?}", q),
                        None => "".to_string(),
                    },
                })
                .collect(),
            feature_flags: cfg
                .feature_flags
                .into_iter()
                .map(|x| ProtocolConfigFeatureFlag {
                    key: x.0,
                    value: x.1,
                })
                .collect(),
            protocol_version: cfg.protocol_version.as_u64(),
        })
    }

    pub(crate) async fn fetch_staked_sui(
        &self,
        address: SuiAddress,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, StakedSui>>, Error> {
        let obj_filter = ObjectFilter {
            package: None,
            module: None,
            ty: Some(MoveObjectType::staked_sui().to_canonical_string(/* with_prefix */ true)),
            owner: Some(address),
            object_ids: None,
            object_keys: None,
        };

        let objs = self
            .multi_get_objs(
                first,
                after,
                last,
                before,
                Some(obj_filter),
                Some(OwnerType::Address),
            )
            .await?;

        let Some((stored_objs, has_next_page)) = objs else {
            return Ok(None);
        };

        let mut connection = Connection::new(false, has_next_page);
        for stored_obj in stored_objs {
            let object = Object::try_from(stored_obj)?;

            let move_object = MoveObject::try_from(&object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {} to be a staked sui, but it is not an object.",
                    object.address,
                ))
            })?;

            let stake_object = StakedSui::try_from(&move_object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {} to be a staked sui, but it is not.",
                    object.address,
                ))
            })?;

            let cursor = move_object
                .native
                .id()
                .to_canonical_string(/* with_prefix */ true);

            connection.edges.push(Edge::new(cursor, stake_object));
        }

        Ok(Some(connection))
    }

    /// Make a request to the RPC for its representations of the staked sui we parsed out of the
    /// object.  Used to implement fields that are implemented in JSON-RPC but not GraphQL (yet).
    pub(crate) async fn fetch_rpc_staked_sui(
        &self,
        stake: NativeStakedSui,
    ) -> Result<RpcStakedSui, Error> {
        let governance_api = GovernanceReadApiV2::new(self.inner.clone());

        let mut delegated_stakes = governance_api
            .get_delegated_stakes(vec![stake])
            .await
            .map_err(|e| Error::Internal(format!("Error fetching delegated stake. {e}")))?;

        let Some(mut delegated_stake) = delegated_stakes.pop() else {
            return Err(Error::Internal(
                "Error fetching delegated stake. No pools returned.".to_string(),
            ));
        };

        let Some(stake) = delegated_stake.stakes.pop() else {
            return Err(Error::Internal(
                "Error fetching delegated stake. No stake in pool.".to_string(),
            ));
        };

        Ok(stake)
    }

    pub(crate) async fn fetch_events(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: EventFilter,
    ) -> Result<Option<Connection<String, Event>>, Error> {
        let event_filter: Result<RpcEventFilter, Error> = if let Some(sender) = filter.sender {
            let sender = NativeSuiAddress::from_bytes(sender.into_array())
                .map_err(|_| Error::InvalidFilter)?;
            Ok(RpcEventFilter::Sender(sender))
        } else if let Some(digest) = filter.transaction_digest {
            let digest = TransactionDigest::from_str(&digest).map_err(|_| Error::InvalidFilter)?;
            Ok(RpcEventFilter::Transaction(digest))
        } else if let Some(package) = filter.emitting_package {
            if let Some(module) = filter.emitting_module {
                let package =
                    ObjectID::from_bytes(package.into_array()).map_err(|_| Error::InvalidFilter)?;
                let module = Identifier::from_str(&module).map_err(|_| Error::InvalidFilter)?;
                Ok(RpcEventFilter::MoveModule { package, module })
            } else {
                let package =
                    ObjectID::from_bytes(package.into_array()).map_err(|_| Error::InvalidFilter)?;
                Ok(RpcEventFilter::Package(package))
            }
        } else if let Some(event_type) = filter.event_type {
            let event_type = StructTag::from_str(&event_type).map_err(|_| Error::InvalidFilter)?;
            Ok(RpcEventFilter::MoveEventType(event_type))
        } else if let Some(package) = filter.event_package {
            if let Some(module) = filter.event_module {
                let package =
                    ObjectID::from_bytes(package.into_array()).map_err(|_| Error::InvalidFilter)?;
                let module = Identifier::from_str(&module).map_err(|_| Error::InvalidFilter)?;
                Ok(RpcEventFilter::MoveModule { package, module })
            } else {
                let package =
                    ObjectID::from_bytes(package.into_array()).map_err(|_| Error::InvalidFilter)?;
                Ok(RpcEventFilter::Package(package))
            }
        } else {
            return Err(Error::InvalidFilter);
        };

        let descending_order = before.is_some();
        let limit = self.validate_page_limit(first, last)? as usize;
        let cursor = after
            .or(before)
            .map(|c| self.parse_event_cursor(c))
            .transpose()?;
        if let Ok(event_filter) = event_filter {
            let results = self
                .inner
                .query_events_in_blocking_task(event_filter, cursor, limit, descending_order)
                .await?;

            let has_next_page = results.len() > limit;

            let mut connection = Connection::new(false, has_next_page);
            connection.edges.extend(results.into_iter().map(|e| {
                let cursor = String::from(e.id);
                let event = Event {
                    sending_package: SuiAddress::from(e.package_id),
                    sending_module: e.transaction_module.to_string(),
                    event_type: Some(MoveType::new(TypeTag::from(e.type_.clone()))),
                    senders: Some(vec![Address {
                        address: SuiAddress::from_array(e.sender.to_inner()),
                    }]),
                    timestamp: e.timestamp_ms.and_then(|t| DateTime::from_ms(t as i64)),
                    json: Some(e.parsed_json.to_string()),
                    bcs: Some(Base64::from(e.bcs)),
                };

                Edge::new(cursor, event)
            }));
            Ok(Some(connection))
        } else {
            Err(Error::InvalidFilter)
        }
    }

    pub(crate) async fn fetch_dynamic_fields(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        address: SuiAddress,
    ) -> Result<Option<Connection<String, DynamicField>>, Error> {
        let filter = ObjectFilter {
            owner: Some(address),
            ..Default::default()
        };

        let objs = self
            .multi_get_objs(
                first,
                after,
                last,
                before,
                Some(filter),
                Some(OwnerType::Object),
            )
            .await?;

        let Some((stored_objs, has_next_page)) = objs else {
            return Ok(None);
        };

        let mut connection = Connection::new(false, has_next_page);

        for stored_obj in stored_objs {
            let df_object_id = stored_obj.df_object_id.as_ref().ok_or_else(|| {
                Error::Internal("Dynamic field does not have df_object_id".to_string())
            })?;
            let cursor = SuiAddress::from_bytes(df_object_id)
                .map_err(|e| Error::Internal(format!("{e}")))?;
            let df_kind = match stored_obj.df_kind {
                None => Err(Error::Internal("Dynamic field type is not set".to_string())),
                Some(df_kind) => match df_kind {
                    0 => Ok(DynamicFieldType::DynamicField),
                    1 => Ok(DynamicFieldType::DynamicObject),
                    _ => Err(Error::Internal("Unexpected df_kind value".to_string())),
                },
            }?;

            connection.edges.push(Edge::new(
                cursor.to_string(),
                DynamicField {
                    stored_object: stored_obj,
                    df_object_id: cursor,
                    df_kind,
                },
            ));
        }
        Ok(Some(connection))
    }

    pub(crate) async fn fetch_dynamic_field(
        &self,
        address: SuiAddress,
        name: DynamicFieldName,
        kind: DynamicFieldType,
    ) -> Result<Option<DynamicField>, Error> {
        let name_bcs_value = &name.bcs.0;
        let parent_object_id =
            ObjectID::from_bytes(address.as_slice()).map_err(|e| Error::Client(e.to_string()))?;
        let mut type_tag =
            TypeTag::from_str(&name.type_).map_err(|e| Error::Client(e.to_string()))?;

        if kind == DynamicFieldType::DynamicObject {
            let dynamic_object_field_struct =
                sui_types::dynamic_field::DynamicFieldInfo::dynamic_object_field_wrapper(type_tag);
            type_tag = TypeTag::Struct(Box::new(dynamic_object_field_struct));
        }

        let id = sui_types::dynamic_field::derive_dynamic_field_id(
            parent_object_id,
            &type_tag,
            name_bcs_value,
        )
        .map_err(|e| Error::Internal(format!("Deriving dynamic field id cannot fail: {e}")))?;

        let stored_obj = self.get_obj(id.to_vec(), None).await?;
        if let Some(stored_object) = stored_obj {
            let df_object_id = stored_object.df_object_id.as_ref().ok_or_else(|| {
                Error::Internal("Dynamic field does not have df_object_id".to_string())
            })?;
            let df_object_id =
                SuiAddress::from_bytes(df_object_id).map_err(|e| Error::Internal(e.to_string()))?;
            return Ok(Some(DynamicField {
                stored_object,
                df_object_id,
                df_kind: kind,
            }));
        }
        Ok(None)
    }
    pub(crate) async fn fetch_coin_metadata(
        &self,
        coin_type: String,
    ) -> Result<Option<CoinMetadata>, Error> {
        let coin_struct =
            parse_to_struct_tag(&coin_type).map_err(|e| Error::InvalidCoinType(e.to_string()))?;

        let coin_metadata_type =
            NativeCoinMetadata::type_(coin_struct).to_canonical_string(/* with_prefix */ true);

        let Some(coin_metadata) = self.get_obj_by_type(coin_metadata_type).await? else {
            return Ok(None);
        };

        let object = Object::try_from(coin_metadata)?;
        let move_object = MoveObject::try_from(&object).map_err(|_| {
            Error::Internal(format!(
                "Expected {} to be coin metadata, but it is not an object.",
                object.address,
            ))
        })?;

        let coin_metadata_object = CoinMetadata::try_from(&move_object).map_err(|_| {
            Error::Internal(format!(
                "Expected {} to be coin metadata, but it is not.",
                object.address,
            ))
        })?;

        Ok(Some(coin_metadata_object))
    }

    pub(crate) async fn fetch_total_supply(&self, coin_type: String) -> Result<Option<u64>, Error> {
        let coin_struct =
            parse_to_struct_tag(&coin_type).map_err(|e| Error::InvalidCoinType(e.to_string()))?;

        let supply = if GAS::is_gas(&coin_struct) {
            TOTAL_SUPPLY_SUI
        } else {
            let treasury_cap_type =
                TreasuryCap::type_(coin_struct).to_canonical_string(/* with_prefix */ true);

            let Some(treasury_cap) = self.get_obj_by_type(treasury_cap_type).await? else {
                return Ok(None);
            };

            let native_object = NativeObject::try_from(treasury_cap)?;
            let object_id = native_object.id();
            let treasury_cap_object = TreasuryCap::try_from(native_object).map_err(|e| {
                Error::Internal(format!(
                    "Error while deserializing treasury cap object {object_id}: {e}"
                ))
            })?;
            treasury_cap_object.total_supply.value
        };

        Ok(Some(supply))
    }
}

impl TryFrom<StoredCheckpoint> for Checkpoint {
    type Error = Error;
    fn try_from(c: StoredCheckpoint) -> Result<Self, Self::Error> {
        let checkpoint_commitments: Vec<CheckpointCommitment> =
            bcs::from_bytes(&c.checkpoint_commitments).map_err(|e| {
                Error::Internal(format!(
                    "Can't convert checkpoint_commitments into CheckpointCommitments. Error: {e}",
                ))
            })?;

        let live_object_set_digest =
            checkpoint_commitments
                .iter()
                .find_map(|commitment| match commitment {
                    CheckpointCommitment::ECMHLiveObjectSetDigest(digest) => {
                        Some(Digest::from_array(digest.digest.into_inner()).to_string())
                    }
                    #[allow(unreachable_patterns)]
                    _ => None,
                });

        let end_of_epoch_data: Option<NativeEndOfEpochData> = if c.end_of_epoch {
            c.end_of_epoch_data
                .map(|data| {
                    bcs::from_bytes(&data).map_err(|e| {
                        Error::Internal(format!(
                            "Can't convert end_of_epoch_data into EndOfEpochData. Error: {e}",
                        ))
                    })
                })
                .transpose()?
        } else {
            None
        };

        let end_of_epoch = end_of_epoch_data.map(|e| {
            let committees = e.next_epoch_committee;
            let new_committee = if committees.is_empty() {
                None
            } else {
                Some(
                    committees
                        .iter()
                        .map(|c| CommitteeMember {
                            authority_name: Some(c.0.into_concise().to_string()),
                            stake_unit: Some(c.1),
                        })
                        .collect::<Vec<_>>(),
                )
            };

            EndOfEpochData {
                new_committee,
                next_protocol_version: Some(e.next_epoch_protocol_version.as_u64()),
            }
        });

        Ok(Self {
            digest: Digest::try_from(c.checkpoint_digest)?.to_string(),
            sequence_number: c.sequence_number as u64,
            timestamp: DateTime::from_ms(c.timestamp_ms),
            validator_signature: Some(c.validator_signature.into()),
            previous_checkpoint_digest: c
                .previous_checkpoint_digest
                .map(|d| Digest::try_from(d).map(|digest| digest.to_string()))
                .transpose()?,
            live_object_set_digest,
            network_total_transactions: Some(c.network_total_transactions as u64),
            rolling_gas_summary: Some(GasCostSummary {
                computation_cost: c.computation_cost as u64,
                storage_cost: c.storage_cost as u64,
                storage_rebate: c.storage_rebate as u64,
                non_refundable_storage_fee: c.non_refundable_storage_fee as u64,
            }),
            epoch_id: c.epoch as u64,
            end_of_epoch,
        })
    }
}

impl TryFrom<StoredTransaction> for TransactionBlock {
    type Error = Error;

    fn try_from(tx: StoredTransaction) -> Result<Self, Self::Error> {
        let digest = Digest::try_from(tx.transaction_digest.as_slice())?;

        let sender_signed_data: SenderSignedData =
            bcs::from_bytes(&tx.raw_transaction).map_err(|e| {
                Error::Internal(format!(
                    "Can't convert raw_transaction into SenderSignedData. Error: {e}",
                ))
            })?;

        let sender = Address {
            address: SuiAddress::from_array(
                sender_signed_data
                    .intent_message()
                    .value
                    .sender()
                    .to_inner(),
            ),
        };

        let gas_input = GasInput::from(sender_signed_data.intent_message().value.gas_data());
        let effects = Some(TransactionBlockEffects::try_from(tx.clone())?);

        let epoch_id = match sender_signed_data.intent_message().value.expiration() {
            TransactionExpiration::None => None,
            TransactionExpiration::Epoch(epoch_id) => Some(*epoch_id),
        };

        // TODO Finish implementing all types of transaction kinds
        let kind = TransactionBlockKind::from(sender_signed_data.transaction_data().kind());
        let signatures = sender_signed_data
            .tx_signatures()
            .iter()
            .map(|s| {
                Some(TransactionSignature {
                    base64_sig: Base64::from(s.as_ref()),
                })
            })
            .collect::<Vec<_>>();

        Ok(Self {
            digest,
            effects,
            sender: Some(sender),
            bcs: Some(Base64::from(&tx.raw_transaction)),
            gas_input: Some(gas_input),
            epoch_id,
            kind: Some(kind),
            signatures: Some(signatures),
        })
    }
}

impl TryFrom<StoredEpochInfo> for Epoch {
    type Error = Error;
    fn try_from(e: StoredEpochInfo) -> Result<Self, Self::Error> {
        let validators: Vec<SuiValidatorSummary> = e
            .validators
            .into_iter()
            .flatten()
            .map(|v| {
                bcs::from_bytes(&v).map_err(|e| {
                    Error::Internal(format!(
                        "Can't convert validator into Validator. Error: {e}",
                    ))
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let active_validators = convert_to_validators(validators, None);

        let validator_set = ValidatorSet {
            total_stake: e.new_total_stake.map(|s| BigInt::from(s as u64)),
            active_validators: Some(active_validators),
            ..Default::default()
        };

        let net_inflow =
            if let (Some(fund_inflow), Some(fund_outflow)) = (e.storage_charge, e.storage_rebate) {
                Some(BigInt::from(fund_inflow - fund_outflow))
            } else {
                None
            };

        Ok(Self {
            epoch_id: e.epoch as u64,
            protocol_version: e.protocol_version as u64,
            reference_gas_price: Some(BigInt::from(e.reference_gas_price as u64)),
            validator_set: Some(validator_set),
            start_timestamp: DateTime::from_ms(e.epoch_start_timestamp),
            end_timestamp: e.epoch_end_timestamp.and_then(DateTime::from_ms),
            total_checkpoints: e
                .last_checkpoint_id
                .map(|last_chckp_id| BigInt::from(last_chckp_id - e.first_checkpoint_id)),
            total_gas_fees: e.total_gas_fees.map(BigInt::from),
            total_stake_rewards: e.total_stake_rewards_distributed.map(BigInt::from),
            total_stake_subsidies: e.stake_subsidy_amount.map(BigInt::from),
            fund_size: e.storage_fund_balance.map(BigInt::from),
            net_inflow,
            fund_inflow: e.storage_charge.map(BigInt::from),
            fund_outflow: e.storage_rebate.map(BigInt::from),
        })
    }
}

impl TryFrom<NativeSuiSystemStateSummary> for SuiSystemStateSummary {
    type Error = Error;
    fn try_from(system_state: NativeSuiSystemStateSummary) -> Result<Self, Self::Error> {
        let active_validators =
            convert_to_validators(system_state.active_validators.clone(), Some(&system_state));

        let start_timestamp = i64::try_from(system_state.epoch_start_timestamp_ms).map_err(|_| {
            Error::Internal(format!(
                "Cannot convert start timestamp u64 ({}) of system state into i64 required by DateTime",
                system_state.epoch_start_timestamp_ms
            ))
        })?;

        let start_timestamp = DateTime::from_ms(start_timestamp).ok_or_else(|| {
            Error::Internal(format!(
                "Cannot convert start timestamp ({}) of system state into a DateTime",
                start_timestamp
            ))
        })?;

        Ok(SuiSystemStateSummary {
            epoch_id: system_state.epoch,
            system_state_version: Some(BigInt::from(system_state.system_state_version)),
            reference_gas_price: Some(BigInt::from(system_state.reference_gas_price)),
            system_parameters: Some(SystemParameters {
                duration_ms: Some(BigInt::from(system_state.epoch_duration_ms)),
                stake_subsidy_start_epoch: Some(system_state.stake_subsidy_start_epoch),
                min_validator_count: Some(system_state.max_validator_count),
                max_validator_count: Some(system_state.max_validator_count),
                min_validator_joining_stake: Some(BigInt::from(
                    system_state.min_validator_joining_stake,
                )),
                validator_low_stake_threshold: Some(BigInt::from(
                    system_state.validator_low_stake_threshold,
                )),
                validator_very_low_stake_threshold: Some(BigInt::from(
                    system_state.validator_very_low_stake_threshold,
                )),
                validator_low_stake_grace_period: Some(BigInt::from(
                    system_state.validator_low_stake_grace_period,
                )),
            }),
            stake_subsidy: Some(StakeSubsidy {
                balance: Some(BigInt::from(system_state.stake_subsidy_balance)),
                distribution_counter: Some(system_state.stake_subsidy_distribution_counter),
                current_distribution_amount: Some(BigInt::from(
                    system_state.stake_subsidy_current_distribution_amount,
                )),
                period_length: Some(system_state.stake_subsidy_period_length),
                decrease_rate: Some(system_state.stake_subsidy_decrease_rate as u64),
            }),
            validator_set: Some(ValidatorSet {
                total_stake: Some(BigInt::from(system_state.total_stake)),
                active_validators: Some(active_validators),
                pending_removals: Some(system_state.pending_removals.clone()),
                pending_active_validators_size: Some(system_state.pending_active_validators_size),
                stake_pool_mappings_size: Some(system_state.staking_pool_mappings_size),
                inactive_pools_size: Some(system_state.inactive_pools_size),
                validator_candidates_size: Some(system_state.validator_candidates_size),
            }),
            storage_fund: Some(StorageFund {
                total_object_storage_rebates: Some(BigInt::from(
                    system_state.storage_fund_total_object_storage_rebates,
                )),
                non_refundable_balance: Some(BigInt::from(
                    system_state.storage_fund_non_refundable_balance,
                )),
            }),
            safe_mode: Some(SafeMode {
                enabled: Some(system_state.safe_mode),
                gas_summary: Some(GasCostSummary {
                    computation_cost: system_state.safe_mode_computation_rewards,
                    storage_cost: system_state.safe_mode_storage_rewards,
                    storage_rebate: system_state.safe_mode_storage_rebates,
                    non_refundable_storage_fee: system_state.safe_mode_non_refundable_storage_fee,
                }),
            }),
            protocol_version: system_state.protocol_version,
            start_timestamp: Some(start_timestamp),
        })
    }
}

impl From<&TransactionKind> for TransactionBlockKind {
    fn from(value: &TransactionKind) -> Self {
        match value {
            TransactionKind::ChangeEpoch(x) => {
                let change = ChangeEpochTransaction {
                    epoch_id: x.epoch,
                    timestamp: DateTime::from_ms(x.epoch_start_timestamp_ms as i64),
                    storage_charge: Some(BigInt::from(x.storage_charge)),
                    computation_charge: Some(BigInt::from(x.computation_charge)),
                    storage_rebate: Some(BigInt::from(x.storage_rebate)),
                };
                TransactionBlockKind::ChangeEpochTransaction(change)
            }
            TransactionKind::ConsensusCommitPrologue(x) => {
                let consensus = ConsensusCommitPrologueTransaction {
                    epoch_id: x.epoch,
                    round: Some(x.round),
                    timestamp: DateTime::from_ms(x.commit_timestamp_ms as i64),
                };
                TransactionBlockKind::ConsensusCommitPrologueTransaction(consensus)
            }
            TransactionKind::ConsensusCommitPrologueV2(x) => {
                let consensus = ConsensusCommitPrologueTransaction {
                    epoch_id: x.epoch,
                    round: Some(x.round),
                    timestamp: DateTime::from_ms(x.commit_timestamp_ms as i64),
                };
                TransactionBlockKind::ConsensusCommitPrologueTransaction(consensus)
            }
            TransactionKind::Genesis(x) => {
                let genesis = GenesisTransaction {
                    objects: Some(
                        x.objects
                            .clone()
                            .into_iter()
                            .map(SuiAddress::from)
                            .collect::<Vec<_>>(),
                    ),
                };
                TransactionBlockKind::GenesisTransaction(genesis)
            }
            // TODO: flesh out type
            TransactionKind::ProgrammableTransaction(pt) => {
                TransactionBlockKind::ProgrammableTransactionBlock(ProgrammableTransaction {
                    value: format!("{:?}", pt),
                })
            }
            // TODO: flesh out type
            TransactionKind::AuthenticatorStateUpdate(asu) => {
                TransactionBlockKind::AuthenticatorStateUpdateTransaction(
                    AuthenticatorStateUpdate {
                        value: format!("{:?}", asu),
                    },
                )
            }
            // TODO: flesh out type
            TransactionKind::RandomnessStateUpdate(rsu) => {
                TransactionBlockKind::RandomnessStateUpdateTransaction(RandomnessStateUpdate {
                    value: format!("{:?}", rsu),
                })
            }
            // TODO: flesh out type
            TransactionKind::EndOfEpochTransaction(et) => {
                TransactionBlockKind::EndOfEpochTransaction(EndOfEpochTransaction {
                    value: format!("{:?}", et),
                })
            }
        }
    }
}

// TODO fix this GenesisObject
impl From<GenesisObject> for SuiAddress {
    fn from(value: GenesisObject) -> Self {
        match value {
            GenesisObject::RawObject { data, owner: _ } => {
                SuiAddress::from_bytes(data.id().to_vec()).unwrap()
            }
        }
    }
}

/// TODO: enfroce limits on first and last
pub(crate) fn validate_cursor_pagination(
    first: &Option<u64>,
    after: &Option<String>,
    last: &Option<u64>,
    before: &Option<String>,
) -> Result<(), Error> {
    if first.is_some() && before.is_some() {
        return Err(DbValidationError::FirstAfter.into());
    }

    if last.is_some() && after.is_some() {
        return Err(DbValidationError::LastBefore.into());
    }

    if before.is_some() && after.is_some() {
        return Err(Error::CursorNoBeforeAfter);
    }

    if first.is_some() && last.is_some() {
        return Err(Error::CursorNoFirstLast);
    }

    Ok(())
}

pub(crate) fn convert_to_validators(
    validators: Vec<SuiValidatorSummary>,
    system_state: Option<&NativeSuiSystemStateSummary>,
) -> Vec<Validator> {
    validators
        .iter()
        .map(|v| {
            let at_risk = system_state
                .and_then(|system_state| {
                    system_state
                        .at_risk_validators
                        .iter()
                        .find(|&(address, _)| address == &v.sui_address)
                })
                .map(|&(_, value)| value);

            let report_records = system_state
                .and_then(|system_state| {
                    system_state
                        .validator_report_records
                        .iter()
                        .find(|&(address, _)| address == &v.sui_address)
                })
                .map(|(_, value)| {
                    value
                        .iter()
                        .map(|address| SuiAddress::from_array(address.to_inner()))
                        .collect::<Vec<_>>()
                });

            let credentials = ValidatorCredentials {
                protocol_pub_key: Some(Base64::from(v.protocol_pubkey_bytes.clone())),
                network_pub_key: Some(Base64::from(v.network_pubkey_bytes.clone())),
                worker_pub_key: Some(Base64::from(v.worker_pubkey_bytes.clone())),
                proof_of_possession: Some(Base64::from(v.proof_of_possession_bytes.clone())),
                net_address: Some(v.net_address.clone()),
                p2p_address: Some(v.p2p_address.clone()),
                primary_address: Some(v.primary_address.clone()),
                worker_address: Some(v.worker_address.clone()),
            };
            Validator {
                address: Address {
                    address: SuiAddress::from(v.sui_address),
                },
                next_epoch_credentials: Some(credentials.clone()),
                credentials: Some(credentials),
                name: Some(v.name.clone()),
                description: Some(v.description.clone()),
                image_url: Some(v.image_url.clone()),
                project_url: Some(v.project_url.clone()),

                operation_cap_id: SuiAddress::from_array(**v.operation_cap_id),
                staking_pool_id: SuiAddress::from_array(**v.staking_pool_id),
                exchange_rates_id: SuiAddress::from_array(**v.exchange_rates_id),
                exchange_rates_size: Some(v.exchange_rates_size),

                staking_pool_activation_epoch: v.staking_pool_activation_epoch,
                staking_pool_sui_balance: Some(BigInt::from(v.staking_pool_sui_balance)),
                rewards_pool: Some(BigInt::from(v.rewards_pool)),
                pool_token_balance: Some(BigInt::from(v.pool_token_balance)),
                pending_stake: Some(BigInt::from(v.pending_stake)),
                pending_total_sui_withdraw: Some(BigInt::from(v.pending_total_sui_withdraw)),
                pending_pool_token_withdraw: Some(BigInt::from(v.pending_pool_token_withdraw)),
                voting_power: Some(v.voting_power),
                // stake_units: todo!(),
                gas_price: Some(BigInt::from(v.gas_price)),
                commission_rate: Some(v.commission_rate),
                next_epoch_stake: Some(BigInt::from(v.next_epoch_stake)),
                next_epoch_gas_price: Some(BigInt::from(v.next_epoch_gas_price)),
                next_epoch_commission_rate: Some(v.next_epoch_commission_rate),
                at_risk,
                report_records,
                // apy: todo!(),
            }
        })
        .collect()
}

impl From<Address> for SuiAddress {
    fn from(a: Address) -> Self {
        a.address
    }
}

impl From<SuiAddress> for Address {
    fn from(a: SuiAddress) -> Self {
        Address { address: a }
    }
}

impl From<NativeSuiAddress> for SuiAddress {
    fn from(a: NativeSuiAddress) -> Self {
        SuiAddress::from_array(a.to_inner())
    }
}

impl From<SuiAddress> for NativeSuiAddress {
    fn from(a: SuiAddress) -> Self {
        NativeSuiAddress::try_from(a.as_slice()).unwrap()
    }
}

impl From<&SuiAddress> for NativeSuiAddress {
    fn from(a: &SuiAddress) -> Self {
        NativeSuiAddress::try_from(a.as_slice()).unwrap()
    }
}
