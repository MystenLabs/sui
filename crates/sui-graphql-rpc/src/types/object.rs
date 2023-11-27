// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{connection::Connection, *};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer::errors::IndexerError;
use sui_indexer::models_v2::objects::StoredObject;
use sui_indexer::types_v2::OwnerType;
use sui_json_rpc::name_service::NameServiceConfig;
use sui_types::dynamic_field::DynamicFieldType;

use super::big_int::BigInt;
use super::dynamic_field::{DynamicField, DynamicFieldName};
use super::move_object::MoveObject;
use super::move_package::MovePackage;
use super::{
    balance::Balance, coin::Coin, owner::Owner, stake::StakedSui, sui_address::SuiAddress,
    transaction_block::TransactionBlock,
};
use crate::context_data::db_data_provider::PgManager;
use crate::error::Error;
use crate::types::base64::Base64;
use sui_types::object::Object as NativeObject;

#[derive(Clone, Debug)]
pub(crate) struct Object {
    pub address: SuiAddress,

    /// Representation of an Object in the Indexer's Store.
    pub stored: StoredObject,

    /// Deserialized representation of `stored_object.serialized_object`.
    pub native: NativeObject,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum ObjectKind {
    Owned,
    Child,
    Shared,
    Immutable,
}

#[derive(InputObject, Default, Clone)]
pub(crate) struct ObjectFilter {
    pub package: Option<SuiAddress>,
    pub module: Option<String>,
    pub ty: Option<String>,

    pub owner: Option<SuiAddress>,
    pub object_ids: Option<Vec<SuiAddress>>,
    pub object_keys: Option<Vec<ObjectKey>>,
}

#[derive(InputObject, Clone)]
pub(crate) struct ObjectKey {
    object_id: SuiAddress,
    version: u64,
}

#[Object]
impl Object {
    async fn version(&self) -> u64 {
        self.stored.object_version as u64
    }

    /// 32-byte hash that identifies the object's current contents, encoded as a Base58 string.
    async fn digest(&self) -> String {
        Base58::encode(&self.stored.object_digest)
    }

    /// The amount of SUI we would rebate if this object gets deleted or mutated.
    /// This number is recalculated based on the present storage gas price.
    async fn storage_rebate(&self) -> Option<BigInt> {
        Some(BigInt::from(self.native.storage_rebate))
    }

    /// The Base64 encoded bcs serialization of the object's content.
    async fn bcs(&self) -> Option<Base64> {
        Some(Base64::from(&self.stored.serialized_object))
    }

    /// The transaction block that created this version of the object.
    async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        let digest = self.native.previous_transaction.to_string();
        ctx.data_unchecked::<PgManager>()
            .fetch_tx(digest.as_str())
            .await
            .extend()
    }

    /// Objects can either be immutable, shared, owned by an address,
    /// or are child objects (part of a dynamic field)
    async fn kind(&self) -> Result<Option<ObjectKind>> {
        let owner_type: OwnerType = self
            .stored
            .owner_type
            .try_into()
            .map_err(|e: IndexerError| Error::Internal(e.to_string()))
            .extend()?;

        Ok(Some(match owner_type {
            OwnerType::Immutable => ObjectKind::Immutable,
            OwnerType::Address => ObjectKind::Owned,
            OwnerType::Object => ObjectKind::Child,
            OwnerType::Shared => ObjectKind::Shared,
        }))
    }

    /// The Address or Object that owns this Object.  Immutable and Shared Objects do not have
    /// owners.
    async fn owner(&self) -> Result<Option<Owner>> {
        let Some(owner_id) = &self.stored.owner_id else {
            return Ok(None);
        };

        let address = addr(owner_id).extend()?;
        Ok(Some(Owner { address }))
    }

    /// Attempts to convert the object into a MoveObject
    async fn as_move_object(&self) -> Option<MoveObject> {
        MoveObject::try_from(self).ok()
    }

    /// Attempts to convert the object into a MovePackage
    async fn as_move_package(&self) -> Option<MovePackage> {
        MovePackage::try_from(self).ok()
    }

    // =========== Owner interface methods =============

    /// The address of the object, named as such to avoid conflict with the address type.
    pub async fn location(&self) -> SuiAddress {
        self.address
    }

    /// The objects owned by this object
    pub async fn object_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Result<Option<Connection<String, Object>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_owned_objs(first, after, last, before, filter, self.address)
            .await
            .extend()
    }

    /// The balance of coin objects of a particular coin type owned by the object.
    pub async fn balance(
        &self,
        ctx: &Context<'_>,
        type_: Option<String>,
    ) -> Result<Option<Balance>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_balance(self.address, type_)
            .await
            .extend()
    }

    /// The balances of all coin types owned by the object. Coins of the same type are grouped together into one Balance.
    pub async fn balance_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Balance>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_balances(self.address, first, after, last, before)
            .await
            .extend()
    }

    /// The coin objects for the given address.
    ///
    /// The type field is a string of the inner type of the coin by which to filter
    /// (e.g. `0x2::sui::SUI`). If no type is provided, it will default to `0x2::sui::SUI`.
    pub async fn coin_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        type_: Option<String>,
    ) -> Result<Option<Connection<String, Coin>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_coins(Some(self.address), type_, first, after, last, before)
            .await
            .extend()
    }

    /// The `0x3::staking_pool::StakedSui` objects owned by the given object.
    pub async fn staked_sui_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, StakedSui>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_staked_sui(self.address, first, after, last, before)
            .await
            .extend()
    }

    /// The domain that a user address has explicitly configured as their default domain
    pub async fn default_name_service_name(&self, ctx: &Context<'_>) -> Result<Option<String>> {
        ctx.data_unchecked::<PgManager>()
            .default_name_service_name(ctx.data_unchecked::<NameServiceConfig>(), self.address)
            .await
            .extend()
    }

    // TODO disabled-for-rpc-1.5
    // pub async fn name_service_connection(
    //     &self,
    //     ctx: &Context<'_>,
    //     first: Option<u64>,
    //     after: Option<String>,
    //     last: Option<u64>,
    //     before: Option<String>,
    // ) -> Result<Option<Connection<String, NameService>>> {
    //     unimplemented!()
    // }

    /// Access a dynamic field on an object using its name.
    /// Names are arbitrary Move values whose type have `copy`, `drop`, and `store`, and are specified
    /// using their type, and their BCS contents, Base64 encoded.
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner type.
    pub async fn dynamic_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_dynamic_field(self.address, name, DynamicFieldType::DynamicField)
            .await
            .extend()
    }

    /// Access a dynamic object field on an object using its name.
    /// Names are arbitrary Move values whose type have `copy`, `drop`, and `store`, and are specified
    /// using their type, and their BCS contents, Base64 encoded.
    /// The value of a dynamic object field can also be accessed off-chain directly via its address (e.g. using `Query.object`).
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner type.
    pub async fn dynamic_object_field(
        &self,
        ctx: &Context<'_>,
        name: DynamicFieldName,
    ) -> Result<Option<DynamicField>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_dynamic_field(self.address, name, DynamicFieldType::DynamicObject)
            .await
            .extend()
    }

    /// The dynamic fields on an object.
    /// Dynamic fields on wrapped objects can be accessed by using the same API under the Owner type.
    pub async fn dynamic_field_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, DynamicField>>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_dynamic_fields(first, after, last, before, self.address)
            .await
            .extend()
    }
}

impl TryFrom<StoredObject> for Object {
    type Error = Error;

    fn try_from(stored_object: StoredObject) -> Result<Self, Error> {
        let address = addr(&stored_object.object_id)?;
        let native_object = bcs::from_bytes(&stored_object.serialized_object)
            .map_err(|_| Error::Internal(format!("Failed to deserialize object {address}")))?;

        Ok(Self {
            address,
            stored: stored_object,
            native: native_object,
        })
    }
}

/// Parse a `SuiAddress` from its stored representation.  Failure is an internal error: the
/// database should never contain a malformed address (containing the wrong number of bytes).
fn addr(bytes: impl AsRef<[u8]>) -> Result<SuiAddress, Error> {
    SuiAddress::from_bytes(bytes.as_ref()).map_err(|e| {
        let bytes = bytes.as_ref().to_vec();
        Error::Internal(format!("Error deserializing address: {bytes:?}: {e}"))
    })
}
