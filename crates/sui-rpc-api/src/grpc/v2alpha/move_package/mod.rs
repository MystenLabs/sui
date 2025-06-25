// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::{ErrorReason, Result, RpcError, RpcService};
use sui_types::{base_types::ObjectID, move_package::MovePackage};

pub mod conversions;
pub mod get_datatype;
pub mod get_function;
pub mod get_module;
pub mod get_package;

pub(crate) fn load_package(service: &RpcService, package_id_str: &str) -> Result<MovePackage> {
    let package_id = package_id_str.parse::<ObjectID>().map_err(|e| {
        FieldViolation::new("package_id")
            .with_description(format!("invalid package_id: {}", e))
            .with_reason(ErrorReason::FieldInvalid)
    })?;

    let object = service
        .reader
        .inner()
        .get_object(&package_id)
        .ok_or_else(RpcError::not_found)?;

    let inner = object.into_inner();
    inner
        .data
        .try_into_package()
        .ok_or_else(|| RpcError::new(tonic::Code::InvalidArgument, "object is not a package"))
}
