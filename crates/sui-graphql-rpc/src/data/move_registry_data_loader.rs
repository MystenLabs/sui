// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::fmt::Write;

use std::{collections::HashMap, str::FromStr, sync::Arc};

use async_graphql::dataloader::{DataLoader, Loader};
use serde::{Deserialize, Serialize};

use crate::metrics::Metrics;
use crate::{
    config::MoveRegistryConfig,
    error::Error,
    types::{
        base64::Base64,
        move_registry::{
            error::MoveRegistryError,
            on_chain::{AppRecord, Name},
        },
    },
};

/// GraphQL fragment to query the values of the dynamic fields.
const QUERY_FRAGMENT: &str =
    "fragment RECORD_VALUES on DynamicField { value { ... on MoveValue { bcs } } }";

pub(crate) struct ExternalNamesLoader {
    client: reqwest::Client,
    config: MoveRegistryConfig,
    metrics: Metrics,
}

/// Helper types for accessing a shared `DataLoader` instance.
#[derive(Clone)]
pub(crate) struct MoveRegistryDataLoader(pub Arc<DataLoader<ExternalNamesLoader>>);

impl ExternalNamesLoader {
    pub(crate) fn new(config: MoveRegistryConfig, metrics: Metrics) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
            metrics,
        }
    }

    /// Constructs the GraphQL Query to query the names on an external graphql endpoint.
    fn construct_names_graphql_query(&self, names: &[Name]) -> (String, HashMap<Name, usize>) {
        let mut mapping: HashMap<Name, usize> = HashMap::new();

        let mut result = format!(r#"{{ owner(address: "{}") {{"#, self.config.registry_id);

        // we create the GraphQL query keys with a `fetch_{id}` prefix, which is accepted on graphql fields.
        for (index, name) in names.iter().enumerate() {
            let bcs_base64 = name.to_base64_string();

            // retain the mapping here (id to bcs representation, so we can pick the right response later on)
            mapping.insert(name.clone(), index);

            // SAFETY: write! to String always succeeds
            write!(
                &mut result,
                r#"{}: dynamicField(name: {{ type: "{}::name::Name", bcs: {} }}) {{ ...RECORD_VALUES }} "#,
                fetch_key(&index),
                self.config.package_address,
                bcs_base64
            ).unwrap();
        }

        result.push_str("}} ");
        result.push_str(QUERY_FRAGMENT);

        (result, mapping)
    }
}

impl MoveRegistryDataLoader {
    pub(crate) fn new(config: MoveRegistryConfig, metrics: Metrics) -> Self {
        let batch_size = config.page_limit as usize;
        let data_loader = DataLoader::new(ExternalNamesLoader::new(config, metrics), tokio::spawn)
            .max_batch_size(batch_size);
        Self(Arc::new(data_loader))
    }
}

#[async_trait::async_trait]
impl Loader<Name> for ExternalNamesLoader {
    type Value = AppRecord;
    type Error = Error;

    /// This function queries the external API to fetch the app records for the requested names.
    /// This is part of the data loader, so all queries are bulked-up to the maximum of {config.page_limit}.
    /// We handle the cases where individual queries fail, to ensure that a failed query cannot affect
    /// a successful one.
    async fn load(&self, keys: &[Name]) -> Result<HashMap<Name, AppRecord>, Error> {
        let Some(api_url) = self.config.external_api_url.as_ref() else {
            return Err(Error::MoveNameRegistry(
                MoveRegistryError::ExternalApiUrlUnavailable,
            ));
        };

        let (query, mapping) = self.construct_names_graphql_query(keys);

        let request_body = GraphQLRequest {
            query,
            variables: serde_json::Value::Null,
        };

        let res = {
            let _timer_guard = self
                .metrics
                .app_metrics
                .external_mvr_resolution_latency
                .start_timer();

            self.client
                .post(api_url)
                .json(&request_body)
                .send()
                .await
                .map_err(|e| {
                    Error::MoveNameRegistry(MoveRegistryError::FailedToQueryExternalApi(
                        e.to_string(),
                    ))
                })?
        };

        if !res.status().is_success() {
            return Err(Error::MoveNameRegistry(
                MoveRegistryError::FailedToQueryExternalApi(format!(
                    "Status code: {}",
                    res.status()
                )),
            ));
        }

        let response_json: GraphQLResponse<Owner> = res.json().await.map_err(|e| {
            Error::MoveNameRegistry(MoveRegistryError::FailedToParseExternalResponse(
                e.to_string(),
            ))
        })?;

        let names = response_json.data.owner.names;

        let results = HashMap::from_iter(mapping.into_iter().filter_map(|(k, idx)| {
            let bcs = names.get(&fetch_key(&idx))?.as_ref()?;
            let Base64(bytes) = Base64::from_str(&bcs.value.bcs).ok()?;
            let app_record: AppRecord = bcs::from_bytes(&bytes).ok()?;
            Some((k, app_record))
        }));

        Ok(results)
    }
}

fn fetch_key(idx: &usize) -> String {
    format!("f_{}", idx)
}

// GraphQL Request and Response types to deserialize for the data loader.
#[derive(Serialize)]
struct GraphQLRequest {
    query: String,
    variables: serde_json::Value,
}

#[derive(Deserialize, Debug)]
struct GraphQLResponse<T> {
    data: T,
}
#[derive(Deserialize, Debug)]
struct Owner {
    owner: Names,
}

#[derive(Deserialize, Debug)]
struct Names {
    #[serde(flatten)]
    names: HashMap<String, Option<OwnerValue>>,
}

#[derive(Deserialize, Debug)]
struct OwnerValue {
    value: NameBCS,
}

#[derive(Deserialize, Debug)]
struct NameBCS {
    bcs: String,
}
