// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ClientError;
use reqwest::header;
use reqwest::header::HeaderValue;
use reqwest::Response;
use serde_json::Value;
use std::collections::BTreeMap;
use sui_graphql_rpc_headers::LIMITS_HEADER;

use super::response::GraphqlResponse;

#[derive(Clone, Debug)]
pub struct GraphqlQueryVariable {
    pub name: String,
    pub ty: String,
    pub value: Value,
}

#[derive(Clone)]
pub struct SimpleClient {
    inner: reqwest::Client,
    url: String,
}

impl SimpleClient {
    pub fn new<S: Into<String>>(base_url: S) -> Self {
        Self {
            inner: reqwest::Client::new(),
            url: base_url.into(),
        }
    }

    pub async fn execute(
        &self,
        query: String,
        headers: Vec<(header::HeaderName, header::HeaderValue)>,
    ) -> Result<serde_json::Value, ClientError> {
        self.execute_impl(query, vec![], headers, false)
            .await?
            .json()
            .await
            .map_err(|e| e.into())
    }

    pub async fn execute_to_graphql(
        &self,
        query: String,
        get_usage: bool,
        variables: Vec<GraphqlQueryVariable>,
        mut headers: Vec<(header::HeaderName, header::HeaderValue)>,
    ) -> Result<GraphqlResponse, ClientError> {
        if get_usage {
            headers.push((
                LIMITS_HEADER.clone().as_str().try_into().unwrap(),
                HeaderValue::from_static("true"),
            ));
        }
        GraphqlResponse::from_resp(self.execute_impl(query, variables, headers, false).await?).await
    }

    async fn execute_impl(
        &self,
        query: String,
        variables: Vec<GraphqlQueryVariable>,
        headers: Vec<(header::HeaderName, header::HeaderValue)>,
        is_mutation: bool,
    ) -> Result<Response, ClientError> {
        let (type_defs, var_vals) = resolve_variables(&variables)?;
        let body = if type_defs.is_empty() {
            serde_json::json!({
                "query": query,
            })
        } else {
            // Make type defs which is a csv is the form of $var_name: $var_type
            let type_defs_csv = type_defs
                .iter()
                .map(|(name, ty)| format!("${}: {}", name, ty))
                .collect::<Vec<_>>()
                .join(", ");
            let query = format!(
                "{} ({}) {}",
                if is_mutation { "mutation" } else { "query" },
                type_defs_csv,
                query
            );
            serde_json::json!({
                "query": query,
                "variables": var_vals,
            })
        };

        let mut builder = self.inner.post(&self.url).json(&body);
        for (key, value) in headers {
            builder = builder.header(key, value);
        }
        builder.send().await.map_err(|e| e.into())
    }

    pub async fn execute_mutation_to_graphql(
        &self,
        mutation: String,
        variables: Vec<GraphqlQueryVariable>,
    ) -> Result<GraphqlResponse, ClientError> {
        GraphqlResponse::from_resp(self.execute_impl(mutation, variables, vec![], true).await?)
            .await
    }

    /// Send a request to the GraphQL server to check if it is alive.
    pub async fn ping(&self) -> Result<(), ClientError> {
        self.inner
            .get(format!("{}/health", self.url))
            .send()
            .await?;
        Ok(())
    }

    pub fn url(&self) -> String {
        self.url.clone()
    }
}

#[allow(clippy::type_complexity)]
pub fn resolve_variables(
    vars: &[GraphqlQueryVariable],
) -> Result<(BTreeMap<String, String>, BTreeMap<String, Value>), ClientError> {
    let mut type_defs: BTreeMap<String, String> = BTreeMap::new();
    let mut var_vals: BTreeMap<String, Value> = BTreeMap::new();

    for (idx, GraphqlQueryVariable { name, ty, value }) in vars.iter().enumerate() {
        if !is_valid_variable_name(name) {
            return Err(ClientError::InvalidVariableName {
                var_name: name.to_owned(),
            });
        }
        if name.trim().is_empty() {
            return Err(ClientError::InvalidEmptyItem {
                item_type: "Variable name".to_owned(),
                idx,
            });
        }
        if ty.trim().is_empty() {
            return Err(ClientError::InvalidEmptyItem {
                item_type: "Variable type".to_owned(),
                idx,
            });
        }
        if let Some(var_type_prev) = type_defs.get(name) {
            if var_type_prev != ty {
                return Err(ClientError::VariableDefinitionConflict {
                    var_name: name.to_owned(),
                    var_type_prev: var_type_prev.to_owned(),
                    var_type_curr: ty.to_owned(),
                });
            }
            if var_vals[name] != *value {
                return Err(ClientError::VariableValueConflict {
                    var_name: name.to_owned(),
                    var_val_prev: var_vals[name].clone(),
                    var_val_curr: value.clone(),
                });
            }
        }
        type_defs.insert(name.to_owned(), ty.to_owned());
        var_vals.insert(name.to_owned(), value.to_owned());
    }

    Ok((type_defs, var_vals))
}

pub fn is_valid_variable_name(s: &str) -> bool {
    let mut cs = s.chars();
    let Some(fst) = cs.next() else { return false };

    match fst {
        '_' => if s.len() > 1 {},
        'a'..='z' | 'A'..='Z' => {}
        _ => return false,
    }

    cs.all(|c| matches!(c, '_' | 'a' ..= 'z' | 'A' ..= 'Z' | '0' ..= '9'))
}
