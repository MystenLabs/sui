// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::{anyhow, Result};
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use shared_crypto::intent::{Intent, IntentMessage};
use sui_sdk::rpc_types::{
    SuiObjectDataFilter, SuiObjectDataOptions, SuiObjectResponseQuery, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_sdk::types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_sdk::types::coin::COIN_MODULE_NAME;
use sui_sdk::types::crypto::{Signature, SuiKeyPair};
use sui_sdk::types::object::Owner;
use sui_sdk::types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_sdk::types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_sdk::types::transaction::{Command, ObjectArg, Transaction, TransactionData};
use sui_sdk::types::{
    TypeTag, SUI_DENY_LIST_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID,
};
use sui_sdk::SuiClient;
use tracing::info;

use super::AppCommand;
use crate::gas::select_gas;

pub async fn get_deny_list(client: &SuiClient) -> Result<(ObjectID, SequenceNumber)> {
    let resp = client
        .read_api()
        .get_object_with_options(
            SUI_DENY_LIST_OBJECT_ID,
            SuiObjectDataOptions {
                show_type: true,
                show_owner: true,
                show_previous_transaction: false,
                show_display: false,
                show_content: false,
                show_bcs: false,
                show_storage_rebate: false,
            },
        )
        .await?;
    let deny_list = resp.data.ok_or(anyhow!("No deny-list found!"))?;
    let Some(Owner::Shared {
        initial_shared_version,
    }) = deny_list.owner
    else {
        return Err(anyhow!("Invalid deny-list owner!"));
    };
    Ok((SUI_DENY_LIST_OBJECT_ID, initial_shared_version))
}

pub async fn get_deny_cap(
    client: &SuiClient,
    owner_addr: SuiAddress,
    type_tag: TypeTag,
) -> Result<ObjectRef> {
    let resp = client
        .read_api()
        .get_owned_objects(
            owner_addr,
            Some(SuiObjectResponseQuery {
                filter: Some(SuiObjectDataFilter::StructType(StructTag {
                    address: SUI_FRAMEWORK_ADDRESS,
                    module: Identifier::from(COIN_MODULE_NAME),
                    name: Identifier::from_str("DenyCap")?,
                    type_params: vec![type_tag],
                })),
                options: None,
            }),
            None,
            None,
        )
        .await?;

    let deny_cap = resp
        .data
        .into_iter()
        .next()
        .ok_or(anyhow!("No deny-cap found!"))?;
    Ok(deny_cap.data.ok_or(anyhow!("DenyCap empty!"))?.object_ref())
}

#[derive(Debug, Copy, Clone)]
pub enum DenyListCommand {
    Add(SuiAddress),
    Remove(SuiAddress),
}

impl TryFrom<AppCommand> for DenyListCommand {
    type Error = anyhow::Error;

    fn try_from(cmd: AppCommand) -> Result<Self> {
        match cmd {
            AppCommand::DenyListAdd(address) => Ok(DenyListCommand::Add(address)),
            AppCommand::DenyListRemove(address) => Ok(DenyListCommand::Remove(address)),
            _ => Err(anyhow!("Invalid command for deny list")),
        }
    }
}

impl DenyListCommand {
    pub fn address(&self) -> SuiAddress {
        match self {
            DenyListCommand::Add(addr) => *addr,
            DenyListCommand::Remove(addr) => *addr,
        }
    }
}

impl ToString for DenyListCommand {
    fn to_string(&self) -> String {
        match self {
            DenyListCommand::Add(_) => "deny_list_add",
            DenyListCommand::Remove(_) => "deny_list_remove",
        }
        .to_string()
    }
}
// docs::#deny
pub async fn deny_list_add(
    client: &SuiClient,
    signer: &SuiKeyPair,
    otw_type: TypeTag,
    deny_list: (ObjectID, SequenceNumber),
    deny_cap: ObjectRef,
    addr: SuiAddress,
) -> Result<SuiTransactionBlockResponse> {
    info!("ADDING {addr} TO DENY_LIST");
    deny_list_cmd(
        client,
        signer,
        DenyListCommand::Add(addr),
        otw_type,
        deny_list,
        deny_cap,
    )
    .await
}

pub async fn deny_list_remove(
    client: &SuiClient,
    signer: &SuiKeyPair,
    otw_type: TypeTag,
    deny_list: (ObjectID, SequenceNumber),
    deny_cap: ObjectRef,
    addr: SuiAddress,
) -> Result<SuiTransactionBlockResponse> {
    info!("REMOVING {addr} FROM DENY_LIST");
    deny_list_cmd(
        client,
        signer,
        DenyListCommand::Remove(addr),
        otw_type,
        deny_list,
        deny_cap,
    )
    .await
}
// docs::/#deny
async fn deny_list_cmd(
    client: &SuiClient,
    signer: &SuiKeyPair,
    cmd: DenyListCommand,
    otw_type: TypeTag,
    deny_list: (ObjectID, SequenceNumber),
    deny_cap: ObjectRef,
) -> Result<SuiTransactionBlockResponse> {
    let signer_addr = SuiAddress::from(&signer.public());
    let gas_data = select_gas(client, signer_addr, None, None, vec![], None).await?;

    let mut ptb = ProgrammableTransactionBuilder::new();

    let deny_list = ptb.obj(ObjectArg::SharedObject {
        id: deny_list.0,
        initial_shared_version: deny_list.1,
        mutable: true,
    })?;
    let deny_cap = ptb.obj(ObjectArg::ImmOrOwnedObject(deny_cap))?;
    let address = ptb.pure(cmd.address())?;
    ptb.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::from(COIN_MODULE_NAME),
        Identifier::from_str(&cmd.to_string())?,
        vec![otw_type],
        vec![deny_list, deny_cap, address],
    ));

    let builder = ptb.finish();

    // Sign transaction
    let msg = IntentMessage {
        intent: Intent::sui_transaction(),
        value: TransactionData::new_programmable(
            signer_addr,
            vec![gas_data.object],
            builder,
            gas_data.budget,
            gas_data.price,
        ),
    };
    let sig = Signature::new_secure(&msg, signer);

    let res = client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(msg.value, vec![sig]),
            SuiTransactionBlockResponseOptions::new()
                .with_effects()
                .with_object_changes()
                .with_input(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    Ok(res)
}
