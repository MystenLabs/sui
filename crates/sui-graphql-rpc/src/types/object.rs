// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::dataloader::{DataLoader, LruCache};
use async_graphql::{connection::Connection, *};

use super::big_int::BigInt;
use super::digest::Digest;
use super::name_service::NameService;
use super::{
    balance::Balance, coin::Coin, owner::Owner, stake::Stake, sui_address::SuiAddress,
    transaction_block::TransactionBlock,
};
use crate::context_data::db_data_provider::PgManager;
use crate::context_data::sui_sdk_data_provider::SuiClientLoader;
use crate::types::base64::Base64;

use sui_types::object::{Data as NativeSuiObjectData, Object as NativeSuiObject};

#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct Object {
    pub address: SuiAddress,
    pub version: u64,
    pub digest: String,
    pub storage_rebate: Option<BigInt>,
    pub owner: Option<SuiAddress>,
    pub bcs: Option<Base64>,
    pub previous_transaction: Option<Digest>,
    pub kind: Option<ObjectKind>,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum ObjectKind {
    Owned,
    Child,
    Shared,
    Immutable,
}

#[derive(InputObject)]
pub(crate) struct ObjectFilter {
    pub package: Option<SuiAddress>,
    pub module: Option<String>,
    pub ty: Option<String>,

    pub owner: Option<SuiAddress>,
    pub object_ids: Option<Vec<SuiAddress>>,
    pub object_keys: Option<Vec<ObjectKey>>,
}

#[derive(InputObject)]
pub(crate) struct ObjectKey {
    object_id: SuiAddress,
    version: u64,
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Object {
    async fn version(&self) -> u64 {
        self.version
    }

    async fn digest(&self) -> String {
        self.digest.clone()
    }

    async fn storage_rebate(&self) -> Option<BigInt> {
        self.storage_rebate.clone()
    }

    async fn bcs(&self) -> Option<Base64> {
        self.bcs.clone()
    }

    async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        if let Some(tx) = &self.previous_transaction {
            let loader = ctx.data_unchecked::<DataLoader<SuiClientLoader, LruCache>>();
            Ok(loader.load_one(*tx).await.unwrap_or(None))
        } else {
            Ok(None)
        }
    }

    async fn kind(&self) -> Option<ObjectKind> {
        self.kind
    }

    async fn owner(&self) -> Option<Owner> {
        self.owner.as_ref().map(|q| Owner { address: *q })
    }

    // =========== Owner interface methods =============

    pub async fn location(&self) -> SuiAddress {
        self.address
    }

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

    pub async fn balance(&self, ctx: &Context<'_>, type_: String) -> Result<Option<Balance>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_balance(self.address, type_)
            .await
            .extend()
    }

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
            .fetch_coins(self.address, type_, first, after, last, before)
            .await
            .extend()
    }

    pub async fn stake_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Option<Connection<String, Stake>> {
        unimplemented!()
    }

    pub async fn default_name_service_name(&self) -> Option<String> {
        unimplemented!()
    }

    pub async fn name_service_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Option<Connection<String, NameService>> {
        unimplemented!()
    }
}

impl From<&NativeSuiObject> for Object {
    fn from(o: &NativeSuiObject) -> Self {
        let kind = Some(match o.owner {
            sui_types::object::Owner::AddressOwner(_) => ObjectKind::Owned,
            sui_types::object::Owner::ObjectOwner(_) => ObjectKind::Child,
            sui_types::object::Owner::Shared {
                initial_shared_version: _,
            } => ObjectKind::Shared,
            sui_types::object::Owner::Immutable => ObjectKind::Immutable,
        });

        let owner_address = o.owner.get_owner_address().ok();
        if matches!(kind, Some(ObjectKind::Immutable) | Some(ObjectKind::Shared))
            && owner_address.is_some()
        {
            panic!("Immutable or Shared object should not have an owner_id");
        }

        let bcs = match &o.data {
            // Do we BCS serialize packages?
            NativeSuiObjectData::Package(package) => Base64::from(
                bcs::to_bytes(package)
                    .expect("Failed to serialize package")
                    .to_vec(),
            ),
            NativeSuiObjectData::Move(move_object) => Base64::from(move_object.contents()),
        };

        Self {
            address: SuiAddress::from_array(o.id().into_bytes()),
            version: o.version().into(),
            digest: o.digest().base58_encode(),
            storage_rebate: Some(BigInt::from(o.storage_rebate)),
            owner: owner_address.map(SuiAddress::from),
            bcs: Some(bcs),
            previous_transaction: Some(Digest::from_array(o.previous_transaction.into_inner())),
            kind,
        }
    }
}
