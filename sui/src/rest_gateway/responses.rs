// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use dropshot::{HttpError, CONTENT_TYPE_JSON};
use http::{Response, StatusCode};
use schemars::gen::SchemaGenerator;
use schemars::schema::Schema;
use schemars::{schema_for_value, JsonSchema};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use serde_with::base64::Base64;
use serde_with::serde_as;

use sui_types::base_types::bytes_as_hex;
use sui_types::base_types::bytes_from_hex;
use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber};
use sui_types::crypto::SignableBytes;
use sui_types::messages::TransactionData;

#[derive(Serialize, Deserialize)]
pub struct ObjectResponse {
    pub objects: Vec<NamedObjectRef>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedObjectRef {
    object_id: ObjectID,
    version: SequenceNumber,
    #[serde(serialize_with = "bytes_as_hex", deserialize_with = "bytes_from_hex")]
    digest: ObjectDigest,
}

impl NamedObjectRef {
    pub fn from((object_id, version, digest): ObjectRef) -> Self {
        Self {
            object_id,
            version,
            digest,
        }
    }

    pub fn to_object_ref(self) -> ObjectRef {
        (self.object_id, self.version, self.digest)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", transparent)]
pub struct JsonResponse<T>(pub T);

impl<T: DeserializeOwned + Serialize> JsonSchema for JsonResponse<T> {
    fn schema_name() -> String {
        serde_name::trace_name::<T>()
            .expect("Self must be a struct or an enum")
            .to_string()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schema_for_value!("").schema.into()
    }
}

pub fn custom_http_response<T: Serialize + JsonSchema>(
    status_code: StatusCode,
    response_body: T,
) -> Result<Response<T>, HttpError> {
    let res = Response::builder()
        .status(status_code)
        .header(http::header::CONTENT_TYPE, CONTENT_TYPE_JSON)
        .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(response_body)?;
    Ok(res)
}

pub fn custom_http_error(status_code: http::StatusCode, message: String) -> HttpError {
    HttpError::for_client_error(None, status_code, message)
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct TransactionBytes {
    #[serde_as(as = "Base64")]
    tx_bytes: Vec<u8>,
}

impl TransactionBytes {
    pub fn new(data: TransactionData) -> Self {
        Self {
            tx_bytes: data.to_bytes(),
        }
    }

    pub fn to_data(self) -> Result<TransactionData, anyhow::Error> {
        TransactionData::from_signable_bytes(self.tx_bytes)
    }
}

impl JsonSchema for TransactionBytes {
    fn schema_name() -> String {
        "transaction_bytes".to_string()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schema_for_value!(TransactionBytes { tx_bytes: vec![] })
            .schema
            .into()
    }
}

/**
Response containing the information of an object schema if found, otherwise an error
is returned.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectSchemaResponse {
    /** JSON representation of the object schema */
    pub schema: serde_json::Value,
}
