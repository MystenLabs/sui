// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::{
    connection::{Connection, Edge},
    Context, InputObject, Object, Union,
};
use move_core_types::language_storage::StructTag;
use sui_types::{
    dynamic_field::{
        derive_dynamic_field_id, visitor as DFV, DynamicFieldInfo, DynamicFieldType,
        DYNAMIC_FIELD_FIELD_STRUCT_NAME, DYNAMIC_FIELD_MODULE_NAME,
    },
    TypeTag, SUI_FRAMEWORK_ADDRESS,
};
use tokio::sync::OnceCell;

use crate::{
    api::scalars::{
        base64::Base64,
        big_int::BigInt,
        owner_kind::OwnerKind,
        sui_address::SuiAddress,
        type_filter::{TypeFilter, TypeInput},
        uint53::UInt53,
    },
    error::{upcast, RpcError},
    pagination::Page,
    scope::Scope,
};

use super::{
    balance::{self, Balance},
    move_object::MoveObject,
    move_type::MoveType,
    move_value::MoveValue,
    object::{self, CLive, CVersion, Object, ObjectKey, VersionFilter},
    object_filter::{ObjectFilter, Validator as OFValidator},
    owner::Owner,
    transaction::Transaction,
};

pub(crate) struct DynamicField {
    pub(crate) super_: MoveObject,

    /// Dynamic field specific data, lazily loaded from the super object.
    native: OnceCell<Option<NativeField>>,
}

/// The product of deserializing the dynamic field's MoveObject contents.
pub(crate) struct NativeField {
    /// Whether the dynamic field is a dynamic field or a dynamic object field.
    kind: DynamicFieldType,

    /// The BCS-encoded bytes of the dynamic field's name.
    name_bytes: Vec<u8>,

    /// The type of the dynamic field's name, like 'u64' or '0x2::kiosk::Listing'. For dynamic
    /// object fields, this type is wrapped with `0x2::dynamic_object_field::Wrapper`.
    name_type: TypeTag,

    /// The BCS-encoded bytes of the dynamic field's value. For a dynamic object field, this is the
    /// object's ID.
    value_bytes: Vec<u8>,

    /// The type of the dynamic field's value, like 'u64' or '0x2::kiosk::Listing'. For dynamic
    /// object fields, this type is `ID` (and not relevant).
    value_type: TypeTag,

    /// The scope under which this dynamic field is fetched. This includes any version bounds.
    scope: Scope,
}

/// A description of a dynamic field's name.
#[derive(InputObject)]
pub(crate) struct DynamicFieldName {
    /// The type of the dynamic field's name, like 'u64' or '0x2::kiosk::Listing'.
    type_: TypeInput,

    /// The Base64-encoded BCS serialization of the dynamic field's 'name'.
    bcs: Base64,
}

/// The value of a dynamic field (`MoveValue`) or dynamic object field (`MoveObject`).
#[derive(Union)]
pub(crate) enum DynamicFieldValue {
    MoveObject(MoveObject),
    MoveValue(MoveValue),
}

/// Dynamic fields are heterogenous fields that can be added or removed from an object at runtime. Their names are arbitrary Move values that have `copy`, `drop`, and `store`.
///
/// There are two sub-types of dynamic fields:
///
/// - Dynamic fields can store any value that has `store`. Objects stored in this kind of field will be considered wrapped (not accessible via its ID by external tools like explorers, wallets, etc. accessing storage).
/// - Dynamic object fields can only store objects (values that have the `key` ability, and an `id: UID` as its first field) that have `store`, but they will still be directly accessible off-chain via their ID after being attached as a field.
#[Object]
impl DynamicField {
    /// The DynamicField's ID.
    pub(crate) async fn address(&self, ctx: &Context<'_>) -> Result<SuiAddress, RpcError> {
        self.super_.address(ctx).await
    }

    /// The version of this object that this content comes from.
    pub(crate) async fn version(&self, ctx: &Context<'_>) -> Result<UInt53, RpcError> {
        self.super_.version(ctx).await
    }

    /// 32-byte hash that identifies the object's contents, encoded in Base58.
    pub(crate) async fn digest(&self, ctx: &Context<'_>) -> Result<String, RpcError> {
        self.super_.digest(ctx).await
    }

    /// Fetch the total balance for coins with marker type `coinType` (e.g. `0x2::sui::SUI`), owned by this address.
    ///
    /// If the address does not own any coins of that type, a balance of zero is returned.
    pub(crate) async fn balance(
        &self,
        ctx: &Context<'_>,
        coin_type: TypeInput,
    ) -> Result<Option<Balance>, RpcError<balance::Error>> {
        self.super_.balance(ctx, coin_type).await
    }

    /// Total balance across coins owned by this address, grouped by coin type.
    pub(crate) async fn balances(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<balance::Cursor>,
        last: Option<u64>,
        before: Option<balance::Cursor>,
    ) -> Result<Option<Connection<String, Balance>>, RpcError<balance::Error>> {
        self.super_.balances(ctx, first, after, last, before).await
    }

    /// The structured representation of the object's contents.
    pub(crate) async fn contents(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<MoveValue>, RpcError<object::Error>> {
        self.super_.contents(ctx).await
    }

    /// Access a dynamic field on an object using its type and BCS-encoded name.
    pub(crate) async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError<object::Error>> {
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
    pub(crate) async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>, RpcError<object::Error>> {
        self.super_.dynamic_object_field(ctx, name).await
    }

    /// Access dynamic fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic fields that is guaranteed to be the same length as `keys`. If a dynamic field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError<object::Error>> {
        self.super_.multi_get_dynamic_fields(ctx, keys).await
    }

    /// Access dynamic object fields on an object using their types and BCS-encoded names.
    ///
    /// Returns a list of dynamic object fields that is guaranteed to be the same length as `keys`. If a dynamic object field in `keys` could not be found in the store, its corresponding entry in the result will be `null`.
    pub(crate) async fn multi_get_dynamic_object_fields(
        &self,
        ctx: &Context<'_>,
        keys: Vec<DynamicFieldName>,
    ) -> Result<Vec<Option<DynamicField>>, RpcError<object::Error>> {
        self.super_.multi_get_dynamic_object_fields(ctx, keys).await
    }

    /// The Base64-encoded BCS serialize of this object, as a `MoveObject`.
    pub(crate) async fn move_object_bcs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Base64>, RpcError<object::Error>> {
        self.super_.move_object_bcs(ctx).await
    }

    /// Fetch the total balances keyed by coin types (e.g. `0x2::sui::SUI`) owned by this address.
    ///
    /// If the address does not own any coins of a given type, a balance of zero is returned for that type.
    pub(crate) async fn multi_get_balances(
        &self,
        ctx: &Context<'_>,
        keys: Vec<TypeInput>,
    ) -> Result<Vec<Balance>, RpcError<balance::Error>> {
        self.super_.multi_get_balances(ctx, keys).await
    }

    /// The dynamic field's name, as a Move value.
    async fn name(&self, ctx: &Context<'_>) -> Result<Option<MoveValue>, RpcError<object::Error>> {
        let Some(native) = self.native(ctx).await? else {
            return Ok(None);
        };

        Ok(Some(MoveValue::new(
            MoveType::from_native(native.name_type.clone(), native.scope.clone()),
            native.name_bytes.clone(),
        )))
    }

    /// Fetch the object with the same ID, at a different version, root version bound, or checkpoint.
    pub(crate) async fn object_at(
        &self,
        ctx: &Context<'_>,
        version: Option<UInt53>,
        root_version: Option<UInt53>,
        checkpoint: Option<UInt53>,
    ) -> Result<Option<Object>, RpcError<object::Error>> {
        self.super_
            .object_at(ctx, version, root_version, checkpoint)
            .await
    }

    /// The Base64-encoded BCS serialization of this object, as an `Object`.
    pub(crate) async fn object_bcs(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Base64>, RpcError<object::Error>> {
        self.super_.object_bcs(ctx).await
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
    ) -> Result<Connection<String, Object>, RpcError<object::Error>> {
        self.super_
            .object_versions_after(ctx, first, after, last, before, filter)
            .await
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
    ) -> Result<Connection<String, Object>, RpcError<object::Error>> {
        self.super_
            .object_versions_before(ctx, first, after, last, before, filter)
            .await
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
    ) -> Result<Option<Connection<String, MoveObject>>, RpcError<object::Error>> {
        self.super_
            .objects(ctx, first, after, last, before, filter)
            .await
    }

    /// The object's owner kind.
    pub(crate) async fn owner(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Owner>, RpcError<object::Error>> {
        self.super_.owner(ctx).await
    }

    /// The transaction that created this version of the object.
    pub(crate) async fn previous_transaction(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<Transaction>, RpcError<object::Error>> {
        self.super_.previous_transaction(ctx).await
    }

    /// The SUI returned to the sponsor or sender of the transaction that modifies or deletes this object.
    pub(crate) async fn storage_rebate(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<BigInt>, RpcError<object::Error>> {
        self.super_.storage_rebate(ctx).await
    }

    /// The dynamic field's value, as a Move value for dynamic fields and as a MoveObject for dynamic object fields.
    async fn value(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<DynamicFieldValue>, RpcError<object::Error>> {
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

        let key = ObjectKey {
            address,
            root_version: native.scope.root_version().map(Into::into),
            version: None,
            at_checkpoint: None,
        };

        let Some(object) = Object::by_key(ctx, native.scope.clone(), key).await? else {
            return Ok(None);
        };

        Ok(Some(DynamicFieldValue::MoveObject(MoveObject::from_super(
            object,
        ))))
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

    /// Create a dynamic field from a `MoveObject`, after checking whether it is a dynamic field.
    pub(crate) async fn from_move_object(
        move_object: &MoveObject,
        ctx: &Context<'_>,
    ) -> Result<Option<Self>, RpcError<object::Error>> {
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
    ) -> Result<Option<Self>, RpcError<object::Error>> {
        let type_ = match kind {
            DynamicFieldType::DynamicField => name.type_.0,
            DynamicFieldType::DynamicObject => {
                DynamicFieldInfo::dynamic_object_field_wrapper(name.type_.0).into()
            }
        };

        let field_id = derive_dynamic_field_id(parent, &type_, &name.bcs.0)
            .context("Failed to derive dynamic field ID")?;

        let key = ObjectKey {
            address: field_id.into(),
            root_version: scope.root_version().map(Into::into),
            version: None,
            at_checkpoint: None,
        };

        let Some(object) = Object::by_key(ctx, scope.clone(), key).await? else {
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
    async fn native(
        &self,
        ctx: &Context<'_>,
    ) -> Result<&Option<NativeField>, RpcError<object::Error>> {
        self.native
            .get_or_try_init(async || {
                let Some(value) = self.super_.contents(ctx).await? else {
                    return Ok(None);
                };

                let Some(layout) = value.type_.layout_impl().await.map_err(upcast)? else {
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
