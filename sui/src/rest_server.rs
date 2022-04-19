// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::anyhow;
use clap::*;
use dropshot::{endpoint, Query, TypedBody};
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

use sui::config::PersistedConfig;
use sui::gateway::GatewayConfig;
use sui::rest_gateway::requests::{
    CallRequest, GetObjectInfoRequest, GetObjectSchemaRequest, GetObjectsRequest, MergeCoinRequest,
    PublishRequest, SignedTransaction, SplitCoinRequest, SyncRequest, TransferTransactionRequest,
};
use sui::rest_gateway::responses::{
    custom_http_error, HttpResponseOk, JsonResponse, NamedObjectRef, ObjectResponse,
    ObjectSchemaResponse, TransactionBytes,
};
use sui::{sui_config_dir, SUI_GATEWAY_CONFIG};
use sui_core::gateway_state::gateway_responses::TransactionResponse;
use sui_core::gateway_state::{GatewayClient, GatewayState};
use sui_types::base_types::*;
use sui_types::crypto;
use sui_types::crypto::SignableBytes;
use sui_types::messages::{Transaction, TransactionData};
use sui_types::object::Object as SuiObject;
use sui_types::object::ObjectRead;

const DEFAULT_REST_SERVER_PORT: &str = "5001";
const DEFAULT_REST_SERVER_ADDR_IPV4: &str = "127.0.0.1";

#[path = "unit_tests/rest_server_tests.rs"]
#[cfg(test)]
mod rest_server_tests;

#[derive(Parser)]
#[clap(
    name = "Sui Rest Gateway",
    about = "A Byzantine fault tolerant chain with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
struct RestServerOpt {
    #[clap(long)]
    config: Option<PathBuf>,

    #[clap(long, default_value = DEFAULT_REST_SERVER_PORT)]
    port: u16,

    #[clap(long, default_value = DEFAULT_REST_SERVER_ADDR_IPV4)]
    host: Ipv4Addr,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let options: RestServerOpt = RestServerOpt::parse();

    let config_dropshot: ConfigDropshot = ConfigDropshot {
        bind_address: SocketAddr::from((options.host, options.port)),
        request_body_max_bytes: usize::pow(10, 6), // 1mb limit, but may need to increase.
    };

    let config_logging = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    };
    let log = config_logging.to_logger("rest_server")?;

    tracing_subscriber::fmt::init();

    let api = create_api();

    let documentation = api.openapi("Sui Gateway API", "0.1").json()?;

    let config_path = options
        .config
        .unwrap_or(sui_config_dir()?.join(SUI_GATEWAY_CONFIG));

    let config: GatewayConfig = PersistedConfig::read(&config_path).map_err(|e| {
        anyhow!(
            "Failed to read config file at {:?}: {}. Have you run `sui genesis` first?",
            config_path,
            e
        )
    })?;
    let committee = config.make_committee();
    let authority_clients = config.make_authority_clients();
    let gateway = Box::new(GatewayState::new(
        config.db_folder_path,
        committee,
        authority_clients,
    )?);

    let api_context = ServerContext::new(documentation, gateway);
    let server = HttpServerStarter::new(&config_dropshot, api, api_context, &log)?.start();
    server.await.map_err(|err| anyhow!(err))
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
    api.register(merge_coin).unwrap();
    api.register(publish).unwrap();
    api.register(move_call).unwrap();
    api.register(sync_account_state).unwrap();
    api.register(execute_transaction).unwrap();

    api
}

/// Server context (state shared by handler functions)
struct ServerContext {
    documentation: serde_json::Value,
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

/// Response containing the API documentation.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct DocumentationResponse {
    /// A JSON object containing the OpenAPI definition for this API.
    documentation: serde_json::Value,
}

/// Generate OpenAPI documentation.
#[endpoint {
    method = GET,
    path = "/docs",
    tags = [ "docs" ],
}]
async fn docs(
    rqctx: Arc<RequestContext<ServerContext>>,
) -> Result<HttpResponseOk<DocumentationResponse>, HttpError> {
    Ok(HttpResponseOk(DocumentationResponse {
        documentation: rqctx.context().documentation.clone(),
    }))
}

/// Returns list of objects owned by an address.
#[endpoint {
    method = GET,
    path = "/api/objects",
    tags = [ "API" ],
}]
async fn get_objects(
    ctx: Arc<RequestContext<ServerContext>>,
    query: Query<GetObjectsRequest>,
) -> Result<HttpResponseOk<ObjectResponse>, HttpError> {
    let gateway = ctx.context().gateway.lock().await;
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
        .await
        .unwrap()
        .into_iter()
        .map(NamedObjectRef::from)
        .collect();

    Ok(HttpResponseOk(ObjectResponse { objects }))
}

/// Returns the schema for a specified object.
#[endpoint {
    method = GET,
    path = "/object_schema",
    tags = [ "API" ],
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

/// Returns the object information for a specified object.
#[endpoint {
    method = GET,
    path = "/api/object_info",
    tags = [ "API" ],
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

/// Transfer object from one address to another. Gas will be paid using the gas
/// provided in the request. This will be done through a native transfer
/// transaction that does not require Move VM executions, hence is much cheaper.
///
/// Notes:
/// - Non-coin objects cannot be transferred natively and will require a Move call
///
/// Example TransferTransactionRequest
/// {
///     "from_address": "1DA89C9279E5199DDC9BC183EB523CF478AB7168",
///     "object_id": "4EED236612B000B9BEBB99BA7A317EFF27556A0C",
///     "to_address": "5C20B3F832F2A36ED19F792106EC73811CB5F62C",
///     "gas_object_id": "96ABE602707B343B571AAAA23E3A4594934159A5"
/// }
#[endpoint {
    method = POST,
    path = "/api/new_transfer",
    tags = [ "API" ],
}]
async fn new_transfer(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<TransferTransactionRequest>,
) -> Result<HttpResponseOk<TransactionBytes>, HttpError> {
    let gateway = ctx.context().gateway.lock().await;
    let request = request.into_inner();

    let tx_data = async {
        let to_address = decode_bytes_hex(request.to_address.as_str())?;
        let object_id = ObjectID::try_from(request.object_id)?;
        let gas_object_id = ObjectID::try_from(request.gas_object_id)?;
        let owner = decode_bytes_hex(request.from_address.as_str())?;
        gateway
            .transfer_coin(
                owner,
                object_id,
                gas_object_id,
                request.gas_budget,
                to_address,
            )
            .await
    }
    .await
    .map_err(|error| custom_http_error(StatusCode::BAD_REQUEST, error.to_string()))?;

    Ok(HttpResponseOk(TransactionBytes::new(tx_data)))
}

#[endpoint {
method = POST,
path = "/api/split_coin",
tags = [ "API" ],
}]
async fn split_coin(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<SplitCoinRequest>,
) -> Result<HttpResponseOk<TransactionBytes>, HttpError> {
    let gateway = ctx.context().gateway.lock().await;
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

#[endpoint {
method = POST,
path = "/api/merge_coin",
tags = [ "API" ],
}]
async fn merge_coin(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<MergeCoinRequest>,
) -> Result<HttpResponseOk<TransactionBytes>, HttpError> {
    let gateway = ctx.context().gateway.lock().await;
    let request = request.into_inner();

    let tx_data = async {
        let signer = decode_bytes_hex(request.signer.as_str())?;
        let primary_coin = ObjectID::try_from(request.primary_coin)?;
        let coin_to_merge = ObjectID::try_from(request.coin_to_merge)?;
        let gas_payment = ObjectID::try_from(request.gas_payment)?;
        gateway
            .merge_coins(
                signer,
                primary_coin,
                coin_to_merge,
                gas_payment,
                request.gas_budget,
            )
            .await
    }
    .await
    .map_err(|error| custom_http_error(StatusCode::BAD_REQUEST, error.to_string()))?;
    Ok(HttpResponseOk(TransactionBytes::new(tx_data)))
}

/// Publish move module. It will perform proper verification and linking to make
/// sure the package is valid. If some modules have initializers, these initializers
/// will also be executed in Move (which means new Move objects can be created in
/// the process of publishing a Move package). Gas budget is required because of the
/// need to execute module initializers.
#[endpoint {
    method = POST,
    path = "/api/publish",
    tags = [ "API" ],
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

/// Execute a Move call transaction by calling the specified function in the
/// module of the given package. Arguments are passed in and type will be
/// inferred from function signature. Gas usage is capped by the gas_budget.
///
/// Example CallRequest
/// {
/// "sender": "b378b8d26c4daa95c5f6a2e2295e6e5f34371c1659e95f572788ffa55c265363",
/// "package_object_id": "0x2",
/// "module": "ObjectBasics",
/// "function": "create",
/// "args": [
///     200,
///     "b378b8d26c4daa95c5f6a2e2295e6e5f34371c1659e95f572788ffa55c265363"
/// ],
/// "gas_object_id": "1AC945CA31E77991654C0A0FCA8B0FD9C469B5C6",
/// "gas_budget": 2000
/// }
#[endpoint {
    method = POST,
    path = "/api/move_call",
    tags = [ "API" ],
}]
async fn move_call(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<CallRequest>,
) -> Result<HttpResponseOk<TransactionBytes>, HttpError> {
    let mut gateway = ctx.context().gateway.lock().await;

    let call_params = request.into_inner();
    let data = handle_move_call(call_params, &mut gateway)
        .await
        .map_err(|err| custom_http_error(StatusCode::BAD_REQUEST, format!("{:#}", err)))?;

    Ok(HttpResponseOk(TransactionBytes::new(data)))
}

/// Synchronize client state with authorities. This will fetch the latest information
/// on all objects owned by each address that is managed by this client state.
#[endpoint {
    method = POST,
    path = "/api/sync_account_state",
    tags = [ "API" ],
}]
async fn sync_account_state(
    ctx: Arc<RequestContext<ServerContext>>,
    request: TypedBody<SyncRequest>,
) -> Result<HttpResponseUpdatedNoContent, HttpError> {
    let sync_params = request.into_inner();
    let gateway = ctx.context().gateway.lock().await;

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

/// Synchronize client state with authorities. This will fetch the latest information
/// on all objects owned by each address that is managed by this client state.
#[endpoint {
method = POST,
path = "/api/execute_transaction",
tags = [ "API" ],
}]
async fn execute_transaction(
    ctx: Arc<RequestContext<ServerContext>>,
    response: TypedBody<SignedTransaction>,
) -> Result<HttpResponseOk<JsonResponse<TransactionResponse>>, HttpError> {
    let response = response.into_inner();
    let gateway = ctx.context().gateway.lock().await;

    let response: Result<_, anyhow::Error> = async {
        let data = base64::decode(response.tx_bytes)?;
        let data = TransactionData::from_signable_bytes(&data)?;

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
                format!("Error while getting object info: {error:?}"),
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
        .map(base64::decode)
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
    let type_args = call_params
        .type_arguments
        .unwrap_or_default()
        .iter()
        .map(|type_arg| parse_type_tag(type_arg))
        .collect::<Result<Vec<_>, _>>()?;
    let gas_budget = call_params.gas_budget;
    let gas_object_id = ObjectID::try_from(call_params.gas_object_id)?;
    let package_object_id = ObjectID::try_from(call_params.package_object_id)?;

    let sender: SuiAddress = decode_bytes_hex(call_params.signer.as_str())?;

    let (package_object_ref, _, _) = get_object_info(gateway, package_object_id).await?;

    // Fetch the object info for the gas obj
    let (gas_obj_ref, _, _) = get_object_info(gateway, gas_object_id).await?;

    // Fetch the objects for the object args
    let mut object_args_refs = Vec::new();
    for obj_id in call_params.object_arguments {
        let (object_ref, _, _) = get_object_info(gateway, ObjectID::try_from(obj_id)?).await?;
        object_args_refs.push(object_ref);
    }
    let pure_arguments = call_params
        .pure_arguments
        .iter()
        .map(base64::decode)
        .collect::<Result<_, _>>()?;

    let shared_object_arguments = call_params
        .shared_object_arguments
        .into_iter()
        .map(ObjectID::try_from)
        .collect::<Result<_, _>>()?;

    gateway
        .move_call(
            sender,
            package_object_ref,
            module.to_owned(),
            function.to_owned(),
            type_args.clone(),
            gas_obj_ref,
            object_args_refs,
            shared_object_arguments,
            pure_arguments,
            gas_budget,
        )
        .await
}
