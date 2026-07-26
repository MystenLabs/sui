// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! GQL Queries
//! Interface to the rpc for the gql schema defined in `crates\sui-indexer-alt-graphql/schema.graphql`.
//! No GQL type escapes this module. From here we return structures defined in this crate
//! or bcs encoded data of runtime structures.
//!
//! This module is private to the `DataStore` and packcaged in its own module for convenience.

use crate::{CheckpointExecutionContext, EpochData, stores::DataStore};
use anyhow::{Context, Error, anyhow, bail};
use cynic::{GraphQlResponse, QueryBuilder};
use fastcrypto::encoding::{Base64 as CryptoBase64, Encoding};

// Register the schema which was loaded in the build.rs call.
#[cynic::schema("rpc")]
mod schema {
    use chrono::{DateTime as ChronoDateTime, Utc};
    cynic::impl_scalar!(u64, UInt53);
    cynic::impl_scalar!(ChronoDateTime<Utc>, DateTime);
}

/// Return response data only when GraphQL reported no field-level errors.
///
/// Use for queries whose result is meaningful only when every requested field resolved.
fn graphql_data<T>(response: GraphQlResponse<T>, operation: &str) -> Result<Option<T>, Error> {
    let GraphQlResponse { data, errors } = response;
    if let Some(errors) = errors.filter(|errors| !errors.is_empty()) {
        bail!("{operation} returned GraphQL errors: {errors:?}");
    }
    Ok(data)
}

pub(crate) mod epoch_query {
    use super::*;
    use chrono::{DateTime as ChronoDateTime, Utc};

    #[derive(cynic::QueryVariables)]
    pub(crate) struct EpochDataArgs {
        pub epoch: Option<u64>,
    }

    #[derive(cynic::QueryFragment)]
    #[cynic(variables = "EpochDataArgs")]
    pub(crate) struct Query {
        #[arguments(epochId: $epoch)]
        epoch: Option<Epoch>,
    }

    #[derive(cynic::Scalar, Clone)]
    #[cynic(graphql_type = "BigInt")]
    pub(crate) struct BigInt(pub(super) String);

    #[derive(cynic::QueryFragment)]
    pub(crate) struct Epoch {
        pub(super) epoch_id: u64,
        pub(super) protocol_configs: Option<ProtocolConfigs>,
        pub(super) reference_gas_price: Option<BigInt>,
        pub(super) start_timestamp: Option<ChronoDateTime<Utc>>,
    }

    #[derive(cynic::QueryFragment)]
    pub(crate) struct ProtocolConfigs {
        pub(super) protocol_version: u64,
    }

    impl Epoch {
        /// Convert a GraphQL epoch fragment into checked runtime epoch metadata.
        pub(super) fn try_into_epoch_data(self) -> Result<EpochData, Error> {
            let epoch_id = self.epoch_id;
            let protocol_version = self
                .protocol_configs
                .ok_or_else(|| anyhow!("Missing protocol configuration for epoch {epoch_id}"))?
                .protocol_version;
            let rgp = self
                .reference_gas_price
                .ok_or_else(|| anyhow!("Missing reference gas price for epoch {epoch_id}"))?
                .0;
            let rgp = rgp
                .parse()
                .with_context(|| format!("Invalid reference gas price for epoch {epoch_id}"))?;
            let start_timestamp = self
                .start_timestamp
                .ok_or_else(|| anyhow!("Missing start timestamp for epoch {epoch_id}"))?
                .timestamp_millis();
            let start_timestamp = u64::try_from(start_timestamp)
                .with_context(|| format!("Invalid start timestamp for epoch {epoch_id}"))?;

            Ok(EpochData {
                epoch_id,
                protocol_version,
                rgp,
                start_timestamp,
            })
        }
    }

    pub(crate) async fn query(
        epoch_id: u64,
        data_store: &DataStore,
    ) -> Result<Option<EpochData>, Error> {
        let query = Query::build(EpochDataArgs {
            epoch: Some(epoch_id),
        });
        let response = data_store.run_query(&query).await?;

        let Some(epoch) = graphql_data(response, "epoch query")?.and_then(|data| data.epoch) else {
            return Ok(None);
        };
        if epoch.epoch_id != epoch_id {
            bail!(
                "Epoch query returned epoch {} for requested epoch {epoch_id}",
                epoch.epoch_id
            );
        }
        Ok(Some(epoch.try_into_epoch_data()?))
    }
}

pub(crate) mod txn_query {
    use super::*;
    use sui_types::transaction::TransactionData;

    #[derive(cynic::Scalar, Debug, Clone)]
    #[cynic(graphql_type = "Base64")]
    pub(crate) struct Base64(pub String);

    #[derive(cynic::QueryVariables)]
    pub(crate) struct TransactionDataArgs {
        pub digest: String,
    }

    #[derive(cynic::QueryFragment)]
    #[cynic(variables = "TransactionDataArgs")]
    pub(crate) struct Query {
        #[arguments(digest: $digest)]
        transaction: Option<Transaction>,
    }

    #[derive(cynic::QueryFragment)]
    pub(crate) struct Transaction {
        transaction_bcs: Option<Base64>,
        effects: Option<TransactionEffects>,
    }

    #[derive(cynic::QueryFragment)]
    pub(crate) struct TransactionEffects {
        checkpoint: Option<Checkpoint>,
        effects_bcs: Option<Base64>,
    }

    #[derive(cynic::QueryFragment)]
    pub(crate) struct Checkpoint {
        sequence_number: u64,
    }

    pub(crate) async fn query(
        digest: String,
        data_store: &DataStore,
    ) -> Result<Option<(TransactionData, sui_types::effects::TransactionEffects, u64)>, Error> {
        let query = Query::build(TransactionDataArgs {
            digest: digest.clone(),
        });
        let response = data_store
            .run_query(&query)
            .await
            .context("Failed to run transaction query")?;

        let Some(transaction) = response.data.and_then(|txn| txn.transaction) else {
            return Ok(None);
        };

        let txn_data: TransactionData = bcs::from_bytes(
            &CryptoBase64::decode(
                &transaction
                    .transaction_bcs
                    .ok_or_else(|| {
                        anyhow!(format!(
                            "Transaction data not available (None) for digest: {}",
                            digest
                        ),)
                    })?
                    .0,
            )
            .context(format!(
                "Transaction data does not decode for digest: {}",
                digest
            ))?,
        )
        .context(format!(
            "Cannot deserialize transaction data for digest {}",
            digest
        ))?;

        let effect_frag = transaction
            .effects
            .ok_or_else(|| anyhow!("Missing effects in transaction data response"))?;
        let effects: sui_types::effects::TransactionEffects = bcs::from_bytes(
            &CryptoBase64::decode(
                &effect_frag
                    .effects_bcs
                    .ok_or_else(|| anyhow!("Missing effects bcs in transaction data response"))?
                    .0,
            )
            .context(format!(
                "Transaction effects do not decode for digest: {}",
                digest
            ))?,
        )
        .context(format!(
            "Cannot deserialize transaction effects for digest {}",
            digest
        ))?;

        let checkpoint = effect_frag
            .checkpoint
            .ok_or_else(|| anyhow!("Missing checkpoint in transaction query response"))?
            .sequence_number;

        Ok(Some((txn_data, effects, checkpoint)))
    }
}

pub(crate) mod object_query {
    use sui_types::object::Object;

    use super::*;
    use crate::{ObjectKey as GqlObjectKey, VersionQuery};

    #[derive(cynic::Scalar, Debug, Clone)]
    #[cynic(graphql_type = "SuiAddress")]
    pub(crate) struct SuiAddress(pub String);

    #[derive(cynic::Scalar, Debug, Clone)]
    #[cynic(graphql_type = "Base64")]
    pub(crate) struct Base64(pub String);

    #[derive(cynic::InputObject, Debug)]
    #[cynic(graphql_type = "ObjectKey")]
    pub(crate) struct ObjectKey {
        pub address: SuiAddress,
        pub version: Option<u64>,
        pub root_version: Option<u64>,
        pub at_checkpoint: Option<u64>,
    }

    #[derive(cynic::QueryVariables)]
    pub(crate) struct MultiGetObjectsVars {
        pub keys: Vec<ObjectKey>,
    }

    #[derive(cynic::QueryFragment)]
    #[cynic(variables = "MultiGetObjectsVars", graphql_type = "Query")]
    pub(crate) struct MultiGetObjectsQuery {
        #[arguments(keys: $keys)]
        pub multi_get_objects: Vec<Option<ObjectFragment>>,
    }

    #[derive(cynic::QueryFragment)]
    #[cynic(graphql_type = "Object", schema_module = "crate::gql_queries::schema")]
    pub(crate) struct ObjectFragment {
        #[allow(dead_code)]
        pub address: SuiAddress,
        pub version: Option<u64>,
        pub object_bcs: Option<Base64>,
    }

    // Maximum number of keys to query in a single request.
    // REVIEW: not clear how this translate to the 5000B limit, so
    // we are picking a "random" and conservative number.
    const MAX_KEYS_SIZE: usize = 30;

    pub(crate) async fn query(
        keys: &[GqlObjectKey],
        data_store: &DataStore,
    ) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let mut keys = keys
            .iter()
            .cloned()
            .map(ObjectKey::from)
            .collect::<Vec<_>>();
        let mut key_chunks = vec![];
        while !keys.is_empty() {
            let chunk: Vec<_> = keys.drain(..MAX_KEYS_SIZE.min(keys.len())).collect();
            key_chunks.push(chunk);
        }

        let mut objects = vec![];

        for keys in key_chunks {
            let query: cynic::Operation<MultiGetObjectsQuery, MultiGetObjectsVars> =
                MultiGetObjectsQuery::build(MultiGetObjectsVars { keys });
            let response = data_store.run_query(&query).await?;

            let list = if let Some(data) = response.data {
                data.multi_get_objects
            } else {
                return Err(anyhow!(
                    "Missing data in transaction query response. Errors: {:?}",
                    response.errors,
                ));
            };

            let chunk = list
                .into_iter()
                .map(|frag| match frag {
                    Some(frag) => {
                        let b64 = frag
                            .object_bcs
                            .ok_or_else(|| anyhow!("Object bcs is None for object"))?
                            .0;
                        let bytes = CryptoBase64::decode(&b64)?;
                        let obj: Object = bcs::from_bytes(&bytes)?;
                        let version = frag
                            .version
                            .ok_or_else(|| anyhow!("Object version is None for object"))?;
                        Ok::<_, Error>(Some((obj, version)))
                    }
                    None => Ok::<_, Error>(None),
                })
                .collect::<Result<Vec<Option<(Object, u64)>>, _>>()?;
            objects.extend(chunk);
        }
        Ok(objects)
    }

    impl From<GqlObjectKey> for ObjectKey {
        fn from(key: GqlObjectKey) -> Self {
            ObjectKey {
                address: SuiAddress(key.object_id.to_string()),
                version: match key.version_query {
                    VersionQuery::Version(v) => Some(v),
                    _ => None,
                },
                root_version: match key.version_query {
                    VersionQuery::RootVersion(v) => Some(v),
                    _ => None,
                },
                at_checkpoint: match key.version_query {
                    VersionQuery::AtCheckpoint(v) => Some(v),
                    _ => None,
                },
            }
        }
    }
}

pub(crate) mod checkpoint_query {
    //! Loads checkpoint and epoch metadata together, then validates their consistency.

    use super::*;
    use sui_types::{
        digests::{ChainIdentifier, CheckpointDigest},
        message_envelope::Message,
        messages_checkpoint::CheckpointSummary,
    };

    /// Base64-encoded BCS data returned by checkpoint fields.
    #[derive(cynic::Scalar, Debug, Clone)]
    #[cynic(graphql_type = "Base64")]
    pub(crate) struct Base64(pub String);

    /// Variables for selecting the latest checkpoint or an explicit sequence number.
    #[derive(cynic::QueryVariables)]
    pub(crate) struct CheckpointArgs {
        /// `None` requests the latest checkpoint known to the GraphQL endpoint.
        pub sequence_number: Option<u64>,
    }

    /// Root response that keeps chain identity and checkpoint metadata in one operation.
    #[derive(cynic::QueryFragment)]
    #[cynic(variables = "CheckpointArgs", graphql_type = "Query")]
    pub(crate) struct Query {
        /// Genesis checkpoint digest identifying the network.
        chain_identifier: String,
        /// GraphQL checkpoint selected by the query.
        #[arguments(sequenceNumber: $sequence_number)]
        checkpoint: Option<GraphQLCheckpoint>,
    }

    /// GraphQL fields needed to construct a checkpoint execution context candidate.
    #[derive(cynic::QueryFragment)]
    #[cynic(graphql_type = "Checkpoint")]
    pub(crate) struct GraphQLCheckpoint {
        /// Serialized summary used for identity and epoch-boundary selection.
        summary_bcs: Option<Base64>,
        /// Epoch metadata nested under the selected checkpoint.
        epoch: Option<epoch_query::Epoch>,
    }

    /// Decoded context plus summary metadata needed before final checkpoint selection.
    #[derive(Debug)]
    struct CheckpointExecutionContextCandidate {
        /// Execution context to return if this candidate is selected.
        context: CheckpointExecutionContext,
        /// Digest linking this checkpoint to its immediate predecessor.
        previous_digest: Option<CheckpointDigest>,
        /// Whether this checkpoint contains the epoch transition.
        is_end_of_epoch: bool,
    }

    /// Query checkpoint metadata and construct a coherent execution context.
    ///
    /// `Some` selects an explicit checkpoint; `None` selects the latest. The final checkpoint
    /// includes the epoch-change transaction, whose object writes advance system state and
    /// packages into the next epoch while the checkpoint still belongs to the ending epoch. Latest
    /// selection therefore uses its predecessor to keep object state and protocol context aligned;
    /// explicitly selecting the final checkpoint returns an error instead of changing the request.
    pub(crate) async fn query(
        sequence_number: Option<u64>,
        data_store: &DataStore,
    ) -> Result<CheckpointExecutionContext, Error> {
        let selected = query_candidate(sequence_number, data_store).await?;
        let Some(previous_sequence) = fallback_sequence(sequence_number, &selected)? else {
            return Ok(selected.context);
        };

        let previous = query_candidate(Some(previous_sequence), data_store).await?;
        validate_fallback(&selected, &previous)?;
        Ok(previous.context)
    }

    /// Fetch checkpoint metadata and decode one candidate without applying selection policy.
    async fn query_candidate(
        sequence_number: Option<u64>,
        data_store: &DataStore,
    ) -> Result<CheckpointExecutionContextCandidate, Error> {
        let query = Query::build(CheckpointArgs { sequence_number });
        let response = data_store.run_query(&query).await?;
        let data = graphql_data(response, "checkpoint metadata query")?
            .ok_or_else(|| anyhow!("Missing checkpoint metadata"))?;
        decode_execution_context_candidate(data, sequence_number)
    }

    /// Build a candidate from its checkpoint summary and nested epoch metadata.
    fn decode_execution_context_candidate(
        data: Query,
        requested_sequence: Option<u64>,
    ) -> Result<CheckpointExecutionContextCandidate, Error> {
        let checkpoint = data
            .checkpoint
            .ok_or_else(|| anyhow!("Checkpoint is unavailable or has been pruned"))?;
        let summary: CheckpointSummary = decode_bcs(checkpoint.summary_bcs, "checkpoint summary")?;
        if let Some(requested_sequence) = requested_sequence
            && summary.sequence_number != requested_sequence
        {
            bail!(
                concat!(
                    "Checkpoint query returned sequence {} for requested checkpoint ",
                    "{}",
                ),
                summary.sequence_number,
                requested_sequence,
            );
        }

        let epoch = checkpoint
            .epoch
            .ok_or_else(|| anyhow!("Missing checkpoint epoch"))?
            .try_into_epoch_data()?;
        if epoch.epoch_id != summary.epoch {
            bail!(
                "Checkpoint {} reports epoch {} but its summary belongs to epoch {}",
                summary.sequence_number,
                epoch.epoch_id,
                summary.epoch,
            );
        }

        let chain_digest = data
            .chain_identifier
            .parse::<CheckpointDigest>()
            .context("Invalid chain identifier in checkpoint response")?;

        Ok(CheckpointExecutionContextCandidate {
            context: CheckpointExecutionContext {
                chain_identifier: ChainIdentifier::from(chain_digest),
                checkpoint: summary.sequence_number,
                checkpoint_digest: summary.digest(),
                epoch,
            },
            previous_digest: summary.previous_digest,
            is_end_of_epoch: summary.is_last_checkpoint_of_epoch(),
        })
    }

    /// Return the predecessor to use when latest selection lands on an epoch boundary.
    fn fallback_sequence(
        requested_sequence: Option<u64>,
        candidate: &CheckpointExecutionContextCandidate,
    ) -> Result<Option<u64>, Error> {
        if !candidate.is_end_of_epoch {
            return Ok(None);
        }
        if requested_sequence.is_some() {
            bail!(
                concat!(
                    "Checkpoint {} is the final checkpoint of epoch {} and cannot be selected for ",
                    "local execution",
                ),
                candidate.context.checkpoint,
                candidate.context.epoch.epoch_id
            );
        }
        candidate
            .context
            .checkpoint
            .checked_sub(1)
            .map(Some)
            .ok_or_else(|| anyhow!("Genesis checkpoint cannot be an end-of-epoch fallback"))
    }

    /// Verify that a fallback is the linked predecessor in the same chain and epoch.
    fn validate_fallback(
        selected: &CheckpointExecutionContextCandidate,
        previous: &CheckpointExecutionContextCandidate,
    ) -> Result<(), Error> {
        if previous.context.chain_identifier != selected.context.chain_identifier {
            bail!(
                "Checkpoint {} and checkpoint {} have different chain identifiers",
                previous.context.checkpoint,
                selected.context.checkpoint
            );
        }
        if previous.context.epoch.epoch_id != selected.context.epoch.epoch_id {
            bail!(
                "Checkpoint {} epoch {} does not match end-of-epoch checkpoint {} epoch {}",
                previous.context.checkpoint,
                previous.context.epoch.epoch_id,
                selected.context.checkpoint,
                selected.context.epoch.epoch_id
            );
        }
        if selected.previous_digest != Some(previous.context.checkpoint_digest) {
            bail!(
                "Checkpoint {} digest does not match the previous digest recorded by checkpoint {}",
                previous.context.checkpoint,
                selected.context.checkpoint
            );
        }
        Ok(())
    }

    /// Decode a required Base64 GraphQL field into its BCS runtime type.
    fn decode_bcs<T>(field: Option<Base64>, label: &str) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned,
    {
        let bytes = CryptoBase64::decode(
            &field
                .ok_or_else(|| anyhow!("Missing {label} in GraphQL response"))?
                .0,
        )
        .with_context(|| format!("{label} does not decode as Base64"))?;
        bcs::from_bytes(&bytes).with_context(|| format!("Cannot deserialize {label}"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use chrono::{DateTime as ChronoDateTime, Utc};
        use sui_types::{
            message_envelope::Message,
            messages_checkpoint::CheckpointSummary,
            test_checkpoint_data_builder::{AdvanceEpochConfig, TestCheckpointBuilder},
        };

        fn checkpoint_summary(sequence: u64, epoch: u64) -> CheckpointSummary {
            let mut builder = TestCheckpointBuilder::new(sequence).with_epoch(epoch);
            builder.build_checkpoint().summary.data().clone()
        }

        fn end_of_epoch_summary(sequence: u64, epoch: u64) -> CheckpointSummary {
            let mut builder = TestCheckpointBuilder::new(sequence).with_epoch(epoch);
            builder
                .advance_epoch(AdvanceEpochConfig::default())
                .summary
                .data()
                .clone()
        }

        fn query_data(summary: &CheckpointSummary) -> (Query, ChainIdentifier) {
            let chain_digest = CheckpointDigest::random();
            let summary_bytes = bcs::to_bytes(summary).unwrap();
            (
                Query {
                    chain_identifier: chain_digest.to_string(),
                    checkpoint: Some(GraphQLCheckpoint {
                        summary_bcs: Some(Base64(
                            CryptoBase64::from_bytes(&summary_bytes).encoded(),
                        )),
                        epoch: Some(epoch_query::Epoch {
                            epoch_id: summary.epoch,
                            protocol_configs: Some(epoch_query::ProtocolConfigs {
                                protocol_version: 42,
                            }),
                            reference_gas_price: Some(epoch_query::BigInt("1000".to_string())),
                            start_timestamp: Some(
                                ChronoDateTime::<Utc>::from_timestamp_millis(123).unwrap(),
                            ),
                        }),
                    }),
                },
                ChainIdentifier::from(chain_digest),
            )
        }

        #[test]
        fn decodes_coherent_checkpoint_context() {
            let summary = checkpoint_summary(7, 3);
            let (data, chain_identifier) = query_data(&summary);

            let candidate = decode_execution_context_candidate(data, Some(7)).unwrap();

            assert_eq!(candidate.context.chain_identifier, chain_identifier);
            assert_eq!(candidate.context.checkpoint, 7);
            assert_eq!(candidate.context.checkpoint_digest, summary.digest());
            assert_eq!(
                candidate.context.epoch,
                EpochData {
                    epoch_id: 3,
                    protocol_version: 42,
                    rgp: 1_000,
                    start_timestamp: 123,
                }
            );
            assert!(!candidate.is_end_of_epoch);
        }

        #[test]
        fn rejects_an_unexpected_checkpoint_sequence() {
            let summary = checkpoint_summary(8, 3);
            let (data, _) = query_data(&summary);

            assert!(decode_execution_context_candidate(data, Some(7)).is_err());
        }

        #[test]
        fn rejects_a_nested_epoch_that_differs_from_the_summary() {
            let summary = checkpoint_summary(7, 3);
            let (mut data, _) = query_data(&summary);
            data.checkpoint
                .as_mut()
                .unwrap()
                .epoch
                .as_mut()
                .unwrap()
                .epoch_id = 4;

            assert!(decode_execution_context_candidate(data, Some(7)).is_err());
        }

        #[test]
        fn invalid_reference_gas_price_returns_an_error() {
            let summary = checkpoint_summary(7, 3);
            let (mut data, _) = query_data(&summary);
            data.checkpoint
                .as_mut()
                .unwrap()
                .epoch
                .as_mut()
                .unwrap()
                .reference_gas_price = Some(epoch_query::BigInt("not-a-number".to_string()));

            assert!(decode_execution_context_candidate(data, Some(7)).is_err());
        }

        #[test]
        fn invalid_chain_identifier_returns_an_error() {
            let summary = checkpoint_summary(7, 3);
            let (mut data, _) = query_data(&summary);
            data.chain_identifier = "not-a-digest".to_string();

            assert!(decode_execution_context_candidate(data, Some(7)).is_err());
        }

        #[test]
        fn latest_end_of_epoch_checkpoint_selects_its_predecessor() {
            let summary = end_of_epoch_summary(7, 3);
            let (data, _) = query_data(&summary);
            let candidate = decode_execution_context_candidate(data, None).unwrap();

            assert_eq!(fallback_sequence(None, &candidate).unwrap(), Some(6));
        }

        #[test]
        fn explicit_end_of_epoch_checkpoint_is_rejected() {
            let summary = end_of_epoch_summary(7, 3);
            let (data, _) = query_data(&summary);
            let candidate = decode_execution_context_candidate(data, Some(7)).unwrap();

            assert!(fallback_sequence(Some(7), &candidate).is_err());
        }

        #[test]
        fn validates_end_of_epoch_fallback_digest() {
            let previous_summary = checkpoint_summary(6, 3);
            let mut selected_summary = end_of_epoch_summary(7, 3);
            selected_summary.previous_digest = Some(previous_summary.digest());
            let (previous_data, _) = query_data(&previous_summary);
            let (mut selected_data, _) = query_data(&selected_summary);
            selected_data.chain_identifier = previous_data.chain_identifier.clone();
            let previous = decode_execution_context_candidate(previous_data, Some(6)).unwrap();
            let selected = decode_execution_context_candidate(selected_data, None).unwrap();

            validate_fallback(&selected, &previous).unwrap();
        }
    }
}

pub(crate) mod chain_id_query {
    use super::*;

    #[derive(cynic::QueryFragment)]
    pub(crate) struct Query {
        chain_identifier: Option<String>,
    }

    pub(crate) async fn query(data_store: &DataStore) -> Result<String, Error> {
        let query = Query::build(());
        let response = data_store.run_query(&query).await?;
        let Some(chain_id) = graphql_data(response, "chain identifier query")?
            .and_then(|data| data.chain_identifier)
        else {
            return Err(anyhow!("Missing chain identifier"));
        };
        Ok(chain_id)
    }
}
