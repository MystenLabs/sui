// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! GQL Queries
//! Interface to the rpc for the gql schema defined in `crates/sui-indexer-alt-graphql/schema.graphql`.
//! Built in modules for epochs, transactions, objects, and checkpoints.
//! No GQL type escapes this module. From here we return structures defined in this crate
//! or bcs encoded data of runtime structures.

#![allow(unused)]

use anyhow::{Context, Error, anyhow};
use cynic::QueryBuilder;
use fastcrypto::encoding::{Base64 as CryptoBase64, Encoding};
use itertools::Itertools;

use crate::EpochData;
use crate::gql::client::GraphQLClient;

// Register the schema which was loaded in the build.rs call.
#[cynic::schema("rpc")]
mod schema {
    use chrono::{DateTime as ChronoDateTime, Utc};
    cynic::impl_scalar!(u64, UInt53);
    cynic::impl_scalar!(ChronoDateTime<Utc>, DateTime);
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
    pub(crate) struct BigInt(String);

    #[derive(cynic::QueryFragment)]
    pub(crate) struct Epoch {
        epoch_id: u64,
        protocol_configs: Option<ProtocolConfigs>,
        reference_gas_price: Option<BigInt>,
        start_timestamp: Option<ChronoDateTime<Utc>>,
    }

    #[derive(cynic::QueryFragment)]
    pub(crate) struct ProtocolConfigs {
        protocol_version: u64,
    }

    pub(crate) async fn query(
        epoch_id: u64,
        data_store: &GraphQLClient,
    ) -> Result<Option<EpochData>, Error> {
        let query = Query::build(EpochDataArgs {
            epoch: Some(epoch_id),
        });
        let response = data_store.run_query(&query).await?;

        let Some(epoch) = response.data.and_then(|epoch| epoch.epoch) else {
            return Ok(None);
        };
        Ok(Some(EpochData {
            epoch_id: epoch.epoch_id,
            protocol_version: epoch
                .protocol_configs
                .map(|config| config.protocol_version)
                .ok_or(anyhow!(format!(
                    "Canot find epoch info for epoch {}",
                    epoch_id
                )))?,
            rgp: epoch
                .reference_gas_price
                .map(|rgp| rgp.0)
                .ok_or(anyhow!(format!(
                    "Canot find epoch info for epoch {}",
                    epoch_id
                )))?
                .parse()
                .unwrap(),
            start_timestamp: epoch
                .start_timestamp
                .map(|ts| ts.timestamp_millis() as u64)
                .ok_or(anyhow!(format!(
                    "Canot find epoch info for epoch {}",
                    epoch_id
                )))?,
        }))
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
        client: &GraphQLClient,
    ) -> Result<Option<(TransactionData, sui_types::effects::TransactionEffects, u64)>, Error> {
        let query = Query::build(TransactionDataArgs {
            digest: digest.clone(),
        });
        let response = client
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
    #[cynic(graphql_type = "Object", schema_module = "crate::gql::queries::schema")]
    pub(crate) struct ObjectFragment {
        #[allow(dead_code)]
        pub address: SuiAddress,
        pub version: Option<u64>,
        pub object_bcs: Option<Base64>,
    }

    #[derive(cynic::QueryVariables)]
    pub(crate) struct VersionAtCheckpointVars {
        pub sequence_number: Option<u64>,
        pub address: SuiAddress,
        pub version: Option<u64>,
    }

    #[derive(cynic::QueryFragment)]
    #[cynic(
        variables = "VersionAtCheckpointVars",
        graphql_type = "Query",
        schema_module = "crate::gql::queries::schema"
    )]
    pub(crate) struct VersionAtCheckpointQuery {
        #[arguments(sequenceNumber: $sequence_number)]
        pub checkpoint: Option<Checkpoint>,
    }

    #[derive(cynic::QueryFragment)]
    #[cynic(
        variables = "VersionAtCheckpointVars",
        schema_module = "crate::gql::queries::schema"
    )]
    pub(crate) struct Checkpoint {
        pub query: Option<ScopedQuery>,
    }

    #[derive(cynic::QueryFragment)]
    #[cynic(
        variables = "VersionAtCheckpointVars",
        graphql_type = "Query",
        schema_module = "crate::gql::queries::schema"
    )]
    pub(crate) struct ScopedQuery {
        #[arguments(address: $address, version: $version)]
        pub object: Option<ObjectFragment>,
    }

    // Maximum number of keys to query in a single request.
    // REVIEW: not clear how this translate to the 5000B limit, so
    // we are picking a "random" and conservative number.
    const MAX_KEYS_SIZE: usize = 30;

    pub(crate) async fn query(
        keys: &[GqlObjectKey],
        data_store: &GraphQLClient,
    ) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let mut results: Vec<Option<Option<(Object, u64)>>> = vec![None; keys.len()];
        let mut standard_indices = Vec::with_capacity(keys.len());
        let mut standard_keys = Vec::with_capacity(keys.len());

        for (idx, key) in keys.iter().cloned().enumerate() {
            match key.version_query {
                VersionQuery::VersionAtCheckpoint {
                    version,
                    checkpoint,
                } => {
                    results[idx] = Some(
                        query_version_at_checkpoint(key.object_id, version, checkpoint, data_store)
                            .await?,
                    );
                }
                _ => {
                    standard_indices.push(idx);
                    standard_keys.push(ObjectKey::from(key));
                }
            }
        }

        let mut keys = standard_keys;
        let mut key_chunks = vec![];
        while !keys.is_empty() {
            let chunk: Vec<_> = keys.drain(..MAX_KEYS_SIZE.min(keys.len())).collect();
            key_chunks.push(chunk);
        }

        let mut standard_results = vec![];

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

            standard_results.extend(
                list.into_iter()
                    .map(decode_object_fragment)
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }

        for (idx, object) in standard_indices.into_iter().zip_eq(standard_results) {
            results[idx] = Some(object);
        }

        Ok(results
            .into_iter()
            .map(|result| result.expect("every query key should produce a result"))
            .collect())
    }

    async fn query_version_at_checkpoint(
        object_id: sui_types::base_types::ObjectID,
        version: u64,
        checkpoint: u64,
        data_store: &GraphQLClient,
    ) -> Result<Option<(Object, u64)>, Error> {
        let query = VersionAtCheckpointQuery::build(VersionAtCheckpointVars {
            sequence_number: Some(checkpoint),
            address: SuiAddress(object_id.to_string()),
            version: Some(version),
        });
        let response = data_store.run_query(&query).await?;
        let checkpoint = response
            .data
            .and_then(|data| data.checkpoint)
            .ok_or_else(|| anyhow!("Missing checkpoint in object query response"))?;
        let scoped_query = checkpoint
            .query
            .ok_or_else(|| anyhow!("Missing checkpoint-scoped query in object response"))?;

        decode_object_fragment(scoped_query.object)
    }

    fn decode_object_fragment(
        frag: Option<ObjectFragment>,
    ) -> Result<Option<(Object, u64)>, Error> {
        match frag {
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
                Ok(Some((obj, version)))
            }
            None => Ok(None),
        }
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

    #[cfg(test)]
    mod tests {
        use sui_types::base_types::ObjectID;

        use super::*;

        #[test]
        fn object_key_from_version_query_sets_only_selected_bound() {
            let object_id = ObjectID::random();

            let version_key = ObjectKey::from(GqlObjectKey {
                object_id,
                version_query: VersionQuery::Version(7),
            });
            assert_eq!(version_key.address.0, object_id.to_string());
            assert_eq!(version_key.version, Some(7));
            assert_eq!(version_key.root_version, None);
            assert_eq!(version_key.at_checkpoint, None);

            let root_version_key = ObjectKey::from(GqlObjectKey {
                object_id,
                version_query: VersionQuery::RootVersion(11),
            });
            assert_eq!(root_version_key.version, None);
            assert_eq!(root_version_key.root_version, Some(11));
            assert_eq!(root_version_key.at_checkpoint, None);

            let checkpoint_key = ObjectKey::from(GqlObjectKey {
                object_id,
                version_query: VersionQuery::AtCheckpoint(19),
            });
            assert_eq!(checkpoint_key.version, None);
            assert_eq!(checkpoint_key.root_version, None);
            assert_eq!(checkpoint_key.at_checkpoint, Some(19));
        }

        #[test]
        fn decode_object_fragment_returns_none_for_missing_object() {
            assert_eq!(decode_object_fragment(None).unwrap(), None);
        }
    }
}

pub(crate) mod checkpoint_query {
    use fastcrypto::traits::ToFromBytes;
    use roaring::RoaringBitmap;
    use sui_types::{
        crypto::{AggregateAuthoritySignature, AuthorityStrongQuorumSignInfo},
        messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSummary, VerifiedCheckpoint},
    };

    use super::*;

    #[derive(cynic::Scalar, Debug, Clone)]
    #[cynic(graphql_type = "Base64")]
    pub(crate) struct Base64(pub String);

    #[derive(cynic::QueryVariables)]
    pub(crate) struct CheckpointArgs {
        pub sequence_number: Option<u64>,
    }

    #[derive(cynic::QueryFragment)]
    #[cynic(variables = "CheckpointArgs", graphql_type = "Query")]
    pub(crate) struct Query {
        #[arguments(sequenceNumber: $sequence_number)]
        checkpoint: Option<Checkpoint>,
    }

    #[derive(cynic::QueryFragment)]
    pub(crate) struct Checkpoint {
        summary_bcs: Option<Base64>,
        validator_signatures: Option<ValidatorAggregatedSignature>,
    }

    #[derive(cynic::QueryFragment)]
    pub(crate) struct ValidatorAggregatedSignature {
        signature: Option<Base64>,
        signers_map: Vec<i32>,
    }

    pub(crate) async fn query(
        sequence_number: Option<u64>,
        data_store: &GraphQLClient,
    ) -> Result<Option<VerifiedCheckpoint>, Error> {
        let query = Query::build(CheckpointArgs { sequence_number });
        let response = data_store.run_query(&query).await?;
        let Some(checkpoint) = response.data.and_then(|data| data.checkpoint) else {
            return Ok(None);
        };
        Ok(Some(decode_checkpoint(checkpoint)?))
    }

    fn decode_checkpoint(checkpoint: Checkpoint) -> Result<VerifiedCheckpoint, Error> {
        let summary: CheckpointSummary = decode_bcs(checkpoint.summary_bcs, "checkpoint summary")?;
        let Some(validator_signatures) = checkpoint.validator_signatures else {
            return Err(anyhow!(
                "Missing validator signatures in checkpoint response"
            ));
        };

        let signature_bytes = CryptoBase64::decode(
            &validator_signatures
                .signature
                .ok_or_else(|| anyhow!("Missing aggregated checkpoint signature"))?
                .0,
        )
        .context("checkpoint signature does not decode")?;
        let signature = AggregateAuthoritySignature::from_bytes(&signature_bytes)
            .context("cannot deserialize aggregated checkpoint signature")?;
        let signers_map = validator_signatures
            .signers_map
            .into_iter()
            .map(|signer| {
                u32::try_from(signer)
                    .map_err(|_| anyhow!("negative signer index in checkpoint signature"))
            })
            .collect::<Result<RoaringBitmap, Error>>()?;

        let certified = CertifiedCheckpointSummary::new_from_data_and_sig(
            summary.clone(),
            AuthorityStrongQuorumSignInfo {
                epoch: summary.epoch,
                signature,
                signers_map,
            },
        );
        // TODO: should we fetch the committee and pass that into try_into_verified instead of
        // constructing this with new_unchecked?
        Ok(VerifiedCheckpoint::new_unchecked(certified))
    }

    fn decode_bcs<T>(field: Option<Base64>, label: &str) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned,
    {
        let bytes = CryptoBase64::decode(
            &field
                .ok_or_else(|| anyhow!("Missing {} in checkpoint response", label))?
                .0,
        )
        .with_context(|| format!("{} does not decode", label))?;
        bcs::from_bytes(&bytes).with_context(|| format!("cannot deserialize {}", label))
    }

    #[cfg(test)]
    mod tests {
        use std::convert::TryFrom;

        use fastcrypto::encoding::Base64 as FastCryptoBase64;
        use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

        use super::{Base64, Checkpoint, ValidatorAggregatedSignature, decode_checkpoint};

        #[test]
        fn decode_checkpoint_reconstructs_verified_checkpoint() {
            let checkpoint = TestCheckpointBuilder::new(7).build_checkpoint();
            let certified = checkpoint.summary;

            let decoded = decode_checkpoint(Checkpoint {
                summary_bcs: Some(Base64(
                    FastCryptoBase64::from_bytes(
                        &bcs::to_bytes(certified.data()).expect("summary should serialize"),
                    )
                    .encoded(),
                )),
                validator_signatures: Some(ValidatorAggregatedSignature {
                    signature: Some(Base64(
                        FastCryptoBase64::from_bytes(certified.auth_sig().signature.as_ref())
                            .encoded(),
                    )),
                    signers_map: certified
                        .auth_sig()
                        .signers_map
                        .iter()
                        .map(|index| i32::try_from(index).expect("test signers fit in i32"))
                        .collect(),
                }),
            })
            .expect("checkpoint should decode");

            assert_eq!(decoded.data(), certified.data());
            assert_eq!(decoded.auth_sig().epoch, certified.auth_sig().epoch);
            assert_eq!(
                decoded.auth_sig().signature.as_ref(),
                certified.auth_sig().signature.as_ref()
            );
            assert_eq!(
                decoded.auth_sig().signers_map,
                certified.auth_sig().signers_map
            );
        }

        #[test]
        fn decode_checkpoint_rejects_missing_validator_signatures() {
            let checkpoint = TestCheckpointBuilder::new(7).build_checkpoint();

            let error = decode_checkpoint(Checkpoint {
                summary_bcs: Some(Base64(
                    FastCryptoBase64::from_bytes(
                        &bcs::to_bytes(checkpoint.summary.data())
                            .expect("summary should serialize"),
                    )
                    .encoded(),
                )),
                validator_signatures: None,
            })
            .expect_err("missing validator signatures should fail");

            assert!(
                error
                    .to_string()
                    .contains("Missing validator signatures in checkpoint response")
            );
        }

        #[test]
        fn decode_checkpoint_rejects_negative_signer_indices() {
            let checkpoint = TestCheckpointBuilder::new(7).build_checkpoint();

            let error = decode_checkpoint(Checkpoint {
                summary_bcs: Some(Base64(
                    FastCryptoBase64::from_bytes(
                        &bcs::to_bytes(checkpoint.summary.data())
                            .expect("summary should serialize"),
                    )
                    .encoded(),
                )),
                validator_signatures: Some(ValidatorAggregatedSignature {
                    signature: Some(Base64(
                        FastCryptoBase64::from_bytes(
                            checkpoint.summary.auth_sig().signature.as_ref(),
                        )
                        .encoded(),
                    )),
                    signers_map: vec![-1],
                }),
            })
            .expect_err("negative signer index should fail");

            assert!(
                error
                    .to_string()
                    .contains("negative signer index in checkpoint signature")
            );
        }
    }
}

pub(crate) mod chain_id_query {
    use super::*;

    #[derive(cynic::QueryFragment)]
    pub(crate) struct Query {
        chain_identifier: Option<String>,
    }

    pub(crate) async fn query(data_store: &GraphQLClient) -> Result<String, Error> {
        let query = Query::build(());
        let response = data_store.run_query(&query).await?;
        let Some(chain_id) = response.data.and_then(|data| data.chain_identifier) else {
            return Err(anyhow!("Missing chain identifier"));
        };
        Ok(chain_id)
    }
}
