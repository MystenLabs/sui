// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! GQL Queries
//! Interface to the rpc for the gql schema defined in `crates\sui-indexer-alt-graphql/schema.graphql`.
//! Built in 3 modules: epoch_query, txn_query, object_query.
//! No GQL type escapes this module. From here we return structures defined by the replay tool
//! or bcs encoded data of runtime structures.
//!
//! This module is private to the `DataStore` and packcaged in its own module for convenience.

use crate::{replay_interface::EpochData, DataStore};
use anyhow::{anyhow, Context, Error};
use cynic::QueryBuilder;
use fastcrypto::encoding::{Base64 as CryptoBase64, Encoding};

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
        data_store: &DataStore,
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
    use crate::replay_interface;

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
    #[cynic(
        graphql_type = "Object",
        schema_module = "crate::data_stores::gql_queries::schema"
    )]
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
        keys: &[replay_interface::ObjectKey],
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

    impl From<replay_interface::ObjectKey> for ObjectKey {
        fn from(key: replay_interface::ObjectKey) -> Self {
            ObjectKey {
                address: SuiAddress(key.object_id.to_string()),
                version: match key.version_query {
                    replay_interface::VersionQuery::Version(v) => Some(v),
                    _ => None,
                },
                root_version: match key.version_query {
                    replay_interface::VersionQuery::RootVersion(v) => Some(v),
                    _ => None,
                },
                at_checkpoint: match key.version_query {
                    replay_interface::VersionQuery::AtCheckpoint(v) => Some(v),
                    _ => None,
                },
            }
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
        let Some(chain_id) = response.data.and_then(|data| data.chain_identifier) else {
            return Err(anyhow!("Missing chain identifier"));
        };
        Ok(chain_id)
    }
}
