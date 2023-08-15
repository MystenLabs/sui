// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// For testing, use existing RPC as data source

use crate::types::address::Address;
use crate::types::balance::Balance;
use crate::types::base64::Base64;
use crate::types::big_int::BigInt;
use crate::types::object::ObjectFilter;
use crate::types::object::ObjectKind;
use crate::types::transaction_block::TransactionBlock;
use crate::types::{object::Object, sui_address::SuiAddress};
use async_graphql::connection::{Connection, Edge};
use async_graphql::*;
use std::str::FromStr;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponseQuery, SuiPastObjectResponse, SuiRawData,
    SuiTransactionBlockDataAPI, SuiTransactionBlockResponseOptions,
};
use sui_sdk::{
    types::{
        base_types::{ObjectID as NativeObjectID, SuiAddress as NativeSuiAddress},
        digests::TransactionDigest,
        object::Owner as NativeOwner,
    },
    SuiClient,
};

pub(crate) async fn fetch_obj(
    cl: &SuiClient,
    address: SuiAddress,
    version: Option<u64>,
) -> Result<Option<Object>> {
    let oid: NativeObjectID = address.into_array().as_slice().try_into()?;
    let opts = SuiObjectDataOptions::full_content();

    let g = match version {
        Some(v) => match cl
            .read_api()
            .try_get_parsed_past_object(oid, v.into(), opts)
            .await?
        {
            SuiPastObjectResponse::VersionFound(x) => x,
            _ => return Ok(None),
        },
        None => {
            let val = cl.read_api().get_object_with_options(oid, opts).await?;
            if val.error.is_some() || val.data.is_none() {
                return Ok(None);
            }
            val.data.unwrap()
        }
    };
    Ok(Some(convert_obj(&g)))
}

pub(crate) async fn fetch_owned_objs(
    cl: &SuiClient,
    owner: &SuiAddress,
    first: Option<u64>,
    after: Option<String>,
    last: Option<u64>,
    before: Option<String>,
    _filter: Option<ObjectFilter>,
) -> Result<Connection<String, Object>> {
    if before.is_some() && after.is_some() {
        return Err(Error::new("before and after must not be used together"));
    }
    if first.is_some() && last.is_some() {
        return Err(Error::new("first and last must not be used together"));
    }
    if before.is_some() || last.is_some() {
        return Err(Error::new("reverse pagination is not supported"));
    }

    let count = first.map(|q| q as usize);
    let native_owner = NativeSuiAddress::from(owner);
    let query = SuiObjectResponseQuery::new_with_options(SuiObjectDataOptions::full_content());

    let cursor = match after {
        Some(q) => Some(
            NativeObjectID::from_hex_literal(&q)
                .map_err(|w| Error::new(format!("invalid object id: {}", w)))?,
        ),
        None => None,
    };

    let pg = cl
        .read_api()
        .get_owned_objects(native_owner, Some(query), cursor, count)
        .await?;

    // TODO: handle errors
    pg.data.iter().try_for_each(|n| {
        if n.error.is_some() || n.data.is_none() {
            return Err(Error::new("error"));
        }
        Ok(())
    })?;
    let mut connection = Connection::new(false, pg.has_next_page);

    connection.edges.extend(pg.data.into_iter().map(|n| {
        let g = n.data.unwrap();
        let o = convert_obj(&g);

        Edge::new(g.object_id.to_string(), o)
    }));
    Ok(connection)
}

pub(crate) async fn fetch_balance(
    cl: &SuiClient,
    address: &SuiAddress,
    type_: Option<String>,
) -> Result<Balance> {
    let b = cl
        .coin_read_api()
        .get_balance(address.into(), type_)
        .await?;
    Ok(convert_bal(b))
}

fn convert_obj(s: &sui_json_rpc_types::SuiObjectData) -> Object {
    Object {
        version: s.version.into(),
        digest: s.digest.to_string(),
        storage_rebate: s.storage_rebate,
        address: SuiAddress::from_array(**s.object_id),
        owner: s
            .owner
            .unwrap()
            .get_owner_address()
            .map(|x| SuiAddress::from_array(x.to_inner()))
            .ok(),
        bcs: s.bcs.as_ref().map(|raw| match raw {
            SuiRawData::Package(raw_package) => Base64::from(bcs::to_bytes(raw_package).unwrap()),
            SuiRawData::MoveObject(raw_object) => Base64::from(&raw_object.bcs_bytes),
        }),
        previous_transaction: Some(s.previous_transaction.unwrap().to_string()),
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

pub(crate) async fn fetch_tx(cl: &SuiClient, digest: &String) -> Result<Option<TransactionBlock>> {
    let tx_digest = TransactionDigest::from_str(digest)?;
    let tx = cl
        .read_api()
        .get_transaction_with_options(
            tx_digest,
            SuiTransactionBlockResponseOptions::full_content(),
        )
        .await?;
    let sender = *tx.transaction.unwrap().data.sender();
    Ok(Some(TransactionBlock {
        digest: digest.to_string(),
        sender: Some(Address {
            address: SuiAddress::from_array(sender.to_inner()),
        }),
        bcs: Some(Base64::from(&tx.raw_transaction)),
    }))
}

pub(crate) async fn fetch_chain_id(cl: &SuiClient) -> Result<String> {
    Ok(cl.read_api().get_chain_identifier().await?)
}

fn convert_bal(b: sui_json_rpc_types::Balance) -> Balance {
    Balance {
        coin_object_count: b.coin_object_count as u64,
        total_balance: BigInt::from_str(&format!("{}", b.total_balance)).unwrap(),
    }
}

impl From<Address> for SuiAddress {
    fn from(a: Address) -> Self {
        a.address
    }
}

impl From<SuiAddress> for Address {
    fn from(a: SuiAddress) -> Self {
        Address { address: a }
    }
}

impl From<NativeSuiAddress> for SuiAddress {
    fn from(a: NativeSuiAddress) -> Self {
        SuiAddress::from_array(a.to_inner())
    }
}

impl From<SuiAddress> for NativeSuiAddress {
    fn from(a: SuiAddress) -> Self {
        NativeSuiAddress::try_from(a.as_slice()).unwrap()
    }
}

impl From<&SuiAddress> for NativeSuiAddress {
    fn from(a: &SuiAddress) -> Self {
        NativeSuiAddress::try_from(a.as_slice()).unwrap()
    }
}
