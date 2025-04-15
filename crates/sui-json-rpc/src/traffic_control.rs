// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::ConnectInfo;
use futures::FutureExt;
use jsonrpsee::server::middleware::rpc::RpcServiceT;
use jsonrpsee::types::{ErrorCode, ErrorObject, Id};
use jsonrpsee::MethodResponse;
use std::net::IpAddr;
use std::time::SystemTime;
use std::{net::SocketAddr, sync::Arc};
use sui_core::traffic_controller::{parse_ip, policies::TrafficTally, TrafficController};
use sui_json_rpc_api::TRANSACTION_EXECUTION_CLIENT_ERROR_CODE;
use sui_types::traffic_control::ClientIdSource;
use sui_types::traffic_control::Weight;
use tracing::error;

const TOO_MANY_REQUESTS_MSG: &str = "Too many requests";

#[derive(Clone)]
pub struct TrafficControllerService<S> {
    inner: S,
    traffic_controller: Option<Arc<TrafficController>>,
}

impl<S> TrafficControllerService<S> {
    pub fn new(service: S, traffic_controller: Option<Arc<TrafficController>>) -> Self {
        Self {
            inner: service,
            traffic_controller,
        }
    }
}

impl<'a, S> RpcServiceT<'a> for TrafficControllerService<S>
where
    S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
    S::Future: 'a,
{
    type Future = futures::future::BoxFuture<'a, jsonrpsee::MethodResponse>;

    fn call(&self, req: jsonrpsee::types::Request<'a>) -> Self::Future {
        let service = self.inner.clone();
        let traffic_controller = self.traffic_controller.clone();

        async move {
            if let Some(traffic_controller) = traffic_controller {
                let client = req.extensions().get::<IpAddr>().cloned();
                if let Err(response) = handle_traffic_req(&traffic_controller, &client).await {
                    response
                } else {
                    let response = service.call(req).await;
                    handle_traffic_resp(&traffic_controller, client, &response);
                    response
                }
            } else {
                service.call(req).await
            }
        }
        .boxed()
    }
}

async fn handle_traffic_req(
    traffic_controller: &TrafficController,
    client: &Option<IpAddr>,
) -> Result<(), MethodResponse> {
    if !traffic_controller.check(client, &None).await {
        // Entity in blocklist
        let err_obj =
            ErrorObject::borrowed(ErrorCode::ServerIsBusy.code(), TOO_MANY_REQUESTS_MSG, None);
        Err(MethodResponse::error(Id::Null, err_obj))
    } else {
        Ok(())
    }
}

fn handle_traffic_resp(
    traffic_controller: &TrafficController,
    client: Option<IpAddr>,
    response: &MethodResponse,
) {
    let error = response.as_error_code().map(ErrorCode::from);
    traffic_controller.tally(TrafficTally {
        direct: client,
        through_fullnode: None,
        error_info: error.map(|e| {
            let error_type = e.to_string();
            let error_weight = normalize(e);
            (error_weight, error_type)
        }),
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
        // e.g. invalid client signature
        ErrorCode::ServerError(i) if i == TRANSACTION_EXECUTION_CLIENT_ERROR_CODE => Weight::one(),
        _ => Weight::zero(),
    }
}

pub fn determine_client_ip<T>(
    client_id_source: ClientIdSource,
    request: &mut axum::http::Request<T>,
) {
    let headers = request.headers();
    let client = match client_id_source {
        ClientIdSource::SocketAddr => request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip()),
        ClientIdSource::XForwardedFor(num_hops) => {
            let do_header_parse = |header: &axum::http::HeaderValue| match header.to_str() {
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
                    parse_ip(client_ip)
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
    };

    if let Some(ip) = client {
        request.extensions_mut().insert(ip);
    }
}
