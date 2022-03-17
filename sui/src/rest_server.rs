// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::fs;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use dropshot::{endpoint, Query, TypedBody};
use dropshot::{
    ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseOk,
    HttpResponseUpdatedNoContent, HttpServerStarter, RequestContext,
};
use futures::lock::Mutex;
use futures::stream::{futures_unordered::FuturesUnordered, StreamExt as _};
use hyper::StatusCode;
use move_core_types::identifier::Identifier;
use move_core_types::parser::parse_type_tag;
use move_core_types::value::MoveStructLayout;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::task::{self, JoinHandle};
use tracing::{error, info};

use sui::config::{GenesisConfig, NetworkConfig};
use sui::gateway::{EmbeddedGatewayConfig, GatewayType};
use sui::keystore::Keystore;
use sui::sui_commands;
use sui::sui_json::{resolve_move_function_args, SuiJsonValue};
use sui::wallet_commands::SimpleTransactionSigner;
use sui_core::gateway_state::GatewayClient;
use sui_types::base_types::*;
use sui_types::committee::Committee;
use sui_types::event::Event;
use sui_types::messages::{ExecutionStatus, TransactionEffects};
use sui_types::move_package::resolve_and_type_check;
use sui_types::object::Object as SuiObject;
use sui_types::object::ObjectRead;

const REST_SERVER_PORT: u16 = 5000;
const REST_SERVER_ADDR_IPV4: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);

#[path = "unit_tests/rest_server_tests.rs"]
#[cfg(test)]
mod rest_server_tests;

#[tokio::main]
async fn main() -> Result<(), String> {
    let config_dropshot: ConfigDropshot = ConfigDropshot {
        bind_address: SocketAddr::from((REST_SERVER_ADDR_IPV4, REST_SERVER_PORT)),
        ..Default::default()
    };

    let config_logging = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    };
    let log = config_logging
        .to_logger("rest_server")
        .map_err(|error| format!("failed to create logger: {}", error))?;

    tracing_subscriber::fmt::init();

    let api = create_api();

    let documentation = api
        .openapi("Sui API", "0.1")
        .json()
        .map_err(|e| e.to_string())?;

    let api_context = ServerContext::new(documentation);

    let server = HttpServerStarter::new(&config_dropshot, api, api_context, &log)
        .map_err(|error| format!("failed to create server: {}", error))?
        .start();

    server.await
}

fn create_api() -> ApiDescription<ServerContext> {
    let mut api = ApiDescription::new();

    // [DOCS]
    api.register(docs).unwrap();

    // [DEBUG]
    api.register(genesis).unwrap();
    api.register(sui_start).unwrap();
    api.register(sui_stop).unwrap();

    // [WALLET]
    api.register(get_addresses).unwrap();
    api.register(get_objects).unwrap();
    api.register(object_schema).unwrap();
    api.register(object_info).unwrap();
    api.register(transfer_object).unwrap();
    api.register(publish).unwrap();
    api.register(call).unwrap();
    api.register(sync).unwrap();

    api
}

/**
 * Server context (state shared by handler functions)
 */
struct ServerContext {
    documentation: serde_json::Value,
    // ServerState is created after genesis.
    server_state: Arc<Mutex<Option<ServerState>>>,
}

pub struct ServerState {
    gateway: GatewayClient,
    // The fields below are for genesis and starting demo network.
    // TODO: Remove these fields when we fully transform rest_server into GatewayServer.
    addresses: Vec<SuiAddress>,
    config: NetworkConfig,
    keystore: Arc<RwLock<Box<dyn Keystore>>>,
    working_dir: PathBuf,
    // Server handles that will be used to restart authorities.
    authority_handles: Vec<JoinHandle<()>>,
}

impl Debug for ServerState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ServerState")
    }
}

impl ServerContext {
    pub fn new(documentation: serde_json::Value) -> ServerContext {
        ServerContext {
            documentation,
            server_state: Arc::new(Mutex::new(None)),
        }
    }
}

fn server_state_error() -> HttpError {
    custom_http_error(
        StatusCode::FAILED_DEPENDENCY,
        "Server state does not exist. Please make a POST request to `sui/genesis/` and `sui/start/` to bootstrap the network."
            .to_string(),
    )
}

/**
Response containing the API documentation.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct DocumentationResponse {
    /** A JSON object containing the OpenAPI definition for this API. */
    documentation: serde_json::Value,
}

/**
Generate OpenAPI documentation.
 */
#[endpoint {
    method = GET,
    path = "/docs",
    tags = [ "docs" ],
}]
async fn docs(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<DocumentationResponse>, HttpError> {
    let server_context = rqctx.context();
    let documentation = &server_context.documentation;

    Ok(HttpResponseOk(DocumentationResponse {
        documentation: documentation.clone(),
    }))
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
    addresses: serde_json::Value,
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
#[endpoint {
    method = POST,
    path = "/sui/genesis",
    tags = [ "debug" ],
}]
async fn genesis(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<GenesisResponse>, HttpError> {
    let context = rqctx.context();
    // Using a new working dir for genesis, this directory will be deleted when stop end point is called.
    let working_dir = PathBuf::from(".").join(format!("{}", ObjectID::random()));

    if context.server_state.lock().await.is_some() {
        return Err(custom_http_error(
            StatusCode::CONFLICT,
            String::from("Cannot run genesis on a existing network, please make a POST request to the `sui/stop` endpoint to reset."),
        ));
    }

    let genesis_conf = GenesisConfig::default_genesis(&working_dir).map_err(|error| {
        custom_http_error(
            StatusCode::CONFLICT,
            format!("Unable to create default genesis configuration: {error}"),
        )
    })?;

    let (network_config, accounts, keystore) =
        sui_commands::genesis(genesis_conf).await.map_err(|err| {
            custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Genesis error: {:?}", err),
            )
        })?;

    let authorities = network_config.get_authority_infos();
    let gateway = GatewayType::Embedded(EmbeddedGatewayConfig {
        authorities,
        db_folder_path: working_dir.join("client_db"),
        ..Default::default()
    });

    let addresses = accounts.iter().map(encode_bytes_hex).collect::<Vec<_>>();
    let addresses_json = json!(addresses);
    let network_config_json = json!(network_config);

    let state = ServerState {
        config: network_config,
        gateway: gateway.init(),
        keystore: Arc::new(RwLock::new(Box::new(keystore))),
        addresses: accounts,
        working_dir: working_dir.to_path_buf(),
        authority_handles: vec![],
    };

    *context.server_state.lock().await = Some(state);

    Ok(HttpResponseOk(GenesisResponse {
        addresses: addresses_json,
        network_config: network_config_json,
    }))
}

/**
Start servers with the specified configurations from the genesis endpoint.

Note: This is a temporary endpoint that will no longer be needed once the
network has been started on testnet or main-net.
 */
#[endpoint {
    method = POST,
    path = "/sui/start",
    tags = [ "debug" ],
}]
async fn sui_start(
    ctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<String>, HttpError> {
    let mut state = ctx.context().server_state.lock().await;
    let state = state.as_mut().ok_or_else(server_state_error)?;

    if !state.authority_handles.is_empty() {
        return Err(custom_http_error(
            StatusCode::FORBIDDEN,
            String::from("Sui network is already running."),
        ));
    }

    let committee = Committee::new(
        state
            .config
            .authorities
            .iter()
            .map(|info| (*info.key_pair.public_key_bytes(), info.stake))
            .collect(),
    );
    let mut handles = FuturesUnordered::new();

    for authority in &state.config.authorities {
        let server = sui_commands::make_server(authority, &committee, state.config.buffer_size)
            .await
            .map_err(|error| {
                custom_http_error(
                    StatusCode::CONFLICT,
                    format!("Unable to make server: {error}"),
                )
            })?;
        handles.push(async move {
            match server.spawn().await {
                Ok(server) => Ok(server),
                Err(err) => {
                    return Err(custom_http_error(
                        StatusCode::FAILED_DEPENDENCY,
                        format!("Failed to start server: {}", err),
                    ));
                }
            }
        })
    }

    let num_authorities = handles.len();
    info!("Started {} authorities", num_authorities);

    while let Some(spawned_server) = handles.next().await {
        state.authority_handles.push(task::spawn(async {
            if let Err(err) = spawned_server.unwrap().join().await {
                error!("Server ended with an error: {}", err);
            }
        }));
    }

    for address in state.addresses.clone() {
        state
            .gateway
            .sync_account_state(address)
            .await
            .map_err(|err| {
                custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("Sync error: {:?}", err),
                )
            })?;
    }
    Ok(HttpResponseOk(format!(
        "Started {} authorities",
        num_authorities
    )))
}

/**
Stop sui network and delete generated configs & storage.

Note: This is a temporary endpoint that will no longer be needed once the
network has been started on testnet or mainnet.
 */
#[endpoint {
    method = POST,
    path = "/sui/stop",
    tags = [ "debug" ],
}]
async fn sui_stop(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    let server_context = rqctx.context();
    // Taking state object without returning ownership
    let mut state = server_context.server_state.lock().await;
    let state = state.take().ok_or_else(server_state_error)?;

    for authority_handle in &state.authority_handles {
        authority_handle.abort();
    }

    // Delete everything from working dir
    fs::remove_dir_all(&state.working_dir).ok();

    Ok(HttpResponseUpdatedNoContent())
}

/**
Response containing the managed addresses for this client.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetAddressResponse {
    /** Vector of hex codes as strings representing the managed addresses */
    addresses: Vec<String>,
}

/**
Retrieve all managed addresses for this client.
 */
#[endpoint {
    method = GET,
    path = "/addresses",
    tags = [ "wallet" ],
}]
async fn get_addresses(
    ctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<GetAddressResponse>, HttpError> {
    let mut state = ctx.context().server_state.lock().await;
    let state = state.as_mut().ok_or_else(server_state_error)?;

    // TODO: Speed up sync operations by kicking them off concurrently.
    // Also need to investigate if this should be an automatic sync or manually triggered.
    let addresses = state.addresses.clone();
    for address in &addresses {
        if let Err(err) = state.gateway.sync_account_state(*address).await {
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Can't create client state: {err}"),
            ));
        }
    }
    Ok(HttpResponseOk(GetAddressResponse {
        addresses: addresses
            .into_iter()
            .map(|address| format!("{}", address))
            .collect(),
    }))
}

/**
Request containing the address of which objecst are to be retrieved.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetObjectsRequest {
    /** Required; Hex code as string representing the address */
    address: String,
}

/**
JSON representation of an object in the Sui network.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Object {
    /** Hex code as string representing the object id */
    object_id: String,
    /** Type of object, i.e. Coin */
    obj_type: String,
    /** Object version */
    version: String,
    /** Hash of the object's contents used for local validation */
    object_digest: String,
}

/**
Returns the list of objects owned by an address.
 */
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetObjectsResponse {
    objects: Vec<Object>,
}

/**
Returns list of objects owned by an address.
 */
#[endpoint {
    method = GET,
    path = "/objects",
    tags = [ "wallet" ],
}]
async fn get_objects(
    ctx: Arc<RequestContext<ServerContext>>,
    query: Query<GetObjectsRequest>,
) -> Result<HttpResponseOk<GetObjectsResponse>, HttpError> {
    let mut state = ctx.context().server_state.lock().await;
    let state = state.as_mut().ok_or_else(server_state_error)?;
    let get_objects_params = query.into_inner();
    let address = get_objects_params.address;
    let address = &decode_bytes_hex(address.as_str()).map_err(|error| {
        custom_http_error(
            StatusCode::FAILED_DEPENDENCY,
            format!("Could not decode address from hex {error}"),
        )
    })?;

    let object_refs = state.gateway.get_owned_objects(*address);
    let mut objects = vec![];
    for (object_id, sequence_number, object_digest) in object_refs {
        let object = match get_object_info(state, object_id).await {
            Ok((_, object, _)) => object,
            Err(error) => {
                return Err(error);
            }
        };
        let obj_type = object
            .data
            .type_()
            .map_or("Unknown Type".to_owned(), |type_| format!("{}", type_));

        objects.push(Object {
            object_id: object_id.to_string(),
            obj_type,
            version: format!("{:?}", sequence_number),
            object_digest: format!("{:?}", object_digest),
        });
    }

    Ok(HttpResponseOk(GetObjectsResponse { objects }))
}

/**
Request containing the object schema for which info is to be retrieved.

If owner is specified we look for this object in that address's account store,
otherwise we look for it in the shared object store.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetObjectSchemaRequest {
    /** Required; Hex code as string representing the object id */
    object_id: String,
}

/**
Response containing the information of an object schema if found, otherwise an error
is returned.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ObjectSchemaResponse {
    /** JSON representation of the object schema */
    schema: serde_json::Value,
}

/**
Returns the schema for a specified object.
 */
#[endpoint {
    method = GET,
    path = "/object_schema",
    tags = [ "wallet" ],
}]
async fn object_schema(
    ctx: Arc<RequestContext<ServerContext>>,
    query: Query<GetObjectSchemaRequest>,
) -> Result<HttpResponseOk<ObjectSchemaResponse>, HttpError> {
    let mut state = ctx.context().server_state.lock().await;
    let state = state.as_mut().ok_or_else(server_state_error)?;
    let object_info_params = query.into_inner();

    let object_id = match ObjectID::try_from(object_info_params.object_id) {
        Ok(object_id) => object_id,
        Err(error) => {
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("{error}"),
            ));
        }
    };

    let layout = match state.gateway.get_object_info(object_id).await {
        Ok(ObjectRead::Exists(_, _, layout)) => layout,
        Ok(ObjectRead::Deleted(_)) => {
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Object ({object_id}) was deleted."),
            ));
        }
        Ok(ObjectRead::NotExists(_)) => {
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Object ({object_id}) does not exist."),
            ));
        }
        Err(error) => {
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Error while getting object info: {:?}", error),
            ));
        }
    };

    match serde_json::to_value(layout) {
        Ok(schema) => Ok(HttpResponseOk(ObjectSchemaResponse { schema })),
        Err(e) => Err(custom_http_error(
            StatusCode::FAILED_DEPENDENCY,
            format!("Error while getting object info: {:?}", e),
        )),
    }
}

/**
Request containing the object for which info is to be retrieved.

If owner is specified we look for this object in that address's account store,
otherwise we look for it in the shared object store.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct GetObjectInfoRequest {
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
    /** Hex code as string representing the object id */
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
#[endpoint {
    method = GET,
    path = "/object_info",
    tags = [ "wallet" ],
}]
async fn object_info(
    ctx: Arc<RequestContext<ServerContext>>,
    query: Query<GetObjectInfoRequest>,
) -> Result<HttpResponseOk<ObjectInfoResponse>, HttpError> {
    let mut state = ctx.context().server_state.lock().await;
    let state = state.as_mut().ok_or_else(server_state_error)?;

    let object_info_params = query.into_inner();
    let object_id = match ObjectID::try_from(object_info_params.object_id) {
        Ok(object_id) => object_id,
        Err(error) => {
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("{error}"),
            ));
        }
    };

    let (_, object, layout) = get_object_info(state, object_id).await?;
    let object_data = object.to_json(&layout).unwrap_or_else(|_| json!(""));
    Ok(HttpResponseOk(ObjectInfoResponse {
        owner: format!("{:?}", object.owner),
        version: format!("{:?}", object.version().value()),
        id: format!("{:?}", object.id()),
        readonly: format!("{:?}", object.is_read_only()),
        obj_type: object
            .data
            .type_()
            .map_or("Unknown Type".to_owned(), |type_| format!("{}", type_)),
        data: object_data,
    }))
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
    /** Integer representing the actual cost of the transaction */
    gas_used: u64,
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
#[endpoint {
    method = POST,
    path = "/transfer",
    tags = [ "wallet" ],
}]
async fn transfer_object(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<TransferTransactionRequest>,
) -> Result<HttpResponseOk<TransactionResponse>, HttpError> {
    let mut state = ctx.context().server_state.lock().await;
    let state = state.as_mut().ok_or_else(server_state_error)?;
    let transfer_order_params = request.into_inner();

    let to_address =
        decode_bytes_hex(transfer_order_params.to_address.as_str()).map_err(|error| {
            custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Could not decode to address from hex {error}"),
            )
        })?;
    let object_id = ObjectID::try_from(transfer_order_params.object_id)
        .map_err(|error| custom_http_error(StatusCode::FAILED_DEPENDENCY, format!("{error}")))?;
    let gas_object_id = ObjectID::try_from(transfer_order_params.gas_object_id)
        .map_err(|error| custom_http_error(StatusCode::FAILED_DEPENDENCY, format!("{error}")))?;
    let owner = decode_bytes_hex(transfer_order_params.from_address.as_str()).map_err(|error| {
        custom_http_error(
            StatusCode::FAILED_DEPENDENCY,
            format!("Could not decode address from hex {error}"),
        )
    })?;

    let tx_signer = Box::pin(SimpleTransactionSigner {
        keystore: state.keystore.clone(),
    });

    let (cert, effects, gas_used) = match state
        .gateway
        .transfer_coin(owner, object_id, gas_object_id, to_address, tx_signer)
        .await
    {
        Ok((cert, effects)) => {
            let gas_used = match effects.status {
                // TODO: handle the actual return value stored in
                // ExecutionStatus::Success
                ExecutionStatus::Success { gas_used, .. } => gas_used,
                ExecutionStatus::Failure { gas_used, error } => {
                    return Err(custom_http_error(
                        StatusCode::FAILED_DEPENDENCY,
                        format!(
                            "Error transferring object: {:#?}, gas used {}",
                            error, gas_used
                        ),
                    ));
                }
            };
            (cert, effects, gas_used)
        }
        Err(err) => {
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Transfer error: {err}"),
            ));
        }
    };

    let object_effects_summary = get_object_effects(state, effects).await?;

    Ok(HttpResponseOk(TransactionResponse {
        gas_used,
        object_effects_summary: json!(object_effects_summary),
        certificate: json!(cert),
    }))
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
sure the package is valid. If some modules have initializers, these initializers
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
        gas_used: 0,
        object_effects_summary: json!(""),
        certificate: json!(""),
    };

    Ok(HttpResponseOk(transaction_response))
}

/**
Request containing the information required to execute a move module.
*/
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
    /** Optional; The argument types to be parsed */
    type_args: Option<Vec<String>>,
    /** Required; JSON representation of the arguments */
    args: Vec<SuiJsonValue>,
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
async fn call(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<CallRequest>,
) -> Result<HttpResponseOk<TransactionResponse>, HttpError> {
    let mut state = ctx.context().server_state.lock().await;
    let state = state.as_mut().ok_or_else(server_state_error)?;

    let call_params = request.into_inner();
    let transaction_response = handle_move_call(call_params, state)
        .await
        .map_err(|err| custom_http_error(StatusCode::BAD_REQUEST, format!("{err}")))?;
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
async fn sync(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<SyncRequest>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    let sync_params = request.into_inner();
    let mut state = ctx.context().server_state.lock().await;
    let state = state.as_mut().ok_or_else(server_state_error)?;

    let address = decode_bytes_hex(sync_params.address.as_str()).map_err(|error| {
        custom_http_error(
            StatusCode::FAILED_DEPENDENCY,
            format!("Could not decode to address from hex {error}"),
        )
    })?;

    state
        .gateway
        .sync_account_state(address)
        .await
        .map_err(|err| {
            custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Can't create client state: {err}"),
            )
        })?;
    Ok(HttpResponseUpdatedNoContent())
}

async fn get_object_effects(
    state: &ServerState,
    transaction_effects: TransactionEffects,
) -> Result<HashMap<String, Vec<HashMap<String, String>>>, HttpError> {
    let mut object_effects_summary = HashMap::new();
    object_effects_summary.insert(
        String::from("created_objects"),
        get_obj_ref_effects(
            state,
            transaction_effects
                .created
                .into_iter()
                .map(|(oref, _)| oref)
                .collect::<Vec<_>>(),
        )
        .await?,
    );
    object_effects_summary.insert(
        String::from("mutated_objects"),
        get_obj_ref_effects(
            state,
            transaction_effects
                .mutated
                .into_iter()
                .map(|(oref, _)| oref)
                .collect::<Vec<_>>(),
        )
        .await?,
    );
    object_effects_summary.insert(
        String::from("unwrapped_objects"),
        get_obj_ref_effects(
            state,
            transaction_effects
                .unwrapped
                .into_iter()
                .map(|(oref, _)| oref)
                .collect::<Vec<_>>(),
        )
        .await?,
    );
    object_effects_summary.insert(
        String::from("deleted_objects"),
        get_obj_ref_effects(state, transaction_effects.deleted).await?,
    );
    object_effects_summary.insert(
        String::from("wrapped_objects"),
        get_obj_ref_effects(state, transaction_effects.wrapped).await?,
    );
    object_effects_summary.insert(
        String::from("events"),
        get_events(transaction_effects.events)?,
    );
    Ok(object_effects_summary)
}

fn get_events(events: Vec<Event>) -> Result<Vec<HashMap<String, String>>, HttpError> {
    let mut effects = Vec::new();
    for event in events {
        let mut effect: HashMap<String, String> = HashMap::new();
        effect.insert("type".to_string(), format!("{}", event.type_));
        effect.insert("contents".to_string(), format!("{:?}", event.contents));
        effects.push(effect);
    }
    Ok(effects)
}

async fn get_obj_ref_effects(
    state: &ServerState,
    object_refs: Vec<ObjectRef>,
) -> Result<Vec<HashMap<String, String>>, HttpError> {
    let mut effects = Vec::new();
    for (object_id, sequence_number, object_digest) in object_refs {
        let effect = get_effect(state, object_id, sequence_number, object_digest)
            .await
            .map_err(|error| error)?;
        effects.push(effect);
    }
    Ok(effects)
}

async fn get_effect(
    state: &ServerState,
    object_id: ObjectID,
    sequence_number: SequenceNumber,
    object_digest: ObjectDigest,
) -> Result<HashMap<String, String>, HttpError> {
    let mut effect = HashMap::new();
    let object = match get_object_info(state, object_id).await {
        Ok((_, object, _)) => object,
        Err(error) => {
            return Err(error);
        }
    };
    effect.insert(
        "type".to_string(),
        object
            .data
            .type_()
            .map_or("Unknown Type".to_owned(), |type_| format!("{}", type_)),
    );
    effect.insert("id".to_string(), object_id.to_string());
    effect.insert("version".to_string(), format!("{:?}", sequence_number));
    effect.insert("object_digest".to_string(), format!("{:?}", object_digest));
    Ok(effect)
}

async fn get_object_info(
    state: &ServerState,
    object_id: ObjectID,
) -> Result<(ObjectRef, SuiObject, Option<MoveStructLayout>), HttpError> {
    let (object_ref, object, layout) = match state.gateway.get_object_info(object_id).await {
        Ok(ObjectRead::Exists(object_ref, object, layout)) => (object_ref, object, layout),
        Ok(ObjectRead::Deleted(_)) => {
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Object ({object_id}) was deleted."),
            ));
        }
        Ok(ObjectRead::NotExists(_)) => {
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Object ({object_id}) does not exist."),
            ));
        }
        Err(error) => {
            return Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Error while getting object info: {:?}", error),
            ));
        }
    };
    Ok((object_ref, object, layout))
}

async fn handle_move_call(
    call_params: CallRequest,
    state: &mut ServerState,
) -> Result<TransactionResponse, anyhow::Error> {
    let module = Identifier::from_str(&call_params.module.to_owned())?;
    let function = Identifier::from_str(&call_params.function.to_owned())?;
    let args = call_params.args;
    let type_args = call_params
        .type_args
        .unwrap_or_default()
        .iter()
        .map(|type_arg| parse_type_tag(type_arg))
        .collect::<Result<Vec<_>, _>>()?;
    let gas_budget = call_params.gas_budget;
    let gas_object_id = ObjectID::try_from(call_params.gas_object_id)?;
    let package_object_id = ObjectID::from_hex_literal(&call_params.package_object_id)?;

    let sender: SuiAddress = decode_bytes_hex(call_params.sender.as_str())?;

    let (package_object_ref, package_object, _) = get_object_info(state, package_object_id).await?;

    // Extract the input args
    let (object_ids, pure_args) =
        resolve_move_function_args(&package_object, module.clone(), function.clone(), args)?;

    info!("Resolved fn to: \n {:?} & {:?}", object_ids, pure_args);

    // Fetch all the objects needed for this call
    let mut input_objs = vec![];
    for obj_id in object_ids.clone() {
        let (_, object, _) = get_object_info(state, obj_id).await?;
        input_objs.push(object);
    }

    // Pass in the objects for a deeper check
    resolve_and_type_check(
        package_object.clone(),
        &module,
        &function,
        &type_args,
        input_objs,
        pure_args.clone(),
    )?;

    // Fetch the object info for the gas obj
    let (gas_obj_ref, _, _) = get_object_info(state, gas_object_id).await?;

    // Fetch the objects for the object args
    let mut object_args_refs = Vec::new();
    for obj_id in object_ids {
        let (object_ref, _, _) = get_object_info(state, obj_id).await?;
        object_args_refs.push(object_ref);
    }

    let tx_signer = Box::pin(SimpleTransactionSigner {
        keystore: state.keystore.clone(),
    });

    let (cert, effects, gas_used) = match state
        .gateway
        .move_call(
            sender,
            package_object_ref,
            module.to_owned(),
            function.to_owned(),
            type_args.clone(),
            gas_obj_ref,
            object_args_refs,
            // TODO: Populate shared object args. sui/issue#719
            vec![],
            pure_args,
            gas_budget,
            tx_signer,
        )
        .await
    {
        Ok((cert, effects)) => {
            let gas_used = match effects.status {
                // TODO: handle the actual return value stored in
                // ExecutionStatus::Success
                ExecutionStatus::Success { gas_used, .. } => gas_used,
                ExecutionStatus::Failure { gas_used, error } => {
                    let context = format!("Error calling move function, gas used {gas_used}");
                    return Err(anyhow::Error::new(error).context(context));
                }
            };
            (cert, effects, gas_used)
        }
        Err(err) => {
            return Err(err);
        }
    };

    let object_effects_summary = get_object_effects(state, effects).await?;

    Ok(TransactionResponse {
        gas_used,
        object_effects_summary: json!(object_effects_summary),
        certificate: json!(cert),
    })
}

fn custom_http_error(status_code: http::StatusCode, message: String) -> HttpError {
    HttpError::for_client_error(None, status_code, message)
}
