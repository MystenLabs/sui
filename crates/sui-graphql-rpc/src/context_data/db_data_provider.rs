// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::Error,
    types::{
        address::Address,
        base64::Base64,
        big_int::BigInt,
        checkpoint::Checkpoint,
        digest::Digest,
        epoch::Epoch,
        gas::{GasCostSummary, GasInput},
        object::{Object, ObjectKind},
        sui_address::SuiAddress,
        transaction_block::{TransactionBlock, TransactionBlockEffects},
    },
};
use diesel::{ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl};
use std::str::FromStr;
use sui_indexer::{
    indexer_reader::IndexerReader,
    models_v2::{
        checkpoints::StoredCheckpoint, epoch::StoredEpochInfo, objects::StoredObject,
        transactions::StoredTransaction,
    },
    schema_v2::{checkpoints, epochs, objects, transactions},
    PgConnectionPoolConfig,
};
use sui_json_rpc_types::SuiTransactionBlockEffects;
use sui_sdk::types::{
    digests::ChainIdentifier,
    effects::TransactionEffects,
    messages_checkpoint::CheckpointDigest,
    object::{Data, Object as SuiObject},
    transaction::{SenderSignedData, TransactionDataAPI},
};

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
}

/// Implement methods to be used by graphql resolvers
impl PgManager {
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
}

impl TryFrom<StoredCheckpoint> for Checkpoint {
    type Error = Error;
    fn try_from(c: StoredCheckpoint) -> Result<Self, Self::Error> {
        Ok(Self {
            digest: Digest::try_from(c.checkpoint_digest)?.to_string(),
            sequence_number: c.sequence_number as u64,
            validator_signature: Some(c.validator_signature.into()),
            previous_checkpoint_digest: c
                .previous_checkpoint_digest
                .map(|d| Digest::try_from(d).map(|digest| digest.to_string()))
                .transpose()?,
            live_object_set_digest: None,
            network_total_transactions: Some(c.network_total_transactions as u64),
            rolling_gas_summary: Some(GasCostSummary {
                computation_cost: c.computation_cost as u64,
                storage_cost: c.storage_cost as u64,
                storage_rebate: c.storage_rebate as u64,
                non_refundable_storage_fee: c.non_refundable_storage_fee as u64,
            }),
            epoch_id: c.epoch as u64,
            end_of_epoch: None,
        })
    }
}

impl TryFrom<StoredTransaction> for TransactionBlock {
    type Error = Error;

    fn try_from(tx: StoredTransaction) -> Result<Self, Self::Error> {
        // TODO (wlmyng): Split the below into resolver methods
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

        Ok(Self {
            digest,
            effects,
            sender: Some(sender),
            bcs: Some(Base64::from(&tx.raw_transaction)),
            gas_input: Some(gas_input),
        })
    }
}

impl TryFrom<StoredObject> for Object {
    type Error = Error;

    // TODO (wlmyng): Refactor into resolvers once we retire sui-sdk data provider
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
