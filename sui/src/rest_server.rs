// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use dropshot::{endpoint, Query, TypedBody};
#[allow(unused_imports)]
use dropshot::{
    ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError,
    HttpResponseUpdatedNoContent, HttpServerStarter, RequestContext,
};
use ed25519_dalek::ed25519::signature::Signature;
use futures::lock::Mutex;
use hyper::StatusCode;
use move_core_types::identifier::Identifier;
use move_core_types::parser::parse_type_tag;
use move_core_types::value::MoveStructLayout;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::info;

use sui::config::{GenesisConfig, NetworkConfig, PersistedConfig};
use sui::gateway::{EmbeddedGatewayConfig, GatewayType};
use sui::sui_commands;
use sui::sui_json::{resolve_move_function_args, SuiJsonValue};
use sui_core::gateway_state::gateway_responses::TransactionResponse;
use sui_core::gateway_state::{GatewayClient, GatewayState};
use sui_types::base_types::*;
use sui_types::crypto;
use sui_types::crypto::SignableBytes;
use sui_types::messages::{Transaction, TransactionData};
use sui_types::move_package::resolve_and_type_check;
use sui_types::object::Object as SuiObject;
use sui_types::object::ObjectRead;

const REST_SERVER_PORT: u16 = 5001;
const REST_SERVER_ADDR_IPV4: Ipv4Addr = Ipv4Addr::new(127, 0, 0, 1);

#[path = "unit_tests/rest_server_tests.rs"]
#[cfg(test)]
mod rest_server_tests;

#[tokio::main]
async fn main() -> Result<(), String> {
    let config_dropshot: ConfigDropshot = ConfigDropshot {
        bind_address: SocketAddr::from((REST_SERVER_ADDR_IPV4, REST_SERVER_PORT)),
        request_body_max_bytes: usize::pow(10, 6), // 1mb limit, but may need to increase.
    };

    let config_logging = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    };
    let log = config_logging
        .to_logger("rest_server")
        .map_err(|error| format!("failed to create logger: {error}"))?;

    tracing_subscriber::fmt::init();

    let api = create_api();

    let documentation = api
        .openapi("Sui API", "0.1")
        .json()
        .map_err(|e| e.to_string())?;

    let config: GatewayConfig = PersistedConfig::read(&PathBuf::from("./gateway.conf")).unwrap();
    let committee = config.make_committee();
    let authority_clients = config.make_authority_clients();
    let gateway = Box::new(GatewayState::new(
        config.db_folder_path,
        committee,
        authority_clients,
    ));

    let api_context = ServerContext::new(documentation, gateway);

    let server = HttpServerStarter::new(&config_dropshot, api, api_context, &log)
        .map_err(|error| format!("failed to create server: {error}"))?
        .start();

    server.await
}

fn create_api() -> ApiDescription<ServerContext> {
    let mut api = ApiDescription::new();

    // [DOCS]
    api.register(docs).unwrap();

    // [API]
    api.register(get_objects).unwrap();
    api.register(object_schema).unwrap();
    api.register(object_info).unwrap();
    api.register(new_transfer).unwrap();
    api.register(split_coin).unwrap();
    api.register(publish).unwrap();
    api.register(move_call).unwrap();
    api.register(sync_account_state).unwrap();
    api.register(execute_transaction).unwrap();

    api
}

/**
 * Server context (state shared by handler functions)
 */
struct ServerContext {
    documentation: serde_json::Value,
    // ServerState is created after genesis.
    gateway: Arc<Mutex<GatewayClient>>,
}

impl ServerContext {
    pub fn new(documentation: serde_json::Value, gateway: GatewayClient) -> ServerContext {
        ServerContext {
            documentation,
            gateway: Arc::new(Mutex::new(gateway)),
        }
    }
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
async fn docs(rqctx: Arc<RequestContext<ServerContext>>) -> Result<Response<Body>, HttpError> {
    let server_context = rqctx.context();
    let documentation = &server_context.documentation;

    custom_http_response(
        StatusCode::OK,
        DocumentationResponse {
            documentation: documentation.clone(),
        },
    )
    .map_err(|err| custom_http_error(StatusCode::BAD_REQUEST, format!("{err}")))
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
    version: u64,
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
    path = "/api/objects",
    tags = [ "api" ],
}]
async fn get_objects(
    ctx: Arc<RequestContext<ServerContext>>,
    query: Query<GetObjectsRequest>,
) -> Result<HttpResponseOk<JsonResponse<ObjectResponse>>, HttpError> {
    let mut gateway = ctx.context().gateway.lock().await;
    let get_objects_params = query.into_inner();
    let address = get_objects_params.address;
    let address = &decode_bytes_hex(address.as_str()).map_err(|error| {
        custom_http_error(
            StatusCode::BAD_REQUEST,
            format!("Could not decode address from hex {error}"),
        )
    })?;

    let objects = gateway
        .get_owned_objects(*address)
        .unwrap()
        .into_iter()
        .map(NamedObjectRef::from)
        .collect();

    let response = ObjectResponse { objects };

    Ok(HttpResponseOk(JsonResponse(response)))
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
    let gateway = ctx.context().gateway.lock().await;
    let object_info_params = query.into_inner();

    let object_id = match ObjectID::try_from(object_info_params.object_id) {
        Ok(object_id) => object_id,
        Err(error) => {
            return Err(custom_http_error(
                StatusCode::BAD_REQUEST,
                format!("{error}"),
            ));
        }
    };

    let layout = match gateway.get_object_info(object_id).await {
        Ok(ObjectRead::Exists(_, _, layout)) => layout,
        Ok(ObjectRead::Deleted(_)) => {
            return Err(custom_http_error(
                StatusCode::NOT_FOUND,
                format!("Object ({object_id}) was deleted."),
            ));
        }
        Ok(ObjectRead::NotExists(_)) => {
            return Err(custom_http_error(
                StatusCode::NOT_FOUND,
                format!("Object ({object_id}) does not exist."),
            ));
        }
        Err(error) => {
            return Err(custom_http_error(
                StatusCode::NOT_FOUND,
                format!("Error while getting object info: {:?}", error),
            ));
        }
    };
    let schema = serde_json::to_value(layout).map_err(|error| {
        custom_http_error(
            StatusCode::FAILED_DEPENDENCY,
            format!("Error while getting object info: {:?}", error),
        )
    })?;

    Ok(HttpResponseOk(ObjectSchemaResponse { schema }))
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
    path = "/api/object_info",
    tags = [ "wallet" ],
}]
async fn object_info(
    ctx: Arc<RequestContext<ServerContext>>,
    query: Query<GetObjectInfoRequest>,
) -> Result<HttpResponseOk<JsonResponse<ObjectRead>>, HttpError> {
    let gateway = ctx.context().gateway.lock().await;

    let object_info_params = query.into_inner();
    let object_id = ObjectID::try_from(object_info_params.object_id)
        .map_err(|error| custom_http_error(StatusCode::BAD_REQUEST, format!("{error}")))?;

    let object_read = gateway.get_object_info(object_id).await.map_err(|error| {
        custom_http_error(
            StatusCode::NOT_FOUND,
            format!("Error while getting object info: {:?}", error),
        )
    })?;
    Ok(HttpResponseOk(JsonResponse(object_read)))
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
    /** Required; Gas budget required as a cap for gas usage */
    gas_budget: u64,
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
    path = "/api/new_transfer",
    tags = [ "api" ],
}]
async fn new_transfer(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<TransferTransactionRequest>,
) -> Result<HttpResponseOk<TransactionBytes>, HttpError> {
    let mut gateway = ctx.context().gateway.lock().await;
    let request = request.into_inner();

    let tx_data = async {
        let to_address = decode_bytes_hex(request.to_address.as_str())?;
        let object_id = ObjectID::try_from(request.object_id)?;
        let gas_object_id = ObjectID::try_from(request.gas_object_id)?;
        let owner = decode_bytes_hex(request.from_address.as_str())?;
        gateway
            .transfer_coin(owner, object_id, gas_object_id, to_address)
            .await
    }
    .await
    .map_err(|error| custom_http_error(StatusCode::BAD_REQUEST, error.to_string()))?;

    Ok(HttpResponseOk(TransactionBytes::new(tx_data)))
}

#[endpoint {
method = POST,
path = "/api/split_coin",
tags = [ "api" ],
}]
async fn split_coin(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<SplitCoinRequest>,
) -> Result<HttpResponseOk<TransactionBytes>, HttpError> {
    let mut gateway = ctx.context().gateway.lock().await;
    let request = request.into_inner();

    let tx_data = async {
        let signer = decode_bytes_hex(request.signer.as_str())?;
        let object_id = ObjectID::try_from(request.coin_object_id)?;
        let gas_object_id = ObjectID::try_from(request.gas_payment)?;
        gateway
            .split_coin(
                signer,
                object_id,
                request.split_amounts,
                gas_object_id,
                request.gas_budget,
            )
            .await
    }
    .await
    .map_err(|error| custom_http_error(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(HttpResponseOk(TransactionBytes::new(tx_data)))
}

/**
Request representing the contents of the Move module to be published.
*/
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct PublishRequest {
    /** Required; Hex code as string representing the sender's address */
    sender: String,
    /** Required; Move modules serialized as hex */
    compiled_modules: Vec<String>,
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
}]
#[allow(unused_variables)]
async fn publish(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<PublishRequest>,
) -> Result<HttpResponseOk<TransactionBytes>, HttpError> {
    let mut gateway = ctx.context().gateway.lock().await;
    let publish_params = request.into_inner();

    let data = handle_publish(publish_params, &mut gateway)
        .await
        .map_err(|err| custom_http_error(StatusCode::BAD_REQUEST, format!("{:#}", err)))?;
    Ok(HttpResponseOk(TransactionBytes::new(data)))
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
    path = "/api/move_call",
    tags = [ "api" ],
}]
async fn move_call(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<CallRequest>,
) -> Result<HttpResponseOk<Value>, HttpError> {
    let mut gateway = ctx.context().gateway.lock().await;

    let call_params = request.into_inner();
    let data = handle_move_call(call_params, &mut gateway)
        .await
        .map_err(|err| custom_http_error(StatusCode::BAD_REQUEST, format!("{:#}", err)))?;

    let body = json!({
        "unsigned_tx_base64" : data.to_base64()
    });
    Ok(HttpResponseOk(body))
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
    path = "/api/sync_account_state",
    tags = [ "api" ],
}]
async fn sync_account_state(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<SyncRequest>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    let sync_params = request.into_inner();
    let mut gateway = ctx.context().gateway.lock().await;

    let address = decode_bytes_hex(sync_params.address.as_str()).map_err(|error| {
        custom_http_error(
            StatusCode::FAILED_DEPENDENCY,
            format!("Could not decode to address from hex {error}"),
        )
    })?;

    gateway.sync_account_state(address).await.map_err(|err| {
        custom_http_error(
            StatusCode::BAD_REQUEST,
            format!("Can't create client state: {err}"),
        )
    })?;
    Ok(HttpResponseUpdatedNoContent())
}

/**
Synchronize client state with authorities. This will fetch the latest information
on all objects owned by each address that is managed by this client state.
 */
#[endpoint {
method = POST,
path = "/api/execute_transaction",
tags = [ "api" ],
}]
async fn execute_transaction(
    ctx: Arc<RequestContext<ServerContext>>,
    response: TypedBody<SignedTransaction>,
) -> Result<HttpResponseOk<JsonResponse<TransactionResponse>>, HttpError> {
    let response = response.into_inner();
    let mut gateway = ctx.context().gateway.lock().await;

    let response: Result<_, anyhow::Error> = async {
        let data = base64::decode(response.unsigned_tx_base64)?;
        let data = TransactionData::from_signable_bytes(data)?;

        let mut signature_bytes = base64::decode(response.signature)?;
        let mut pub_key_bytes = base64::decode(response.pub_key)?;
        signature_bytes.append(&mut pub_key_bytes);
        let signature = crypto::Signature::from_bytes(&*signature_bytes)?;
        gateway
            .execute_transaction(Transaction::new(data, signature))
            .await
    }
    .await;
    let response = response
        .map_err(|err| custom_http_error(StatusCode::FAILED_DEPENDENCY, err.to_string()))?;
    Ok(HttpResponseOk(JsonResponse(response)))
}

async fn get_object_info(
    gateway: &GatewayClient,
    object_id: ObjectID,
) -> Result<(ObjectRef, SuiObject, Option<MoveStructLayout>), HttpError> {
    let (object_ref, object, layout) = match gateway.get_object_info(object_id).await {
        Ok(ObjectRead::Exists(object_ref, object, layout)) => (object_ref, object, layout),
        Ok(ObjectRead::Deleted(_)) => {
            return Err(custom_http_error(
                StatusCode::NOT_FOUND,
                format!("Object ({object_id}) was deleted."),
            ));
        }
        Ok(ObjectRead::NotExists(_)) => {
            return Err(custom_http_error(
                StatusCode::NOT_FOUND,
                format!("Object ({object_id}) does not exist."),
            ));
        }
        Err(error) => {
            return Err(custom_http_error(
                StatusCode::NOT_FOUND,
                format!("Error while getting object info: {:?}", error),
            ));
        }
    };
    Ok((object_ref, object, layout))
}

async fn handle_publish(
    publish_params: PublishRequest,
    gateway: &mut GatewayClient,
) -> Result<TransactionData, anyhow::Error> {
    let compiled_modules = publish_params
        .compiled_modules
        .iter()
        .map(|module| decode_bytes_hex(module))
        .collect::<Result<Vec<_>, _>>()?;

    let gas_budget = publish_params.gas_budget;
    let gas_object_id = ObjectID::try_from(publish_params.gas_object_id)?;
    let sender: SuiAddress = decode_bytes_hex(publish_params.sender.as_str())?;
    let (gas_obj_ref, _, _) = get_object_info(gateway, gas_object_id).await?;

    gateway
        .publish(sender, compiled_modules, gas_obj_ref, gas_budget)
        .await
}

async fn handle_move_call(
    call_params: CallRequest,
    gateway: &mut GatewayClient,
) -> Result<TransactionData, anyhow::Error> {
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

    let (package_object_ref, package_object, _) =
        get_object_info(gateway, package_object_id).await?;

    // Extract the input args
    let (object_ids, pure_args) =
        resolve_move_function_args(&package_object, module.clone(), function.clone(), args)?;

    info!("Resolved fn to: \n {:?} & {:?}", object_ids, pure_args);

    // Fetch all the objects needed for this call
    let mut input_objs = vec![];
    for obj_id in object_ids.clone() {
        let (_, object, _) = get_object_info(gateway, obj_id).await?;
        input_objs.push(object);
    }

    // Pass in the objects for a deeper check
    resolve_and_type_check(
        &package_object,
        &module,
        &function,
        &type_args,
        input_objs,
        pure_args.clone(),
    )?;

    // Fetch the object info for the gas obj
    let (gas_obj_ref, _, _) = get_object_info(gateway, gas_object_id).await?;

    // Fetch the objects for the object args
    let mut object_args_refs = Vec::new();
    for obj_id in object_ids {
        let (object_ref, _, _) = get_object_info(gateway, obj_id).await?;
        object_args_refs.push(object_ref);
    }

    gateway
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
        )
        .await
}

/*fn publish_response_to_json(
    publish_response: PublishResponse,
) -> Result<serde_json::Value, HttpError> {
    // TODO: impl JSON Schema for Gateway Responses
    Ok(json!({
        "publishResults": {
            "package": {
                    "object_id": publish_response.package.0.to_string(),
                    "version": u64::from(publish_response.package.1),
                    "object_digest": format!("{:?}", publish_response.package.2),
                },
            "createdObjects": json!(publish_response
                .created_objects
                .iter()
                .map(|obj| {
                    json!({
                        "owner": format!("{:?}", obj.owner),
                        "version": format!("{:?}", obj.version().value()),
                        "id": format!("{:?}", obj.id()),
                        "readonly": format!("{:?}", obj.is_read_only()),
                        "obj_type": obj
                            .data
                            .type_()
                            .map_or("Unknown Type".to_owned(), |type_| format!("{}", type_)),
                    })
                })
                .collect::<Vec<_>>()),
            "updatedGas": GasCoin::try_from(&publish_response.updated_gas)
                .map_err(|err| custom_http_error(StatusCode::INTERNAL_SERVER_ERROR, format!("{err}")))?
        },
        "certificate": {
            "signedAuthorities": publish_response
                .certificate
                .signatures
                .iter()
                .map(|(bytes, _)| encode_bytes_hex(bytes))
                .collect::<Vec<_>>(),
        }
    }))
}*/
