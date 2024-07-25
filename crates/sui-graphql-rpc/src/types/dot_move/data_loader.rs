// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, str::FromStr, sync::Arc};

use async_graphql::dataloader::{DataLoader, Loader};
use serde::{Deserialize, Serialize};

use crate::{error::Error, types::base64::Base64};

use super::config::{AppRecord, DotMoveConfig, DotMoveServiceError, Name};

/// GraphQL fragment to query the values of the dynamic fields.
const QUERY_FRAGMENT: &str =
    "fragment RECORD_VALUES on DynamicField { value { ... on MoveValue { bcs } } }";

pub(crate) struct MainnetNamesLoader {
    client: reqwest::Client,
    config: DotMoveConfig,
}
/// Helper types for accessing a shared `DataLoader` instance.
#[derive(Clone)]
pub(crate) struct DotMoveDataLoader(pub Arc<DataLoader<MainnetNamesLoader>>);

impl MainnetNamesLoader {
    pub(crate) fn new(config: &DotMoveConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config: config.clone(),
        }
    }

    /// Constructs the GraphQL Query to query the names on a mainnet graphql endpoint.
    pub(crate) fn construct_names_graphql_query(
        &self,
        names: &[Name],
        mapping: &mut HashMap<Name, usize>,
    ) -> String {
        let mut result = format!(r#"{{ owner(address: "{}") {{"#, self.config.registry_id);

        // we create the GraphQL query keys with a `fetch_{id}` prefix, which is accepted on graphql fields.
        for (index, name) in names.iter().enumerate() {
            let bcs_base64 = name.to_base64_string();

            // retain the mapping here (id to bcs representation, so we can pick the right response later on)
            mapping.insert(name.clone(), index);

            let field_str = format!(
                r#"{}: dynamicField(name: {{ type: "{}::name::Name", bcs: {} }}) {{ ...RECORD_VALUES }}"#,
                fetch_key(&index),
                self.config.package_address,
                bcs_base64
            );

            result.push_str(&field_str);
        }

        result.push_str("}} ");
        result.push_str(QUERY_FRAGMENT);

        result
    }
}

impl DotMoveDataLoader {
    pub(crate) fn new(config: &DotMoveConfig) -> Self {
        let data_loader = DataLoader::new(MainnetNamesLoader::new(config), tokio::spawn)
            .max_batch_size(config.page_limit as usize);
        Self(Arc::new(data_loader))
    }
}

#[async_trait::async_trait]
impl Loader<Name> for MainnetNamesLoader {
    type Value = AppRecord;
    type Error = Error;

    /// This function queries the mainnet API to fetch the app records for the requested names.
    /// This is part of the data loader, so all queries are bulked-up to the maximum of {config.page_limit}.
    /// We handle the cases where individual queries fail, to ensure that a failed query cannot affect
    /// a successful one.
    async fn load(&self, keys: &[Name]) -> Result<HashMap<Name, AppRecord>, Error> {
        let Some(mainnet_api_url) = self.config.mainnet_api_url.as_ref() else {
            return Err(Error::DotMove(
                DotMoveServiceError::MainnetApiUrlUnavailable,
            ));
        };

        let mut results: HashMap<Name, AppRecord> = HashMap::new();
        let mut mapping: HashMap<Name, usize> = HashMap::new();

        let request_body = GraphQLRequest {
            query: self.construct_names_graphql_query(keys, &mut mapping),
            variables: serde_json::Value::Null,
        };

        let res = self
            .client
            .post(mainnet_api_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                Error::DotMove(DotMoveServiceError::FailedToQueryMainnetApi(e.to_string()))
            })?;

        if !res.status().is_success() {
            return Err(Error::DotMove(
                DotMoveServiceError::FailedToQueryMainnetApi(format!(
                    "Status code: {}",
                    res.status()
                )),
            ));
        }

        let response_json: GraphQLResponse<Owner> = res.json().await.map_err(|e| {
            Error::DotMove(DotMoveServiceError::FailedToParseMainnetResponse(
                e.to_string(),
            ))
        })?;

        let names = response_json.data.owner.names;

        for k in mapping.keys() {
            // Safe unwrap: we inserted the keys in the mapping before.
            let idx = mapping.get(k).unwrap();

            let Some(Some(bcs)) = names.get(&fetch_key(idx)) else {
                continue;
            };

            let Some(bytes) = Base64::from_str(&bcs.value.bcs).ok() else {
                continue;
            };

            let Some(app_record) = bcs::from_bytes::<AppRecord>(&bytes.0).ok() else {
                continue;
            };

            // only insert the record if it is a valid `app_record`
            results.insert(k.clone(), app_record);
        }

        Ok(results)
    }
}

impl Default for MainnetNamesLoader {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
            config: DotMoveConfig::default(),
        }
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
