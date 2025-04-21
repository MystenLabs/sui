// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::rpc::v2alpha::CommandOutput;
use crate::proto::rpc::v2alpha::CommandResult;
use crate::proto::rpc::v2alpha::ViewFunctionRequest;
use crate::proto::rpc::v2alpha::ViewFunctionResponse;
use crate::proto::rpc::v2beta::Bcs;
use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_protocol_config::ProtocolConfig;

use super::resolve::called_packages;
use super::resolve::resolve_ptb;

pub fn view_function(
    service: &RpcService,
    request: ViewFunctionRequest,
) -> Result<ViewFunctionResponse> {
    let executor = service
        .executor
        .as_ref()
        .ok_or_else(|| RpcError::new(tonic::Code::Unimplemented, "no transaction executor"))?;

    let ptb = request.view_functions.ok_or_else(|| {
        FieldViolation::new("view_functions").with_reason(ErrorReason::FieldMissing)
    })?;

    let commands = ptb
        .commands
        .iter()
        .map(sui_sdk_types::Command::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            FieldViolation::new("commands")
                .with_description(format!("invalid command: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    // TODO make this more efficient
    let protocol_config = {
        let system_state = service.reader.get_system_state_summary()?;

        let current_protocol_version = system_state.protocol_version;

        ProtocolConfig::get_for_version_if_supported(
            current_protocol_version.into(),
            service.reader.inner().get_chain_identifier()?.chain(),
        )
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                "unable to get current protocol config",
            )
        })?
    };

    let mut called_packages = called_packages(&service.reader, &protocol_config, &commands)?;
    let ptb = resolve_ptb(&service.reader, &mut called_packages, &ptb.inputs, commands)?;

    //TODO check commands and inputs and reject non move calls and gas input

    let execution_results = executor
        .view_function(ptb)
        .map_err(anyhow::Error::from)?
        .map_err(|e| RpcError::new(tonic::Code::InvalidArgument, e.to_string()))?;

    Ok(ViewFunctionResponse {
        outputs: execution_results
            .into_iter()
            .map(|(reference_outputs, return_values)| CommandResult {
                return_values: return_values
                    .into_iter()
                    .map(|(bcs, ty)| to_command_output(service, None, bcs, ty))
                    .collect(),
                mutated_by_ref: reference_outputs
                    .into_iter()
                    .map(|(arg, bcs, ty)| to_command_output(service, Some(arg), bcs, ty))
                    .collect(),
            })
            .collect(),
    })
}

fn to_command_output(
    service: &RpcService,
    arg: Option<sui_types::transaction::Argument>,
    bcs: Vec<u8>,
    ty: sui_types::TypeTag,
) -> CommandOutput {
    let json = service
        .reader
        .inner()
        .get_type_layout(&ty)
        .ok()
        .flatten()
        .and_then(|layout| {
            sui_types::proto_value::ProtoVisitor::new(service.config.max_json_move_value_size())
                .deserialize_value(&bcs, &layout)
                .map_err(|e| tracing::debug!("unable to convert to JSON: {e}"))
                .ok()
                .map(Box::new)
        });

    CommandOutput {
        argument: arg.map(Into::into),
        value: Some(Bcs {
            name: Some(ty.to_canonical_string(true)),
            value: Some(bcs.into()),
        }),
        json,
    }
}

impl From<sui_types::transaction::Argument> for crate::proto::rpc::v2beta::Argument {
    fn from(value: sui_types::transaction::Argument) -> Self {
        use crate::proto::rpc::v2beta::argument::ArgumentKind;
        use sui_types::transaction::Argument;

        let mut message = Self::default();

        let kind = match value {
            Argument::GasCoin => ArgumentKind::Gas,
            Argument::Input(input) => {
                message.index = Some(input.into());
                ArgumentKind::Input
            }
            Argument::Result(result) => {
                message.index = Some(result.into());
                ArgumentKind::Result
            }
            Argument::NestedResult(result, subresult) => {
                message.index = Some(result.into());
                message.subresult = Some(subresult.into());
                ArgumentKind::Result
            }
        };

        message.set_kind(kind);
        message
    }
}
