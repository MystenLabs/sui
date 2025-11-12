// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context as _, anyhow};
use sui_json::{MoveTypeLayout, SuiJsonValue};
use sui_json_rpc_types::{
    BcsName, DynamicFieldInfo as DynamicFieldInfoResponse, SuiMoveValue, SuiObjectDataOptions,
    SuiObjectResponse,
};
use sui_types::{
    TypeTag,
    base_types::ObjectID,
    dynamic_field::{DynamicFieldInfo, DynamicFieldName, derive_dynamic_field_id, visitor as DFV},
    error::SuiObjectResponseError,
    object::{Object, bounded_visitor::BoundedVisitor},
};
use tokio::try_join;

use crate::{
    api::objects,
    context::Context,
    data::load_live,
    error::{RpcError, invalid_params, rpc_bail},
};

use super::error::Error;

/// Fetch the latest version of a dynamic field object, identified by its parent ID and name.
pub(super) async fn dynamic_field_object(
    ctx: &Context,
    parent_object_id: ObjectID,
    name: DynamicFieldName,
) -> Result<SuiObjectResponse, RpcError<Error>> {
    let layout = resolve_type(ctx, &name.type_).await?;
    let bytes = SuiJsonValue::new(name.value)
        .map_err(|e| invalid_params(Error::BadName(e)))?
        .to_bcs_bytes(&layout)
        .map_err(|e| invalid_params(Error::TypeMismatch(name.type_.clone(), e)))?;

    let df = load_df(ctx, parent_object_id, &name.type_, &bytes);
    let dof = load_dof(ctx, parent_object_id, &name.type_, &bytes);
    let (df, dof) = try_join!(df, dof)
        .with_context(|| format!("Failed to fetch dynamic field on {parent_object_id}"))?;

    let Some(object) = df.or(dof) else {
        return Ok(SuiObjectResponse::new_with_error(
            SuiObjectResponseError::DynamicFieldNotFound { parent_object_id },
        ));
    };

    let options = SuiObjectDataOptions::full_content();

    use RpcError as E;
    Ok(SuiObjectResponse::new_with_data(
        objects::response::object_data_with_options(ctx, object, &options)
            .await
            .map_err(|e| match e {
                E::InvalidParams(e) => match e {},
                E::Timeout(e) => E::Timeout(e),
                E::InternalError(e) => E::InternalError(e),
            })?,
    ))
}

/// Fetch the latest version of the object identified by `object_id`, treat it as if it is a
/// `sui::dynamic_field::Field<K, V>`, and extract the name and value from it.
pub(super) async fn dynamic_field_info(
    ctx: &Context,
    object_id: ObjectID,
) -> Result<DynamicFieldInfoResponse, RpcError<Error>> {
    let object = load_live(ctx, object_id)
        .await
        .context("Failed to load dynamic field")?
        .context("Could not find latest content for dynamic field")?;

    let Some(move_object) = object.data.try_as_move() else {
        rpc_bail!("Dynamic field at {object_id} is not a Move Object");
    };

    let type_ = move_object.type_().clone().into();
    let layout = resolve_type(ctx, &type_).await?;

    let field = DFV::FieldVisitor::deserialize(move_object.contents(), &layout)
        .context("Failed to deserialize dynamic field info")?;

    let type_ = field.kind;
    let name_type: TypeTag = field.name_layout.into();
    let bcs_name = BcsName::Base64 {
        bcs_name: field.name_bytes.to_owned(),
    };

    let name_value = BoundedVisitor::deserialize_value(field.name_bytes, field.name_layout)
        .context("Failed to deserialize dynamic field name")?;

    let name = DynamicFieldName {
        type_: name_type,
        value: SuiMoveValue::from(name_value).to_json_value(),
    };

    let value_metadata = field
        .value_metadata()
        .context("Failed to extract dynamic field value metadata")?;

    Ok(match value_metadata {
        DFV::ValueMetadata::DynamicField(object_type) => DynamicFieldInfoResponse {
            name,
            bcs_name,
            type_,
            object_type: object_type.to_canonical_string(/* with_prefix */ true),
            object_id: object.id(),
            version: object.version(),
            digest: object.digest(),
        },

        DFV::ValueMetadata::DynamicObjectField(object_id) => {
            let object = load_live(ctx, object_id)
                .await
                .context("Failed to load dynamic object field value")?
                .context("Could not find latest content for dynamic object field value")?;

            let Some(object_type) = object.data.type_().cloned() else {
                rpc_bail!("Dynamic object field value at {object_id} is not a Move Object");
            };

            DynamicFieldInfoResponse {
                name,
                bcs_name,
                type_,
                object_type: object_type.to_canonical_string(/* with_prefix */ true),
                object_id: object.id(),
                version: object.version(),
                digest: object.digest(),
            }
        }
    })
}

/// Resolve the layout for a given type tag, using the package resolver in the context.
/// Re-interprets errors from the package resolver, categorizing them as RPC user errors or
/// internal errors.
async fn resolve_type(ctx: &Context, type_: &TypeTag) -> Result<MoveTypeLayout, RpcError<Error>> {
    ctx.package_resolver()
        .type_layout(type_.clone())
        .await
        .map_err(|e| {
            use sui_package_resolver::error::Error as PRE;
            match &e {
                // These errors can be triggered by passing a type that doesn't exist for the
                // dynamic field name.
                PRE::NotAPackage(_)
                | PRE::PackageNotFound(_)
                | PRE::ModuleNotFound(_, _)
                | PRE::DatatypeNotFound(_, _, _)
                | PRE::TypeArityMismatch(_, _) => invalid_params(Error::BadType(type_.clone(), e)),

                // These errors can be triggered by requesting a type whose layout is too large
                // (requires too may resources to resolve)
                PRE::TooManyTypeNodes(_, _)
                | PRE::TooManyTypeParams(_, _)
                | PRE::TypeParamNesting(_, _) => invalid_params(Error::BadType(type_.clone(), e)),

                // The other errors are a form of internal error.
                PRE::Bcs(_)
                | PRE::Store { .. }
                | PRE::Deserialize(_)
                | PRE::EmptyPackage(_)
                | PRE::FunctionNotFound(_, _, _)
                | PRE::LinkageNotFound(_)
                | PRE::NoTypeOrigin(_, _, _)
                | PRE::NotAnIdentifier(_)
                | PRE::TypeParamOOB(_, _)
                | PRE::UnexpectedReference
                | PRE::UnexpectedSigner
                | PRE::UnexpectedError(_)
                | PRE::ValueNesting(_) => {
                    RpcError::from(anyhow!(e).context("Failed to resolve type layout"))
                }
            }
        })
}

/// Try to load a dynamic field from `parent_id`, whose name has type `type_` and value `name` (as
/// BCS bytes). Fetches the `Field<K, V>` object from store.
async fn load_df(
    ctx: &Context,
    parent_id: ObjectID,
    type_: &TypeTag,
    value: &[u8],
) -> Result<Option<Object>, RpcError<Error>> {
    let id = derive_dynamic_field_id(parent_id, type_, value)
        .context("Failed to derive dynamic field ID")?;

    Ok(load_live(ctx, id)
        .await
        .context("Failed to load dynamic field")?)
}

/// Try to load a dynamic object field from `parent_id`, whose name has type `type_` and value
/// `name` (as BCS bytes). Fetches the object pointed to by the `Field<Wrapper<K>, ID>` object.
///
/// This function returns `None`, if the Field object does not exist in the store or does not have
/// contents.
async fn load_dof(
    ctx: &Context,
    parent_id: ObjectID,
    type_: &TypeTag,
    name: &[u8],
) -> Result<Option<Object>, RpcError<Error>> {
    let wrapper: TypeTag = DynamicFieldInfo::dynamic_object_field_wrapper(type_.clone()).into();
    let id = derive_dynamic_field_id(parent_id, &wrapper, name)
        .context("Failed to derive dynamic object field ID")?;

    let Some(object) = load_live(ctx, id)
        .await
        .context("Failed to load dynamic object field")?
    else {
        return Ok(None);
    };

    let Some(move_object) = object.data.try_as_move() else {
        rpc_bail!("Dynamic field at {id} is not a Move Object");
    };

    // Peel off the UID and the name from the serialized object. An ObjectID should be left.
    let value = ObjectID::from_bytes(&move_object.contents()[ObjectID::LENGTH + name.len()..])
        .context("Failed to extract object ID from dynamic object field")?;

    Ok(load_live(ctx, value)
        .await
        .context("Failed to load dynamic field object")?)
}
