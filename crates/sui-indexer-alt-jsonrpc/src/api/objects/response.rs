// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt::Write};

use anyhow::{bail, Context as _};
use futures::future::OptionFuture;
use move_core_types::{annotated_value::MoveTypeLayout, language_storage::StructTag};
use sui_display::v1::Format;
use sui_indexer_alt_reader::displays::DisplayKey;
use sui_json_rpc_types::{
    DisplayFieldsResponse, SuiData, SuiObjectData, SuiObjectDataOptions, SuiObjectResponse,
    SuiParsedData, SuiPastObjectResponse, SuiRawData,
};
use sui_types::{
    base_types::{ObjectID, ObjectType, SequenceNumber},
    display::DisplayVersionUpdatedEvent,
    error::SuiObjectResponseError,
    object::{Data, Object},
    TypeTag,
};
use tokio::join;

use crate::{
    context::Context,
    data::load_live,
    error::{rpc_bail, InternalContext, RpcError},
};

/// Fetch the necessary data from the stores in `ctx` and transform it to build a response for a
/// the latest version of an object, identified by its ID, according to the response `options`.
pub(super) async fn live_object(
    ctx: &Context,
    object_id: ObjectID,
    options: &SuiObjectDataOptions,
) -> Result<SuiObjectResponse, RpcError> {
    let Some(object) = load_live(ctx, object_id)
        .await
        .context("Failed to load latest object")?
    else {
        return Ok(SuiObjectResponse::new_with_error(
            SuiObjectResponseError::NotExists { object_id },
        ));
    };

    Ok(SuiObjectResponse::new_with_data(
        object_data_with_options(ctx, object, options).await?,
    ))
}

/// Fetch the necessary data from the stores in `ctx` and transform it to build a response for a
/// past object identified by its ID and version, according to the response `options`.
pub(super) async fn past_object(
    ctx: &Context,
    object_id: ObjectID,
    version: SequenceNumber,
    options: &SuiObjectDataOptions,
) -> Result<SuiPastObjectResponse, RpcError> {
    let Some(object) = ctx
        .kv_loader()
        .load_one_object(object_id, version.value())
        .await
        .context("Failed to load object from store")?
    else {
        return Ok(SuiPastObjectResponse::VersionNotFound(object_id, version));
    };

    Ok(SuiPastObjectResponse::VersionFound(
        object_data_with_options(ctx, object, options).await?,
    ))
}

/// Extract a representation of the object according to its response options.
pub(crate) async fn object_data_with_options(
    ctx: &Context,
    object: Object,
    options: &SuiObjectDataOptions,
) -> Result<SuiObjectData, RpcError> {
    let type_ = options.show_type.then(|| ObjectType::from(&object));

    let owner = options.show_owner.then(|| object.owner().clone());

    let previous_transaction = options
        .show_previous_transaction
        .then(|| object.previous_transaction);

    let storage_rebate = options.show_storage_rebate.then(|| object.storage_rebate);

    let content: OptionFuture<_> = options
        .show_content
        .then(|| object_data::<SuiParsedData>(ctx, &object))
        .into();

    let bcs: OptionFuture<_> = options
        .show_bcs
        .then(|| object_data::<SuiRawData>(ctx, &object))
        .into();

    let display: OptionFuture<_> = options.show_display.then(|| display(ctx, &object)).into();

    let (content, bcs, display) = join!(content, bcs, display);

    let content = content
        .transpose()
        .internal_context("Failed to deserialize object content")?;

    let bcs = bcs
        .transpose()
        .internal_context("Failed to deserialize object to BCS")?;

    Ok(SuiObjectData {
        object_id: object.id(),
        version: object.version(),
        digest: object.digest(),
        type_,
        owner,
        previous_transaction,
        storage_rebate,
        display,
        content,
        bcs,
    })
}

/// Extract the contents of an object, in a format chosen by the `D` type parameter.
/// This operaton can fail if it's not possible to get the type layout for the object's type.
async fn object_data<D: SuiData>(ctx: &Context, object: &Object) -> Result<D, RpcError> {
    Ok(match object.data.clone() {
        Data::Package(move_package) => D::try_from_package(move_package)?,

        Data::Move(move_object) => {
            let type_: TypeTag = move_object.type_().clone().into();
            let MoveTypeLayout::Struct(layout) = ctx
                .package_resolver()
                .type_layout(type_.clone())
                .await
                .with_context(|| {
                    format!(
                        "Failed to resolve type layout for {}",
                        type_.to_canonical_display(/*with_prefix */ true)
                    )
                })?
            else {
                rpc_bail!(
                    "Type {} is not a struct",
                    type_.to_canonical_display(/*with_prefix */ true)
                );
            };

            D::try_from_object(move_object, *layout)?
        }
    })
}

/// Creates a response containing an object's Display fields. If this operation fails for any
/// reason, the value is captured in the response's error field, rather than using a `Result`, so
/// that the failure to generate a Display does not prevent the rest of the object's data from
/// being returned.
async fn display(ctx: &Context, object: &Object) -> DisplayFieldsResponse {
    let fields = match display_fields(ctx, object).await {
        Ok(fields) => fields,
        Err(e) => {
            return DisplayFieldsResponse {
                data: None,
                error: Some(SuiObjectResponseError::DisplayError {
                    error: format!("{e:#}"),
                }),
            }
        }
    };

    let mut field_values = BTreeMap::new();
    let mut field_errors = String::new();
    let mut prefix = "";

    for (name, value) in fields {
        match value {
            Ok(value) => {
                field_values.insert(name, value);
            }
            Err(e) => {
                write!(field_errors, "{prefix}Error for field {name:?}: {e:#}").unwrap();
                prefix = "; ";
            }
        }
    }

    DisplayFieldsResponse {
        data: Some(field_values),
        error: (!field_errors.is_empty()).then_some(SuiObjectResponseError::DisplayError {
            error: field_errors,
        }),
    }
}

/// Generate the Display fields for an object by fetching its latest Display format, parsing it,
/// and extracting values from the object's contents according to expressions in each field's
/// format string.
///
/// This operation can fail if the object is not a Move object, the Display format is not found, or
/// one of its fields fails to parse as a valid format string. Generating each field can also fail
/// if a field is nested too deeply, is not present, or has an invalid type for a format string.
async fn display_fields(
    ctx: &Context,
    object: &Object,
) -> anyhow::Result<BTreeMap<String, anyhow::Result<String>>> {
    let Some(object) = object.data.try_as_move() else {
        bail!("Display is only supported for Move objects");
    };

    let config = &ctx.config().objects;
    let type_: StructTag = object.type_().clone().into();

    let layout = ctx.package_resolver().type_layout(type_.clone().into());
    let display = ctx.pg_loader().load_one(DisplayKey(type_.clone()));

    let (layout, display) = join!(layout, display);

    let layout = layout.context("Failed to resolve type layout")?;
    let Some(stored) = display.context("Failed to load Display format")? else {
        bail!(
            "Display format not found for {}",
            type_.to_canonical_display(/*with_prefix */ true)
        );
    };

    let event: DisplayVersionUpdatedEvent =
        bcs::from_bytes(&stored.display).context("Failed to deserialize Display format")?;

    let format = Format::parse(config.max_display_field_depth, &event.fields)?;
    format.display(config.max_display_output_size, object.contents(), &layout)
}
