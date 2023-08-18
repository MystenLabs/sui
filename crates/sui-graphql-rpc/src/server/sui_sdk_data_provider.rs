// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// For testing, use existing RPC as data source

use crate::error::Error;
use crate::types::address::Address;
use crate::types::balance::Balance;
use crate::types::base64::Base64;
use crate::types::big_int::BigInt;
use crate::types::object::ObjectFilter;
use crate::types::object::ObjectKind;
use crate::types::protocol_config::ProtocolConfigAttr;
use crate::types::protocol_config::ProtocolConfigFeatureFlag;
use crate::types::protocol_config::ProtocolConfigs;
use crate::types::transaction_block::TransactionBlock;
use crate::types::{object::Object, sui_address::SuiAddress};
use async_graphql::connection::{Connection, Edge};
use async_graphql::*;
use async_trait::async_trait;
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

use crate::server::data_provider::DataProvider;

#[async_trait]
impl DataProvider for SuiClient {
    async fn fetch_obj(&self, address: SuiAddress, version: Option<u64>) -> Result<Option<Object>> {
        let oid: NativeObjectID = address.into_array().as_slice().try_into()?;
        let opts = SuiObjectDataOptions::full_content();

        let g = match version {
            Some(v) => match self
                .read_api()
                .try_get_parsed_past_object(oid, v.into(), opts)
                .await?
            {
                SuiPastObjectResponse::VersionFound(x) => x,
                _ => return Ok(None),
            },
            None => {
                let val = self.read_api().get_object_with_options(oid, opts).await?;
                if val.error.is_some() || val.data.is_none() {
                    return Ok(None);
                }
                val.data.unwrap()
            }
        };
        Ok(Some(convert_obj(&g)))
    }

    async fn fetch_owned_objs(
        &self,
        owner: &SuiAddress,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        _filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, Object>> {
        if before.is_some() && after.is_some() {
            return Err(Error::CursorNoBeforeAfter.extend());
        }
        if first.is_some() && last.is_some() {
            return Err(Error::CursorNoFirstLast.extend());
        }
        if before.is_some() || last.is_some() {
            return Err(Error::CursorNoReversePagination.extend());
        }

        let count = first.map(|q| q as usize);
        let native_owner = NativeSuiAddress::from(owner);
        let query = SuiObjectResponseQuery::new_with_options(SuiObjectDataOptions::full_content());

        let cursor = match after {
            Some(q) => Some(
                NativeObjectID::from_hex_literal(&q)
                    .map_err(|w| Error::InvalidCursor(w.to_string()).extend())?,
            ),
            None => None,
        };

        let pg = self
            .read_api()
            .get_owned_objects(native_owner, Some(query), cursor, count)
            .await?;

        // TODO: support partial success/ failure responses
        pg.data.iter().try_for_each(|n| {
            if n.error.is_some() {
                return Err(Error::CursorConnectionFetchFailed(
                    n.error.as_ref().unwrap().to_string(),
                )
                .extend());
            } else if n.data.is_none() {
                return Err(Error::Internal(
                    "Expected either data or error fields, received neither".to_string(),
                )
                .extend());
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

    async fn fetch_balance(&self, address: &SuiAddress, type_: Option<String>) -> Result<Balance> {
        let b = self
            .coin_read_api()
            .get_balance(address.into(), type_)
            .await?;
        Ok(convert_bal(b))
    }

    async fn fetch_tx(&self, digest: &str) -> Result<Option<TransactionBlock>> {
        let tx_digest = TransactionDigest::from_str(digest)?;
        let tx = self
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

    async fn fetch_chain_id(&self) -> Result<String> {
        Ok(self.read_api().get_chain_identifier().await?)
    }

    async fn fetch_protocol_config(&self, version: Option<u64>) -> Result<ProtocolConfigs> {
        let cfg = self
            .read_api()
            .get_protocol_config(version.map(|x| x.into()))
            .await?;

        Ok(ProtocolConfigs {
            configs: cfg
                .attributes
                .into_iter()
                .map(|(k, v)| ProtocolConfigAttr {
                    key: k,
                    // TODO:  what to return when value is None? nothing?
                    // TODO: do we want to return type info separately?
                    value: match v {
                        Some(q) => format!("{:?}", q),
                        None => "".to_string(),
                    },
                })
                .collect(),
            feature_flags: cfg
                .feature_flags
                .into_iter()
                .map(|x| ProtocolConfigFeatureFlag {
                    key: x.0,
                    value: x.1,
                })
                .collect(),
            protocol_version: cfg.protocol_version.as_u64(),
        })
    }
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
