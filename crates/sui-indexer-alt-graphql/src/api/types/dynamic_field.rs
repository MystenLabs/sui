// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use anyhow::bail;
use async_graphql::Context;
use async_graphql::InputObject;
use async_graphql::Object;
use async_graphql::Union;
use async_graphql::connection::Connection;
use async_graphql::connection::Edge;
use async_trait::async_trait;
use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_value::MoveTypeLayout;
use move_core_types::language_storage::StructTag;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::TypeTag;
use sui_types::dynamic_field::DYNAMIC_FIELD_FIELD_STRUCT_NAME;
use sui_types::dynamic_field::DYNAMIC_FIELD_MODULE_NAME;
use sui_types::dynamic_field::DynamicFieldInfo;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::dynamic_field::derive_dynamic_field_id;
use sui_types::dynamic_field::visitor as DFV;
use tokio::sync::OnceCell;

use crate::api::scalars::base64::Base64;
use crate::api::scalars::big_int::BigInt;
use crate::api::scalars::id::Id;
use crate::api::scalars::owner_kind::OwnerKind;
use crate::api::scalars::sui_address::SuiAddress;
use crate::api::scalars::type_filter::TypeFilter;
use crate::api::scalars::type_filter::TypeInput;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::address;
use crate::api::types::address::Address;
use crate::api::types::balance::Balance;
use crate::api::types::balance::{self as balance};
use crate::api::types::move_object::MoveObject;
use crate::api::types::move_type::MoveType;
use crate::api::types::move_value::MoveValue;
use crate::api::types::object::CLive;
use crate::api::types::object::CVersion;
use crate::api::types::object::Object;
use crate::api::types::object::VersionFilter;
use crate::api::types::object::{self as object};
use crate::api::types::object_filter::ObjectFilter;
use crate::api::types::object_filter::ObjectFilterValidator as OFValidator;
use crate::api::types::owner::Owner;
use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::config::Limits;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::error::upcast;
use crate::pagination::Page;
use crate::scope::Scope;

pub(crate) struct DynamicField {
    pub(crate) super_: MoveObject,

    /// Dynamic field specific data, lazily loaded from the super object.
    native: OnceCell<Option<NativeField>>,
}

/// The product of deserializing the dynamic field's MoveObject contents.
pub(crate) struct NativeField {
    /// Whether the dynamic field is a dynamic field or a dynamic object field.
    pub(crate) kind: DynamicFieldType,

    /// The BCS-encoded bytes of the dynamic field's name.
    pub(crate) name_bytes: Vec<u8>,

    /// The type of the dynamic field's name, like 'u64' or '0x2::kiosk::Listing'. For dynamic
    /// object fields, this type is wrapped with `0x2::dynamic_object_field::Wrapper`.
    pub(crate) name_type: TypeTag,

    /// The BCS-encoded bytes of the dynamic field's value. For a dynamic object field, this is the
    /// object's ID.
    pub(crate) value_bytes: Vec<u8>,

    /// The type of the dynamic field's value, like 'u64' or '0x2::kiosk::Listing'. For dynamic
    /// object fields, this type is `ID` (and not relevant).
    pub(crate) value_type: TypeTag,

    /// The scope under which this dynamic field is fetched. This includes any version bounds.
    scope: Scope,
}

/// A description of a dynamic field's name.
///
/// Names can either be given as serialized `bcs` accompanied by its `type`, or as a Display v2 `literal` expression. Other combinations of inputs are not supported.
#[derive(InputObject)]
pub(crate) struct DynamicFieldName {
    /// The type of the dynamic field's name, like 'u64' or '0x2::kiosk::Listing'.
    pub(crate) type_: Option<TypeInput>,

    /// The Base64-encoded BCS serialization of the dynamic field's 'name'.
    pub(crate) bcs: Option<Base64>,

    /// The name represented as a Display v2 literal expression.
    pub(crate) literal: Option<String>,
}

/// The value of a dynamic field (`MoveValue`) or dynamic object field (`MoveObject`).
#[derive(Union)]
pub(crate) enum DynamicFieldValue {
    MoveObject(MoveObject),
    MoveValue(MoveValue),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Name literals cannot contain field accesses")]
    FieldAccess,

    #[error("Literal error: {0}")]
    Literal(#[from] sui_display::v2::FormatError),

    #[error("Name must specify either both 'type' and 'bcs', or 'literal'")]
    NameInput,

    #[error("Name literals cannot fetch other dynamic fields")]
    StoreAccess,
}

/// Dynamic fields are heterogenous fields that can be added or removed from an object at runtime. Their names are arbitrary Move values that have `copy`, `drop`, and `store`.
///
/// There are two sub-types of dynamic fields:
///
/// - Dynamic fields can store any value that has `store`. Objects stored in this kind of field will be considered wrapped (not accessible via its ID by external tools like explorers, wallets, etc. accessing storage).
/// - Dynamic object fields can only store objects (values that have the `key` ability, and an `id: UID` as its first field) that have `store`, but they will still be directly accessible off-chain via their ID after being attached as a field.
#[Object]
impl DynamicField {
    /// The dynamic field's globally unique identifier, which can be passed to `Query.node` to refetch it.
    pub(crate) async fn id(&self) -> Id {
        let a = self.super_.super_.super_.address;
        if let Some((v, d)) = self.super_.super_.version_digest {
            Id::DynamicFieldByRef(a, v, d)
        } else {
            Id::DynamicFieldByAddress(a)
        }
    }

    /// The DynamicField's ID.
    pub(crate) async fn address(&self, ctx: &Context<'_>) -> Result<SuiAddress, RpcError> {
        self.super_.address(ctx).await
    }

    /// Fetch the address as it was at a different root version, or checkpoint.
    ///
    /// If no additional bound is provided, the address is fetched at the latest checkpoint known to the RPC.
    pub(crate) async fn address_at(
        &self,
        ctx: &Context<'_>,
        root_version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Result<Option<Address>, RpcError<address::Error>> {
        self.super_.address_at(ctx, root_version, checkpoint).await
    }

    /// The version of this object that this content comes from.
    pub(crate) async fn version(&self, ctx: &Context<'_>) -> Option<Result<UInt53, RpcError>> {
        self.super_.version(ctx).await.ok()?
    }

    /// 32-byte hash that identifies the object's contents, encoded in Base58.
    pub(crate) async fn digest(&self, ctx: &Context<'_>) -> Option<Result<String, RpcError>> {
        self.super_.digest(ctx).await.ok()?
    }

    /// Fetch the total balance for coins with marker type `coinType` (e.g. `0x2::sui::SUI`), owned by this address.
    ///
    /// If the address does not own any coins of that type, a balance of zero is returned.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        coin_type: TypeInput,
    ) -> Option<Result<Balance, RpcError<balance::Error>>> {
        self.super_.balance(ctx, coin_type).await.ok()?
    }

    /// Total balance across coins owned by this address, grouped by coin type.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Option<Result<Connection<String, Balance>, RpcError<balance::Error>>> {
        self.super_
            .balances(ctx, first, after, last, before)
            .await
            .ok()?
    }

    /// The structured representation of the object's contents.
    pub(crate) async fn contents(&self, ctx: &Context<'_>) -> Result<Option<MoveValue>, RpcError> {
        self.super_.contents(ctx).await
    }

    /// The domain explicitly configured as the default SuiNS name for this address.
    pub(crate) async fn default_suins_name(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<String, RpcError>> {
        self.super_.default_suins_name(ctx).await.ok()?
    }

    /// Access a dynamic field on an object using its type and BCS-encoded name.
    ///
    /// Returns `null` if a dynamic field with that name could not be found attached to this object.
    pub(crate) async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError<Error>> {
        self.super_.dynamic_field(ctx, name).await
    }

    /// Dynamic fields owned by this object.
    ///
    /// Dynamic fields on wrapped objects can be accessed using `Address.dynamicFields`.
    pub(crate) async fn dynamic_fields(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CLive>,
        last: Option<u64>,
        before: Option<CLive>,
    ) -> Result<Option<Connection<String, DynamicField>>, RpcError<object::Error>> {
        self.super_
            .dynamic_fields(ctx, first, after, last, before)
            .await
    }

    /// Access a dynamic object field on an object using its type and BCS-encoded name.
    ///
    /// Returns `null` if a dynamic object field with that name could not be found attached to this object.
    pub(crate) async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError<Error>> {
        self.super_.dynamic_object_field(ctx, name).await
    }

    /// Whether this object can be transfered using the `TransferObjects` Programmable Transaction Command or `sui::transfer::public_transfer`.
    ///
    /// Both these operations require the object to have both the `key` and `store` abilities.
    pub(crate) async fn has_public_transfer(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<bool>, RpcError> {
        self.super_.has_public_transfer(ctx).await
    }

    /// Access dynamic fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic fields that is guaranteed to be the same length as `keys`. If a dynamic field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError<Error>> {
        self.super_.multi_get_dynamic_fields(ctx, keys).await
    }

    /// Access dynamic object fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic object fields that is guaranteed to be the same length as `keys`. If a dynamic object field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_object_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError<Error>> {
        self.super_.multi_get_dynamic_object_fields(ctx, keys).await
    }

    /// The Base64-encoded BCS serialize of this object, as a `MoveObject`.
    pub(crate) async fn move_object_bcs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Base64>, RpcError> {
        self.super_.move_object_bcs(ctx).await
    }

    /// Fetch the total balances keyed by coin types (e.g. `0x2::sui::SUI`) owned by this address.
    ///
    /// If the address does not own any coins of a given type, a balance of zero is returned for that type.
    pub(crate) async fn multi_get_balances(
        &self,
        ctx: &Context<'_>,
        keys: Vec<TypeInput>,
    ) -> Option<Result<Vec<Balance>, RpcError<balance::Error>>> {
        self.super_.multi_get_balances(ctx, keys).await.ok()?
    }

    /// The dynamic field's name, as a Move value.
    async fn name(&self, ctx: &Context<'_>) -> Option<Result<MoveValue, RpcError>> {
        async {
            let Some(native) = self.native(ctx).await? else {
                return Ok(None);
            };

            Ok(Some(MoveValue::new(
                MoveType::from_native(native.name_type.clone(), native.scope.clone()),
                native.name_bytes.clone(),
            )))
        }
        .await
        .transpose()
    }

    /// Fetch the object with the same ID, at a different version, root version bound, or checkpoint.
    pub(crate) async fn object_at(
        &self,
        ctx: &Context<'_>,
        version: Option<UInt53>,
        root_version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Option<Result<Object, RpcError<object::Error>>> {
        self.super_
            .object_at(ctx, version, root_version, checkpoint)
            .await
            .ok()?
    }

    /// The Base64-encoded BCS serialization of this object, as an `Object`.
    pub(crate) async fn object_bcs(&self, ctx: &Context<'_>) -> Option<Result<Base64, RpcError>> {
        self.super_.object_bcs(ctx).await.ok()?
    }

    /// Paginate all versions of this object after this one.
    pub(crate) async fn object_versions_after(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Option<Result<Connection<String, Object>, RpcError>> {
        self.super_
            .object_versions_after(ctx, first, after, last, before, filter)
            .await
            .ok()?
    }

    /// Paginate all versions of this object before this one.
    pub(crate) async fn object_versions_before(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CVersion>,
        last: Option<u64>,
        before: Option<CVersion>,
        filter: Option<VersionFilter>,
    ) -> Option<Result<Connection<String, Object>, RpcError>> {
        self.super_
            .object_versions_before(ctx, first, after, last, before, filter)
            .await
            .ok()?
    }

    /// Objects owned by this object, optionally filtered by type.
    pub(crate) async fn objects(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CLive>,
        last: Option<u64>,
        before: Option<CLive>,
        #[graphql(validator(custom = "OFValidator::allows_empty()"))] filter: Option<ObjectFilter>,
    ) -> Option<Result<Connection<String, MoveObject>, RpcError<object::Error>>> {
        self.super_
            .objects(ctx, first, after, last, before, filter)
            .await
            .ok()?
    }

    /// The object's owner kind.
    pub(crate) async fn owner(&self, ctx: &Context<'_>) -> Option<Result<Owner, RpcError>> {
        self.super_.owner(ctx).await.ok()?
    }

    /// The transaction that created this version of the object.
    pub(crate) async fn previous_transaction(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<Transaction, RpcError>> {
        self.super_.previous_transaction(ctx).await.ok()?
    }

    /// The SUI returned to the sponsor or sender of the transaction that modifies or deletes this object.
    pub(crate) async fn storage_rebate(
        &self,
        ctx: &Context<'_>,
    ) -> Option<Result<BigInt, RpcError>> {
        self.super_.storage_rebate(ctx).await.ok()?
    }

    /// The transactions that sent objects to this object.
    pub(crate) async fn received_transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
        filter: Option<TransactionFilter>,
    ) -> Option<Result<Connection<String, Transaction>, RpcError>> {
        self.super_
            .received_transactions(ctx, first, after, last, before, filter)
            .await
            .ok()?
    }

    /// The dynamic field's value, as a Move value for dynamic fields and as a MoveObject for dynamic object fields.
    async fn value(&self, ctx: &Context<'_>) -> Option<Result<DynamicFieldValue, RpcError>> {
        async {
            let Some(native) = self.native(ctx).await? else {
                return Ok(None);
            };

            if native.kind == DynamicFieldType::DynamicField {
                return Ok(Some(DynamicFieldValue::MoveValue(MoveValue::new(
                    MoveType::from_native(native.value_type.clone(), native.scope.clone()),
                    native.value_bytes.clone(),
                ))));
            }

            let address: SuiAddress = bcs::from_bytes(&native.value_bytes)
                .context("Failed to deserialize dynamic object field ID")?;

            let object = Object::latest(ctx, native.scope.clone(), address).await?;

            let Some(object) = object else {
                return Ok(None);
            };

            Ok(Some(DynamicFieldValue::MoveObject(MoveObject::from_super(
                object,
            ))))
        }
        .await
        .transpose()
    }
}

impl DynamicField {
    /// Create a dynamic field from a `MoveObject`, assuming (but not checking) that it is a
    /// dynamic field.
    pub(crate) fn from_super(super_: MoveObject) -> Self {
        Self {
            super_,
            native: OnceCell::new(),
        }
    }

    /// Create a dynamic field from an `Object`, after checking whether it is a dynamic field.
    pub(crate) async fn from_object(
        object: &Object,
        ctx: &Context<'_>,
    ) -> Result<Option<Self>, RpcError> {
        let Some(move_object) = MoveObject::from_object(object, ctx).await? else {
            return Ok(None);
        };

        Self::from_move_object(&move_object, ctx).await
    }

    /// Create a dynamic field from a `MoveObject`, after checking whether it is a dynamic field.
    pub(crate) async fn from_move_object(
        move_object: &MoveObject,
        ctx: &Context<'_>,
    ) -> Result<Option<Self>, RpcError> {
        let Some(native) = move_object.native(ctx).await?.as_ref() else {
            return Ok(None);
        };

        if !native.type_().is_dynamic_field() {
            return Ok(None);
        }

        Ok(Some(Self::from_super(move_object.clone())))
    }

    /// Look up a dynamic field owned by `parent`, with the given `kind` and `name`. `scope`
    /// includes checkpoint and version bounds that should be applied to the lookup.
    pub(crate) async fn by_name(
        ctx: &Context<'_>,
        scope: Scope,
        parent: SuiAddress,
        kind: DynamicFieldType,
        name: DynamicFieldName,
    ) -> Result<Option<Self>, RpcError<Error>> {
        match name {
            DynamicFieldName {
                type_: Some(type_),
                bcs: Some(bcs),
                literal: None,
            } => Self::by_serialized_name(ctx, scope, parent, kind, type_, bcs)
                .await
                .map_err(upcast),

            DynamicFieldName {
                literal: Some(literal),
                type_: None,
                bcs: None,
            } => Self::by_literal_name(ctx, scope, parent, kind, literal).await,

            _ => Err(bad_user_input(Error::NameInput)),
        }
    }

    /// Look up a dynamic field by its serialized name (type and BCS bytes).
    pub(crate) async fn by_serialized_name(
        ctx: &Context<'_>,
        scope: Scope,
        parent: SuiAddress,
        kind: DynamicFieldType,
        type_: TypeInput,
        bcs: Base64,
    ) -> Result<Option<Self>, RpcError> {
        use DynamicFieldType as DFT;

        let type_ = match kind {
            DFT::DynamicField => type_.0,
            DFT::DynamicObject => DynamicFieldInfo::dynamic_object_field_wrapper(type_.0).into(),
        };

        let field_id: SuiAddress = derive_dynamic_field_id(parent, &type_, &bcs.0)?.into();

        let object = Object::latest(ctx, scope.clone(), field_id).await?;

        let Some(object) = object else {
            return Ok(None);
        };

        let move_object = MoveObject::from_super(object);
        Ok(Some(DynamicField::from_super(move_object)))
    }

    /// Look up a dynamic field by its literal name (Display v2 expression).
    ///
    /// Literals don't support field accesses or dynamic loads, so the interpreter is supplied
    /// with a dummy store and root object.
    pub(crate) async fn by_literal_name(
        ctx: &Context<'_>,
        scope: Scope,
        parent: SuiAddress,
        kind: DynamicFieldType,
        literal: String,
    ) -> Result<Option<Self>, RpcError<Error>> {
        use DynamicFieldType as DFT;

        struct NopStore;

        #[async_trait]
        impl sui_display::v2::Store for NopStore {
            async fn object(
                &self,
                _: AccountAddress,
            ) -> anyhow::Result<Option<sui_display::v2::OwnedSlice>> {
                bail!("Dynamic loads not supported")
            }
        }

        let limits: &Limits = ctx.data()?;
        let limits = limits.display();

        let root = sui_display::v2::OwnedSlice {
            layout: MoveTypeLayout::Bool,
            bytes: bcs::to_bytes(&false).unwrap(),
        };

        let parsed =
            sui_display::v2::Name::parse(limits, &literal).map_err(|e| bad_user_input(e.into()))?;

        let interpreter = sui_display::v2::Interpreter::new(root, NopStore);

        let value = match parsed.eval(&interpreter).await {
            Ok(Some(value)) => value,
            Ok(None) => return Err(bad_user_input(Error::FieldAccess)),
            Err(sui_display::v2::FormatError::Store(_)) => {
                return Err(bad_user_input(Error::StoreAccess));
            }
            Err(e) => return Err(bad_user_input(e.into())),
        };

        let field_id: SuiAddress = match kind {
            DFT::DynamicField => value
                .derive_dynamic_field_id(parent)
                .context("Failed to derive dynamic field ID")?,

            DFT::DynamicObject => value
                .derive_dynamic_object_field_id(parent)
                .context("Failed to derive dynamic object field ID")?,
        }
        .into();

        let object = Object::latest(ctx, scope.clone(), field_id)
            .await
            .map_err(upcast)?;

        let Some(object) = object else {
            return Ok(None);
        };

        let move_object = MoveObject::from_super(object);
        Ok(Some(DynamicField::from_super(move_object)))
    }

    /// Paginate dynamic fields owned by a parent object
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        parent: SuiAddress,
        page: Page<CLive>,
    ) -> Result<Connection<String, DynamicField>, RpcError<object::Error>> {
        // Create a filter for dynamic fields: they are objects owned by the parent
        // with type 0x2::dynamic_field::Field
        let filter = ObjectFilter {
            owner_kind: Some(OwnerKind::Object),
            owner: Some(parent),
            type_: Some(TypeFilter::Type(StructTag {
                address: SUI_FRAMEWORK_ADDRESS,
                module: DYNAMIC_FIELD_MODULE_NAME.to_owned(),
                name: DYNAMIC_FIELD_FIELD_STRUCT_NAME.to_owned(),
                type_params: vec![],
            })),
        };

        let objects = Object::paginate_live(ctx, scope, page, filter).await?;
        let mut dynamic_fields = Connection::new(objects.has_previous_page, objects.has_next_page);

        for edge in objects.edges {
            let move_obj = MoveObject::from_super(edge.node);
            let dynamic_field = DynamicField::from_super(move_obj);
            dynamic_fields
                .edges
                .push(Edge::new(edge.cursor, dynamic_field));
        }

        Ok(dynamic_fields)
    }

    /// Get the native dynamic field data, loading it lazily if needed.
    pub(crate) async fn native(&self, ctx: &Context<'_>) -> Result<&Option<NativeField>, RpcError> {
        self.native
            .get_or_try_init(async || {
                let Some(value) = self.super_.contents(ctx).await? else {
                    return Ok(None);
                };

                let Some(layout) = value.type_.layout_impl().await? else {
                    return Ok(None);
                };

                let DFV::Field {
                    kind,
                    name_layout,
                    name_bytes,
                    value_layout,
                    value_bytes,
                    ..
                } = DFV::FieldVisitor::deserialize(&value.native, &layout)
                    .context("Failed to deserialize dynamic field")?;

                Ok(Some(NativeField {
                    kind,
                    name_bytes: name_bytes.to_owned(),
                    name_type: name_layout.into(),
                    value_bytes: value_bytes.to_owned(),
                    value_type: value_layout.into(),
                    scope: self.super_.super_.super_.scope.clone(),
                }))
            })
            .await
    }
}
