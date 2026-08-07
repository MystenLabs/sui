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
        pub object_bcs: Option<Base64>,
    }

    // Maximum number of keys to query in a single request.
    // REVIEW: not clear how this translate to the 5000B limit, so
    // we are picking a "random" and conservative number.
    pub(super) const MAX_KEYS_SIZE: usize = 30;

    /// Decode fields common to every GraphQL object response.
    pub(super) fn decode_object_fragment(fragment: ObjectFragment) -> Result<Object, Error> {
        let b64 = fragment
            .object_bcs
            .ok_or_else(|| anyhow!("Object bcs is None for object"))?
            .0;
        let bytes = CryptoBase64::decode(&b64)?;
        Ok(bcs::from_bytes(&bytes)?)
    }

    pub(crate) async fn query(
        keys: &[GqlObjectKey],
        data_store: &DataStore,
    ) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let mut objects = vec![];
        for key_chunk in keys.chunks(MAX_KEYS_SIZE) {
            let keys = key_chunk.iter().cloned().map(ObjectKey::from).collect();
            let query: cynic::Operation<MultiGetObjectsQuery, MultiGetObjectsVars> =
                MultiGetObjectsQuery::build(MultiGetObjectsVars { keys });
            let response = data_store.run_query(&query).await?;
            let list = graphql_data(response, "object query")?
                .ok_or_else(|| anyhow!("Missing object query data"))?
                .multi_get_objects;

            let chunk = list
                .into_iter()
                .map(|frag| match frag {
                    Some(frag) => {
                        let object = decode_object_fragment(frag)?;
                        let version = object.version().value();
                        Ok::<_, Error>(Some((object, version)))
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

pub(crate) mod checkpoint_object_query {
    //! Loads objects through the query scope of a selected checkpoint execution context.

    #[cfg(test)]
    use super::object_query::Base64;
    use super::object_query::{
        MAX_KEYS_SIZE, ObjectFragment, ObjectKey as GraphQLObjectKey, SuiAddress,
        decode_object_fragment,
    };
    use super::*;
    use crate::{CheckpointObjectRequest, CheckpointObjectSelector};
    use sui_types::{
        digests::{ChainIdentifier, CheckpointDigest},
        object::Object,
    };

    #[derive(cynic::QueryVariables)]
    struct CheckpointObjectArgs {
        /// Explicit checkpoint from the execution context.
        sequence_number: Option<u64>,
        /// Object selectors translated into GraphQL keys.
        keys: Vec<GraphQLObjectKey>,
        /// GraphQL query type whose object metadata range is required.
        range_type: String,
        /// Singular object field backed by the same metadata as multi-get object lookup.
        range_field: Option<String>,
    }

    /// Root response used to verify that the endpoint and checkpoint match the supplied context.
    #[derive(cynic::QueryFragment)]
    #[cynic(variables = "CheckpointObjectArgs", graphql_type = "Query")]
    struct Query {
        /// Genesis checkpoint digest identifying the endpoint's network.
        chain_identifier: String,
        /// Service retention metadata evaluated in the same request as the object reads.
        service_config: ServiceConfig,
        /// Checkpoint whose nested query scopes every object read.
        #[arguments(sequenceNumber: $sequence_number)]
        checkpoint: Option<GraphQLCheckpoint>,
    }

    /// Retention configuration for the object metadata needed by checkpoint-latest reads.
    #[derive(cynic::QueryFragment)]
    #[cynic(variables = "CheckpointObjectArgs")]
    struct ServiceConfig {
        #[arguments(type: $range_type, field: $range_field)]
        available_range: AvailableRange,
    }

    /// Inclusive checkpoint range advertised for the selected GraphQL field.
    #[derive(cynic::QueryFragment)]
    struct AvailableRange {
        first: Option<RangeCheckpoint>,
        last: Option<RangeCheckpoint>,
    }

    /// Checkpoint bound returned by `availableRange`.
    #[derive(cynic::QueryFragment)]
    #[cynic(graphql_type = "Checkpoint")]
    struct RangeCheckpoint {
        sequence_number: u64,
    }

    /// Selected checkpoint and its nested, checkpoint-scoped query root.
    #[derive(cynic::QueryFragment)]
    #[cynic(variables = "CheckpointObjectArgs", graphql_type = "Checkpoint")]
    struct GraphQLCheckpoint {
        /// Sequence returned for the explicitly requested checkpoint.
        sequence_number: u64,
        /// Digest used to verify checkpoint identity against the execution context.
        digest: Option<String>,
        /// Query root scoped to state at the end of this checkpoint.
        query: Option<ScopedQuery>,
    }

    /// Query root whose unbounded reads are interpreted at the enclosing checkpoint.
    #[derive(cynic::QueryFragment)]
    #[cynic(variables = "CheckpointObjectArgs", graphql_type = "Query")]
    struct ScopedQuery {
        /// Results corresponding positionally to the requested keys.
        #[arguments(keys: $keys)]
        multi_get_objects: Vec<Option<ObjectFragment>>,
    }

    /// Load requests in order at one immutable checkpoint and one object-availability range.
    pub(crate) async fn query(
        context: &CheckpointExecutionContext,
        requests: &[CheckpointObjectRequest],
        data_store: &DataStore,
    ) -> Result<Vec<Option<Object>>, Error> {
        let mut objects = Vec::with_capacity(requests.len());
        for request_chunk in requests.chunks(MAX_KEYS_SIZE) {
            let operation = Query::build(CheckpointObjectArgs {
                sequence_number: Some(context.checkpoint),
                keys: request_chunk.iter().map(graphql_key).collect(),
                range_type: "Query".to_string(),
                range_field: Some("object".to_string()),
            });
            let response = data_store.run_query(&operation).await?;
            let data = graphql_data(response, "checkpoint object query")?
                .ok_or_else(|| anyhow!("Missing data for checkpoint object query"))?;
            objects.extend(decode_response(data, context, request_chunk)?);
        }
        Ok(objects)
    }

    /// Validate the checkpoint envelope and decode one batch of object results.
    fn decode_response(
        data: Query,
        context: &CheckpointExecutionContext,
        requests: &[CheckpointObjectRequest],
    ) -> Result<Vec<Option<Object>>, Error> {
        let chain_digest = data
            .chain_identifier
            .parse::<CheckpointDigest>()
            .context("Invalid chain identifier in checkpoint object response")?;
        let chain_identifier = ChainIdentifier::from(chain_digest);
        if chain_identifier != context.chain_identifier {
            bail!(
                concat!(
                    "Checkpoint object response chain {} does not match context ",
                    "chain {}",
                ),
                chain_identifier,
                context.chain_identifier
            );
        }

        let history_start = validate_available_range(data.service_config, context.checkpoint)?;

        let checkpoint = data.checkpoint.ok_or_else(|| {
            anyhow!(
                "Checkpoint {} is unavailable or has been pruned",
                context.checkpoint
            )
        })?;
        if checkpoint.sequence_number != context.checkpoint {
            bail!(
                "Checkpoint object query returned sequence {} for context checkpoint {}",
                checkpoint.sequence_number,
                context.checkpoint
            );
        }
        let checkpoint_digest = checkpoint
            .digest
            .ok_or_else(|| anyhow!("Missing digest for checkpoint {}", context.checkpoint))?
            .parse::<CheckpointDigest>()
            .with_context(|| format!("Invalid digest for checkpoint {}", context.checkpoint))?;
        if checkpoint_digest != context.checkpoint_digest {
            bail!(
                concat!(
                    "Checkpoint object response digest {} does not match context ",
                    "digest {}",
                ),
                checkpoint_digest,
                context.checkpoint_digest
            );
        }

        let scoped_query = checkpoint.query.ok_or_else(|| {
            anyhow!(
                "Missing checkpoint-scoped query for checkpoint {}",
                context.checkpoint
            )
        })?;
        if scoped_query.multi_get_objects.len() != requests.len() {
            bail!(
                "Checkpoint object query returned {} results for {} requests",
                scoped_query.multi_get_objects.len(),
                requests.len()
            );
        }

        std::iter::zip(requests, scoped_query.multi_get_objects)
            .map(|(request, object)| {
                decode_object(request, object, context.checkpoint, history_start)
            })
            .collect()
    }

    /// Validate that object metadata covers the selected checkpoint.
    fn validate_available_range(
        service_config: ServiceConfig,
        checkpoint: u64,
    ) -> Result<u64, Error> {
        let range = service_config.available_range;
        let first = range
            .first
            .ok_or_else(|| anyhow!("Missing first checkpoint in object availability range"))?
            .sequence_number;
        let last = range
            .last
            .ok_or_else(|| anyhow!("Missing last checkpoint in object availability range"))?
            .sequence_number;

        if first > last {
            bail!("Invalid object availability range {first}..={last}");
        }
        if checkpoint < first || checkpoint > last {
            bail!("Checkpoint {checkpoint} is outside object availability range {first}..={last}");
        }
        Ok(first)
    }

    /// Decode one positional result from checkpoint-scoped `multiGetObjects`.
    ///
    /// Each result has two optional levels:
    ///
    /// `Option<ObjectFragment { object_bcs: Option<Base64> }>`
    ///
    /// The outer `Option` says whether GraphQL returned an object. The inner `Option` says whether
    /// that object's serialized body is available. Their meanings depend on the requested selector:
    ///
    /// `Latest`:
    /// - `Some` with a body: the object is live at the checkpoint and its body is available; decode
    ///   it.
    /// - `Some` without a body: the object is live, but its body is unavailable; return an error.
    /// - `None`: no live object was returned. This proves absence when `history_start == 0`;
    ///   otherwise state from before the retained range may be missing, so return an error.
    ///
    /// `ExactVersion`:
    /// - `Some` with a body: the exact version is visible at the checkpoint; decode and validate
    ///   it.
    /// - `Some` without a body: the exact object body is unavailable; return an error.
    /// - `None`: the API cannot distinguish an absent or not-yet-written version from an
    ///   unavailable body, so return an error.
    ///
    /// Transport and GraphQL errors, range and checkpoint identity, and response cardinality are
    /// validated before this function is called.
    fn decode_object(
        request: &CheckpointObjectRequest,
        fragment: Option<ObjectFragment>,
        checkpoint: u64,
        history_start: u64,
    ) -> Result<Option<Object>, Error> {
        let Some(fragment) = fragment else {
            return match request.selector {
                CheckpointObjectSelector::Latest if history_start == 0 => Ok(None),
                CheckpointObjectSelector::Latest => bail!(
                    concat!(
                        "Latest object lookup for {} at checkpoint {} is indeterminate because ",
                        "the endpoint's object history starts at checkpoint {}",
                    ),
                    request.object_id,
                    checkpoint,
                    history_start,
                ),
                CheckpointObjectSelector::ExactVersion(version) => bail!(
                    concat!(
                        "Exact object lookup for {} at version {} and checkpoint {} is ",
                        "indeterminate: the version may not exist, may be newer than the ",
                        "checkpoint, or its body may be unavailable",
                    ),
                    request.object_id,
                    version,
                    checkpoint,
                ),
            };
        };

        let object = decode_object_fragment(fragment)?;

        if object.id() != request.object_id {
            bail!(
                "Decoded object {} does not match requested object {}",
                object.id(),
                request.object_id
            );
        }
        let object_version = object.version().value();

        match request.selector {
            CheckpointObjectSelector::ExactVersion(expected) if object_version != expected => {
                bail!(
                    concat!(
                        "Object {} resolved to version {}, expected exact version ",
                        "{}",
                    ),
                    request.object_id,
                    object_version,
                    expected,
                );
            }
            CheckpointObjectSelector::ExactVersion(_) | CheckpointObjectSelector::Latest => {}
        }

        Ok(Some(object))
    }

    /// Translate only the selector; the enclosing query supplies the checkpoint bound.
    fn graphql_key(request: &CheckpointObjectRequest) -> GraphQLObjectKey {
        let version = match request.selector {
            CheckpointObjectSelector::ExactVersion(version) => Some(version),
            CheckpointObjectSelector::Latest => None,
        };
        GraphQLObjectKey {
            address: SuiAddress(request.object_id.to_string()),
            version,
            root_version: None,
            at_checkpoint: None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::EpochData;
        use sui_types::{
            base_types::{ObjectID, SequenceNumber, SuiAddress as RuntimeAddress},
            object::Owner,
        };

        fn context() -> CheckpointExecutionContext {
            CheckpointExecutionContext {
                chain_identifier: ChainIdentifier::random(),
                checkpoint: 7,
                checkpoint_digest: CheckpointDigest::random(),
                epoch: EpochData {
                    epoch_id: 3,
                    protocol_version: 42,
                    rgp: 1_000,
                    start_timestamp: 123,
                },
            }
        }

        fn move_object(id: ObjectID, version: u64) -> Object {
            Object::with_id_owner_version_for_testing(
                id,
                SequenceNumber::from_u64(version),
                Owner::AddressOwner(RuntimeAddress::random_for_testing_only()),
            )
        }

        fn graphql_object(object: Object) -> ObjectFragment {
            ObjectFragment {
                object_bcs: Some(Base64(
                    CryptoBase64::from_bytes(&bcs::to_bytes(&object).unwrap()).encoded(),
                )),
            }
        }

        fn response(context: &CheckpointExecutionContext, objects: Vec<Option<Object>>) -> Query {
            response_with_range(context, objects, Some(0), Some(context.checkpoint))
        }

        fn response_with_range(
            context: &CheckpointExecutionContext,
            objects: Vec<Option<Object>>,
            first: Option<u64>,
            last: Option<u64>,
        ) -> Query {
            Query {
                chain_identifier: CheckpointDigest::new(*context.chain_identifier.as_bytes())
                    .to_string(),
                service_config: ServiceConfig {
                    available_range: AvailableRange {
                        first: first.map(|sequence_number| RangeCheckpoint { sequence_number }),
                        last: last.map(|sequence_number| RangeCheckpoint { sequence_number }),
                    },
                },
                checkpoint: Some(GraphQLCheckpoint {
                    sequence_number: context.checkpoint,
                    digest: Some(context.checkpoint_digest.to_string()),
                    query: Some(ScopedQuery {
                        multi_get_objects: objects
                            .into_iter()
                            .map(|object| object.map(graphql_object))
                            .collect(),
                    }),
                }),
            }
        }

        fn request(
            object_id: ObjectID,
            selector: CheckpointObjectSelector,
        ) -> CheckpointObjectRequest {
            CheckpointObjectRequest {
                object_id,
                selector,
            }
        }

        #[test]
        fn selectors_map_to_checkpoint_scoped_object_keys() {
            let id = ObjectID::random();
            let exact = graphql_key(&request(id, CheckpointObjectSelector::ExactVersion(3)));
            let latest = graphql_key(&request(id, CheckpointObjectSelector::Latest));

            assert_eq!(exact.version, Some(3));
            assert_eq!(exact.root_version, None);
            assert_eq!(latest.version, None);
            assert_eq!(latest.root_version, None);
            assert!(
                [&exact, &latest]
                    .into_iter()
                    .all(|key| key.at_checkpoint.is_none())
            );
        }

        #[test]
        fn generated_query_nests_object_reads_under_checkpoint_scope() {
            let context = context();
            let operation = Query::build(CheckpointObjectArgs {
                sequence_number: Some(context.checkpoint),
                keys: vec![graphql_key(&request(
                    ObjectID::random(),
                    CheckpointObjectSelector::Latest,
                ))],
                range_type: "Query".to_string(),
                range_field: Some("object".to_string()),
            });

            let range = operation.query.find("availableRange").unwrap();
            let checkpoint = operation.query.find("checkpoint(").unwrap();
            let scoped_query = operation.query[checkpoint..].find("query {").unwrap();
            let objects = operation.query[checkpoint + scoped_query..]
                .find("multiGetObjects")
                .unwrap();

            // This guards against incidental changes in how query is defined
            // where `service_config and `checkpoint` are at the same nesting level.
            assert!(range < checkpoint);
            assert!(objects > 0);
            assert_eq!(
                operation.variables.sequence_number,
                Some(context.checkpoint)
            );
            assert_eq!(operation.variables.range_type, "Query");
            assert_eq!(operation.variables.range_field.as_deref(), Some("object"));
            assert!(!operation.query.contains("filters:"));
        }

        #[test]
        fn decodes_selectors_and_preserves_result_order() {
            let context = context();
            let exact = move_object(ObjectID::random(), 3);
            let latest = move_object(ObjectID::random(), 4);
            let requests = vec![
                request(exact.id(), CheckpointObjectSelector::ExactVersion(3)),
                request(latest.id(), CheckpointObjectSelector::Latest),
            ];

            let results = decode_response(
                response_with_range(
                    &context,
                    vec![Some(exact.clone()), Some(latest.clone())],
                    Some(context.checkpoint),
                    Some(context.checkpoint),
                ),
                &context,
                &requests,
            )
            .unwrap();

            assert_eq!(
                results
                    .iter()
                    .map(|object| object.as_ref().map(|object| object.id()))
                    .collect::<Vec<_>>(),
                vec![Some(exact.id()), Some(latest.id())]
            );
        }

        #[test]
        fn decodes_latest_null_with_complete_history_as_absence() {
            let context = context();
            let object_id = ObjectID::random();
            let request = request(object_id, CheckpointObjectSelector::Latest);

            assert_eq!(
                decode_response(response(&context, vec![None]), &context, &[request]).unwrap(),
                vec![None],
            );
        }

        #[test]
        fn rejects_null_without_complete_history_or_for_exact_version() {
            let context = context();
            let object_id = ObjectID::random();
            let latest = request(object_id, CheckpointObjectSelector::Latest);
            let exact = request(object_id, CheckpointObjectSelector::ExactVersion(3));

            let incomplete = decode_response(
                response_with_range(&context, vec![None], Some(1), Some(context.checkpoint)),
                &context,
                &[latest],
            )
            .unwrap_err()
            .to_string();
            assert!(incomplete.contains(&object_id.to_string()));
            assert!(incomplete.contains("starts at checkpoint 1"));

            let exact_error = decode_response(response(&context, vec![None]), &context, &[exact])
                .unwrap_err()
                .to_string();
            assert!(exact_error.contains(&object_id.to_string()));
            assert!(exact_error.contains("may be newer than the checkpoint"));
        }

        #[test]
        fn rejects_missing_invalid_or_out_of_bounds_availability_range() {
            let context = context();
            let object = move_object(ObjectID::random(), 3);
            let object_request = request(object.id(), CheckpointObjectSelector::Latest);

            for response in [
                response_with_range(
                    &context,
                    vec![Some(object.clone())],
                    None,
                    Some(context.checkpoint),
                ),
                response_with_range(&context, vec![Some(object.clone())], Some(0), None),
                response_with_range(&context, vec![Some(object.clone())], Some(8), Some(7)),
                response_with_range(&context, vec![Some(object.clone())], Some(8), Some(9)),
                response_with_range(&context, vec![Some(object.clone())], Some(0), Some(6)),
            ] {
                assert!(decode_response(response, &context, &[object_request]).is_err());
            }
        }

        #[test]
        fn rejects_a_response_for_another_checkpoint_context() {
            let context = context();
            let request = request(ObjectID::random(), CheckpointObjectSelector::Latest);

            let mut wrong_chain = response(&context, vec![None]);
            wrong_chain.chain_identifier = CheckpointDigest::random().to_string();
            assert!(decode_response(wrong_chain, &context, &[request]).is_err());

            let mut wrong_sequence = response(&context, vec![None]);
            wrong_sequence.checkpoint.as_mut().unwrap().sequence_number += 1;
            assert!(decode_response(wrong_sequence, &context, &[request]).is_err());

            let mut wrong_digest = response(&context, vec![None]);
            wrong_digest.checkpoint.as_mut().unwrap().digest =
                Some(CheckpointDigest::random().to_string());
            assert!(decode_response(wrong_digest, &context, &[request]).is_err());
        }

        #[test]
        fn rejects_misaligned_or_wrong_object_results() {
            let context = context();
            let object = move_object(ObjectID::random(), 3);
            let object_request = request(object.id(), CheckpointObjectSelector::Latest);

            let too_few = response(&context, vec![]);
            assert!(decode_response(too_few, &context, &[object_request]).is_err());

            let wrong_id = response(&context, vec![Some(move_object(ObjectID::random(), 3))]);
            assert!(decode_response(wrong_id, &context, &[object_request]).is_err());

            let mut missing_bcs = response(&context, vec![Some(object)]);
            missing_bcs
                .checkpoint
                .as_mut()
                .unwrap()
                .query
                .as_mut()
                .unwrap()
                .multi_get_objects[0]
                .as_mut()
                .unwrap()
                .object_bcs = None;
            assert!(decode_response(missing_bcs, &context, &[object_request]).is_err());
        }

        #[test]
        fn enforces_exact_selector() {
            let context = context();
            let object = move_object(ObjectID::random(), 4);

            let exact = request(object.id(), CheckpointObjectSelector::ExactVersion(3));
            assert!(
                decode_response(
                    response(&context, vec![Some(object.clone())]),
                    &context,
                    &[exact]
                )
                .is_err()
            );
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
