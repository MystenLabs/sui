// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::bail;
use anyhow::Context as _;
use async_trait::async_trait;
use futures::future::OptionFuture;
use move_core_types::annotated_value::MoveTypeLayout;
use move_core_types::language_storage::StructTag;
use serde_json::Value as Json;
use sui_display::v1::Format;
use sui_indexer_alt_reader::displays::DisplayKey;
use sui_json_rpc_types::DisplayFieldsResponse;
use sui_json_rpc_types::SuiData;
use sui_json_rpc_types::SuiObjectData;
use sui_json_rpc_types::SuiObjectDataOptions;
use sui_json_rpc_types::SuiObjectResponse;
use sui_json_rpc_types::SuiParsedData;
use sui_json_rpc_types::SuiPastObjectResponse;
use sui_json_rpc_types::SuiRawData;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectType;
use sui_types::base_types::SequenceNumber;
use sui_types::display::DisplayVersionUpdatedEvent;
use sui_types::display_registry;
use sui_types::error::SuiObjectResponseError;
use sui_types::object::Data;
use sui_types::object::Object;
use sui_types::TypeTag;
use tokio::join;

use crate::context::Context;
use crate::data::load_live;
use crate::error::rpc_bail;
use crate::error::InternalContext;
use crate::error::RpcError;

struct DisplayStore<'c> {
    ctx: &'c Context,
}

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
            };
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
) -> anyhow::Result<BTreeMap<String, anyhow::Result<Json>>> {
    let Some(object) = object.data.try_as_move() else {
        bail!("Display is only supported for Move objects");
    };

    let config = &ctx.config().objects;
    let type_: StructTag = object.type_().clone().into();

    let layout = ctx.package_resolver().type_layout(type_.clone().into());
    let display_v1 = display_v1(ctx, &type_);
    let display_v2 = display_v2(ctx, &type_);

    let (layout, display_v1, display_v2) = join!(layout, display_v1, display_v2);

    let layout = layout.context("Failed to resolve type layout")?;

    if let Some(display_v2) = display_v2? {
        let store = DisplayStore::new(ctx);
        let root = sui_display::v2::OwnedSlice {
            bytes: object.contents().to_owned(),
            layout,
        };

        let interpreter = sui_display::v2::Interpreter::new(root, store);
        let fields = sui_display::v2::Display::parse(config.display(), display_v2.fields())?
            .display(
                ctx.config().package_resolver.max_move_value_depth,
                config.max_display_output_size,
                &interpreter,
            )
            .await?;

        Ok(fields
            .into_iter()
            .map(|(field, value)| (field, value.map_err(Into::into)))
            .collect())
    } else if let Some(display_v1) = display_v1? {
        let format = Format::parse(config.max_display_field_depth, &display_v1.fields)?;
        Ok(format
            .display(config.max_display_output_size, object.contents(), &layout)?
            .into_iter()
            .map(|(field, value)| (field, value.map(Json::String)))
            .collect())
    } else {
        bail!(
            "Display format not found for {}",
            type_.to_canonical_display(/*with_prefix */ true)
        );
    }
}

/// Try to load the V1 Display format for this type.
async fn display_v1(
    ctx: &Context,
    type_: &StructTag,
) -> anyhow::Result<Option<DisplayVersionUpdatedEvent>> {
    let Some(stored) = ctx
        .pg_loader()
        .load_one(DisplayKey(type_.clone()))
        .await
        .context("Failed to load Display v1")?
    else {
        return Ok(None);
    };

    let event: DisplayVersionUpdatedEvent =
        bcs::from_bytes(&stored.display).context("Failed to deserialize Display v1")?;

    Ok(Some(event))
}

/// Try to load the V2 Display format for this type.
async fn display_v2(
    ctx: &Context,
    type_: &StructTag,
) -> anyhow::Result<Option<display_registry::Display>> {
    let object_id = display_registry::display_object_id(type_.clone().into())
        .context("Failed to derive Display v2 object ID")?;

    let Some(object) = load_live(ctx, object_id)
        .await
        .context("Failed to fetch Display v2 object")?
    else {
        return Ok(None);
    };

    let Some(move_object) = object.data.try_as_move() else {
        return Ok(None);
    };

    let display = bcs::from_bytes(move_object.contents())
        .context("Failed to deserialize Display v2 object")?;
    Ok(Some(display))
}

impl<'c> DisplayStore<'c> {
    fn new(ctx: &'c Context) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl sui_display::v2::Store for DisplayStore<'_> {
    async fn object(
        &self,
        id: move_core_types::account_address::AccountAddress,
    ) -> anyhow::Result<Option<sui_display::v2::OwnedSlice>> {
        let Some(object) = load_live(self.ctx, id.into())
            .await
            .context("Failed to fetch object")?
        else {
            return Ok(None);
        };

        let Some(move_object) = object.data.try_as_move() else {
            return Ok(None);
        };

        let type_: TypeTag = move_object.type_().clone().into();
        let layout = self
            .ctx
            .package_resolver()
            .type_layout(type_)
            .await
            .context("Failed to resolve type layout")?;

        Ok(Some(sui_display::v2::OwnedSlice {
            layout,
            bytes: move_object.contents().to_owned(),
        }))
    }
}
