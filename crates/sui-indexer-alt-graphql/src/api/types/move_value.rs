// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use anyhow::anyhow;
use async_graphql::Context;
use async_graphql::Name;
use async_graphql::Object;
use async_graphql::Value;
use async_graphql::connection::Connection;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::indexmap::IndexMap;
use async_trait::async_trait;
use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_value as A;
use move_core_types::annotated_visitor as AV;
use move_core_types::u256::U256;
use move_core_types::visitor_default;
use sui_types::TypeTag;
use sui_types::id::ID;
use sui_types::id::UID;
use sui_types::object::rpc_visitor as RV;
use tokio::join;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::cursor::JsonCursor;
use crate::api::scalars::json::Json;
use crate::api::types::address::Address;
use crate::api::types::display::Display;
use crate::api::types::display::display_v1;
use crate::api::types::display::display_v2;
use crate::api::types::move_type::MoveType;
use crate::api::types::object::Object;
use crate::config::Limits;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::error::resource_exhausted;
use crate::error::upcast;
use crate::pagination::Page;
use crate::pagination::PaginationConfig;
use crate::scope::Scope;

type CVector = JsonCursor<usize>;

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

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Display error: {0}")]
    Display(sui_display::v2::Error),

    #[error("Format error: {0}")]
    Format(sui_display::v2::FormatError),

    #[error("Path error: {0}")]
    Path(sui_display::v2::FormatError),

    #[error("Extracted value is not a slice of existing on-chain data")]
    NotASlice,
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
    async fn as_address(&self) -> Option<Result<Address, RpcError>> {
        use TypeTag as T;
        let tag = self.type_.to_type_tag()?;

        async {
            let address = match tag {
                T::Address => Address::with_address(
                    self.type_.scope.without_root_bound(),
                    bcs::from_bytes(&self.native)?,
                ),

                T::Struct(s) if *s == ID::type_() => Address::with_address(
                    self.type_.scope.without_root_bound(),
                    bcs::from_bytes(&self.native)?,
                ),

                T::Struct(s) if *s == UID::type_() => {
                    Address::with_address(self.type_.scope.clone(), bcs::from_bytes(&self.native)?)
                }

                _ => return Ok(None),
            };

            Ok(Some(address))
        }
        .await
        .transpose()
    }

    /// Attempts to treat this value as a `vector<T>` and paginate over its elements.
    ///
    /// Values of other types cannot be interpreted as vectors, and `null` is returned.
    async fn as_vector(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVector>,
        last: Option<u64>,
        before: Option<CVector>,
    ) -> Option<Result<Connection<String, MoveValue>, RpcError>> {
        struct Visitor<'p, 's> {
            page: &'p Page<CVector>,
            scope: &'s Scope,
        }

        impl AV::Visitor<'_, '_> for Visitor<'_, '_> {
            type Value = Option<Connection<String, MoveValue>>;
            type Error = AV::Error;

            visitor_default! { <'_, '_> u8, u16, u32, u64, u128, u256 = Ok(None) }
            visitor_default! { <'_, '_> bool, address, signer, struct, variant = Ok(None) }

            fn visit_vector(
                &mut self,
                driver: &mut AV::VecDriver<'_, '_, '_>,
            ) -> Result<Self::Value, Self::Error> {
                let mut conn = Connection::new(false, false);

                let total = driver.len() as usize;
                let Some(range) = self.page.range(total) else {
                    return Ok(Some(conn));
                };

                conn.has_previous_page = 0 < range.start;
                conn.has_next_page = range.end < total;

                let layout = driver.element_layout().clone();
                let type_ = MoveType::from_layout(layout, self.scope.clone());

                for i in 0..total {
                    let start = driver.position();
                    driver.skip_element()?;
                    let end = driver.position();

                    if range.contains(&i) {
                        conn.edges.push(Edge::new(
                            JsonCursor::new(i).encode_cursor(),
                            MoveValue::new(type_.clone(), driver.bytes()[start..end].to_vec()),
                        ));
                    }
                }

                Ok(Some(conn))
            }
        }

        async {
            let pagination: &PaginationConfig = ctx.data()?;
            let limits = pagination.limits("MoveValue", "asVector");
            let page = Page::from_params(limits, first, after, last, before)?;

            if !matches!(self.type_.to_type_tag(), Some(TypeTag::Vector(_))) {
                return Ok(None);
            }

            let Some(layout) = self.type_.layout_impl().await? else {
                return Ok(None);
            };

            let mut visitor = Visitor {
                page: &page,
                scope: &self.type_.scope,
            };

            Ok(
                A::MoveValue::visit_deserialize(&self.native, &layout, &mut visitor)
                    .context("Failed to deserialize vector")?,
            )
        }
        .await
        .transpose()
    }

    /// The BCS representation of this value, Base64-encoded.
    async fn bcs(&self) -> Option<Base64> {
        Some(Base64::from(self.native.clone()))
    }

    /// A rendered JSON blob based on an on-chain template, substituted with data from this value.
    ///
    /// Returns `null` if the value's type does not have an associated `Display` template.
    async fn display(&self, ctx: &Context<'_>) -> Option<Result<Display, RpcError<Error>>> {
        async {
            let limits: &Limits = ctx.data()?;

            let Some(TypeTag::Struct(type_)) = self.type_.to_type_tag() else {
                return Ok(None);
            };

            let (layout, display_v1, display_v2) = join!(
                self.type_.layout_impl(),
                display_v1(ctx, *type_.clone()),
                display_v2(ctx, self.type_.scope.clone(), *type_)
            );

            let Some(layout) = layout.map_err(upcast)? else {
                return Ok(None);
            };

            let mut output = IndexMap::new();
            let mut errors = IndexMap::new();

            if let Some(display_v2) = display_v2.map_err(upcast)? {
                let store = DisplayStore::new(ctx, &self.type_.scope);

                let root = sui_display::v2::OwnedSlice {
                    bytes: self.native.clone(),
                    layout,
                };

                let interpreter = sui_display::v2::Interpreter::new(root, store);

                for (field, value) in
                    sui_display::v2::Display::parse(limits.display(), display_v2.fields())
                        .map_err(display_error)?
                        .display::<serde_json::Value>(
                            limits.max_move_value_depth,
                            limits.max_display_output_size,
                            &interpreter,
                        )
                        .await
                        .map_err(display_error)?
                {
                    match value {
                        Ok(v) => {
                            output.insert(
                                Name::new(&field),
                                v.try_into().context("Failed to serialize JSON")?,
                            );
                        }

                        Err(e) => {
                            output.insert(Name::new(&field), Value::Null);
                            errors.insert(Name::new(&field), Value::String(e.to_string()));
                        }
                    }
                }
            } else if let Some(display_v1) = display_v1.map_err(upcast)? {
                for (field, value) in sui_display::v1::Format::parse(
                    limits.max_display_field_depth,
                    &display_v1.fields,
                )
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
            } else {
                return Ok(None);
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
    ) -> Option<Result<MoveValue, RpcError<Error>>> {
        async {
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
        .await
        .transpose()
    }

    /// Render a single Display v2 format string against this value.
    ///
    /// Returns `null` if the value does not have a valid type, or if any of the expressions in the format string fail to evaluate (e.g. field does not exist).
    async fn format(
        &self,
        ctx: &Context<'_>,
        format: String,
    ) -> Option<Result<Json, RpcError<Error>>> {
        async {
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
                .format::<serde_json::Value>(
                    &interpreter,
                    limits.max_move_value_depth,
                    limits.max_display_output_size,
                )
                .await
                .map_err(|e| format_error(Error::Format, e))?;

            Ok(Some(Json::try_from(value).map_err(upcast)?))
        }
        .await
        .transpose()
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
                    RV::Error::Meter(_) => resource_exhausted(e),
                    RV::Error::Visitor(_) | RV::Error::Option(_) | RV::Error::UnexpectedType => {
                        anyhow!(e).into()
                    }
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
    ) -> Result<serde_json::Value, RV::Error> {
        A::MoveValue::visit_deserialize(
            bytes,
            layout,
            &mut RV::RpcVisitor::new(RV::LocalMeter::new(
                &mut self.size_budget,
                self.depth_budget,
            )),
        )
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

fn display_error(e: sui_display::v2::Error) -> RpcError<Error> {
    if e.is_internal_error() {
        anyhow!(e).into()
    } else if e.is_resource_limit_error() {
        resource_exhausted(e)
    } else {
        bad_user_input(Error::Display(e))
    }
}

fn format_error(
    wrap: impl FnOnce(sui_display::v2::FormatError) -> Error,
    e: sui_display::v2::FormatError,
) -> RpcError<Error> {
    if e.is_internal_error() {
        anyhow!(e).into()
    } else if e.is_resource_limit_error() {
        resource_exhausted(e)
    } else {
        bad_user_input(wrap(e))
    }
}
