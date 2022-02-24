// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use dropshot::{
    endpoint, ApiDescription, HttpError, HttpResponseOk, HttpResponseUpdatedNoContent,
    PaginationParams, Query, RequestContext, ResultsPage, TypedBody,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sui::wallet_commands::WalletContext;
use tokio::sync::Mutex;

fn main() -> Result<(), String> {
    /*
     * Build a description of the API.
     */
    let mut api = ApiDescription::new();
    // [DEBUG][SUI]
    api.register(genesis).unwrap();
    api.register(sui_start).unwrap();
    api.register(sui_stop).unwrap();

    // [WALLET]
    api.register(get_addresses).unwrap();
    api.register(get_objects).unwrap();
    api.register(object_info).unwrap();
    api.register(transfer_object).unwrap();
    api.register(publish).unwrap();
    api.register(call).unwrap();
    api.register(sync).unwrap();

    api.openapi("Sui API", "0.1")
        .write(&mut std::io::stdout())
        .map_err(|e| e.to_string())?;

    Ok(())
}

/**
 * Server context (state shared by handler functions)
 */
#[allow(dead_code)]
struct ServerContext {
    // Used to manage addresses for client.
    wallet_context: Arc<Mutex<Option<WalletContext>>>,
}

#[allow(dead_code)]
impl ServerContext {
    pub fn new() -> ServerContext {
        ServerContext {
            wallet_context: Arc::new(Mutex::new(None)),
        }
    }
}

/**
* 'GenesisRequest' represents the server configuration.
*
* Example GenesisRequest
* ------------------------
{
    "num_authorities": 4,
    "num_accounts": 4,
    "num_objects": 2,
}
* ------------------------
* All attributes in GenesisRequest are optional, a default value will be use if
* the fields are not set. For example, the request shown above will create a
* network of 4 authorities, and pre-populate 2 objects for 4 accounts.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GenesisRequest {
    num_authorities: Option<u16>,
    num_accounts: Option<u16>,
    num_objects: Option<u16>,
}

/**
 * 'GenesisResponse' returns the resulting wallet & network config of the
 * provided genesis configuration.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GenesisResponse {
    wallet_config: serde_json::Value,
    network_config: serde_json::Value,
}

/**
 * [DEBUG][SUI] Use to provide network/wallet configurations for Sui genesis.
 */
#[allow(unused_variables)]
#[endpoint {
    method = POST,
    path = "/debug/sui/genesis",
    tags = [ "debug", "sui" ],
}]
async fn genesis(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<GenesisRequest>,
) -> Result<HttpResponseOk<GenesisResponse>, HttpError> {
    let genesis_response = GenesisResponse {
        wallet_config: json!(""),
        network_config: json!(""),
    };

    Ok(HttpResponseOk(genesis_response))
}

/**
 * [DEBUG][SUI] Start servers with specified configurations from genesis.
 */
#[allow(unused_variables)]
#[endpoint {
    method = POST,
    path = "/debug/sui/start",
    tags = [ "debug", "sui" ],
}]
async fn sui_start(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<()>, HttpError> {
    unimplemented!()
}

/**
 * [DEBUG][SUI] Stop sui network and delete generated configs & storage.
 */
#[allow(unused_variables)]
#[endpoint {
    method = POST,
    path = "/debug/sui/stop",
    tags = [ "debug", "sui" ],
}]
async fn sui_stop(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    unimplemented!()
}

/**
 * `GetAddressResponse` represents the list of managed accounts for this client.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetAddressResponse {
    addresses: Vec<String>,
}

/**
 * [WALLET] Retrieve all managed accounts.
 */
#[allow(unused_variables)]
#[endpoint {
    method = GET,
    path = "/wallet/addresses",
    tags = [ "wallet" ],
}]
async fn get_addresses(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<GetAddressResponse>, HttpError> {
    let get_address_response = GetAddressResponse {
        addresses: vec![String::new()],
    };

    Ok(HttpResponseOk(get_address_response))
}

/**
* 'GetObjectsScanParams' represents the objects scan parameters
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetObjectsScanParams {
    address: String,
}

/**
* 'GetObjectsPageSelector' represents the objects page selector
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetObjectsPageSelector {
    address: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Object {
    object_id: String,
    object_ref: serde_json::Value,
}

/**
 * [WALLET] Return all objects owned by the account address.
 */
#[allow(unused_variables)]
#[endpoint {
    method = GET,
    path = "/wallet/objects",
    tags = [ "wallet" ],
}]
async fn get_objects(
    rqctx: Arc<RequestContext<ServerContext>>,
    query: Query<PaginationParams<GetObjectsScanParams, GetObjectsPageSelector>>,
) -> Result<HttpResponseOk<ResultsPage<Object>>, HttpError> {
    let pag_params = query.into_inner();
    let limit = rqctx.page_limit(&pag_params)?.get();
    let tmp;
    let (objects, scan_params) = match &pag_params.page {
        dropshot::WhichPage::First(scan_params) => {
            let object = Object {
                object_id: String::new(),
                object_ref: json!(""),
            };
            (vec![object], scan_params)
        }
        dropshot::WhichPage::Next(page_selector) => {
            let object = Object {
                object_id: String::new(),
                object_ref: json!(""),
            };
            tmp = GetObjectsScanParams {
                address: page_selector.address.clone(),
            };
            (vec![object], &tmp)
        }
    };

    Ok(HttpResponseOk(ResultsPage::new(
        objects,
        scan_params,
        |last, scan_params| GetObjectsPageSelector {
            address: scan_params.address.clone(),
        },
    )?))
}

/**
* `GetObjectInfoRequest` represents the owner & object for which info is to be
* retrieved.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetObjectInfoRequest {
    owner: String,
    object_id: String,
}

/**
* 'ObjectInfoResponse' represents the object info on the network.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ObjectInfoResponse {
    owner: String,
    version: String,
    id: String,
    readonly: String,
    obj_type: String,
    data: serde_json::Value,
}

/**
 * [WALLET] Get object info.
 */
#[allow(unused_variables)]
#[endpoint {
    method = GET,
    path = "/wallet/object_info",
    tags = [ "wallet" ],
}]
async fn object_info(
    rqctx: Arc<RequestContext<ServerContext>>,
    query: Query<GetObjectInfoRequest>,
) -> Result<HttpResponseOk<ObjectInfoResponse>, HttpError> {
    let object_info_response = ObjectInfoResponse {
        owner: String::new(),
        version: String::new(),
        id: String::new(),
        readonly: String::new(),
        obj_type: String::new(),
        data: json!(""),
    };

    Ok(HttpResponseOk(object_info_response))
}

/**
* 'TransferTransactionRequest' represents the transaction to be sent to the
* network.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct TransferTransactionRequest {
    from_address: String,
    object_id: String,
    to_address: String,
    gas_object_id: String,
}

/**
* 'TransactionResponse' represents the transaction response
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct TransactionResponse {
    object_effects_summary: serde_json::Value,
    certificate: serde_json::Value,
}

/**
 * [WALLET] Transfer object from one address to another.
 */
#[allow(unused_variables)]
#[endpoint {
    method = POST,
    path = "/wallet/transfer",
    tags = [ "wallet" ],
}]
async fn transfer_object(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<TransferTransactionRequest>,
) -> Result<HttpResponseOk<TransactionResponse>, HttpError> {
    let transaction_response = TransactionResponse {
        object_effects_summary: json!(""),
        certificate: json!(""),
    };

    Ok(HttpResponseOk(transaction_response))
}

/**
* 'PublishRequest' represents the publish request
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PublishRequest {
    sender: String,
    path: String,
    gas_object_id: String,
    gas_budget: u64,
}

/**
 * [WALLET] Publish move module.
 */
#[endpoint {
    method = POST,
    path = "/wallet/publish",
    tags = [ "wallet" ],
    // TODO: Figure out how to pass modules over the network before publishing this.
    unpublished = true
}]
#[allow(unused_variables)]
async fn publish(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<PublishRequest>,
) -> Result<HttpResponseOk<TransactionResponse>, HttpError> {
    let transaction_response = TransactionResponse {
        object_effects_summary: json!(""),
        certificate: json!(""),
    };

    Ok(HttpResponseOk(transaction_response))
}

/**
* 'CallRequest' represents the call request
*
* Example CallRequest
* ------------------------
{
    "sender": "b378b8d26c4daa95c5f6a2e2295e6e5f34371c1659e95f572788ffa55c265363",
    "package_object_id": "0x2",
    "module": "ObjectBasics",
    "function": "create",
    "args": [
        200,
        "b378b8d26c4daa95c5f6a2e2295e6e5f34371c1659e95f572788ffa55c265363"
    ],
    "gas_object_id": "1AC945CA31E77991654C0A0FCA8B0FD9C469B5C6",
    "gas_budget": 2000
}
* ------------------------
*/
// TODO: Adjust call specs based on how linter officially lands (pull#508)
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CallRequest {
    sender: String,
    package_object_id: String,
    module: String,
    function: String,
    args: Vec<serde_json::Value>,
    gas_object_id: String,
    gas_budget: u64,
}

/**
 * [WALLET] Call move module.
 */
#[endpoint {
    method = POST,
    path = "/wallet/call",
}]
#[allow(unused_variables)]
async fn call(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<CallRequest>,
) -> Result<HttpResponseOk<TransactionResponse>, HttpError> {
    let transaction_response = TransactionResponse {
        object_effects_summary: json!(""),
        certificate: json!(""),
    };

    Ok(HttpResponseOk(transaction_response))
}

/**
* 'SyncRequest' represents the address that requires a sync.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SyncRequest {
    address: String,
}

/**
 * [WALLET] Synchronize client state with authorities.
 */
#[endpoint {
    method = POST,
    path = "/wallet/sync",
    tags = [ "wallet" ],
}]
#[allow(unused_variables)]
async fn sync(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<SyncRequest>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    unimplemented!()
}
