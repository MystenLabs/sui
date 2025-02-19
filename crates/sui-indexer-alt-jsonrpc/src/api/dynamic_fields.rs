// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context as _};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiObjectResponse};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    base_types::ObjectID,
    dynamic_field::{derive_dynamic_field_id, DynamicFieldInfo, DynamicFieldName},
    error::SuiObjectResponseError,
    object::Object,
    TypeTag,
};
use tokio::try_join;

use crate::{
    context::Context,
    data::objects::load_latest,
    error::{invalid_params, rpc_bail, RpcError},
};

use super::{objects, rpc_module::RpcModule};

#[open_rpc(namespace = "suix", tag = "Dynamic Fields API")]
#[rpc(server, namespace = "suix")]
trait DynamicFieldsApi {
    /// Return the information from a dynamic field based on its parent ID and name.
    #[method(name = "getDynamicFieldObject")]
    async fn get_dynamic_field_object(
        &self,
        /// The ID of the parent object
        parent_object_id: ObjectID,
        /// The Name of the dynamic field
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse>;
}

pub struct DynamicFields(pub Context);

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Bad dynamic field name: {0}")]
    BadName(anyhow::Error),

    #[error("Invalid type {0}: {1}")]
    BadType(TypeTag, sui_package_resolver::error::Error),

    #[error("Could not serialize dynamic field name as {0}: {1}")]
    TypeMismatch(TypeTag, anyhow::Error),
}

#[async_trait::async_trait]
impl DynamicFieldsApiServer for DynamicFields {
    async fn get_dynamic_field_object(
        &self,
        parent_object_id: ObjectID,
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse> {
        let Self(ctx) = self;
        Ok(dynamic_field_object_response(ctx, parent_object_id, name).await?)
    }
}

impl RpcModule for DynamicFields {
    fn schema(&self) -> Module {
        DynamicFieldsApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

async fn dynamic_field_object_response(
    ctx: &Context,
    parent_object_id: ObjectID,
    name: DynamicFieldName,
) -> Result<SuiObjectResponse, RpcError<Error>> {
    let layout = ctx
        .package_resolver()
        .type_layout(name.type_.clone())
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
                | PRE::TypeArityMismatch(_, _) => {
                    invalid_params(Error::BadType(name.type_.clone(), e))
                }

                // These errors can be triggered by requesting a type whose layout is too large
                // (requires too may resources to resolve)
                PRE::TooManyTypeNodes(_, _)
                | PRE::TooManyTypeParams(_, _)
                | PRE::TypeParamNesting(_, _) => {
                    invalid_params(Error::BadType(name.type_.clone(), e))
                }

                // The other errors are a form of internal error.
                PRE::Bcs(_)
                | PRE::Store { .. }
                | PRE::Deserialize(_)
                | PRE::EmptyPackage(_)
                | PRE::FunctionNotFound(_, _, _)
                | PRE::InputTypeConflict(_, _, _)
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
        })?;

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
        objects::response::object(ctx, object, &options)
            .await
            .map_err(|e| match e {
                E::InvalidParams(e) => match e {},
                E::InternalError(e) => E::InternalError(e),
            })?,
    ))
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

    Ok(load_latest(ctx.loader(), id)
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

    let Some(object) = load_latest(ctx.loader(), id)
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

    Ok(load_latest(ctx.loader(), value)
        .await
        .context("Failed to load dynamic field object")?)
}
