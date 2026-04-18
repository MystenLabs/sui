// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    env,
    io::Write,
    path::{Path, PathBuf},
};

use tonic_build::manual::{Builder, Method, Service};

type Result<T> = ::std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> Result<()> {
    let out_dir = if env::var("DUMP_GENERATED_GRPC").is_ok() {
        PathBuf::from("")
    } else {
        PathBuf::from(env::var("OUT_DIR")?)
    };

    let codec_path = "mysten_network::codec::BcsCodec";
    let prost_codec_path = "tonic_prost::ProstCodec";

    let package = "sui.validator";
    let service_name = "Validator";

    struct MethodDef {
        name: &'static str,
        route_name: &'static str,
        input_type: &'static str,
        output_type: &'static str,
        use_prost: bool,
    }

    let methods = &[
        MethodDef {
            name: "submit_transaction",
            route_name: "SubmitTransaction",
            input_type: "sui_types::messages_grpc::RawSubmitTxRequest",
            output_type: "sui_types::messages_grpc::RawSubmitTxResponse",
            use_prost: true,
        },
        MethodDef {
            name: "wait_for_effects",
            route_name: "WaitForEffects",
            input_type: "sui_types::messages_grpc::RawWaitForEffectsRequest",
            output_type: "sui_types::messages_grpc::RawWaitForEffectsResponse",
            use_prost: true,
        },
        MethodDef {
            name: "object_info",
            route_name: "ObjectInfo",
            input_type: "sui_types::messages_grpc::ObjectInfoRequest",
            output_type: "sui_types::messages_grpc::ObjectInfoResponse",
            use_prost: false,
        },
        MethodDef {
            name: "transaction_info",
            route_name: "TransactionInfo",
            input_type: "sui_types::messages_grpc::TransactionInfoRequest",
            output_type: "sui_types::messages_grpc::TransactionInfoResponse",
            use_prost: false,
        },
        MethodDef {
            name: "checkpoint",
            route_name: "Checkpoint",
            input_type: "sui_types::messages_checkpoint::CheckpointRequest",
            output_type: "sui_types::messages_checkpoint::CheckpointResponse",
            use_prost: false,
        },
        MethodDef {
            name: "checkpoint_v2",
            route_name: "CheckpointV2",
            input_type: "sui_types::messages_checkpoint::CheckpointRequestV2",
            output_type: "sui_types::messages_checkpoint::CheckpointResponseV2",
            use_prost: false,
        },
        MethodDef {
            name: "get_system_state_object",
            route_name: "GetSystemStateObject",
            input_type: "sui_types::messages_grpc::SystemStateRequest",
            output_type: "sui_types::sui_system_state::SuiSystemState",
            use_prost: false,
        },
        MethodDef {
            name: "validator_health",
            route_name: "ValidatorHealth",
            input_type: "sui_types::messages_grpc::RawValidatorHealthRequest",
            output_type: "sui_types::messages_grpc::RawValidatorHealthResponse",
            use_prost: true,
        },
    ];

    let mut service_builder = Service::builder()
        .name(service_name)
        .package(package)
        .comment("The Validator interface");
    for m in methods {
        service_builder = service_builder.method(
            Method::builder()
                .name(m.name)
                .route_name(m.route_name)
                .input_type(m.input_type)
                .output_type(m.output_type)
                .codec_path(if m.use_prost {
                    prost_codec_path
                } else {
                    codec_path
                })
                .build(),
        );
    }
    let validator_service = service_builder.build();

    Builder::new()
        .out_dir(&out_dir)
        .compile(&[validator_service]);
    inject_submit_transaction_debug(&out_dir)?;

    let route_names: Vec<&str> = methods.iter().map(|m| m.route_name).collect();
    generate_paths_constant(&out_dir, package, service_name, &route_names)?;

    build_anemo_services(&out_dir);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DUMP_GENERATED_GRPC");

    Ok(())
}

fn inject_submit_transaction_debug(out_dir: &Path) -> Result<()> {
    let path = out_dir.join("sui.validator.Validator.rs");
    let contents = std::fs::read_to_string(&path)?;
    let old = r#"        pub async fn submit_transaction(
            &mut self,
            request: impl tonic::IntoRequest<
                sui_types::messages_grpc::RawSubmitTxRequest,
            >,
        ) -> std::result::Result<
            tonic::Response<sui_types::messages_grpc::RawSubmitTxResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/sui.validator.Validator/SubmitTransaction",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("sui.validator.Validator", "SubmitTransaction"));
            self.inner.unary(req, path, codec).await
        }
"#;
    let new = r#"        pub async fn submit_transaction(
            &mut self,
            request: impl tonic::IntoRequest<
                sui_types::messages_grpc::RawSubmitTxRequest,
            >,
        ) -> std::result::Result<
            tonic::Response<sui_types::messages_grpc::RawSubmitTxResponse>,
            tonic::Status,
        > {
            use tokio_stream::StreamExt as _;

            tracing::info!("exec_tx_debug: ValidatorClient::submit_transaction before ready");
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    let status = tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    );
                    tracing::info!(
                        error = %status,
                        "exec_tx_debug: ValidatorClient::submit_transaction ready returned error"
                    );
                    status
                })?;
            tracing::info!("exec_tx_debug: ValidatorClient::submit_transaction after ready");

            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/sui.validator.Validator/SubmitTransaction",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("sui.validator.Validator", "SubmitTransaction"));

            let request = req.map(|m| tokio_stream::once(m));
            tracing::info!("exec_tx_debug: ValidatorClient::submit_transaction before streaming");
            let response = self.inner.streaming(request, path, codec).await;
            let (parts, body, extensions) = match response {
                Ok(response) => {
                    tracing::info!(
                        "exec_tx_debug: ValidatorClient::submit_transaction after streaming"
                    );
                    response.into_parts()
                }
                Err(status) => {
                    tracing::info!(
                        error = %status,
                        "exec_tx_debug: ValidatorClient::submit_transaction streaming returned error"
                    );
                    return Err(status);
                }
            };

            let mut body = std::pin::pin!(body);
            tracing::info!("exec_tx_debug: ValidatorClient::submit_transaction before try_next");
            let message = body
                .try_next()
                .await
                .map_err(|status| {
                    tracing::info!(
                        error = %status,
                        "exec_tx_debug: ValidatorClient::submit_transaction try_next returned error"
                    );
                    status
                })?
                .ok_or_else(|| {
                    let status = tonic::Status::internal("Missing response message.");
                    tracing::info!(
                        error = %status,
                        "exec_tx_debug: ValidatorClient::submit_transaction try_next missing response"
                    );
                    status
                })?;
            tracing::info!("exec_tx_debug: ValidatorClient::submit_transaction after try_next");

            tracing::info!("exec_tx_debug: ValidatorClient::submit_transaction before trailers");
            let _trailers = body.trailers().await.map_err(|status| {
                tracing::info!(
                    error = %status,
                    "exec_tx_debug: ValidatorClient::submit_transaction trailers returned error"
                );
                status
            })?;
            tracing::info!("exec_tx_debug: ValidatorClient::submit_transaction after trailers");

            Ok(tonic::Response::from_parts(parts, message, extensions))
        }
"#;

    if contents.contains(new) {
        return Ok(());
    }
    if !contents.contains(old) {
        return Err(format!("failed to find submit_transaction body in {}", path.display()).into());
    }

    std::fs::write(path, contents.replacen(old, new, 1))?;
    Ok(())
}

fn generate_paths_constant(
    out_dir: &Path,
    package: &str,
    service_name: &str,
    route_names: &[&str],
) -> Result<()> {
    let mut file = std::fs::File::create(out_dir.join("sui.validator.paths.rs"))?;
    writeln!(
        file,
        "pub const KNOWN_VALIDATOR_GRPC_PATHS_LIST: &[&str] = &["
    )?;
    for route in route_names {
        writeln!(file, "    \"/{package}.{service_name}/{route}\",")?;
    }
    writeln!(file, "];")?;
    Ok(())
}

fn build_anemo_services(out_dir: &Path) {
    let codec_path = "mysten_network::codec::anemo::BcsSnappyCodec";

    let discovery = anemo_build::manual::Service::builder()
        .name("Discovery")
        .package("sui")
        .method(
            anemo_build::manual::Method::builder()
                .name("get_known_peers_v2")
                .route_name("GetKnownPeersV2")
                .request_type("()")
                .response_type("crate::discovery::GetKnownPeersResponseV2")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_known_peers_v3")
                .route_name("GetKnownPeersV3")
                .request_type("crate::discovery::GetKnownPeersRequestV3")
                .response_type("crate::discovery::GetKnownPeersResponseV3")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    let state_sync = anemo_build::manual::Service::builder()
        .name("StateSync")
        .package("sui")
        .method(
            anemo_build::manual::Method::builder()
                .name("push_checkpoint_summary")
                .route_name("PushCheckpointSummary")
                .request_type("sui_types::messages_checkpoint::CertifiedCheckpointSummary")
                .response_type("()")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_checkpoint_summary")
                .route_name("GetCheckpointSummary")
                .request_type("crate::state_sync::GetCheckpointSummaryRequest")
                .response_type("Option<sui_types::messages_checkpoint::CertifiedCheckpointSummary>")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_checkpoint_contents")
                .route_name("GetCheckpointContents")
                .request_type("sui_types::messages_checkpoint::CheckpointContentsDigest")
                .response_type("Option<sui_types::messages_checkpoint::FullCheckpointContents>")
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_checkpoint_contents_v2")
                .route_name("GetCheckpointContentsV2")
                .request_type("sui_types::messages_checkpoint::CheckpointContentsDigest")
                .response_type(
                    "Option<sui_types::messages_checkpoint::VersionedFullCheckpointContents>",
                )
                .codec_path(codec_path)
                .build(),
        )
        .method(
            anemo_build::manual::Method::builder()
                .name("get_checkpoint_availability")
                .route_name("GetCheckpointAvailability")
                .request_type("()")
                .response_type("crate::state_sync::GetCheckpointAvailabilityResponse")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    let randomness = anemo_build::manual::Service::builder()
        .name("Randomness")
        .package("sui")
        .method(
            anemo_build::manual::Method::builder()
                .name("send_signatures")
                .route_name("SendSignatures")
                .request_type("crate::randomness::SendSignaturesRequest")
                .response_type("()")
                .codec_path(codec_path)
                .build(),
        )
        .build();

    anemo_build::manual::Builder::new()
        .out_dir(out_dir)
        .compile(&[discovery, state_sync, randomness]);
}
