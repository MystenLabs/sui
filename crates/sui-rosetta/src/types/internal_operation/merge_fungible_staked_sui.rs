// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use move_core_types::identifier::Identifier;
use prost_types::FieldMask;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use sui_rpc::client::Client;

use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_types::SUI_SYSTEM_PACKAGE_ID;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::transaction::{Command, ObjectArg, ProgrammableTransaction};

use crate::errors::Error;

use super::{TransactionObjectData, TryConstructTransaction, simulate_transaction};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MergeFungibleStakedSui {
    pub sender: SuiAddress,
    pub primary_id: ObjectID,
    pub merge_id: ObjectID,
}

#[async_trait]
impl TryConstructTransaction for MergeFungibleStakedSui {
    async fn try_fetch_needed_objects(
        self,
        client: &mut Client,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionObjectData, Error> {
        let Self {
            sender,
            primary_id,
            merge_id,
        } = self;

        let mut object_refs = Vec::new();
        for object_id in [&primary_id, &merge_id] {
            let address: sui_sdk_types::Address = (*object_id).into();
            let request = GetObjectRequest::new(&address)
                .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"]));

            let response = client
                .ledger_client()
                .get_object(request)
                .await?
                .into_inner();

            if let Some(object) = response.object {
                let oid = object.object_id.ok_or_else(|| {
                    Error::InvalidInput("Object ID missing in response".to_string())
                })?;
                let version = object.version.ok_or_else(|| {
                    Error::InvalidInput("Version missing in response".to_string())
                })?;
                let digest = object
                    .digest
                    .ok_or_else(|| Error::InvalidInput("Digest missing in response".to_string()))?;

                object_refs.push((
                    ObjectID::from_str(&oid).map_err(|e| {
                        Error::InvalidInput(format!("Failed to parse object ID: {}", e))
                    })?,
                    version.into(),
                    digest.parse().map_err(|e| {
                        Error::InvalidInput(format!("Failed to parse digest: {}", e))
                    })?,
                ));
            } else {
                return Err(Error::InvalidInput(format!(
                    "FungibleStakedSui object {} not found",
                    object_id
                )));
            }
        }

        let pt = merge_fungible_staked_sui_pt(object_refs.clone())?;
        let (budget, gas_coin_objs) =
            simulate_transaction(client, pt, sender, vec![], gas_price, budget).await?;

        let total_sui_balance = gas_coin_objs.iter().map(|c| c.balance()).sum::<u64>() as i128;
        let gas_coins = gas_coin_objs
            .iter()
            .map(|obj| obj.object_reference().try_to_object_ref())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TransactionObjectData {
            gas_coins,
            objects: object_refs,
            party_objects: vec![],
            total_sui_balance,
            budget,
            address_balance_withdrawal: 0,
        })
    }
}

pub fn merge_fungible_staked_sui_pt(
    objects: Vec<ObjectRef>,
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();

    let primary_obj = builder.obj(ObjectArg::ImmOrOwnedObject(objects[0]))?;
    let merge_obj = builder.obj(ObjectArg::ImmOrOwnedObject(objects[1]))?;

    builder.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        Identifier::new("staking_pool")?,
        Identifier::new("join_fungible_staked_sui")?,
        vec![],
        vec![primary_obj, merge_obj],
    ));

    Ok(builder.finish())
}
