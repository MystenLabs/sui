// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::dataloader::{DataLoader, LruCache};
use async_graphql::{connection::Connection, *};
use sui_json_rpc_types::SuiRawData;
use sui_sdk::types::object::Owner as NativeOwner;

use super::big_int::BigInt;
use super::name_service::NameService;
use super::tx_digest::TransactionDigest;
use super::{
    balance::Balance, coin::Coin, owner::Owner, stake::Stake, sui_address::SuiAddress,
    transaction_block::TransactionBlock,
};
use crate::server::sui_sdk_data_provider::SuiClientLoader;
use crate::{server::context_ext::DataProviderContextExt, types::base64::Base64};

#[derive(Clone, Eq, PartialEq, Debug, SimpleObject)]
#[graphql(complex)]
pub(crate) struct Object {
    pub address: SuiAddress,
    pub version: u64,
    pub digest: String,
    pub storage_rebate: Option<BigInt>,
    pub owner: Option<Owner>,
    pub bcs: Option<Base64>,
    pub previous_transaction: Option<TransactionDigest>,
    pub kind: Option<ObjectKind>,
}

impl From<&sui_json_rpc_types::SuiObjectData> for Object {
    fn from(s: &sui_json_rpc_types::SuiObjectData) -> Self {
        Self {
            version: s.version.into(),
            digest: s.digest.to_string(),
            storage_rebate: s.storage_rebate.map(BigInt::from),
            address: SuiAddress::from_array(**s.object_id),
            owner: s
                .owner
                .unwrap()
                .get_owner_address()
                .map(|x| SuiAddress::from_array(x.to_inner()))
                .ok()
                .map(|q| Owner { address: q }),
            bcs: s.bcs.as_ref().map(|raw| match raw {
                SuiRawData::Package(raw_package) => {
                    Base64::from(bcs::to_bytes(raw_package).unwrap())
                }
                SuiRawData::MoveObject(raw_object) => Base64::from(&raw_object.bcs_bytes),
            }),
            previous_transaction: Some(TransactionDigest::from_array(
                s.previous_transaction.unwrap().into_inner(),
            )),
            kind: Some(match s.owner.unwrap() {
                NativeOwner::AddressOwner(_) => ObjectKind::Owned,
                NativeOwner::ObjectOwner(_) => ObjectKind::Child,
                NativeOwner::Shared {
                    initial_shared_version: _,
                } => ObjectKind::Shared,
                NativeOwner::Immutable => ObjectKind::Immutable,
            }),
        }
    }
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
    package: Option<SuiAddress>,
    module: Option<String>,
    ty: Option<String>,

    owner: Option<SuiAddress>,
    object_ids: Option<Vec<SuiAddress>>,
    object_keys: Option<Vec<ObjectKey>>,
}

#[derive(InputObject)]
pub(crate) struct ObjectKey {
    object_id: SuiAddress,
    version: u64,
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[ComplexObject]
impl Object {
    async fn previous_transaction_block(
        &self,
        ctx: &Context<'_>,
    ) -> Result<Option<TransactionBlock>> {
        if let Some(tx) = &self.previous_transaction {
            let loader = ctx.data_unchecked::<DataLoader<SuiClientLoader, LruCache>>();
            loader.load_one(*tx).await
        } else {
            Ok(None)
        }
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
    ) -> Result<Connection<String, Object>> {
        ctx.data_provider()
            .fetch_owned_objs(&self.address, first, after, last, before, filter)
            .await
    }

    pub async fn balance(&self, ctx: &Context<'_>, type_: Option<String>) -> Result<Balance> {
        ctx.data_provider()
            .fetch_balance(&self.address, type_)
            .await
    }

    pub async fn balance_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, Balance>> {
        ctx.data_provider()
            .fetch_balance_connection(&self.address, first, after, last, before)
            .await
    }

    pub async fn coin_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        type_: Option<String>,
    ) -> Option<Connection<String, Coin>> {
        unimplemented!()
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
