// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::IpAddr;
use std::time::SystemTime;
use std::{net::SocketAddr, sync::Arc};
use sui_types::traffic_control::RemoteFirewallConfig;

use axum::extract::{ConnectInfo, Json, State};
use futures::StreamExt;
use hyper::header::HeaderValue;
use hyper::HeaderMap;
use jsonrpsee::core::server::helpers::BoundedSubscriptions;
use jsonrpsee::core::server::helpers::MethodResponse;
use jsonrpsee::core::server::helpers::MethodSink;
use jsonrpsee::core::server::rpc_module::MethodKind;
use jsonrpsee::server::logger::{self, TransportProtocol};
use jsonrpsee::server::RandomIntegerIdProvider;
use jsonrpsee::types::error::{ErrorCode, BATCHES_NOT_SUPPORTED_CODE, BATCHES_NOT_SUPPORTED_MSG};
use jsonrpsee::types::{ErrorObject, Id, InvalidRequest, Params, Request};
use jsonrpsee::{core::server::rpc_module::Methods, server::logger::Logger};
use serde_json::value::RawValue;
use sui_core::traffic_controller::{
    metrics::TrafficControllerMetrics, policies::TrafficTally, TrafficController,
};
use sui_types::traffic_control::ClientIdSource;
use sui_types::traffic_control::{PolicyConfig, Weight};
use tracing::error;

use crate::routing_layer::RpcRouter;
use sui_json_rpc_api::CLIENT_TARGET_API_VERSION_HEADER;

pub const MAX_RESPONSE_SIZE: u32 = 2 << 30;
const TOO_MANY_REQUESTS_MSG: &str = "Too many requests";

#[derive(Clone, Debug)]
pub struct JsonRpcService<L> {
    logger: L,

    id_provider: Arc<RandomIntegerIdProvider>,

    /// Registered server methods.
    methods: Methods,
    rpc_router: RpcRouter,
    traffic_controller: Option<Arc<TrafficController>>,
    client_id_source: Option<ClientIdSource>,
}

impl<L> JsonRpcService<L> {
    pub fn new(
        methods: Methods,
        rpc_router: RpcRouter,
        logger: L,
        remote_fw_config: Option<RemoteFirewallConfig>,
        policy_config: Option<PolicyConfig>,
        traffic_controller_metrics: TrafficControllerMetrics,
    ) -> Self {
        Self {
            methods,
            rpc_router,
            logger,
            id_provider: Arc::new(RandomIntegerIdProvider),
            traffic_controller: policy_config.clone().map(|policy| {
                Arc::new(TrafficController::spawn(
                    policy,
                    traffic_controller_metrics,
                    remote_fw_config,
                ))
            }),
            client_id_source: policy_config.map(|policy| policy.client_id_source),
        }
    }
}

impl<L: Logger> JsonRpcService<L> {
    fn call_data(&self) -> CallData<'_, L> {
        CallData {
            logger: &self.logger,
            methods: &self.methods,
            rpc_router: &self.rpc_router,
            max_response_body_size: MAX_RESPONSE_SIZE,
            request_start: self.logger.on_request(TransportProtocol::Http),
        }
    }

    fn ws_call_data<'c, 'a: 'c, 'b: 'c>(
        &'a self,
        bounded_subscriptions: BoundedSubscriptions,
        sink: &'b MethodSink,
    ) -> ws::WsCallData<'c, L> {
        ws::WsCallData {
            logger: &self.logger,
            methods: &self.methods,
            max_response_body_size: MAX_RESPONSE_SIZE,
            request_start: self.logger.on_request(TransportProtocol::Http),
            bounded_subscriptions,
            id_provider: &*self.id_provider,
            sink,
        }
    }
}

/// Create a response body.
fn from_template<S: Into<hyper::Body>>(
    status: hyper::StatusCode,
    body: S,
    content_type: &'static str,
) -> hyper::Response<hyper::Body> {
    hyper::Response::builder()
        .status(status)
        .header(
            "content-type",
            hyper::header::HeaderValue::from_static(content_type),
        )
        .body(body.into())
        // Parsing `StatusCode` and `HeaderValue` is infalliable but
        // parsing body content is not.
        .expect("Unable to parse response body for type conversion")
}

/// Create a valid JSON response.
pub(crate) fn ok_response(body: String) -> hyper::Response<hyper::Body> {
    const JSON: &str = "application/json; charset=utf-8";
    from_template(hyper::StatusCode::OK, body, JSON)
}

pub async fn json_rpc_handler<L: Logger>(
    ConnectInfo(client_addr): ConnectInfo<SocketAddr>,
    State(service): State<JsonRpcService<L>>,
    headers: HeaderMap,
    Json(raw_request): Json<Box<RawValue>>,
) -> impl axum::response::IntoResponse {
    let headers_clone = headers.clone();
    // Get version from header.
    let api_version = headers
        .get(CLIENT_TARGET_API_VERSION_HEADER)
        .and_then(|h| h.to_str().ok());
    let response = process_raw_request(
        &service,
        api_version,
        raw_request.get(),
        client_addr,
        headers_clone,
    )
    .await;

    ok_response(response.result)
}

async fn process_raw_request<L: Logger>(
    service: &JsonRpcService<L>,
    api_version: Option<&str>,
    raw_request: &str,
    client_addr: SocketAddr,
    headers: HeaderMap,
) -> MethodResponse {
    let client = match service.client_id_source {
        Some(ClientIdSource::SocketAddr) => Some(client_addr.ip()),
        Some(ClientIdSource::XForwardedFor(num_hops)) => {
            let do_header_parse = |header: &HeaderValue| match header.to_str() {
                Ok(header_val) => {
                    let header_contents = header_val.split(',').map(str::trim).collect::<Vec<_>>();
                    if num_hops == 0 {
                        error!(
                                "x-forwarded-for: 0 specified. x-forwarded-for contents: {:?}. Please assign nonzero value for \
                                number of hops here, or use `socket-addr` client-id-source type if requests are not being proxied \
                                to this node. Skipping traffic controller request handling.",
                                header_contents,
                            );
                        return None;
                    }
                    let contents_len = header_contents.len();
                    let Some(client_ip) = header_contents.get(contents_len - num_hops) else {
                        error!(
                                "x-forwarded-for header value of {:?} contains {} values, but {} hops were specificed. \
                                Expected {} values. Skipping traffic controller request handling.",
                                header_contents,
                                contents_len,
                                num_hops,
                                num_hops + 1,
                            );
                        return None;
                    };
                    client_ip.parse::<IpAddr>().ok().or_else(|| {
                        client_ip.parse::<SocketAddr>().ok().map(|socket_addr| socket_addr.ip()).or_else(|| {
                                error!(
                                    "Failed to parse x-forwarded-for header value of {:?} to ip address or socket. \
                                    Please ensure that your proxy is configured to resolve client domains to an \
                                    IP address before writing header",
                                    client_ip,
                                );
                                None
                            })
                        })
                }
                Err(e) => {
                    error!("Invalid UTF-8 in x-forwarded-for header: {:?}", e);
                    None
                }
            };
            if let Some(header) = headers.get("x-forwarded-for") {
                do_header_parse(header)
            } else if let Some(header) = headers.get("X-Forwarded-For") {
                do_header_parse(header)
            } else {
                error!("x-forwarded-for header not present for request despite node configuring x-forwarded-for tracking type");
                None
            }
        }
        None => None,
    };
    if let Ok(request) = serde_json::from_str::<Request>(raw_request) {
        // check if either IP is blocked, in which case return early
        if let Some(traffic_controller) = &service.traffic_controller {
            if let Err(blocked_response) =
                handle_traffic_req(traffic_controller.clone(), &client).await
            {
                return blocked_response;
            }
        }

        // handle response tallying
        let response = process_request(request, api_version, service.call_data()).await;
        if let Some(traffic_controller) = &service.traffic_controller {
            handle_traffic_resp(traffic_controller.clone(), client, &response);
        }

        response
    } else if let Ok(_batch) = serde_json::from_str::<Vec<&RawValue>>(raw_request) {
        MethodResponse::error(
            Id::Null,
            ErrorObject::borrowed(BATCHES_NOT_SUPPORTED_CODE, &BATCHES_NOT_SUPPORTED_MSG, None),
        )
    } else {
        let (id, code) = prepare_error(raw_request);
        MethodResponse::error(id, ErrorObject::from(code))
    }
}

async fn handle_traffic_req(
    traffic_controller: Arc<TrafficController>,
    client: &Option<IpAddr>,
) -> Result<(), MethodResponse> {
    if !traffic_controller.check(client, &None).await {
        // Entity in blocklist
        let err_obj =
            ErrorObject::borrowed(ErrorCode::ServerIsBusy.code(), &TOO_MANY_REQUESTS_MSG, None);
        Err(MethodResponse::error(Id::Null, err_obj))
    } else {
        Ok(())
    }
}

fn handle_traffic_resp(
    traffic_controller: Arc<TrafficController>,
    client: Option<IpAddr>,
    response: &MethodResponse,
) {
    let error = response.error_code.map(ErrorCode::from);
    traffic_controller.tally(TrafficTally {
        direct: client,
        through_fullnode: None,
        error_weight: error.map(normalize).unwrap_or(Weight::zero()),
        // For now, count everything as spam with equal weight
        // on the rpc node side, including gas-charging endpoints
        // such as `sui_executeTransactionBlock`, as this can enable
        // node operators who wish to rate limit their transcation
        // traffic and incentivize high volume clients to choose a
        // suitable rpc provider (or run their own). Later we may want
        // to provide a weight distribution based on the method being called.
        spam_weight: Weight::one(),
        timestamp: SystemTime::now(),
    });
}

// TODO: refine error matching here
fn normalize(err: ErrorCode) -> Weight {
    match err {
        ErrorCode::InvalidRequest | ErrorCode::InvalidParams => Weight::one(),
        _ => Weight::zero(),
    }
}

async fn process_request<L: Logger>(
    req: Request<'_>,
    api_version: Option<&str>,
    call: CallData<'_, L>,
) -> MethodResponse {
    let CallData {
        methods,
        rpc_router,
        logger,
        max_response_body_size,
        request_start,
    } = call;
    let conn_id = 0; // unused

    let name = rpc_router.route(&req.method, api_version);
    let raw_params: Option<&RawValue> = req.params;
    let params = Params::new(raw_params.map(|params| params.get()));

    let id = req.id;

    let response = match methods.method_with_name(name) {
        None => {
            logger.on_call(
                name,
                params.clone(),
                logger::MethodKind::Unknown,
                TransportProtocol::Http,
            );
            MethodResponse::error(id, ErrorObject::from(ErrorCode::MethodNotFound))
        }
        Some((name, method)) => match method.inner() {
            MethodKind::Sync(callback) => {
                logger.on_call(
                    name,
                    params.clone(),
                    logger::MethodKind::MethodCall,
                    TransportProtocol::Http,
                );
                (callback)(id, params, max_response_body_size as usize)
            }
            MethodKind::Async(callback) => {
                logger.on_call(
                    name,
                    params.clone(),
                    logger::MethodKind::MethodCall,
                    TransportProtocol::Http,
                );

                let id = id.into_owned();
                let params = params.into_owned();
                (callback)(id, params, conn_id, max_response_body_size as usize, None).await
            }
            MethodKind::Subscription(_) | MethodKind::Unsubscription(_) => {
                logger.on_call(
                    name,
                    params.clone(),
                    logger::MethodKind::Unknown,
                    TransportProtocol::Http,
                );
                // Subscriptions not supported on HTTP
                MethodResponse::error(id, ErrorObject::from(ErrorCode::InternalError))
            }
        },
    };

    logger.on_result(
        name,
        response.success,
        response.error_code,
        request_start,
        TransportProtocol::Http,
    );
    response
}

/// Figure out if this is a sufficiently complete request that we can extract an [`Id`] out of, or just plain
/// unparsable garbage.
pub fn prepare_error(data: &str) -> (Id<'_>, ErrorCode) {
    match serde_json::from_str::<InvalidRequest>(data) {
        Ok(InvalidRequest { id }) => (id, ErrorCode::InvalidRequest),
        Err(_) => (Id::Null, ErrorCode::ParseError),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CallData<'a, L: Logger> {
    logger: &'a L,
    methods: &'a Methods,
    rpc_router: &'a RpcRouter,
    max_response_body_size: u32,
    request_start: L::Instant,
}

pub mod ws {
    use axum::{
        extract::{
            ws::{Message, WebSocket},
            WebSocketUpgrade,
        },
        response::Response,
    };
    use futures::channel::mpsc;
    use jsonrpsee::{
        core::server::{
            helpers::{BoundedSubscriptions, MethodSink},
            rpc_module::ConnState,
        },
        server::IdProvider,
        types::error::reject_too_many_subscriptions,
    };

    use super::*;

    #[derive(Debug, Clone)]
    pub(crate) struct WsCallData<'a, L: Logger> {
        pub bounded_subscriptions: BoundedSubscriptions,
        pub id_provider: &'a dyn IdProvider,
        pub methods: &'a Methods,
        pub max_response_body_size: u32,
        pub sink: &'a MethodSink,
        pub logger: &'a L,
        pub request_start: L::Instant,
    }

    // A WebSocket handler that echos any message it receives.
    //
    // This one we'll be integration testing so it can be written in the regular way.
    pub async fn ws_json_rpc_upgrade<L: Logger>(
        ws: WebSocketUpgrade,
        State(service): State<JsonRpcService<L>>,
    ) -> Response {
        ws.on_upgrade(|ws| ws_json_rpc_handler(ws, service))
    }

    async fn ws_json_rpc_handler<L: Logger>(mut socket: WebSocket, service: JsonRpcService<L>) {
        #[allow(clippy::disallowed_methods)]
        let (tx, mut rx) = mpsc::unbounded::<String>();
        let sink = MethodSink::new_with_limit(tx, MAX_RESPONSE_SIZE, MAX_RESPONSE_SIZE);
        let bounded_subscriptions = BoundedSubscriptions::new(100);

        loop {
            tokio::select! {
                maybe_message = socket.recv() => {
                    if let Some(Ok(message)) = maybe_message {
                        if let Message::Text(msg) = message {
                            let response =
                                process_raw_request(&service, &msg, bounded_subscriptions.clone(), &sink).await;
                            if let Some(response) = response {
                                let _ = sink.send_raw(response.result);
                            }
                        }
                    } else {
                        break;
                    }
                },
                Some(response) = rx.next() => {
                    if socket.send(Message::Text(response)).await.is_err() {
                        break;
                    }
                },
            }
        }
    }

    async fn process_raw_request<L: Logger>(
        service: &JsonRpcService<L>,
        raw_request: &str,
        bounded_subscriptions: BoundedSubscriptions,
        sink: &MethodSink,
    ) -> Option<MethodResponse> {
        if let Ok(request) = serde_json::from_str::<Request>(raw_request) {
            process_request(request, service.ws_call_data(bounded_subscriptions, sink)).await
        } else if let Ok(_batch) = serde_json::from_str::<Vec<&RawValue>>(raw_request) {
            Some(MethodResponse::error(
                Id::Null,
                ErrorObject::borrowed(BATCHES_NOT_SUPPORTED_CODE, &BATCHES_NOT_SUPPORTED_MSG, None),
            ))
        } else {
            let (id, code) = prepare_error(raw_request);
            Some(MethodResponse::error(id, ErrorObject::from(code)))
        }
    }

    async fn process_request<L: Logger>(
        req: Request<'_>,
        call: WsCallData<'_, L>,
    ) -> Option<MethodResponse> {
        let WsCallData {
            methods,
            logger,
            max_response_body_size,
            request_start,
            bounded_subscriptions,
            id_provider,
            sink,
        } = call;
        let conn_id = 0; // unused

        let params = Params::new(req.params.map(|params| params.get()));
        let name = &req.method;
        let id = req.id;

        let response = match methods.method_with_name(name) {
            None => {
                logger.on_call(
                    name,
                    params.clone(),
                    logger::MethodKind::Unknown,
                    TransportProtocol::Http,
                );
                Some(MethodResponse::error(
                    id,
                    ErrorObject::from(ErrorCode::MethodNotFound),
                ))
            }
            Some((name, method)) => match method.inner() {
                MethodKind::Sync(callback) => {
                    logger.on_call(
                        name,
                        params.clone(),
                        logger::MethodKind::MethodCall,
                        TransportProtocol::Http,
                    );
                    Some((callback)(id, params, max_response_body_size as usize))
                }
                MethodKind::Async(callback) => {
                    logger.on_call(
                        name,
                        params.clone(),
                        logger::MethodKind::MethodCall,
                        TransportProtocol::Http,
                    );

                    let id = id.into_owned();
                    let params = params.into_owned();

                    Some(
                        (callback)(id, params, conn_id, max_response_body_size as usize, None)
                            .await,
                    )
                }

                MethodKind::Subscription(callback) => {
                    logger.on_call(
                        name,
                        params.clone(),
                        logger::MethodKind::Subscription,
                        TransportProtocol::WebSocket,
                    );
                    if let Some(cn) = bounded_subscriptions.acquire() {
                        let conn_state = ConnState {
                            conn_id,
                            close_notify: cn,
                            id_provider,
                        };
                        callback(id.clone(), params, sink.clone(), conn_state, None).await;
                        None
                    } else {
                        Some(MethodResponse::error(
                            id,
                            reject_too_many_subscriptions(bounded_subscriptions.max()),
                        ))
                    }
                }

                MethodKind::Unsubscription(callback) => {
                    logger.on_call(
                        name,
                        params.clone(),
                        logger::MethodKind::Unsubscription,
                        TransportProtocol::WebSocket,
                    );

                    Some(callback(
                        id,
                        params,
                        conn_id,
                        max_response_body_size as usize,
                    ))
                }
            },
        };

        if let Some(response) = &response {
            logger.on_result(
                name,
                response.success,
                response.error_code,
                request_start,
                TransportProtocol::WebSocket,
            );
        }
        response
    }
}
