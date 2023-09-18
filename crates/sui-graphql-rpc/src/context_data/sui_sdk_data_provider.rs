// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// For testing, use existing RPC as data source

use crate::error::Error;
use crate::types::address::Address;
use crate::types::balance::Balance;
use crate::types::base64::Base64;
use crate::types::big_int::BigInt;
use crate::types::checkpoint::Checkpoint;
use crate::types::object::{Object, ObjectFilter, ObjectKind};
use crate::types::protocol_config::{
    ProtocolConfigAttr, ProtocolConfigFeatureFlag, ProtocolConfigs,
};
use crate::types::sui_address::SuiAddress;
use crate::types::transaction_block::TransactionBlock;
use crate::types::tx_digest::TransactionDigest;
use async_graphql::connection::{Connection, Edge};
use async_graphql::dataloader::*;
use async_graphql::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponseQuery, SuiPastObjectResponse, SuiRawData,
    SuiTransactionBlockResponseOptions,
};
use sui_sdk::types::sui_serde::BigInt as SerdeBigInt;
use sui_sdk::types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use sui_sdk::{
    types::{
        base_types::{ObjectID as NativeObjectID, SuiAddress as NativeSuiAddress},
        digests::TransactionDigest as NativeTransactionDigest,
        object::Owner as NativeOwner,
    },
    SuiClient,
};

use super::data_provider::DataProvider;

const RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD: Duration = Duration::from_millis(10_000);
const MAX_CONCURRENT_REQUESTS: usize = 1_000;
const DATA_LOADER_LRU_CACHE_SIZE: usize = 1_000;

const DEFAULT_PAGE_SIZE: usize = 50;

pub(crate) struct SuiClientLoader {
    pub client: SuiClient,
}

#[async_trait::async_trait]
impl Loader<TransactionDigest> for SuiClientLoader {
    type Value = TransactionBlock;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[TransactionDigest],
    ) -> Result<HashMap<TransactionDigest, Self::Value>, Self::Error> {
        let mut map = HashMap::new();
        let keys: Vec<_> = keys
            .iter()
            .map(|x| NativeTransactionDigest::new(x.into_array()))
            .collect();
        for tx in self
            .client
            .read_api()
            .multi_get_transactions_with_options(
                keys,
                SuiTransactionBlockResponseOptions::full_content(),
            )
            .await?
        {
            let digest = TransactionDigest::from_array(tx.digest.into_inner());
            let mtx = TransactionBlock::from(tx);
            map.insert(digest, mtx);
        }
        Ok(map)
    }
}

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
        ensure_forward_pagination(&first, &after, &last, &before)?;

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

    async fn get_object_with_options(
        &self,
        object_id: NativeObjectID,
        options: SuiObjectDataOptions,
    ) -> Result<Option<Object>> {
        let obj = self
            .read_api()
            .get_object_with_options(object_id, options)
            .await?;

        if obj.error.is_some() || obj.data.is_none() {
            return Ok(None);
        }
        Ok(Some(convert_obj(&obj.data.unwrap())))
    }

    async fn multi_get_object_with_options(
        &self,
        object_ids: Vec<NativeObjectID>,
        options: SuiObjectDataOptions,
    ) -> Result<Vec<Object>> {
        let obj_responses = self
            .read_api()
            .multi_get_object_with_options(object_ids, options)
            .await?;

        let mut objs = Vec::new();

        for n in obj_responses.iter() {
            if n.error.is_some() {
                return Err(Error::MultiGet(n.error.as_ref().unwrap().to_string()).extend());
            } else if n.data.is_none() {
                return Err(Error::Internal(
                    "Expected either data or error fields, received neither".to_string(),
                )
                .extend());
            }
            objs.push(convert_obj(n.data.as_ref().unwrap()));
        }
        Ok(objs)
    }

    async fn fetch_balance(&self, address: &SuiAddress, type_: Option<String>) -> Result<Balance> {
        let b = self
            .coin_read_api()
            .get_balance(address.into(), type_)
            .await?;
        Ok(Balance::from(b))
    }

    async fn fetch_balance_connection(
        &self,
        address: &SuiAddress,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, Balance>> {
        ensure_forward_pagination(&first, &after, &last, &before)?;

        let count = first.unwrap_or(DEFAULT_PAGE_SIZE as u64) as usize;
        let offset = after
            .map(|q| q.parse::<usize>().unwrap())
            .unwrap_or(0_usize);

        // This fetches all balances but we only want a slice
        // The pagination logic here can break if data is added
        // This is okay for now as we're only using this for testing
        let balances = self
            .coin_read_api()
            .get_all_balances(NativeSuiAddress::from(address))
            .await?;

        let max = balances.len();

        let bs = balances.into_iter().skip(offset).take(count);

        let mut connection = Connection::new(false, offset + count < max);

        connection
            .edges
            .extend(bs.into_iter().enumerate().map(|(i, b)| {
                let balance = Balance::from(b);
                Edge::new(format!("{:032}", offset + i), balance)
            }));
        Ok(connection)
    }

    // TODO: support backward pagination as fetching checkpoints
    // API allows for it
    async fn fetch_checkpoint_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, Checkpoint>> {
        ensure_forward_pagination(&first, &after, &last, &before)?;

        let count = first.map(|q| q as usize);
        let after = after
            .map(|x| x.parse::<u64>())
            .transpose()
            .map_err(|_| {
                Error::InvalidCursor(
                    "Cannot convert after parameter into u64 in the checkpoint connection"
                        .to_string(),
                )
            })?
            .map(SerdeBigInt::from);

        let pg = self.read_api().get_checkpoints(after, count, false).await?;
        let checkpoints: Vec<_> = pg.data.iter().map(Checkpoint::from).collect();

        let mut connection = Connection::new(false, pg.has_next_page);
        connection.edges.extend(
            checkpoints
                .iter()
                .map(|x| Edge::new(x.sequence_number.to_string(), x.clone())),
        );

        Ok(connection)
    }

    async fn fetch_tx(&self, digest: &str) -> Result<Option<TransactionBlock>> {
        let tx_digest = NativeTransactionDigest::from_str(digest)?;
        let tx = self
            .read_api()
            .get_transaction_with_options(
                tx_digest,
                SuiTransactionBlockResponseOptions::full_content(),
            )
            .await?;
        Ok(Some(TransactionBlock::from(tx)))
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

    async fn get_latest_sui_system_state(&self) -> Result<SuiSystemStateSummary> {
        Ok(self.governance_api().get_latest_sui_system_state().await?)
    }
}

pub(crate) async fn sui_sdk_client_v0(rpc_url: impl AsRef<str>) -> SuiClient {
    sui_sdk::SuiClientBuilder::default()
        .request_timeout(RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD)
        .max_concurrent_requests(MAX_CONCURRENT_REQUESTS)
        .build(rpc_url)
        .await
        .expect("Failed to create SuiClient")
}

pub(crate) async fn lru_cache_data_loader(
    client: &SuiClient,
) -> DataLoader<SuiClientLoader, LruCache> {
    let data_loader = DataLoader::with_cache(
        SuiClientLoader {
            client: client.clone(),
        },
        tokio::spawn,
        async_graphql::dataloader::LruCache::new(DATA_LOADER_LRU_CACHE_SIZE),
    );
    data_loader.enable_all_cache(true);
    data_loader
}

pub(crate) fn convert_obj(s: &sui_json_rpc_types::SuiObjectData) -> Object {
    Object {
        version: s.version.into(),
        digest: s.digest.to_string(),
        storage_rebate: s.storage_rebate.map(BigInt::from),
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

fn ensure_forward_pagination(
    first: &Option<u64>,
    after: &Option<String>,
    last: &Option<u64>,
    before: &Option<String>,
) -> Result<()> {
    if before.is_some() && after.is_some() {
        return Err(Error::CursorNoBeforeAfter.extend());
    }
    if first.is_some() && last.is_some() {
        return Err(Error::CursorNoFirstLast.extend());
    }
    if before.is_some() || last.is_some() {
        return Err(Error::CursorNoReversePagination.extend());
    }
    Ok(())
}
