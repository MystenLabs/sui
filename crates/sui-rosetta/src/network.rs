// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::Extension;

use crate::errors::Error;
use crate::types::{NetworkIdentifier, NetworkListResponse};
use crate::ApiState;

pub async fn list(
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<NetworkListResponse, Error> {
    Ok(NetworkListResponse {
        network_identifiers: state
            .get_envs()
            .into_iter()
            .map(|env| NetworkIdentifier {
                blockchain: "Sui".to_string(),
                network: env,
            })
            .collect(),
    })
}
