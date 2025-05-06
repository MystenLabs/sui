// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{data_store::DataStore, errors::ReplayError, replay_interface::EpochData};
use cynic::QueryBuilder;

// GQL Queries

mod schema {
    cynic::use_schema!("../sui-indexer-alt-graphql/schema.graphql");
    cynic::impl_scalar!(u64, UInt53);

    use chrono::{DateTime as ChronoDateTime, Utc};
    cynic::impl_scalar!(ChronoDateTime<Utc>, DateTime);
}

pub mod epoch_data {
    use super::*;
    use chrono::{DateTime as ChronoDateTime, Utc};

    #[derive(cynic::QueryVariables)]
    pub struct EpochDataArgs {
        pub epoch: Option<u64>,
    }

    #[derive(cynic::QueryFragment)]
    #[cynic(variables = "EpochDataArgs")]
    #[cynic(schema_path = "../sui-indexer-alt-graphql/schema.graphql")]
    pub struct Query {
        #[arguments(epochId: $epoch)]
        epoch: Option<Epoch>,
    }

    #[derive(cynic::Scalar, Clone)]
    #[cynic(graphql_type = "BigInt")]
    pub struct BigInt(u64);

    #[derive(cynic::QueryFragment)]
    #[cynic(schema_path = "../sui-indexer-alt-graphql/schema.graphql")]
    pub struct Epoch {
        epoch_id: u64,
        protocol_configs: Option<ProtocolConfigs>,
        reference_gas_price: Option<epoch_data::BigInt>,
        start_timestamp: Option<ChronoDateTime<Utc>>,
    }

    #[derive(cynic::QueryFragment)]
    #[cynic(schema_path = "../sui-indexer-alt-graphql/schema.graphql")]
    pub struct ProtocolConfigs {
        protocol_version: u64,
    }

    pub async fn query(epoch_id: u64, data_store: &DataStore) -> Result<EpochData, ReplayError> {
        println!("Querying epoch data for epoch {}", epoch_id);
        let query = Query::build(EpochDataArgs {
            epoch: Some(epoch_id),
        });
        println!("Querying epoch data query built");
        let response = data_store
            .run_query(&query)
            .await
            .map_err(|e| ReplayError::RPCError { err: e.to_string() })?;
        println!("Querying epoch data response received");

        let epoch = response
            .data
            .and_then(|epoch| epoch.epoch)
            .ok_or_else(|| ReplayError::MissingEpochData { epoch: epoch_id })?;
        Ok(EpochData {
            epoch_id: epoch.epoch_id,
            protocol_version: epoch
                .protocol_configs
                .map(|config| config.protocol_version)
                .ok_or_else(|| ReplayError::MissingEpochData { epoch: epoch_id })?,
            rgp: epoch
                .reference_gas_price
                .map(|rgp| rgp.0)
                .ok_or_else(|| ReplayError::MissingEpochData { epoch: epoch_id })?,
            start_timestamp: epoch
                .start_timestamp
                .map(|ts| ts.timestamp_millis() as u64)
                .ok_or_else(|| ReplayError::MissingEpochData { epoch: epoch_id })?,
        })
        
    }
}

// impl_scalar!(u64, schema::UInt53);

// #[derive(cynic::Scalar, Debug, Clone)]
// #[cynic(graphql_type = "BigInt")]
// pub struct BigInt(pub String);

// #[derive(cynic::Scalar, Debug, Clone)]
// #[cynic(graphql_type = "DateTime")]
// pub struct DateTime(pub String);

// https://cynic-rs.dev/derives/scalars
