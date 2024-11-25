use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::governance::WITHDRAW_STAKE_FUN_NAME;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{CallArg, Command, ObjectArg, ProgrammableTransaction};
use sui_types::SUI_SYSTEM_PACKAGE_ID;

use crate::errors::Error;

use super::{TransactionAndObjectData, TryConstructTransaction};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawStake {
    pub sender: SuiAddress,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stake_ids: Vec<ObjectID>,
}

#[async_trait]
impl TryConstructTransaction for WithdrawStake {
    async fn try_fetch_needed_objects(
        self,
        _client: &SuiClient,
        _gas_price: Option<u64>,
        _budget: Option<u64>,
    ) -> Result<TransactionAndObjectData, Error> {
        todo!();
    }
}

pub fn withdraw_stake_pt(
    stake_objs: Vec<ObjectRef>,
    withdraw_all: bool,
) -> anyhow::Result<ProgrammableTransaction> {
    let mut builder = ProgrammableTransactionBuilder::new();

    for stake_id in stake_objs {
        // [WORKAROUND] - this is a hack to work out if the withdraw stake ops is for selected stake_ids or None (all stakes) using the index of the call args.
        // if stake_ids is not empty, id input will be created after the system object input
        let (system_state, id) = if !withdraw_all {
            let system_state = builder.input(CallArg::SUI_SYSTEM_MUT)?;
            let id = builder.obj(ObjectArg::ImmOrOwnedObject(stake_id))?;
            (system_state, id)
        } else {
            let id = builder.obj(ObjectArg::ImmOrOwnedObject(stake_id))?;
            let system_state = builder.input(CallArg::SUI_SYSTEM_MUT)?;
            (system_state, id)
        };

        let arguments = vec![system_state, id];
        builder.command(Command::move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            WITHDRAW_STAKE_FUN_NAME.to_owned(),
            vec![],
            arguments,
        ));
    }
    Ok(builder.finish())
}
