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

    // [DEBUG]
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
Request containing the server configuration.

All attributes in GenesisRequest are optional, a default value will be used if
the fields are not set.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GenesisRequest {
    /** Optional; Number of authorities to be started in the network */
    num_authorities: Option<u16>,
    /** Optional; Number of managed addresses to be created at genesis */
    num_addresses: Option<u16>,
    /** Optional; Number of gas objects to be created for each address */
    num_gas_objects: Option<u16>,
}

/**
Response containing the resulting wallet & network config of the
provided genesis configuration.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GenesisResponse {
    /** List of managed addresses and the list of authorities */
    wallet_config: serde_json::Value,
    /** Information about authorities and the list of loaded move packages. */
    network_config: serde_json::Value,
}

/**
Specify the genesis state of the network.

You can specify the number of authorities, an initial number of addresses
and the number of gas objects to be assigned to those addresses.

Note: This is a temporary endpoint that will no longer be needed once the
network has been started on testnet or mainnet.
 */
#[allow(unused_variables)]
#[endpoint {
    method = POST,
    path = "/sui/genesis",
    tags = [ "debug" ],
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
Start servers with the specified configurations from the genesis endpoint.

Note: This is a temporary endpoint that will no longer be needed once the
network has been started on testnet or mainnet.
 */
#[allow(unused_variables)]
#[endpoint {
    method = POST,
    path = "/sui/start",
    tags = [ "debug" ],
}]
async fn sui_start(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<()>, HttpError> {
    unimplemented!()
}

/**
Stop sui network and delete generated configs & storage.

Note: This is a temporary endpoint that will no longer be needed once the
network has been started on testnet or mainnet.
 */
#[allow(unused_variables)]
#[endpoint {
    method = POST,
    path = "/sui/stop",
    tags = [ "debug" ],
}]
async fn sui_stop(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    unimplemented!()
}

/**
Response containing the managed addresses for this client.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetAddressResponse {
    /** Vector of hex codes as strings representing the managed addresses */
    addresses: Vec<String>,
}

/**
Retrieve all managed addresses for this client.
 */
#[allow(unused_variables)]
#[endpoint {
    method = GET,
    path = "/addresses",
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
Scan parameters used to retrieve objects owned by an address.

Describes the set of querystring parameters that your endpoint
accepts for the first request of the scan.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetObjectsScanParams {
    /** Required; Hex code as string representing the address */
    address: String,
}

/**
Page selector used to retrieve the next set of objects owned by an address.

Describes the information your endpoint needs for requests after the first one.
Typically this would include an id of some sort for the last item on the
previous page. The entire PageSelector will be serialized to an opaque string
and included in the ResultsPage. The client is expected to provide this string
as the "page_token" querystring parameter in the subsequent request.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetObjectsPageSelector {
    /** Required; Hex code as string representing the address */
    address: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Object {
    /** Hex code as string representing the object id */
    object_id: String,
    /** Contains the object id, sequence number and object digest */
    object_ref: serde_json::Value,
}

/**
Returns list of objects owned by an address.
 */
#[allow(unused_variables)]
#[endpoint {
    method = GET,
    path = "/objects",
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
Request containing the object for which info is to be retrieved.

If owner is specified we look for this obejct in that address's account store,
otherwise we look for it in the shared object store.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetObjectInfoRequest {
    /** Optional; Hex code as string representing the owner's address */
    owner: Option<String>,
    /** Required; Hex code as string representing the object id */
    object_id: String,
}

/**
Response containing the information of an object if found, otherwise an error
is returned.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ObjectInfoResponse {
    /** Hex code as string representing the owner's address */
    owner: String,
    /** Sequence number of the object */
    version: String,
    /** Hex code as string representing the objet id */
    id: String,
    /** Boolean representing if the object is mutable */
    readonly: String,
    /** Type of object, i.e. Coin */
    obj_type: String,
    /** JSON representation of the object data */
    data: serde_json::Value,
}

/**
Returns the object information for a specified object.
 */
#[allow(unused_variables)]
#[endpoint {
    method = GET,
    path = "/object_info",
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
Request containing the information needed to execute a transfer transaction.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct TransferTransactionRequest {
    /** Required; Hex code as string representing the address to be sent from */
    from_address: String,
    /** Required; Hex code as string representing the object id */
    object_id: String,
    /** Required; Hex code as string representing the address to be sent to */
    to_address: String,
    /** Required; Hex code as string representing the gas object id to be used as payment */
    gas_object_id: String,
}

/**
Response containing the summary of effects made on an object and the certificate
associated with the transaction that verifies the transaction.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct TransactionResponse {
    /** JSON representation of the list of resulting effects on the object */
    object_effects_summary: serde_json::Value,
    /** JSON representation of the certificate verifying the transaction */
    certificate: serde_json::Value,
}

/**
Transfer object from one address to another. Gas will be paid using the gas
provided in the request. This will be done through a native transfer
transaction that does not require Move VM executions, hence is much cheaper.

Notes:
- Non-coin objects cannot be transferred natively and will require a Move call

Example TransferTransactionRequest
{
    "from_address": "1DA89C9279E5199DDC9BC183EB523CF478AB7168",
    "object_id": "4EED236612B000B9BEBB99BA7A317EFF27556A0C",
    "to_address": "5C20B3F832F2A36ED19F792106EC73811CB5F62C",
    "gas_object_id": "96ABE602707B343B571AAAA23E3A4594934159A5"
}
 */
#[allow(unused_variables)]
#[endpoint {
    method = POST,
    path = "/transfer",
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
Request representing the contents of the Move module to be published.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PublishRequest {
    /** Required; Hex code as string representing the sender's address */
    sender: String,
    /** Required; Move module serialized as bytes? */
    module: String,
    /** Required; Hex code as string representing the gas object id */
    gas_object_id: String,
    /** Required; Gas budget required because of the need to execute module initializers */
    gas_budget: u64,
}

/**
Publish move module. It will perform proper verification and linking to make
sure the pacakge is valid. If some modules have initializers, these initializers
will also be executed in Move (which means new Move objects can be created in
the process of publishing a Move package). Gas budget is required because of the
need to execute module initializers.
 */
#[endpoint {
    method = POST,
    path = "/publish",
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
Request containing the information required to execute a move module.
*/
// TODO: Adjust call specs based on how linter officially lands (pull#508)
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CallRequest {
    /** Required; Hex code as string representing the sender's address */
    sender: String,
    /** Required; Hex code as string representing Move module location */
    package_object_id: String,
    /** Required; Name of the move module */
    module: String,
    /** Required; Name of the function to be called in the move module */
    function: String,
    /** Required; JSON representation of the arguments */
    args: Vec<serde_json::Value>,
    /** Required; Hex code as string representing the gas object id */
    gas_object_id: String,
    /** Required; Gas budget required as a cap for gas usage */
    gas_budget: u64,
}

/**
Execute a Move call transaction by calling the specified function in the
module of the given package. Arguments are passed in and type will be
inferred from function signature. Gas usage is capped by the gas_budget.

Example CallRequest
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
 */
#[endpoint {
    method = POST,
    path = "/call",
    tags = [ "wallet" ],
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
Request containing the address that requires a sync.
*/
// TODO: This call may not be required. Sync should not need to be triggered by user.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SyncRequest {
    /** Required; Hex code as string representing the address */
    address: String,
}

/**
Synchronize client state with authorities. This will fetch the latest information
on all objects owned by each address that is managed by this client state.
 */
#[endpoint {
    method = POST,
    path = "/sync",
    tags = [ "wallet" ],
}]
#[allow(unused_variables)]
async fn sync(
    rqctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<SyncRequest>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    unimplemented!()
}
