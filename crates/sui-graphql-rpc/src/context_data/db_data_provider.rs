// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use crate::{
    error::Error,
    types::{
        address::{Address, AddressTransactionBlockRelationship},
        base64::Base64,
        big_int::BigInt,
        checkpoint::Checkpoint,
        committee_member::CommitteeMember,
        date_time::DateTime,
        digest::Digest,
        end_of_epoch_data::EndOfEpochData,
        epoch::Epoch,
        gas::{GasCostSummary, GasInput},
        object::{Object, ObjectFilter, ObjectKind},
        sui_address::SuiAddress,
        transaction_block::{TransactionBlock, TransactionBlockEffects, TransactionBlockFilter},
        transaction_block_kind::{
            ChangeEpochTransaction, ConsensusCommitPrologueTransaction, GenesisTransaction,
            TransactionBlockKind,
        },
    },
};
use async_graphql::connection::{Connection, Edge};
use diesel::{
    ExpressionMethods, JoinOnDsl, OptionalExtension, PgArrayExpressionMethods, PgConnection,
    QueryDsl, RunQueryDsl,
};
use std::str::FromStr;
use sui_indexer::{
    indexer_reader::IndexerReader,
    models_v2::{
        checkpoints::StoredCheckpoint, epoch::StoredEpochInfo, objects::StoredObject,
        packages::StoredPackage, transactions::StoredTransaction,
    },
    schema_v2::{checkpoints, epochs, objects, packages, transactions, tx_indices},
    PgConnectionPoolConfig,
};
use sui_json_rpc_types::SuiTransactionBlockEffects;
use sui_sdk::types::{
    digests::ChainIdentifier,
    effects::TransactionEffects,
    messages_checkpoint::{
        CheckpointCommitment, CheckpointDigest, EndOfEpochData as NativeEndOfEpochData,
    },
    move_package::MovePackage as SuiMovePackage,
    object::{Data, Object as SuiObject},
    transaction::{
        GenesisObject, SenderSignedData, TransactionDataAPI, TransactionExpiration, TransactionKind,
    },
};

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
}

pub(crate) struct PgManager {
    pub inner: IndexerReader,
}

impl PgManager {
    pub(crate) fn new<T: Into<String>>(
        db_url: T,
        config: Option<PgConnectionPoolConfig>,
    ) -> Result<Self, Error> {
        // TODO (wlmyng): support config
        let mut config = config.unwrap_or(PgConnectionPoolConfig::default());
        config.set_pool_size(30);
        let inner = IndexerReader::new_with_config(db_url, config)
            .map_err(|e| Error::Internal(e.to_string()))?;

        Ok(Self { inner })
    }

    pub async fn run_query_async<T, E, F>(&self, query: F) -> Result<T, Error>
    where
        F: FnOnce(&mut PgConnection) -> Result<T, E> + Send + 'static,
        E: From<diesel::result::Error> + std::error::Error + Send + 'static,
        T: Send + 'static,
    {
        self.inner
            .run_query_async(query)
            .await
            .map_err(|e| Error::Internal(e.to_string()))
    }
}

/// Implement methods to query db and return StoredData
impl PgManager {
    async fn get_tx(&self, digest: Vec<u8>) -> Result<Option<StoredTransaction>, Error> {
        self.run_query_async(|conn| {
            transactions::dsl::transactions
                .filter(transactions::dsl::transaction_digest.eq(digest))
                .get_result::<StoredTransaction>(conn) // Expect exactly 0 to 1 result
                .optional()
        })
        .await
    }

    async fn get_obj(
        &self,
        address: Vec<u8>,
        version: Option<i64>,
    ) -> Result<Option<StoredObject>, Error> {
        let mut query = objects::dsl::objects.into_boxed();
        query = query.filter(objects::dsl::object_id.eq(address));

        if let Some(version) = version {
            query = query.filter(objects::dsl::object_version.eq(version));
        }

        self.run_query_async(|conn| query.get_result::<StoredObject>(conn).optional())
            .await
    }

    /// TODO: cache modules/packages
    async fn get_package(&self, address: Vec<u8>) -> Result<Option<StoredPackage>, Error> {
        let mut query = packages::dsl::packages.into_boxed();
        query = query.filter(packages::dsl::package_id.eq(address));

        self.run_query_async(|conn| query.get_result::<StoredPackage>(conn).optional())
            .await
    }

    pub async fn fetch_native_package(&self, package_id: Vec<u8>) -> Result<SuiMovePackage, Error> {
        let package = self
            .get_package(package_id)
            .await?
            .ok_or_else(|| Error::Internal("Package not found".to_string()))?;

        bcs::from_bytes(&package.move_package).map_err(|e| Error::Internal(e.to_string()))
    }

    async fn get_epoch(&self, epoch_id: Option<i64>) -> Result<Option<StoredEpochInfo>, Error> {
        match epoch_id {
            Some(epoch_id) => {
                self.run_query_async(move |conn| {
                    epochs::dsl::epochs
                        .filter(epochs::dsl::epoch.eq(epoch_id))
                        .get_result::<StoredEpochInfo>(conn)
                        .optional()
                })
                .await
            }
            None => Some(
                self.run_query_async(|conn| {
                    epochs::dsl::epochs
                        .order_by(epochs::dsl::epoch.desc())
                        .limit(1)
                        .first::<StoredEpochInfo>(conn)
                })
                .await,
            )
            .transpose(),
        }
    }

    async fn get_checkpoint(
        &self,
        digest: Option<Vec<u8>>,
        sequence_number: Option<i64>,
    ) -> Result<Option<StoredCheckpoint>, Error> {
        let mut query = checkpoints::dsl::checkpoints.into_boxed();

        match (digest, sequence_number) {
            (Some(digest), None) => {
                query = query.filter(checkpoints::dsl::checkpoint_digest.eq(digest));
            }
            (None, Some(sequence_number)) => {
                query = query.filter(checkpoints::dsl::sequence_number.eq(sequence_number));
            }
            (None, None) => {
                query = query
                    .order_by(checkpoints::dsl::sequence_number.desc())
                    .limit(1);
            }
            _ => (), // No-op if invalid input
        }

        self.run_query_async(|conn| query.get_result::<StoredCheckpoint>(conn).optional())
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
        let mut query =
            transactions::dsl::transactions
                .inner_join(tx_indices::dsl::tx_indices.on(
                    transactions::dsl::tx_sequence_number.eq(tx_indices::dsl::tx_sequence_number),
                ))
                .into_boxed();
        if let Some(after) = after {
            let after = self.parse_tx_cursor(&after)?;
            query = query
                .filter(transactions::dsl::tx_sequence_number.gt(after))
                .order(transactions::dsl::tx_sequence_number.asc());
        } else if let Some(before) = before {
            let before = self.parse_tx_cursor(&before)?;
            query = query
                .filter(transactions::dsl::tx_sequence_number.lt(before))
                .order(transactions::dsl::tx_sequence_number.desc());
        }

        if let Some(filter) = filter {
            // Filters for transaction table
            // at_checkpoint mutually exclusive with before_ and after_checkpoint
            if let Some(checkpoint) = filter.at_checkpoint {
                query = query
                    .filter(transactions::dsl::checkpoint_sequence_number.eq(checkpoint as i64));
            } else {
                if let Some(checkpoint) = filter.after_checkpoint {
                    query = query.filter(
                        transactions::dsl::checkpoint_sequence_number.gt(checkpoint as i64),
                    );
                }
                if let Some(checkpoint) = filter.before_checkpoint {
                    query = query.filter(
                        transactions::dsl::checkpoint_sequence_number.lt(checkpoint as i64),
                    );
                }
            }
            if let Some(transaction_ids) = filter.transaction_ids {
                let digests = transaction_ids
                    .into_iter()
                    .map(|id| Ok::<Vec<u8>, Error>(Digest::from_str(&id)?.into_vec()))
                    .collect::<Result<Vec<_>, _>>()?;

                query = query.filter(transactions::dsl::transaction_digest.eq_any(digests));
            }

            // Filters for tx_indices table
            match (filter.package, filter.module, filter.function) {
                (Some(p), None, None) => {
                    query = query.filter(
                        tx_indices::dsl::packages.contains(vec![Some(p.into_array().to_vec())]),
                    );
                }
                (Some(p), Some(m), None) => {
                    query = query.filter(
                        tx_indices::dsl::package_modules
                            .contains(vec![Some(format!("{}::{}", p, m))]),
                    );
                }
                (Some(p), Some(m), Some(f)) => {
                    query = query.filter(
                        tx_indices::dsl::package_module_functions
                            .contains(vec![Some(format!("{}::{}::{}", p, m, f))]),
                    );
                }
                _ => {}
            }
            // sign and sent are indexed the same
            let mut senders = Vec::new();
            if let Some(signer) = filter.sign_address {
                senders.push(signer.into_vec());
            }
            if let Some(sender) = filter.sent_address {
                senders.push(sender.into_vec());
            }
            if !senders.is_empty() {
                query = query.filter(tx_indices::dsl::senders.contains(senders));
            }
            if let Some(receiver) = filter.recv_address {
                query =
                    query.filter(tx_indices::dsl::recipients.contains(vec![receiver.into_vec()]));
            }
            if let Some(paid_address) = filter.paid_address {
                query =
                    query.filter(tx_indices::dsl::payers.contains(vec![paid_address.into_vec()]));
            }

            if let Some(input_object) = filter.input_object {
                query = query
                    .filter(tx_indices::dsl::input_objects.contains(vec![input_object.into_vec()]));
            }
            if let Some(changed_object) = filter.changed_object {
                query = query.filter(
                    tx_indices::dsl::changed_objects.contains(vec![changed_object.into_vec()]),
                );
            }
        };

        let limit = first.or(last).unwrap_or(10) as i64;
        query = query.limit(limit + 1);

        let result: Option<Vec<StoredTransaction>> = self
            .run_query_async(|conn| {
                query
                    .select(transactions::all_columns)
                    .load(conn)
                    .optional()
            })
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
    ) -> Result<Option<(Vec<StoredCheckpoint>, bool)>, Error> {
        let mut query = checkpoints::dsl::checkpoints.into_boxed();

        if let Some(after) = after {
            let after = self.parse_checkpoint_cursor(&after)?;
            query = query
                .filter(checkpoints::dsl::sequence_number.gt(after))
                .order(checkpoints::dsl::sequence_number.asc());
        } else if let Some(before) = before {
            let before = self.parse_obj_cursor(&before)?;
            query = query
                .filter(checkpoints::dsl::sequence_number.lt(before))
                .order(checkpoints::dsl::sequence_number.desc());
        }

        let limit = first.or(last).unwrap_or(10) as i64;
        query = query.limit(limit + 1);

        let result: Option<Vec<StoredCheckpoint>> = self
            .run_query_async(|conn| query.load(conn).optional())
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
    ) -> Result<Option<(Vec<StoredObject>, bool)>, Error> {
        let mut query = objects::dsl::objects.into_boxed();

        if let Some(filter) = filter {
            if let Some(object_ids) = filter.object_ids {
                query = query.filter(
                    objects::dsl::object_id.eq_any(
                        object_ids
                            .into_iter()
                            .map(|id| id.into_vec())
                            .collect::<Vec<_>>(),
                    ),
                );
            }

            if let Some(owner) = filter.owner {
                query = query
                    .filter(objects::dsl::owner_id.eq(owner.into_vec()))
                    .filter(objects::dsl::owner_type.between(1, 2));
            }
        }

        if let Some(after) = after {
            let after = self.parse_obj_cursor(&after)?;
            query = query
                .filter(objects::dsl::checkpoint_sequence_number.gt(after))
                .order(objects::dsl::checkpoint_sequence_number.asc());
        } else if let Some(before) = before {
            let before = self.parse_obj_cursor(&before)?;
            query = query
                .filter(objects::dsl::checkpoint_sequence_number.lt(before))
                .order(objects::dsl::checkpoint_sequence_number.desc());
        }

        let limit = first.or(last).unwrap_or(10) as i64;
        query = query.limit(limit + 1);

        let result: Option<Vec<StoredObject>> = self
            .run_query_async(|conn| query.load(conn).optional())
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
            .map_err(|_| Error::Internal("Invalid transaction cursor".to_string()))?;
        Ok(tx_sequence_number)
    }

    pub(crate) fn parse_obj_cursor(&self, cursor: &str) -> Result<i64, Error> {
        let checkpoint_sequence_number = cursor
            .parse::<i64>()
            .map_err(|_| Error::Internal("Invalid object cursor".to_string()))?;
        Ok(checkpoint_sequence_number)
    }

    pub(crate) fn parse_checkpoint_cursor(&self, cursor: &str) -> Result<i64, Error> {
        let sequence_number = cursor
            .parse::<i64>()
            .map_err(|_| Error::Internal("Invalid checkpoint cursor".to_string()))?;
        Ok(sequence_number)
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

    pub(crate) fn validate_cursor_pagination(
        &self,
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

    pub(crate) async fn fetch_tx(&self, digest: &str) -> Result<Option<TransactionBlock>, Error> {
        let digest = Digest::from_str(digest)?.into_vec();

        self.get_tx(digest)
            .await?
            .map(TransactionBlock::try_from)
            .transpose()
    }

    pub(crate) async fn fetch_latest_epoch(&self) -> Result<Epoch, Error> {
        self.get_epoch(None)
            .await?
            .map(Epoch::from)
            .ok_or_else(|| Error::Internal("Latest epoch not found".to_string()))
    }

    // To be used in scenarios where epoch may not exist, such as when epoch_id is provided by caller
    pub(crate) async fn fetch_epoch(&self, epoch_id: u64) -> Result<Option<Epoch>, Error> {
        let epoch_id = i64::try_from(epoch_id)
            .map_err(|_| Error::Internal("Failed to convert epoch id to i64".to_string()))?;
        Ok(self.get_epoch(Some(epoch_id)).await?.map(Epoch::from))
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
        let mut stored_checkpoint = None;

        match (digest, sequence_number) {
            (Some(digest), None) => {
                let digest = Digest::from_str(digest)?.into_vec();
                stored_checkpoint = self.get_checkpoint(Some(digest), None).await?;
            }
            (None, Some(sequence_number)) => {
                stored_checkpoint = self
                    .get_checkpoint(None, Some(sequence_number as i64))
                    .await?;
            }
            _ => (), // No-op if invalid input
        }

        stored_checkpoint.map(Checkpoint::try_from).transpose()
    }

    pub(crate) async fn fetch_chain_identifier(&self) -> Result<String, Error> {
        let result = self
            .get_checkpoint(None, Some(0))
            .await?
            .ok_or_else(|| Error::Internal("Genesis checkpoint cannot be found".to_string()))?;

        let digest = CheckpointDigest::try_from(result.checkpoint_digest).map_err(|e| {
            Error::Internal(format!(
                "Failed to convert checkpoint digest to CheckpointDigest. Error: {e}",
            ))
        })?;
        Ok(ChainIdentifier::from(digest).to_string())
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
        self.validate_cursor_pagination(&first, &after, &last, &before)?;
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

    pub(crate) async fn fetch_owner(
        &self,
        address: SuiAddress,
    ) -> Result<Option<SuiAddress>, Error> {
        let address = address.into_vec();

        let stored_obj = self.get_obj(address, None).await?;

        Ok(stored_obj
            .and_then(|obj| obj.owner_id.map(|id| SuiAddress::try_from(id).ok()))
            .flatten())
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

    pub(crate) async fn fetch_owned_objs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
        owner: SuiAddress,
    ) -> Result<Option<Connection<String, Object>>, Error> {
        let filter = filter.map(|mut x| {
            x.owner = Some(owner);
            x
        });
        self.fetch_objs(first, after, last, before, filter).await
    }

    pub(crate) async fn fetch_objs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, Object>>, Error> {
        self.validate_cursor_pagination(&first, &after, &last, &before)?;
        if let Some(filter) = &filter {
            self.validate_obj_filter(filter)?;
        }
        let objects = self
            .multi_get_objs(first, after, last, before, filter)
            .await?;

        if let Some((stored_objs, has_next_page)) = objects {
            let mut connection = Connection::new(false, has_next_page);
            connection
                .edges
                .extend(stored_objs.into_iter().filter_map(|stored_obj| {
                    let cursor = stored_obj.checkpoint_sequence_number.to_string();
                    Object::try_from(stored_obj)
                        .map_err(|e| eprintln!("Error converting object: {:?}", e))
                        .ok()
                        .map(|obj| Edge::new(cursor, obj))
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
    ) -> Result<Option<Connection<String, Checkpoint>>, Error> {
        self.validate_cursor_pagination(&first, &after, &last, &before)?;
        let checkpoints = self
            .multi_get_checkpoints(first, after, last, before)
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
        let effects: TransactionEffects = bcs::from_bytes(&tx.raw_effects).map_err(|e| {
            Error::Internal(format!(
                "Can't convert raw_effects into TransactionEffects. Error: {e}",
            ))
        })?;
        let effects = match SuiTransactionBlockEffects::try_from(effects) {
            Ok(effects) => Ok(Some(TransactionBlockEffects::from(&effects))),
            Err(e) => Err(Error::Internal(format!(
                "Can't convert TransactionEffects into SuiTransactionBlockEffects. Error: {e}",
            ))),
        }?;

        let epoch_id = match sender_signed_data.intent_message().value.expiration() {
            TransactionExpiration::None => None,
            TransactionExpiration::Epoch(epoch_id) => Some(*epoch_id),
        };

        let kind = TransactionBlockKind::from(sender_signed_data.transaction_data().kind());
        Ok(Self {
            digest,
            effects,
            sender: Some(sender),
            bcs: Some(Base64::from(&tx.raw_transaction)),
            gas_input: Some(gas_input),
            epoch_id,
            kind: Some(kind),
        })
    }
}

impl TryFrom<StoredObject> for Object {
    type Error = Error;

    fn try_from(o: StoredObject) -> Result<Self, Self::Error> {
        let version = o.object_version as u64;
        let (object_id, _sequence_number, digest) = &o.get_object_ref()?;
        let object: SuiObject = o.try_into()?;

        let kind = if object.owner.is_immutable() {
            Some(ObjectKind::Immutable)
        } else if object.owner.is_shared() {
            Some(ObjectKind::Shared)
        } else if object.owner.is_child_object() {
            Some(ObjectKind::Child)
        } else if object.owner.is_address_owned() {
            Some(ObjectKind::Owned)
        } else {
            None
        };

        let owner_address = object.owner.get_owner_address().ok();
        if matches!(kind, Some(ObjectKind::Immutable) | Some(ObjectKind::Shared))
            && owner_address.is_some()
        {
            return Err(Error::Internal(
                "Immutable or Shared object should not have an owner_id".to_string(),
            ));
        }

        let bcs = match object.data {
            // Do we BCS serialize packages?
            Data::Package(package) => Base64::from(
                bcs::to_bytes(&package)
                    .map_err(|e| Error::Internal(format!("Failed to serialize package: {e}")))?,
            ),
            Data::Move(move_object) => Base64::from(&move_object.into_contents()),
        };

        Ok(Self {
            address: SuiAddress::from_array(***object_id),
            version,
            digest: digest.base58_encode(),
            storage_rebate: Some(BigInt::from(object.storage_rebate)),
            owner: owner_address.map(SuiAddress::from),
            bcs: Some(bcs),
            previous_transaction: Some(Digest::from_array(
                object.previous_transaction.into_inner(),
            )),
            kind,
        })
    }
}

impl From<StoredEpochInfo> for Epoch {
    fn from(e: StoredEpochInfo) -> Self {
        Self {
            epoch_id: e.epoch as u64,
            system_state_version: None,
            protocol_configs: None,
            reference_gas_price: Some(BigInt::from(e.reference_gas_price as u64)),
            system_parameters: None,
            stake_subsidy: None,
            validator_set: None,
            storage_fund: None,
            safe_mode: None,
            start_timestamp: None,
        }
    }
}

impl From<&TransactionKind> for TransactionBlockKind {
    fn from(value: &TransactionKind) -> Self {
        match value {
            TransactionKind::ConsensusCommitPrologue(x) => {
                let consensus = ConsensusCommitPrologueTransaction {
                    epoch_id: x.epoch,
                    round: Some(x.round),
                    timestamp: DateTime::from_ms(x.commit_timestamp_ms as i64),
                };
                TransactionBlockKind::ConsensusCommitPrologueTransaction(consensus)
            }
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
            _ => unimplemented!(),
        }
    }
}

// TODO fix this GenesisObject
#[allow(unreachable_code)]
impl From<GenesisObject> for SuiAddress {
    fn from(value: GenesisObject) -> Self {
        match value {
            GenesisObject::RawObject { data, owner: _ } => {
                SuiAddress::from_bytes(data.id().to_vec()).unwrap()
            }
        }
    }
}
