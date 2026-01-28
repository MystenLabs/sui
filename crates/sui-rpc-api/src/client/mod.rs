// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use futures::stream::Stream;
use futures::stream::TryStreamExt;
use prost_types::FieldMask;
use std::time::Duration;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::TryFromProtoError;
use sui_rpc::proto::sui::rpc::v2::{self as proto, GetServiceInfoRequest};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::digests::ChainIdentifier;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSequenceNumber};
use sui_types::object::Object;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use tap::Pipe;
use tonic::Status;
use tonic::metadata::MetadataMap;

pub use sui_rpc::client::HeadersInterceptor;
pub use sui_rpc::client::ResponseExt;

pub type Result<T, E = tonic::Status> = std::result::Result<T, E>;
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub struct Page<T> {
    pub items: Vec<T>,
    pub next_page_token: Option<Bytes>,
}

#[derive(Clone)]
pub struct Client(sui_rpc::Client);

impl Client {
    pub fn new<T>(uri: T) -> Result<Self>
    where
        T: TryInto<http::Uri>,
        T::Error: Into<BoxError>,
    {
        sui_rpc::Client::new(uri).map(Self)
    }

    pub fn with_headers(self, headers: HeadersInterceptor) -> Self {
        Self(self.0.with_headers(headers))
    }

    pub async fn get_latest_checkpoint(&mut self) -> Result<CertifiedCheckpointSummary> {
        self.get_checkpoint_internal(None).await
    }

    pub async fn get_checkpoint_summary(
        &mut self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<CertifiedCheckpointSummary> {
        self.get_checkpoint_internal(Some(sequence_number)).await
    }

    async fn get_checkpoint_internal(
        &mut self,
        sequence_number: Option<CheckpointSequenceNumber>,
    ) -> Result<CertifiedCheckpointSummary> {
        let mut request = proto::GetCheckpointRequest::default()
            .with_read_mask(FieldMask::from_paths(["summary.bcs", "signature"]));
        request.checkpoint_id = sequence_number.map(|sequence_number| {
            proto::get_checkpoint_request::CheckpointId::SequenceNumber(sequence_number)
        });

        let (metadata, checkpoint, _extentions) = self
            .0
            .ledger_client()
            .get_checkpoint(request)
            .await?
            .into_parts();

        let checkpoint = checkpoint
            .checkpoint
            .ok_or_else(|| tonic::Status::not_found("no checkpoint returned"))?;
        certified_checkpoint_summary_try_from_proto(&checkpoint)
            .map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    pub async fn get_full_checkpoint(
        &mut self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Checkpoint> {
        let request = proto::GetCheckpointRequest::by_sequence_number(sequence_number)
            .with_read_mask(Checkpoint::proto_field_mask());

        let (metadata, response, _extentions) = self
            .0
            .ledger_client()
            .max_decoding_message_size(128 * 1024 * 1024)
            .get_checkpoint(request)
            .await?
            .into_parts();

        let checkpoint = response
            .checkpoint
            .ok_or_else(|| tonic::Status::not_found("no checkpoint returned"))?;
        sui_types::full_checkpoint_content::Checkpoint::try_from(&checkpoint)
            .map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    pub async fn get_object(&mut self, object_id: ObjectID) -> Result<Object> {
        self.get_object_internal(object_id, None).await
    }

    pub async fn get_object_with_version(
        &mut self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Object> {
        self.get_object_internal(object_id, Some(version.value()))
            .await
    }

    async fn get_object_internal(
        &mut self,
        object_id: ObjectID,
        version: Option<u64>,
    ) -> Result<Object> {
        let mut request = proto::GetObjectRequest::new(&object_id.into())
            .with_read_mask(FieldMask::from_paths(["bcs"]));
        request.version = version;

        let (metadata, object, _extentions) = self
            .0
            .ledger_client()
            .get_object(request)
            .await?
            .into_parts();

        let object = object
            .object
            .ok_or_else(|| tonic::Status::not_found("no object returned"))?;
        object_try_from_proto(&object).map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    pub async fn execute_transaction(
        &mut self,
        transaction: &Transaction,
    ) -> Result<ExecutedTransaction> {
        let request = Self::create_executed_transaction_request(transaction)?;

        let (metadata, response, _extentions) = self
            .0
            .execution_client()
            .execute_transaction(request)
            .await?
            .into_parts();

        execute_transaction_response_try_from_proto(&response)
            .map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    pub async fn execute_transaction_and_wait_for_checkpoint(
        &self,
        transaction: &Transaction,
    ) -> Result<ExecutedTransaction> {
        const WAIT_FOR_CHECKPOINT_TIMEOUT: Duration = Duration::from_secs(30);

        let request = Self::create_executed_transaction_request(transaction)?;

        let (metadata, response, _extentions) = self
            .0
            .clone()
            .execute_transaction_and_wait_for_checkpoint(request, WAIT_FOR_CHECKPOINT_TIMEOUT)
            .await
            .map_err(|e| Status::from_error(e.into()))?
            .into_parts();

        execute_transaction_response_try_from_proto(&response)
            .map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    fn create_executed_transaction_request(
        transaction: &Transaction,
    ) -> Result<proto::ExecuteTransactionRequest> {
        let signatures = transaction
            .inner()
            .tx_signatures
            .iter()
            .map(|signature| {
                let mut message = proto::UserSignature::default();
                message.bcs = Some(signature.as_ref().to_vec().into());
                message
            })
            .collect();

        let request = proto::ExecuteTransactionRequest::new({
            let mut tx = proto::Transaction::default();
            tx.bcs = Some(
                proto::Bcs::serialize(&transaction.inner().intent_message.value)
                    .map_err(|e| Status::from_error(e.into()))?,
            );
            tx
        })
        .with_signatures(signatures)
        .with_read_mask(ExecutedTransaction::proto_read_mask());

        Ok(request)
    }

    pub async fn get_transaction(
        &mut self,
        digest: &TransactionDigest,
    ) -> Result<ExecutedTransaction> {
        let request = proto::GetTransactionRequest::new(&(*digest).into())
            .with_read_mask(ExecutedTransaction::proto_read_mask());

        let (metadata, resp, _extentions) = self
            .0
            .ledger_client()
            .get_transaction(request)
            .await?
            .into_parts();

        let transaction = resp
            .transaction
            .ok_or_else(|| tonic::Status::not_found("no transaction returned"))?;
        executed_transaction_try_from_proto(&transaction)
            .map_err(|e| status_from_error_with_metadata(e, metadata))
    }

    pub async fn get_chain_identifier(&self) -> Result<ChainIdentifier> {
        let response = self
            .0
            .clone()
            .ledger_client()
            .get_service_info(GetServiceInfoRequest::default())
            .await?
            .into_inner();
        let chain_id = response
            .chain_id()
            .parse::<sui_sdk_types::Digest>()
            .map_err(|e| TryFromProtoError::invalid("chain_id", e))
            .map_err(|e| Status::from_error(e.into()))?;

        Ok(ChainIdentifier::from(
            sui_types::digests::CheckpointDigest::from(chain_id),
        ))
    }

    pub async fn get_owned_objects(
        &self,
        owner: SuiAddress,
        object_type: Option<move_core_types::language_storage::StructTag>,
        page_size: Option<u32>,
        page_token: Option<Bytes>,
    ) -> Result<Page<Object>> {
        let mut request = proto::ListOwnedObjectsRequest::default()
            .with_owner(owner.to_string())
            .with_read_mask(FieldMask::from_paths(["bcs"]));
        if let Some(object_type) = object_type {
            request.set_object_type(object_type.to_canonical_string(true));
        }

        if let Some(page_size) = page_size {
            request.set_page_size(page_size);
        }

        if let Some(page_token) = page_token {
            request.set_page_token(page_token);
        }

        let (metadata, response, _extentions) = self
            .0
            .clone()
            .state_client()
            .list_owned_objects(request)
            .await?
            .into_parts();

        let objects = response
            .objects()
            .iter()
            .map(object_try_from_proto)
            .collect::<Result<_, _>>()
            .map_err(|e| status_from_error_with_metadata(e, metadata))?;

        Ok(Page {
            items: objects,
            next_page_token: response.next_page_token,
        })
    }

    pub fn list_owned_objects(
        &self,
        owner: SuiAddress,
        object_type: Option<move_core_types::language_storage::StructTag>,
    ) -> impl Stream<Item = Result<Object>> + 'static {
        let mut request = proto::ListOwnedObjectsRequest::default()
            .with_owner(owner.to_string())
            .with_read_mask(FieldMask::from_paths(["bcs"]));

        if let Some(object_type) = object_type {
            request.set_object_type(object_type.to_canonical_string(true));
        }

        self.0
            .list_owned_objects(request)
            .and_then(|object| async move {
                object_try_from_proto(&object).map_err(|e| Status::from_error(e.into()))
            })
    }

    pub async fn get_reference_gas_price(&self) -> Result<u64> {
        let request = proto::GetEpochRequest::default()
            .with_read_mask(FieldMask::from_paths(["epoch", "reference_gas_price"]));

        let response = self
            .0
            .clone()
            .ledger_client()
            .get_epoch(request)
            .await?
            .into_inner();

        Ok(response.epoch().reference_gas_price())
    }

    /// Wait for a transaction to be available in the ledger AND indexed (equivalent to WaitForLocalExecution)
    pub async fn wait_for_transaction(
        &self,
        digest: &sui_types::digests::TransactionDigest,
    ) -> Result<(), anyhow::Error> {
        const WAIT_FOR_LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(30);
        const WAIT_FOR_LOCAL_EXECUTION_DELAY: Duration = Duration::from_millis(200);
        const WAIT_FOR_LOCAL_EXECUTION_INTERVAL: Duration = Duration::from_millis(500);

        let mut client = self.0.clone();
        let mut client = client.ledger_client();

        tokio::time::timeout(WAIT_FOR_LOCAL_EXECUTION_TIMEOUT, async {
            // Apply a short delay to give the full node a chance to catch up.
            tokio::time::sleep(WAIT_FOR_LOCAL_EXECUTION_DELAY).await;

            let mut interval = tokio::time::interval(WAIT_FOR_LOCAL_EXECUTION_INTERVAL);
            loop {
                interval.tick().await;

                let request = proto::GetTransactionRequest::default()
                    .with_digest(digest.to_string())
                    .with_read_mask(prost_types::FieldMask::from_paths(["digest", "checkpoint"]));

                if let Ok(response) = client.get_transaction(request).await {
                    let tx = response.into_inner().transaction;
                    if let Some(executed_tx) = tx {
                        // Check that transaction is indexed (checkpoint field is populated)
                        if executed_tx.checkpoint.is_some() {
                            break;
                        }
                    }
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for transaction indexing: {}", digest))?;

        Ok(())
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ExecutedTransaction {
    pub transaction: TransactionData,
    pub effects: TransactionEffects,
    pub clever_error: Option<proto::CleverError>,
    pub events: Option<TransactionEvents>,
    #[allow(unused)]
    changed_objects: Vec<proto::ChangedObject>,
    #[allow(unused)]
    unchanged_loaded_runtime_objects: Vec<proto::ObjectReference>,
    pub balance_changes: Vec<sui_sdk_types::BalanceChange>,
    pub checkpoint: Option<u64>,
    #[allow(unused)]
    #[serde(skip)]
    timestamp: Option<prost_types::Timestamp>,
}

impl ExecutedTransaction {
    fn proto_read_mask() -> FieldMask {
        use proto::ExecutedTransaction;
        FieldMask::from_paths([
            ExecutedTransaction::path_builder()
                .transaction()
                .bcs()
                .finish(),
            ExecutedTransaction::path_builder().effects().bcs().finish(),
            ExecutedTransaction::path_builder()
                .effects()
                .status()
                .error()
                .abort()
                .clever_error()
                .finish(),
            ExecutedTransaction::path_builder()
                .effects()
                .unchanged_loaded_runtime_objects()
                .finish(),
            ExecutedTransaction::path_builder()
                .effects()
                .changed_objects()
                .finish(),
            ExecutedTransaction::path_builder().events().bcs().finish(),
            ExecutedTransaction::path_builder()
                .balance_changes()
                .finish(),
            ExecutedTransaction::path_builder().checkpoint(),
            ExecutedTransaction::path_builder().timestamp(),
        ])
    }

    pub fn get_new_package_obj(&self) -> Option<sui_types::base_types::ObjectRef> {
        use sui_rpc::proto::sui::rpc::v2::changed_object::OutputObjectState;

        self.changed_objects
            .iter()
            .find(|o| matches!(o.output_state(), OutputObjectState::PackageWrite))
            .and_then(|o| {
                let id = o.object_id().parse().ok()?;
                let version = o.output_version().into();
                let digest = o.output_digest().parse().ok()?;
                Some((id, version, digest))
            })
    }

    pub fn get_new_package_upgrade_cap(&self) -> Option<sui_types::base_types::ObjectRef> {
        use sui_rpc::proto::sui::rpc::v2::changed_object::OutputObjectState;
        use sui_rpc::proto::sui::rpc::v2::owner::OwnerKind;

        const UPGRADE_CAP: &str = "0x0000000000000000000000000000000000000000000000000000000000000002::package::UpgradeCap";

        self.changed_objects
            .iter()
            .find(|o| {
                matches!(o.output_state(), OutputObjectState::ObjectWrite)
                    && matches!(
                        o.output_owner().kind(),
                        OwnerKind::Address | OwnerKind::ConsensusAddress
                    )
                    && o.object_type() == UPGRADE_CAP
            })
            .and_then(|o| {
                let id = o.object_id().parse().ok()?;
                let version = o.output_version().into();
                let digest = o.output_digest().parse().ok()?;
                Some((id, version, digest))
            })
    }
}

/// Attempts to parse `CertifiedCheckpointSummary` from a proto::Checkpoint
#[allow(clippy::result_large_err)]
fn certified_checkpoint_summary_try_from_proto(
    checkpoint: &proto::Checkpoint,
) -> Result<CertifiedCheckpointSummary, TryFromProtoError> {
    let summary = checkpoint
        .summary
        .as_ref()
        .and_then(|summary| summary.bcs.as_ref())
        .ok_or_else(|| TryFromProtoError::missing("summary.bcs"))?
        .deserialize()
        .map_err(|e| TryFromProtoError::invalid("summary.bcs", e))?;

    let signature = sui_types::crypto::AuthorityStrongQuorumSignInfo::from(
        sui_sdk_types::ValidatorAggregatedSignature::try_from(
            checkpoint
                .signature
                .as_ref()
                .ok_or_else(|| TryFromProtoError::missing("signature"))?,
        )
        .map_err(|e| TryFromProtoError::invalid("signature", e))?,
    );

    Ok(CertifiedCheckpointSummary::new_from_data_and_sig(
        summary, signature,
    ))
}

/// Attempts to parse `Object` from the bcs fields in `GetObjectResponse`
#[allow(clippy::result_large_err)]
fn object_try_from_proto(object: &proto::Object) -> Result<Object, TryFromProtoError> {
    object
        .bcs
        .as_ref()
        .ok_or_else(|| TryFromProtoError::missing("bcs"))?
        .deserialize()
        .map_err(|e| TryFromProtoError::invalid("bcs", e))
}

/// Attempts to parse `ExecutedTransaction` from the fields in `proto::ExecuteTransactionResponse`
#[allow(clippy::result_large_err)]
fn execute_transaction_response_try_from_proto(
    response: &proto::ExecuteTransactionResponse,
) -> Result<ExecutedTransaction, TryFromProtoError> {
    let executed_transaction = response
        .transaction
        .as_ref()
        .ok_or_else(|| TryFromProtoError::missing("transaction"))?;

    executed_transaction_try_from_proto(executed_transaction)
}

#[allow(clippy::result_large_err)]
fn executed_transaction_try_from_proto(
    executed_transaction: &proto::ExecutedTransaction,
) -> Result<ExecutedTransaction, TryFromProtoError> {
    let transaction = executed_transaction
        .transaction()
        .bcs()
        .deserialize()
        .map_err(|e| TryFromProtoError::invalid("transaction.bcs", e))?;

    let effects = executed_transaction
        .effects()
        .bcs()
        .deserialize()
        .map_err(|e| TryFromProtoError::invalid("effects.bcs", e))?;
    let clever_error = executed_transaction
        .effects()
        .status()
        .error()
        .abort()
        .clever_error_opt()
        .cloned();
    let events = executed_transaction
        .events
        .as_ref()
        .and_then(|events| events.bcs.as_ref())
        .map(|bcs| bcs.deserialize())
        .transpose()
        .map_err(|e| TryFromProtoError::invalid("events.bcs", e))?;

    let balance_changes = executed_transaction
        .balance_changes
        .iter()
        .map(TryInto::try_into)
        .collect::<Result<_, _>>()?;

    ExecutedTransaction {
        transaction,
        effects,
        clever_error,
        events,
        balance_changes,
        checkpoint: executed_transaction.checkpoint,
        changed_objects: executed_transaction.effects().changed_objects().to_owned(),
        unchanged_loaded_runtime_objects: executed_transaction
            .effects()
            .unchanged_loaded_runtime_objects()
            .to_owned(),
        timestamp: executed_transaction.timestamp,
    }
    .pipe(Ok)
}

fn status_from_error_with_metadata<T: Into<BoxError>>(err: T, metadata: MetadataMap) -> Status {
    let mut status = Status::from_error(err.into());
    *status.metadata_mut() = metadata;
    status
}
