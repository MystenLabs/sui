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
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::transaction::{CallArg, Command, ObjectArg, ProgrammableTransaction};
use sui_types::{SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID};

use crate::errors::Error;

use super::{TransactionObjectData, TryConstructTransaction, simulate_transaction};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RedeemFungibleStakedSui {
    pub sender: SuiAddress,
    pub fungible_staked_sui_id: ObjectID,
}

#[async_trait]
impl TryConstructTransaction for RedeemFungibleStakedSui {
    async fn try_fetch_needed_objects(
        self,
        client: &mut Client,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionObjectData, Error> {
        let Self {
            sender,
            fungible_staked_sui_id,
        } = self;

        let address: sui_sdk_types::Address = fungible_staked_sui_id.into();
        let request = GetObjectRequest::new(&address)
            .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"]));

        let response = client
            .ledger_client()
            .get_object(request)
            .await?
            .into_inner();

        let object_ref = if let Some(object) = response.object {
            let oid = object.object_id.ok_or_else(|| {
                Error::InvalidInput("Object ID missing in response".to_string())
            })?;
            let version = object.version.ok_or_else(|| {
                Error::InvalidInput("Version missing in response".to_string())
            })?;
            let digest = object
                .digest
                .ok_or_else(|| Error::InvalidInput("Digest missing in response".to_string()))?;

            (
                ObjectID::from_str(&oid).map_err(|e| {
                    Error::InvalidInput(format!("Failed to parse object ID: {}", e))
                })?,
                version.into(),
                digest.parse().map_err(|e| {
                    Error::InvalidInput(format!("Failed to parse digest: {}", e))
                })?,
            )
        } else {
            return Err(Error::InvalidInput(format!(
                "FungibleStakedSui object {} not found",
                fungible_staked_sui_id
            )));
        };

        let pt = redeem_fungible_staked_sui_pt(sender, vec![object_ref])?;
        let (budget, gas_coin_objs) =
            simulate_transaction(client, pt, sender, vec![], gas_price, budget).await?;

        let total_sui_balance = gas_coin_objs.iter().map(|c| c.balance()).sum::<u64>() as i128;
        let gas_coins = gas_coin_objs
            .iter()
            .map(|obj| obj.object_reference().try_to_object_ref())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TransactionObjectData {
            gas_coins,
            objects: vec![object_ref],
            party_objects: vec![],
            total_sui_balance,
            budget,
            address_balance_withdrawal: 0,
        })
    }
}

/// Build a 3-command PTB: redeem_fungible_staked_sui -> coin::from_balance -> TransferObjects.
/// The redeem call returns Balance<SUI>, which must be wrapped into Coin<SUI> before transfer.
pub fn redeem_fungible_staked_sui_pt(
    sender: SuiAddress,
    objects: Vec<ObjectRef>,
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();

    let system_state = builder.input(CallArg::SUI_SYSTEM_MUT)?;
    let fungible_obj = builder.obj(ObjectArg::ImmOrOwnedObject(objects[0]))?;

    // cmd 0: 0x3::sui_system::redeem_fungible_staked_sui -> Balance<SUI>
    let balance_result = builder.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        Identifier::new("sui_system")?,
        Identifier::new("redeem_fungible_staked_sui")?,
        vec![],
        vec![system_state, fungible_obj],
    ));

    // cmd 1: 0x2::coin::from_balance<0x2::sui::SUI> -> Coin<SUI>
    let coin_result = builder.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin")?,
        Identifier::new("from_balance")?,
        vec![GAS::type_tag()],
        vec![balance_result],
    ));

    // cmd 2: TransferObjects([Coin<SUI>], sender)
    let sender_arg = builder.pure(sender)?;
    builder.command(Command::TransferObjects(vec![coin_result], sender_arg));

    Ok(builder.finish())
}
