// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use anyhow::anyhow;
use async_graphql::Context;
use async_graphql::Name;
use async_graphql::Object;
use async_graphql::Value;
use async_graphql::dataloader::DataLoader;
use async_graphql::indexmap::IndexMap;
use async_trait::async_trait;
use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_value as A;
use move_core_types::annotated_visitor as AV;
use sui_indexer_alt_reader::displays::DisplayKey;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_types::TypeTag;
use sui_types::display::DisplayVersionUpdatedEvent;
use sui_types::id::ID;
use sui_types::id::UID;
use sui_types::object::option_visitor as OV;
use sui_types::object::rpc_visitor as RV;
use tokio::join;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::json::Json;
use crate::api::types::address::Address;
use crate::api::types::display::Display;
use crate::api::types::move_type::MoveType;
use crate::api::types::object::Object;
use crate::config::Limits;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::error::resource_exhausted;
use crate::error::upcast;
use crate::scope::Scope;

#[derive(Clone)]
pub(crate) struct MoveValue {
    pub(crate) type_: MoveType,
    pub(crate) native: Vec<u8>,
}

/// Store implementation that fetches objects for dynamic field/object field resolution during
/// path extraction. The Interpreter handles caching.
struct DisplayStore<'f, 'r> {
    ctx: &'f Context<'r>,
    scope: &'f Scope,
}

struct JsonVisitor {
    size_budget: usize,
    depth_budget: usize,
}

struct JsonWriter<'b> {
    size_budget: &'b mut usize,
    depth_budget: usize,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Format error: {0}")]
    Format(sui_display::v2::FormatError),

    #[error("Path error: {0}")]
    Path(sui_display::v2::FormatError),

    #[error("Extracted value is not a slice of existing on-chain data")]
    NotASlice,
}

#[derive(thiserror::Error, Debug)]
enum VisitorError {
    #[error(transparent)]
    Visitor(#[from] AV::Error),

    #[error("Unexpected type")]
    UnexpectedType,

    #[error("Value too big")]
    TooBig,

    #[error("Value too deep")]
    TooDeep,
}

#[Object]
impl MoveValue {
    /// Attempts to treat this value as an `Address`.
    ///
    /// If the value is of type `address` or `0x2::object::ID`, it is interpreted as an address pointer, and it is scoped to the current checkpoint.
    ///
    /// If the value is of type `0x2::object::UID`, it is interpreted as a wrapped object whose version is bounded by the root version of the current value. Such values do not support nested owned object queries, but `Address.addressAt` can be used to re-scope it to a checkpoint (defaults to the current checkpoint), instead of a root version, allowing owned object queries.
    ///
    /// Values of other types cannot be interpreted as addresses, and `null` is returned.
    async fn as_address(&self) -> Result<Option<Address>, RpcError> {
        use TypeTag as T;

        let Some(tag) = self.type_.to_type_tag() else {
            return Ok(None);
        };

        match tag {
            T::Address => {
                let address = bcs::from_bytes(&self.native)?;
                Ok(Some(Address::with_address(
                    self.type_.scope.without_root_bound(),
                    address,
                )))
            }

            T::Struct(s) if *s == ID::type_() => {
                let address = bcs::from_bytes(&self.native)?;
                Ok(Some(Address::with_address(
                    self.type_.scope.without_root_bound(),
                    address,
                )))
            }

            T::Struct(s) if *s == UID::type_() => {
                let address = bcs::from_bytes(&self.native)?;
                Ok(Some(Address::with_address(
                    self.type_.scope.clone(),
                    address,
                )))
            }

            _ => Ok(None),
        }
    }

    /// The BCS representation of this value, Base64-encoded.
    async fn bcs(&self) -> Option<Base64> {
        Some(Base64::from(self.native.clone()))
    }

    /// A rendered JSON blob based on an on-chain template, substituted with data from this value.
    ///
    /// Returns `null` if the value's type does not have an associated `Display` template.
    async fn display(&self, ctx: &Context<'_>) -> Option<Result<Display, RpcError>> {
        async {
            let limits: &Limits = ctx.data()?;
            let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

            let Some(TypeTag::Struct(type_)) = self.type_.to_type_tag() else {
                return Ok(None);
            };

            let (layout, display) = join!(
                self.type_.layout_impl(),
                pg_loader.load_one(DisplayKey(*type_)),
            );

            let (Some(layout), Some(display)) =
                (layout?, display.context("Failed to fetch Display")?)
            else {
                return Ok(None);
            };

            let event: DisplayVersionUpdatedEvent = bcs::from_bytes(&display.display)
                .context("Failed to deserialize DisplayVersionUpdatedEvent")?;

            let mut output = IndexMap::new();
            let mut errors = IndexMap::new();

            for (field, value) in
                sui_display::v1::Format::parse(limits.max_display_field_depth, &event.fields)
                    .map_err(resource_exhausted)?
                    .display(limits.max_display_output_size, &self.native, &layout)
                    .map_err(resource_exhausted)?
            {
                match value {
                    Ok(v) => {
                        output.insert(Name::new(&field), Value::String(v));
                    }

                    Err(e) => {
                        output.insert(Name::new(&field), Value::Null);
                        errors.insert(Name::new(&field), Value::String(e.to_string()));
                    }
                };
            }

            Ok(Some(Display {
                output: (!output.is_empty()).then(|| Json::from(Value::from(output))),
                errors: (!errors.is_empty()).then(|| Json::from(Value::from(errors))),
            }))
        }
        .await
        .transpose()
    }

    /// Extract a nested value at the given path.
    ///
    /// `path` is a Display v2 'chain' expression, allowing access to nested, named and positional fields, vector indices, VecMap keys, and dynamic (object) field accesses.
    async fn extract(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> Result<Option<MoveValue>, RpcError<Error>> {
        let limits: &Limits = ctx.data()?;
        let extract = sui_display::v2::Extract::parse(limits.display(), &path)
            .map_err(|e| format_error(Error::Path, e))?;

        let Some(layout) = self.type_.layout_impl().await.map_err(upcast)? else {
            return Ok(None);
        };

        // Create a store for dynamic field resolution
        let store = DisplayStore::new(ctx, &self.type_.scope);

        // Create an interpreter that combines the root value with the store
        let root = sui_display::v2::OwnedSlice {
            bytes: self.native.clone(),
            layout,
        };

        // Evaluate the extraction and convert to an owned slice
        let interpreter = sui_display::v2::Interpreter::new(root, store);
        let Some(value) = extract
            .extract(&interpreter)
            .await
            .map_err(|e| format_error(Error::Path, e))?
        else {
            return Ok(None);
        };

        let Some(sui_display::v2::OwnedSlice {
            layout,
            bytes: native,
        }) = value.into_owned_slice()
        else {
            return Err(bad_user_input(Error::NotASlice));
        };

        let type_ = MoveType::from_layout(layout, self.type_.scope.clone());
        Ok(Some(MoveValue { type_, native }))
    }

    /// Render a single Display v2 format string against this value.
    ///
    /// Returns `null` if the value does not have a valid type, or if any of the expressions in the format string fail to evaluate (e.g. field does not exist).
    async fn format(
        &self,
        ctx: &Context<'_>,
        format: String,
    ) -> Result<Option<Json>, RpcError<Error>> {
        let limits: &Limits = ctx.data()?;
        let parsed = sui_display::v2::Format::parse(limits.display(), &format)
            .map_err(|e| format_error(Error::Format, e))?;

        let Some(layout) = self.type_.layout_impl().await.map_err(upcast)? else {
            return Ok(None);
        };

        let store = DisplayStore::new(ctx, &self.type_.scope);
        let root = sui_display::v2::OwnedSlice {
            bytes: self.native.clone(),
            layout,
        };

        let interpreter = sui_display::v2::Interpreter::new(root, store);
        let value = parsed
            .format(
                &interpreter,
                limits.max_move_value_depth,
                limits.max_display_output_size,
            )
            .await
            .map_err(|e| format_error(Error::Format, e))?;

        Ok(Some(Json::try_from(value).map_err(upcast)?))
    }

    /// Representation of a Move value in JSON, where:
    ///
    /// - Addresses, IDs, and UIDs are represented in canonical form, as JSON strings.
    /// - Bools are represented by JSON boolean literals.
    /// - u8, u16, and u32 are represented as JSON numbers.
    /// - u64, u128, and u256 are represented as JSON strings.
    /// - Balances, Strings, and Urls are represented as JSON strings.
    /// - Vectors of bytes are represented as Base64 blobs, and other vectors are represented by JSON arrays.
    /// - Structs are represented by JSON objects.
    /// - Enums are represented by JSON objects, with a field named `@variant` containing the variant name.
    /// - Empty optional values are represented by `null`.
    async fn json(&self, ctx: &Context<'_>) -> Option<Result<Json, RpcError>> {
        async {
            let limits: &Limits = ctx.data()?;

            let Some(layout) = self.type_.layout_impl().await? else {
                return Ok(None);
            };

            let value = JsonVisitor::new(limits)
                .deserialize_value(&self.native, &layout)
                .map_err(|e| match &e {
                    VisitorError::Visitor(_) | VisitorError::UnexpectedType => anyhow!(e).into(),
                    VisitorError::TooBig | VisitorError::TooDeep => resource_exhausted(e),
                })?;

            Ok(Some(Json::try_from(value)?))
        }
        .await
        .transpose()
    }

    /// The value's type.
    async fn type_(&self) -> Option<MoveType> {
        Some(self.type_.clone())
    }
}

impl MoveValue {
    pub(crate) fn new(type_: MoveType, native: Vec<u8>) -> Self {
        Self { type_, native }
    }
}

impl<'f, 'r> DisplayStore<'f, 'r> {
    fn new(ctx: &'f Context<'r>, scope: &'f Scope) -> Self {
        Self { ctx, scope }
    }
}

impl JsonVisitor {
    fn new(limits: &Limits) -> Self {
        Self {
            size_budget: limits.max_move_value_bound,
            depth_budget: limits.max_move_value_depth,
        }
    }

    fn deserialize_value(
        &mut self,
        bytes: &[u8],
        layout: &A::MoveTypeLayout,
    ) -> Result<serde_json::Value, VisitorError> {
        A::MoveValue::visit_deserialize(
            bytes,
            layout,
            &mut RV::RpcVisitor::new(JsonWriter {
                size_budget: &mut self.size_budget,
                depth_budget: self.depth_budget,
            }),
        )
    }
}

impl JsonWriter<'_> {
    fn debit(&mut self, size: usize) -> Result<(), VisitorError> {
        if *self.size_budget < size {
            return Err(VisitorError::TooBig);
        }

        *self.size_budget -= size;
        Ok(())
    }
}

#[async_trait]
impl<'f, 'r> sui_display::v2::Store for DisplayStore<'f, 'r> {
    async fn object(
        &self,
        id: AccountAddress,
    ) -> anyhow::Result<Option<sui_display::v2::OwnedSlice>> {
        // NOTE: We can't use `anyhow::Context` here because `RpcError` doesn't implement
        // `std::error::Error`.
        let object = Object::latest(self.ctx, self.scope.clone(), id.into())
            .await
            .map_err(|e| anyhow!("Failed to fetch object: {e:?}"))?;

        let Some(object) = object else {
            return Ok(None);
        };

        let Some(native) = object
            .contents(self.ctx)
            .await
            .map_err(|e| anyhow!("Failed to get object contents: {e:?}"))?
        else {
            return Ok(None);
        };

        let Some(move_object) = native.data.try_as_move() else {
            return Ok(None);
        };

        let type_ = MoveType::from_native(
            move_object.type_().clone().into(),
            object.super_.scope.clone(),
        );

        let Some(layout) = type_
            .layout_impl()
            .await
            .map_err(|e| anyhow!("Failed to get layout: {e:?}"))?
        else {
            return Ok(None);
        };

        let bytes = move_object.contents().to_owned();
        Ok(Some(sui_display::v2::OwnedSlice { layout, bytes }))
    }
}

impl RV::Writer for JsonWriter<'_> {
    type Value = serde_json::Value;
    type Error = VisitorError;

    type Vec = Vec<serde_json::Value>;
    type Map = serde_json::Map<String, serde_json::Value>;

    type Nested<'b>
        = JsonWriter<'b>
    where
        Self: 'b;

    fn nest(&mut self) -> Result<Self::Nested<'_>, Self::Error> {
        if self.depth_budget == 0 {
            return Err(VisitorError::TooDeep);
        }

        Ok(JsonWriter {
            size_budget: self.size_budget,
            depth_budget: self.depth_budget - 1,
        })
    }

    fn write_null(&mut self) -> Result<Self::Value, Self::Error> {
        self.debit("null".len())?;
        Ok(serde_json::Value::Null)
    }

    fn write_bool(&mut self, value: bool) -> Result<Self::Value, Self::Error> {
        self.debit(if value { "true".len() } else { "false".len() })?;
        Ok(serde_json::Value::Bool(value))
    }

    fn write_number(&mut self, value: u32) -> Result<Self::Value, Self::Error> {
        self.debit(if value == 0 { 1 } else { value.ilog10() } as usize)?;
        Ok(serde_json::Value::Number(value.into()))
    }

    fn write_str(&mut self, value: String) -> Result<Self::Value, Self::Error> {
        // Account for the quotes around the string.
        self.debit(2 + value.len())?;
        Ok(serde_json::Value::String(value))
    }

    fn write_vec(&mut self, value: Self::Vec) -> Result<Self::Value, Self::Error> {
        // Account for the opening bracket.
        self.debit(1)?;
        Ok(serde_json::Value::Array(value))
    }

    fn write_map(&mut self, value: Self::Map) -> Result<Self::Value, Self::Error> {
        // Account for the opening brace.
        self.debit(1)?;
        Ok(serde_json::Value::Object(value))
    }

    fn vec_push_element(
        &mut self,
        vec: &mut Self::Vec,
        val: Self::Value,
    ) -> Result<(), Self::Error> {
        // Account for comma (or closing bracket).
        self.debit(1)?;
        vec.push(val);
        Ok(())
    }

    fn map_push_field(
        &mut self,
        map: &mut Self::Map,
        key: String,
        val: Self::Value,
    ) -> Result<(), Self::Error> {
        // Account for quotes, colon, and comma (or closing brace).
        self.debit(4 + key.len())?;
        map.insert(key, val);
        Ok(())
    }
}

impl From<OV::Error> for VisitorError {
    fn from(OV::Error: OV::Error) -> Self {
        VisitorError::UnexpectedType
    }
}

impl From<RV::Error> for VisitorError {
    fn from(RV::Error: RV::Error) -> Self {
        VisitorError::UnexpectedType
    }
}

fn format_error(
    wrap: impl FnOnce(sui_display::v2::FormatError) -> Error,
    e: sui_display::v2::FormatError,
) -> RpcError<Error> {
    use sui_display::v2::FormatError as FE;
    match &e {
        FE::InvalidHexCharacter(_)
        | FE::InvalidIdentifier(_)
        | FE::InvalidNumber { .. }
        | FE::OddHexLiteral(_)
        | FE::TransformInvalid(_)
        | FE::TransformInvalid_ { .. }
        | FE::UnexpectedEos { .. }
        | FE::UnexpectedRemaining(_)
        | FE::UnexpectedToken { .. }
        | FE::VectorArity { .. }
        | FE::VectorNoType
        | FE::VectorTypeMismatch { .. } => bad_user_input(wrap(e)),

        FE::TooBig | FE::TooDeep | FE::TooManyLoads | FE::TooMuchOutput => resource_exhausted(e),
        FE::Bcs(_) | FE::Visitor(_) | FE::Store(_) => anyhow!(e).into(),
    }
}
