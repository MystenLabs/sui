// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::Error as DeError;
use serde::ser::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;
use std::str::FromStr;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumProperty};
use thiserror::Error;
use tokio::task::JoinError;

use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::types::digests::TransactionDigest;

#[derive(Debug, Error, EnumDiscriminants, EnumProperty)]
#[strum_discriminants(
    name(ErrorType),
    derive(Display, EnumIter),
    strum(serialize_all = "kebab-case")
)]
#[allow(clippy::enum_variant_names)]
pub enum Error {
    #[error(transparent)]
    UncategorizedError(#[from] anyhow::Error),

    #[error(transparent)]
    SuiError(#[from] sui_sdk::error::Error),

    #[error(transparent)]
    TokioJoinError(#[from] JoinError),

    #[error(transparent)]
    BCSError(#[from] bcs::Error),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let error = json!(self);
        error.serialize(serializer)
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProcessCommandsResponse {
    pub tx_hash: TransactionDigest,
}

impl IntoResponse for ProcessCommandsResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AxelarMessage {
    chain_id: u64,
    command_ids: Vec<SuiAddress>,
    commands: Vec<String>,
    #[serde(serialize_with = "to_bcs_bytes_vec")]
    params: Vec<AxelarParameter>,
}

fn to_bcs_bytes_vec<S>(params: &[AxelarParameter], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let params = params
        .iter()
        .map(bcs::to_bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(S::Error::custom)?;
    params.serialize(serializer)
}

fn to_bcs_bytes<S, T>(data: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    bcs::to_bytes(data)
        .map_err(S::Error::custom)?
        .serialize(serializer)
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged, rename_all = "camelCase")]
pub enum AxelarParameter {
    GenericMessage {
        source_chain: String,
        source_address: String,
        target_id: SuiAddress,
        payload_hash: Vec<u8>,
    },
    TransferOperatorshipMessage {
        operators: Vec<Vec<u8>>,
        #[serde(deserialize_with = "string_to_u128_vec")]
        weights: Vec<u128>,
        #[serde(deserialize_with = "string_to_u128")]
        threshold: u128,
    },
}

fn string_to_u128_vec<'de, D>(deserializer: D) -> Result<Vec<u128>, D::Error>
where
    D: Deserializer<'de>,
{
    let input = Vec::<String>::deserialize(deserializer)?;
    input
        .iter()
        .map(|s| u128::from_str(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(D::Error::custom)
}

fn string_to_u128<'de, D>(deserializer: D) -> Result<u128, D::Error>
where
    D: Deserializer<'de>,
{
    let input = String::deserialize(deserializer)?;
    u128::from_str(&input).map_err(D::Error::custom)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Proof {
    operators: Vec<Vec<u8>>,
    #[serde(deserialize_with = "string_to_u128_vec")]
    weights: Vec<u128>,
    #[serde(deserialize_with = "string_to_u128")]
    threshold: u128,
    signatures: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Input {
    #[serde(serialize_with = "to_bcs_bytes")]
    pub message: AxelarMessage,
    #[serde(serialize_with = "to_bcs_bytes")]
    pub proof: Proof,
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use sui_sdk::types::base_types::SuiAddress;

    use crate::types::AxelarParameter::TransferOperatorshipMessage;
    use crate::types::{AxelarMessage, Input, Proof};

    #[test]
    fn test_message_to_bcs() {
        // From preset/index.js
        let input = hex::decode("8501010000000000000001000000000000000000000000000000000000000000000000000000000000000101147472616e736665724f70657261746f727368697001440121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902801c80000000000000000000000000000001400000000000000000000000000000087010121037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff59902801640000000000000000000000000000000a000000000000000000000000000000014198b04944e2009969c93226ec6c97a7b9cc655b4ac52f7eeefd6cf107981c063a56a419cb149ea8a9cd49e8c745c655c5ccc242d35a9bebe7cebf6751121092a301").unwrap();
        let operator =
            hex::decode("037286a4f1177bea06c8e15cf6ec3df0b7747a01ac2329ca2999dfd74eff599028")
                .unwrap();

        let message = AxelarMessage {
            chain_id: 1,
            command_ids: vec![SuiAddress::from_str(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            )
            .unwrap()],
            commands: vec!["transferOperatorship".into()],
            params: vec![TransferOperatorshipMessage {
                operators: vec![operator.to_vec()],
                weights: vec![200],
                threshold: 20,
            }],
        };
        let proof = Proof {
            operators: vec![operator.to_vec()],
            weights: vec![100],
            threshold: 10,
            signatures: vec![hex::decode("98b04944e2009969c93226ec6c97a7b9cc655b4ac52f7eeefd6cf107981c063a56a419cb149ea8a9cd49e8c745c655c5ccc242d35a9bebe7cebf6751121092a301").unwrap()],
        };
        let payload = bcs::to_bytes(&Input { message, proof }).unwrap();

        assert_eq!(input, payload)
    }
}
