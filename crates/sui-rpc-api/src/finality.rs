// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Finality levels, and what a validator serves at each.
//!
//! A request names the least settled data it accepts in the
//! [`X_SUI_FINALITY_LEVEL`] header; a request without the header accepts only
//! certified data. A level is a minimum, so data more settled than the request
//! asks for always satisfies it.
//!
//! A validator's live state includes checkpoints whose transactions are still
//! executing, and the validator keeps no rpc indexes and no replaced object
//! versions. [`ValidatorRequestLayer`] therefore serves:
//!
//! - at either level: `GetServiceInfo` and `GetCheckpoint`, which read only
//!   certified checkpoints;
//! - at `executed` only: the subscription streams, which carry each checkpoint
//!   once the validator has executed it, usually before it is certified, and
//!   reads of live state, Display and `VerifySignature` included.
//!
//! It refuses every other `sui.rpc` method the server mounts with
//! `UNIMPLEMENTED` / `NOT_SERVED_HERE` (`no_rpc_indexes`), and a request at a
//! level its method is not served at with `UNIMPLEMENTED` / `NOT_SERVED_HERE`
//! (`uncertified_live_state`). The server does not mount
//! `TransactionExecutionService`. Responses of the methods served only at
//! `executed` carry no `x-sui-checkpoint-height`: the validator's certified
//! checkpoint height does not bound what they contain.

use std::collections::HashSet;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;

use http::HeaderMap;
use sui_rpc::headers::X_SUI_CHECKPOINT_HEIGHT;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_server::SERVICE_NAME as TRANSACTION_EXECUTION_SERVICE;
use tonic::Code;
use tower::Layer;
use tower::Service;

use crate::RpcError;
use crate::reader::NotServedHereCause;
use crate::reader::not_served_here;

/// Request header naming the least settled data a request accepts: `executed`
/// or `certified`.
pub const X_SUI_FINALITY_LEVEL: &str = "x-sui-finality-level";

/// How settled data must be for a request to accept it, from least to most
/// settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FinalityLevel {
    /// Executed locally, before its checkpoint is certified. How much of the
    /// checkpoint has executed depends on the method: a checkpoint stream
    /// waits for all of it, a point read sees the node's live state.
    Executed,
    /// In a checkpoint certified by a quorum of validators, and executed.
    Certified,
}

impl FinalityLevel {
    fn header_value(self) -> &'static str {
        match self {
            Self::Executed => "executed",
            Self::Certified => "certified",
        }
    }

    /// The level `headers` asks for: certified when the header is absent.
    fn from_headers(headers: &HeaderMap) -> Result<Self, RpcError> {
        let Some(value) = headers.get(X_SUI_FINALITY_LEVEL) else {
            return Ok(Self::Certified);
        };
        [Self::Executed, Self::Certified]
            .into_iter()
            .find(|level| value.as_bytes() == level.header_value().as_bytes())
            .ok_or_else(|| {
                RpcError::new(
                    Code::InvalidArgument,
                    format!(
                        "{X_SUI_FINALITY_LEVEL}: unknown level {value:?}; expected executed or \
                         certified"
                    ),
                )
            })
    }
}

/// Every `sui.rpc` method a validator serves, with the most settled level it
/// serves the method at.
const VALIDATOR_METHODS: &[(&str, FinalityLevel)] = &[
    (
        "/sui.rpc.v2.LedgerService/GetServiceInfo",
        FinalityLevel::Certified,
    ),
    (
        "/sui.rpc.v2.LedgerService/GetCheckpoint",
        FinalityLevel::Certified,
    ),
    (
        "/sui.rpc.v2.SignatureVerificationService/VerifySignature",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.SubscriptionService/SubscribeCheckpoints",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.SubscriptionService/SubscribeTransactions",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.SubscriptionService/SubscribeEvents",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.LedgerService/GetObject",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.LedgerService/BatchGetObjects",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.LedgerService/GetEpoch",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.LedgerService/GetProtocolConfig",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.MovePackageService/GetPackage",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.MovePackageService/GetDatatype",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.MovePackageService/GetFunction",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.NameService/LookupName",
        FinalityLevel::Executed,
    ),
    (
        "/sui.rpc.v2.NameService/ReverseLookupName",
        FinalityLevel::Executed,
    ),
];

/// Whether `path` names a method of `service`.
fn is_method_of(path: &str, service: &str) -> bool {
    path.strip_prefix('/')
        .and_then(|path| path.strip_prefix(service))
        .is_some_and(|method| method.starts_with('/'))
}

/// How settled the data a validator serves for a request is, or its refusal.
/// Requests outside `sui.rpc` (gRPC health and reflection, `/health`) have no
/// level and are always served. Requests for methods that are not in
/// `known_methods` or that the server does not mount have no level either, and
/// the router refuses them.
fn admit_on_validator(
    path: &str,
    headers: &HeaderMap,
    known_methods: &HashSet<String>,
) -> Result<Option<FinalityLevel>, RpcError> {
    if !path.starts_with("/sui.rpc.") {
        return Ok(None);
    }
    let requested = FinalityLevel::from_headers(headers)?;
    let Some(&(_, most_settled)) = VALIDATOR_METHODS.iter().find(|(method, _)| *method == path)
    else {
        if !known_methods.contains(path) || is_method_of(path, TRANSACTION_EXECUTION_SERVICE) {
            return Ok(None);
        }
        return Err(not_served_here(
            NotServedHereCause::NoRpcIndexes,
            format!("validators do not keep the rpc indexes {path} needs"),
        ));
    };
    if requested > most_settled {
        return Err(not_served_here(
            NotServedHereCause::UncertifiedLiveState,
            format!(
                "validators serve {path} at finality level {served}, not {requested}: their live \
                 state includes data the network has not certified yet; send \
                 `{X_SUI_FINALITY_LEVEL}: {served}`",
                served = most_settled.header_value(),
                requested = requested.header_value(),
            ),
        ));
    }
    Ok(Some(most_settled))
}

/// Admits requests to a validator's rpc server: see the module docs.
#[derive(Clone)]
pub(crate) struct ValidatorRequestLayer {
    /// Every gRPC method path in the server's descriptor sets.
    known_methods: Arc<HashSet<String>>,
}

impl ValidatorRequestLayer {
    pub(crate) fn new(known_methods: Arc<HashSet<String>>) -> Self {
        Self { known_methods }
    }
}

impl<S> Layer<S> for ValidatorRequestLayer {
    type Service = ValidatorRequests<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ValidatorRequests {
            inner,
            known_methods: self.known_methods.clone(),
        }
    }
}

/// See [`ValidatorRequestLayer`].
#[derive(Clone)]
pub(crate) struct ValidatorRequests<S> {
    inner: S,
    known_methods: Arc<HashSet<String>>,
}

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for ValidatorRequests<S>
where
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Default,
{
    type Response = http::Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: http::Request<ReqBody>) -> Self::Future {
        let level = match admit_on_validator(
            request.uri().path(),
            request.headers(),
            &self.known_methods,
        ) {
            Ok(level) => level,
            Err(error) => {
                return Box::pin(async move { Ok(tonic::Status::from(error).into_http()) });
            }
        };

        // Only the service `poll_ready` readied may be called, so move that one into the future
        // and leave a fresh clone behind to be readied for the next request. See
        // https://docs.rs/tower/latest/tower/trait.Service.html#be-careful-when-cloning-inner-services
        let not_ready_inner = self.inner.clone();
        let mut ready_inner = std::mem::replace(&mut self.inner, not_ready_inner);
        Box::pin(async move {
            let mut response = ready_inner.call(request).await?;
            if level.is_some_and(|level| level < FinalityLevel::Certified) {
                response.headers_mut().remove(X_SUI_CHECKPOINT_HEIGHT);
            }
            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(level: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(level) = level {
            headers.insert(X_SUI_FINALITY_LEVEL, level.parse().unwrap());
        }
        headers
    }

    #[test]
    fn header_names_the_least_settled_level_accepted() {
        let parse = |level| FinalityLevel::from_headers(&headers(level));
        assert_eq!(parse(None).unwrap(), FinalityLevel::Certified);
        assert_eq!(parse(Some("executed")).unwrap(), FinalityLevel::Executed);
        assert_eq!(parse(Some("certified")).unwrap(), FinalityLevel::Certified);
        for invalid in ["", "Certified", "ordered"] {
            assert_eq!(
                parse(Some(invalid)).unwrap_err().code(),
                tonic::Code::InvalidArgument
            );
        }
    }

    fn sui_rpc_v2_methods() -> HashSet<String> {
        crate::grpc_method_paths_from_file_descriptor_sets(&[
            sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET,
        ])
        .unwrap()
    }

    #[test]
    fn validator_methods_exist() {
        let methods = sui_rpc_v2_methods();
        for (method, _) in VALIDATOR_METHODS {
            assert!(
                methods.contains(*method),
                "{method} is not a sui.rpc.v2 method"
            );
        }
    }

    #[test]
    fn validator_serves_each_method_up_to_its_level() {
        let known_methods = sui_rpc_v2_methods();
        let admit = |path, level| admit_on_validator(path, &headers(level), &known_methods);
        let assert_refused = |path, level, cause| {
            crate::reader::assert_not_served_here(admit(path, level).unwrap_err(), cause)
        };

        // Certified data at either level, so its responses keep the checkpoint
        // height.
        let get_checkpoint = "/sui.rpc.v2.LedgerService/GetCheckpoint";
        for level in [None, Some("certified"), Some("executed")] {
            assert_eq!(
                admit(get_checkpoint, level).unwrap(),
                Some(FinalityLevel::Certified)
            );
        }

        // Streams of checkpoints built before certification, and reads of live
        // state.
        for path in [
            "/sui.rpc.v2.SubscriptionService/SubscribeCheckpoints",
            "/sui.rpc.v2.LedgerService/GetObject",
            "/sui.rpc.v2.SignatureVerificationService/VerifySignature",
        ] {
            for level in [None, Some("certified")] {
                assert_refused(path, level, NotServedHereCause::UncertifiedLiveState);
            }
            assert_eq!(
                admit(path, Some("executed")).unwrap(),
                Some(FinalityLevel::Executed)
            );
        }

        // Methods that need data a validator does not keep are refused at every
        // level, saying which.
        for path in [
            "/sui.rpc.v2.LedgerService/GetTransaction",
            "/sui.rpc.v2.LedgerService/ListTransactions",
            "/sui.rpc.v2.StateService/ListOwnedObjects",
        ] {
            for level in [None, Some("executed")] {
                assert_refused(path, level, NotServedHereCause::NoRpcIndexes);
            }
        }

        // Methods of services the validator does not mount, and unknown
        // methods, go on to the router, which refuses them.
        for path in [
            "/sui.rpc.v2.TransactionExecutionService/ExecuteTransaction",
            "/sui.rpc.v2.LedgerService/NoSuchMethod",
        ] {
            assert_eq!(admit(path, Some("executed")).unwrap(), None);
        }

        // An unknown level is refused before the method is looked up.
        assert_eq!(
            admit("/sui.rpc.v2.LedgerService/GetTransaction", Some("final"))
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );

        // Requests outside sui.rpc have no level and ignore the header.
        assert_eq!(
            admit("/grpc.health.v1.Health/Check", Some("final")).unwrap(),
            None
        );
    }
}
