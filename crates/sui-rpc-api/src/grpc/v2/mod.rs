// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod ledger_service;
mod move_package_service;
mod name_service;
mod signature_verification_service;
mod state_service;
mod subscription_service;
mod transaction_execution_service;
pub use ledger_service::protocol_config_to_proto;

fn render_json(
    service: &crate::RpcService,
    struct_tag: &move_core_types::language_storage::StructTag,
    contents: &[u8],
) -> Option<prost_types::Value> {
    let layout = service
        .reader
        .inner()
        .get_struct_layout(struct_tag)
        .ok()
        .flatten()?;

    sui_types::proto_value::ProtoVisitorBuilder::new(service.config.max_json_move_value_size())
        .deserialize_value(contents, &layout)
        .map_err(|e| tracing::debug!("unable to convert move value to JSON: {e}"))
        .ok()
}

fn render_object_to_json(
    service: &crate::RpcService,
    object: &sui_types::object::Object,
) -> Option<prost_types::Value> {
    object.data.try_as_move().and_then(|move_object| {
        render_json(
            service,
            &move_object.type_().clone().into(),
            move_object.contents(),
        )
    })
}
