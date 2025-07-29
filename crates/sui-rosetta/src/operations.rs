// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::ops::Not;
use std::str::FromStr;
use std::vec;

use anyhow::anyhow;
use move_core_types::ident_str;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::resolver::ModuleResolver;
use serde::Deserialize;
use serde::Serialize;

use sui_json_rpc_types::SuiProgrammableMoveCall;
use sui_json_rpc_types::SuiProgrammableTransactionBlock;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_json_rpc_types::{BalanceChange, SuiArgument};
use sui_json_rpc_types::{SuiCallArg, SuiCommand};
use sui_rpc::proto::sui::rpc::v2beta2::ExecutedTransaction as ProtoExecutedTransaction;
use sui_sdk::rpc_types::{
    SuiTransactionBlockData, SuiTransactionBlockDataAPI, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockKind,
};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GasCoin;
use sui_types::governance::{ADD_STAKE_FUN_NAME, WITHDRAW_STAKE_FUN_NAME};
use sui_types::object::Owner;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{SenderSignedData, TransactionData, TransactionDataAPI};
use sui_types::{SUI_SYSTEM_ADDRESS, SUI_SYSTEM_PACKAGE_ID};

use crate::types::{
    AccountIdentifier, Amount, CoinAction, CoinChange, CoinID, CoinIdentifier, Currency,
    InternalOperation, OperationIdentifier, OperationStatus, OperationType,
};
use crate::{CoinMetadataCache, Error, SUI};

#[cfg(test)]
#[path = "unit_tests/operations_tests.rs"]
mod operations_tests;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct Operations(Vec<Operation>);

impl FromIterator<Operation> for Operations {
    fn from_iter<T: IntoIterator<Item = Operation>>(iter: T) -> Self {
        Operations::new(iter.into_iter().collect())
    }
}

impl FromIterator<Vec<Operation>> for Operations {
    fn from_iter<T: IntoIterator<Item = Vec<Operation>>>(iter: T) -> Self {
        iter.into_iter().flatten().collect()
    }
}

impl IntoIterator for Operations {
    type Item = Operation;
    type IntoIter = vec::IntoIter<Operation>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Operations {
    pub fn new(mut ops: Vec<Operation>) -> Self {
        for (index, op) in ops.iter_mut().enumerate() {
            op.operation_identifier = (index as u64).into()
        }
        Self(ops)
    }

    pub fn contains(&self, other: &Operations) -> bool {
        for (i, other_op) in other.0.iter().enumerate() {
            if let Some(op) = self.0.get(i) {
                if op != other_op {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    pub fn set_status(mut self, status: Option<OperationStatus>) -> Self {
        for op in &mut self.0 {
            op.status = status
        }
        self
    }

    pub fn type_(&self) -> Option<OperationType> {
        self.0.first().map(|op| op.type_)
    }

    /// Parse operation input from rosetta operation to intermediate internal operation;
    pub fn into_internal(self) -> Result<InternalOperation, Error> {
        let type_ = self
            .type_()
            .ok_or_else(|| Error::MissingInput("Operation type".into()))?;
        match type_ {
            OperationType::PaySui => self.pay_sui_ops_to_internal(),
            OperationType::PayCoin => self.pay_coin_ops_to_internal(),
            OperationType::Stake => self.stake_ops_to_internal(),
            OperationType::WithdrawStake => self.withdraw_stake_ops_to_internal(),
            op => Err(Error::UnsupportedOperation(op)),
        }
    }

    fn pay_sui_ops_to_internal(self) -> Result<InternalOperation, Error> {
        let mut recipients = vec![];
        let mut amounts = vec![];
        let mut sender = None;
        for op in self {
            if let (Some(amount), Some(account)) = (op.amount.clone(), op.account.clone()) {
                if amount.value.is_negative() {
                    sender = Some(account.address)
                } else {
                    recipients.push(account.address);
                    let amount = amount.value.abs();
                    if amount > u64::MAX as i128 {
                        return Err(Error::InvalidInput(
                            "Input amount exceed u64::MAX".to_string(),
                        ));
                    }
                    amounts.push(amount as u64)
                }
            }
        }
        let sender = sender.ok_or_else(|| Error::MissingInput("Sender address".to_string()))?;
        Ok(InternalOperation::PaySui {
            sender,
            recipients,
            amounts,
        })
    }

    fn pay_coin_ops_to_internal(self) -> Result<InternalOperation, Error> {
        let mut recipients = vec![];
        let mut amounts = vec![];
        let mut sender = None;
        let mut currency = None;
        for op in self {
            if let (Some(amount), Some(account)) = (op.amount.clone(), op.account.clone()) {
                currency = currency.or(Some(amount.currency));
                if amount.value.is_negative() {
                    sender = Some(account.address)
                } else {
                    recipients.push(account.address);
                    let amount = amount.value.abs();
                    if amount > u64::MAX as i128 {
                        return Err(Error::InvalidInput(
                            "Input amount exceed u64::MAX".to_string(),
                        ));
                    }
                    amounts.push(amount as u64)
                }
            }
        }
        let sender = sender.ok_or_else(|| Error::MissingInput("Sender address".to_string()))?;
        let currency = currency.ok_or_else(|| Error::MissingInput("Currency".to_string()))?;
        Ok(InternalOperation::PayCoin {
            sender,
            recipients,
            amounts,
            currency,
        })
    }

    fn stake_ops_to_internal(self) -> Result<InternalOperation, Error> {
        let mut ops = self
            .0
            .into_iter()
            .filter(|op| op.type_ == OperationType::Stake)
            .collect::<Vec<_>>();
        if ops.len() != 1 {
            return Err(Error::MalformedOperationError(
                "Delegation should only have one operation.".into(),
            ));
        }
        // Checked above, safe to unwrap.
        let op = ops.pop().unwrap();
        let sender = op
            .account
            .ok_or_else(|| Error::MissingInput("Sender address".to_string()))?
            .address;
        let metadata = op
            .metadata
            .ok_or_else(|| Error::MissingInput("Stake metadata".to_string()))?;

        // Total issued SUi is less than u64, safe to cast.
        let amount = if let Some(amount) = op.amount {
            if amount.value.is_positive() {
                return Err(Error::MalformedOperationError(
                    "Stake amount should be negative.".into(),
                ));
            }
            Some(amount.value.unsigned_abs() as u64)
        } else {
            None
        };

        let OperationMetadata::Stake { validator } = metadata else {
            return Err(Error::InvalidInput(
                "Cannot find delegation info from metadata.".into(),
            ));
        };

        Ok(InternalOperation::Stake {
            sender,
            validator,
            amount,
        })
    }

    fn withdraw_stake_ops_to_internal(self) -> Result<InternalOperation, Error> {
        let mut ops = self
            .0
            .into_iter()
            .filter(|op| op.type_ == OperationType::WithdrawStake)
            .collect::<Vec<_>>();
        if ops.len() != 1 {
            return Err(Error::MalformedOperationError(
                "Delegation should only have one operation.".into(),
            ));
        }
        // Checked above, safe to unwrap.
        let op = ops.pop().unwrap();
        let sender = op
            .account
            .ok_or_else(|| Error::MissingInput("Sender address".to_string()))?
            .address;

        let stake_ids = if let Some(metadata) = op.metadata {
            let OperationMetadata::WithdrawStake { stake_ids } = metadata else {
                return Err(Error::InvalidInput(
                    "Cannot find withdraw stake info from metadata.".into(),
                ));
            };
            stake_ids
        } else {
            vec![]
        };

        Ok(InternalOperation::WithdrawStake { sender, stake_ids })
    }

    fn from_transaction(
        tx: SuiTransactionBlockKind,
        sender: SuiAddress,
        status: Option<OperationStatus>,
    ) -> Result<Vec<Operation>, Error> {
        Ok(match tx {
            SuiTransactionBlockKind::ProgrammableTransaction(pt)
                if status != Some(OperationStatus::Failure) =>
            {
                Self::parse_programmable_transaction(sender, status, pt)?
            }
            _ => vec![Operation::generic_op(status, sender, tx)],
        })
    }

    fn parse_programmable_transaction(
        sender: SuiAddress,
        status: Option<OperationStatus>,
        pt: SuiProgrammableTransactionBlock,
    ) -> Result<Vec<Operation>, Error> {
        #[derive(Debug)]
        enum KnownValue {
            GasCoin(u64),
        }
        fn resolve_result(
            known_results: &[Vec<KnownValue>],
            i: u16,
            j: u16,
        ) -> Option<&KnownValue> {
            known_results
                .get(i as usize)
                .and_then(|inner| inner.get(j as usize))
        }
        fn split_coins(
            inputs: &[SuiCallArg],
            known_results: &[Vec<KnownValue>],
            coin: SuiArgument,
            amounts: &[SuiArgument],
        ) -> Option<Vec<KnownValue>> {
            match coin {
                SuiArgument::Result(i) => {
                    let KnownValue::GasCoin(_) = resolve_result(known_results, i, 0)?;
                }
                SuiArgument::NestedResult(i, j) => {
                    let KnownValue::GasCoin(_) = resolve_result(known_results, i, j)?;
                }
                SuiArgument::GasCoin => (),
                // Might not be a SUI coin
                SuiArgument::Input(_) => (),
            };
            let amounts = amounts
                .iter()
                .map(|amount| {
                    let value: u64 = match *amount {
                        SuiArgument::Input(i) => {
                            u64::from_str(inputs.get(i as usize)?.pure()?.to_json_value().as_str()?)
                                .ok()?
                        }
                        SuiArgument::GasCoin
                        | SuiArgument::Result(_)
                        | SuiArgument::NestedResult(_, _) => return None,
                    };
                    Some(KnownValue::GasCoin(value))
                })
                .collect::<Option<_>>()?;
            Some(amounts)
        }
        fn transfer_object(
            aggregated_recipients: &mut HashMap<SuiAddress, u64>,
            inputs: &[SuiCallArg],
            known_results: &[Vec<KnownValue>],
            objs: &[SuiArgument],
            recipient: SuiArgument,
        ) -> Option<Vec<KnownValue>> {
            let addr = match recipient {
                SuiArgument::Input(i) => inputs.get(i as usize)?.pure()?.to_sui_address().ok()?,
                SuiArgument::GasCoin | SuiArgument::Result(_) | SuiArgument::NestedResult(_, _) => {
                    return None
                }
            };
            for obj in objs {
                let value = match *obj {
                    SuiArgument::Result(i) => {
                        let KnownValue::GasCoin(value) = resolve_result(known_results, i, 0)?;
                        value
                    }
                    SuiArgument::NestedResult(i, j) => {
                        let KnownValue::GasCoin(value) = resolve_result(known_results, i, j)?;
                        value
                    }
                    SuiArgument::GasCoin | SuiArgument::Input(_) => return None,
                };
                let aggregate = aggregated_recipients.entry(addr).or_default();
                *aggregate += value;
            }
            Some(vec![])
        }
        fn stake_call(
            inputs: &[SuiCallArg],
            known_results: &[Vec<KnownValue>],
            call: &SuiProgrammableMoveCall,
        ) -> Result<Option<(Option<u64>, SuiAddress)>, Error> {
            let SuiProgrammableMoveCall { arguments, .. } = call;
            let (amount, validator) = match &arguments[..] {
                [_, coin, validator] => {
                    let amount = match coin {
                        SuiArgument::Result(i) =>{
                            let KnownValue::GasCoin(value) = resolve_result(known_results, *i, 0).ok_or_else(||anyhow!("Cannot resolve Gas coin value at Result({i})"))?;
                            value
                        },
                        _ => return Ok(None),
                    };
                    let (some_amount, validator) = match validator {
                        // [WORKAROUND] - this is a hack to work out if the staking ops is for a selected amount or None amount (whole wallet).
                        // We use the position of the validator arg as a indicator of if the rosetta stake
                        // transaction is staking the whole wallet or not, if staking whole wallet,
                        // we have to omit the amount value in the final operation output.
                        SuiArgument::Input(i) => (*i==1, inputs.get(*i as usize).and_then(|input| input.pure()).map(|v|v.to_sui_address()).transpose()),
                        _=> return Ok(None),
                    };
                    (some_amount.then_some(*amount), validator)
                },
                _ => Err(anyhow!("Error encountered when extracting arguments from move call, expecting 3 elements, got {}", arguments.len()))?,
            };
            Ok(validator.map(|v| v.map(|v| (amount, v)))?)
        }

        fn unstake_call(
            inputs: &[SuiCallArg],
            call: &SuiProgrammableMoveCall,
        ) -> Result<Option<ObjectID>, Error> {
            let SuiProgrammableMoveCall { arguments, .. } = call;
            let id = match &arguments[..] {
                [_, stake_id] => {
                    match stake_id {
                        SuiArgument::Input(i) => {
                            let id = inputs.get(*i as usize).and_then(|input| input.object()).ok_or_else(|| anyhow!("Cannot find stake id from input args."))?;
                            // [WORKAROUND] - this is a hack to work out if the withdraw stake ops is for a selected stake or None (all stakes).
                            // this hack is similar to the one in stake_call.
                            let some_id = i % 2 == 1;
                            some_id.then_some(id)
                        },
                        _=> return Ok(None),
                    }
                },
                _ => Err(anyhow!("Error encountered when extracting arguments from move call, expecting 3 elements, got {}", arguments.len()))?,
            };
            Ok(id.cloned())
        }
        let SuiProgrammableTransactionBlock { inputs, commands } = &pt;
        let mut known_results: Vec<Vec<KnownValue>> = vec![];
        let mut aggregated_recipients: HashMap<SuiAddress, u64> = HashMap::new();
        let mut needs_generic = false;
        let mut operations = vec![];
        let mut stake_ids = vec![];
        let mut currency: Option<Currency> = None;
        for command in commands {
            let result = match command {
                SuiCommand::SplitCoins(coin, amounts) => {
                    split_coins(inputs, &known_results, *coin, amounts)
                }
                SuiCommand::TransferObjects(objs, addr) => transfer_object(
                    &mut aggregated_recipients,
                    inputs,
                    &known_results,
                    objs,
                    *addr,
                ),
                SuiCommand::MoveCall(m) if Self::is_stake_call(m) => {
                    stake_call(inputs, &known_results, m)?.map(|(amount, validator)| {
                        let amount = amount.map(|amount| Amount::new(-(amount as i128), None));
                        operations.push(Operation {
                            operation_identifier: Default::default(),
                            type_: OperationType::Stake,
                            status,
                            account: Some(sender.into()),
                            amount,
                            coin_change: None,
                            metadata: Some(OperationMetadata::Stake { validator }),
                        });
                        vec![]
                    })
                }
                SuiCommand::MoveCall(m) if Self::is_unstake_call(m) => {
                    let stake_id = unstake_call(inputs, m)?;
                    stake_ids.push(stake_id);
                    Some(vec![])
                }
                _ => None,
            };
            if let Some(result) = result {
                known_results.push(result)
            } else {
                needs_generic = true;
                break;
            }
        }

        if !needs_generic && !aggregated_recipients.is_empty() {
            let total_paid: u64 = aggregated_recipients.values().copied().sum();
            operations.extend(
                aggregated_recipients
                    .into_iter()
                    .map(|(recipient, amount)| {
                        currency = inputs.iter().last().and_then(|arg| {
                            if let SuiCallArg::Pure(value) = arg {
                                let bytes = value
                                    .value()
                                    .to_json_value()
                                    .as_array()?
                                    .clone()
                                    .into_iter()
                                    .map(|v| v.as_u64().map(|n| n as u8))
                                    .collect::<Option<Vec<u8>>>()?;
                                bcs::from_bytes::<String>(&bytes)
                                    .ok()
                                    .and_then(|bcs_str| serde_json::from_str(&bcs_str).ok())
                            } else {
                                None
                            }
                        });
                        match currency {
                            Some(_) => Operation::pay_coin(
                                status,
                                recipient,
                                amount.into(),
                                currency.clone(),
                            ),
                            None => Operation::pay_sui(status, recipient, amount.into()),
                        }
                    }),
            );
            match currency {
                Some(_) => operations.push(Operation::pay_coin(
                    status,
                    sender,
                    -(total_paid as i128),
                    currency.clone(),
                )),
                _ => operations.push(Operation::pay_sui(status, sender, -(total_paid as i128))),
            }
        } else if !stake_ids.is_empty() {
            let stake_ids = stake_ids.into_iter().flatten().collect::<Vec<_>>();
            let metadata = stake_ids
                .is_empty()
                .not()
                .then_some(OperationMetadata::WithdrawStake { stake_ids });
            operations.push(Operation {
                operation_identifier: Default::default(),
                type_: OperationType::WithdrawStake,
                status,
                account: Some(sender.into()),
                amount: None,
                coin_change: None,
                metadata,
            });
        } else if operations.is_empty() {
            operations.push(Operation::generic_op(
                status,
                sender,
                SuiTransactionBlockKind::ProgrammableTransaction(pt),
            ))
        }
        Ok(operations)
    }

    fn is_stake_call(tx: &SuiProgrammableMoveCall) -> bool {
        tx.package == SUI_SYSTEM_PACKAGE_ID
            && tx.module == SUI_SYSTEM_MODULE_NAME.as_str()
            && tx.function == ADD_STAKE_FUN_NAME.as_str()
    }

    fn is_unstake_call(tx: &SuiProgrammableMoveCall) -> bool {
        tx.package == SUI_SYSTEM_PACKAGE_ID
            && tx.module == SUI_SYSTEM_MODULE_NAME.as_str()
            && tx.function == WITHDRAW_STAKE_FUN_NAME.as_str()
    }

    fn process_balance_change(
        gas_owner: SuiAddress,
        gas_used: i128,
        balance_changes: Vec<(BalanceChange, Currency)>,
        status: Option<OperationStatus>,
        balances: HashMap<(SuiAddress, Currency), i128>,
    ) -> impl Iterator<Item = Operation> {
        let mut balances =
            balance_changes
                .iter()
                .fold(balances, |mut balances, (balance_change, ccy)| {
                    // Rosetta only care about address owner
                    if let Owner::AddressOwner(owner) = balance_change.owner {
                        *balances.entry((owner, ccy.clone())).or_default() += balance_change.amount;
                    }
                    balances
                });
        // separate gas from balances
        *balances.entry((gas_owner, SUI.clone())).or_default() -= gas_used;

        let balance_change = balances.into_iter().filter(|(_, amount)| *amount != 0).map(
            move |((addr, currency), amount)| {
                Operation::balance_change(status, addr, amount, currency)
            },
        );

        let gas = if gas_used != 0 {
            vec![Operation::gas(gas_owner, gas_used)]
        } else {
            // Gas can be 0 for system tx
            vec![]
        };
        balance_change.chain(gas)
    }
}

impl Operations {
    fn try_from_data(
        data: SuiTransactionBlockData,
        status: Option<OperationStatus>,
    ) -> Result<Self, anyhow::Error> {
        let sender = *data.sender();
        Ok(Self::new(Self::from_transaction(
            data.transaction().clone(),
            sender,
            status,
        )?))
    }
}
impl Operations {
    pub async fn try_from_grpc_response(
        response: sui_rpc::proto::sui::rpc::v2beta2::ExecutedTransaction,
        cache: &CoinMetadataCache,
    ) -> Result<Self, Error> {
        use sui_types::effects::TransactionEffects;
        use sui_types::transaction::{Transaction, TransactionKind};

        // Extract transaction BCS data
        let tx_bcs = response
            .transaction
            .as_ref()
            .and_then(|t| t.bcs.as_ref())
            .and_then(|bcs| bcs.value.as_ref())
            .ok_or_else(|| Error::DataError("Missing transaction BCS data".to_string()))?;

        // Deserialize transaction to get sender and transaction data
        let transaction: Transaction = bcs::from_bytes(tx_bcs)
            .map_err(|e| Error::DataError(format!("Failed to deserialize transaction: {}", e)))?;

        let tx_data = transaction.transaction_data();
        let sender = tx_data.sender();

        // Extract effects BCS data
        let effects_bcs = response
            .effects
            .as_ref()
            .and_then(|e| e.bcs.as_ref())
            .and_then(|bcs| bcs.value.as_ref())
            .ok_or_else(|| Error::DataError("Missing effects BCS data".to_string()))?;

        // Deserialize effects
        let effects: TransactionEffects = bcs::from_bytes(effects_bcs)
            .map_err(|e| Error::DataError(format!("Failed to deserialize effects: {}", e)))?;

        let (_gas_obj_ref, gas_obj_owner) = effects.gas_object();
        let gas_owner = gas_obj_owner
            .get_owner_address()
            .map_err(|e| Error::DataError(format!("Failed to get gas owner: {}", e)))?;
        let gas_summary = effects.gas_cost_summary();
        let gas_used = gas_summary.storage_rebate as i128
            - gas_summary.storage_cost as i128
            - gas_summary.computation_cost as i128;

        let status = if effects.status().is_ok() {
            Some(OperationStatus::Success)
        } else {
            Some(OperationStatus::Failure)
        };

        // Parse transaction kind to extract operations
        let mut ops = vec![];
        match tx_data {
            TransactionData::V1(v1) => {
                match &v1.kind {
                    TransactionKind::ProgrammableTransaction(pt) => {
                        // For programmable transactions, analyze the commands to determine operation type
                        // This is a simplified version - in production we'd parse all commands
                        let has_stake_call = pt.commands.iter().any(|cmd| {
                            matches!(cmd, sui_types::transaction::Command::MoveCall(call)
                                if call.package == SUI_SYSTEM_PACKAGE_ID
                                && call.module.as_str() == "sui_system"
                                && (call.function.as_str() == "request_add_stake" || call.function.as_str() == "request_withdraw_stake"))
                        });

                        if has_stake_call {
                            // For stake operations, we'll let the balance change processing handle the details
                            ops.push(Operation {
                                operation_identifier: Default::default(),
                                type_: OperationType::Stake,
                                status,
                                account: Some(sender.into()),
                                amount: None,
                                coin_change: None,
                                metadata: None,
                            });
                        } else {
                            // For other programmable transactions, create a pay operation
                            // The actual amounts will be determined by balance changes
                            ops.push(Operation {
                                operation_identifier: Default::default(),
                                type_: OperationType::PaySui,
                                status,
                                account: Some(sender.into()),
                                amount: None,
                                coin_change: None,
                                metadata: None,
                            });
                        }
                    }
                    _ => {
                        // For non-programmable transactions (Genesis, ChangeEpoch, etc)
                        ops.push(Operation {
                            operation_identifier: Default::default(),
                            type_: OperationType::Genesis,
                            status,
                            account: Some(sender.into()),
                            amount: None,
                            coin_change: None,
                            metadata: None,
                        });
                    }
                }
            }
        }

        let ops = Operations::new(ops);
        let ops = ops.into_iter();

        // We will need to subtract the operation amounts from the actual balance
        // change amount extracted from event to prevent double counting.
        let mut accounted_balances =
            ops.as_ref()
                .iter()
                .fold(HashMap::new(), |mut balances, op| {
                    if let (Some(acc), Some(amount), Some(OperationStatus::Success)) =
                        (&op.account, &op.amount, &op.status)
                    {
                        *balances
                            .entry((acc.address, amount.clone().currency))
                            .or_default() -= amount.value;
                    }
                    balances
                });

        let mut principal_amounts = 0;
        let mut reward_amounts = 0;

        // Extract balance change from unstake events
        if let Some(events) = &response.events {
            if let Some(events_bcs) = &events.bcs {
                if let Some(events_data) = &events_bcs.value {
                    if let Ok(events) =
                        bcs::from_bytes::<sui_types::effects::TransactionEvents>(events_data)
                    {
                        for event in &events.data {
                            if is_unstake_event(&event.type_) {
                                // Parse event data to extract amounts
                                if let Ok(parsed_json) =
                                    serde_json::from_slice::<serde_json::Value>(&event.contents)
                                {
                                    let principal_amount = parsed_json
                                        .pointer("/principal_amount")
                                        .and_then(|v| v.as_str())
                                        .and_then(|v| i128::from_str(v).ok());
                                    let reward_amount = parsed_json
                                        .pointer("/reward_amount")
                                        .and_then(|v| v.as_str())
                                        .and_then(|v| i128::from_str(v).ok());
                                    if let (Some(principal_amount), Some(reward_amount)) =
                                        (principal_amount, reward_amount)
                                    {
                                        principal_amounts += principal_amount;
                                        reward_amounts += reward_amount;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let staking_balance = if principal_amounts != 0 {
            *accounted_balances.entry((sender, SUI.clone())).or_default() -= principal_amounts;
            *accounted_balances.entry((sender, SUI.clone())).or_default() -= reward_amounts;
            vec![
                Operation::stake_principle(status, sender, principal_amounts),
                Operation::stake_reward(status, sender, reward_amounts),
            ]
        } else {
            vec![]
        };

        let mut balance_changes = vec![];

        // Convert proto balance changes to sui_json_rpc_types::BalanceChange
        for proto_bc in &response.balance_changes {
            let address: SuiAddress = proto_bc
                .address
                .as_ref()
                .ok_or_else(|| Error::DataError("Missing address in balance change".to_string()))?
                .parse()
                .map_err(|e| {
                    Error::DataError(format!("Invalid address in balance change: {}", e))
                })?;

            let coin_type = proto_bc.coin_type.as_ref().ok_or_else(|| {
                Error::DataError("Missing coin type in balance change".to_string())
            })?;

            let amount_str = proto_bc
                .amount
                .as_ref()
                .ok_or_else(|| Error::DataError("Missing amount in balance change".to_string()))?;
            let amount = amount_str.parse::<i128>().map_err(|e| {
                Error::DataError(format!("Invalid amount in balance change: {}", e))
            })?;

            // Parse coin type
            let type_tag = coin_type
                .parse()
                .map_err(|e| Error::DataError(format!("Invalid coin type: {}", e)))?;

            // Get currency info from cache
            if let Ok(currency) = cache.get_currency(&type_tag).await {
                if !currency.symbol.is_empty() {
                    let balance_change = BalanceChange {
                        owner: Owner::AddressOwner(address),
                        coin_type: type_tag,
                        amount,
                    };
                    balance_changes.push((balance_change, currency));
                }
            }
        }

        // Extract coin change operations from balance changes
        let coin_change_operations = Self::process_balance_change(
            gas_owner,
            gas_used,
            balance_changes,
            status,
            accounted_balances,
        );

        let ops: Operations = ops
            .into_iter()
            .chain(coin_change_operations)
            .chain(staking_balance)
            .collect();

        // This is a workaround for the payCoin cases that are mistakenly considered to be paySui operations
        let mutually_cancelling_balances: HashMap<_, _> = ops
            .clone()
            .into_iter()
            .fold(
                HashMap::new(),
                |mut balances: HashMap<(SuiAddress, Currency), i128>, op| {
                    if let (Some(acc), Some(amount)) = (&op.account, &op.amount) {
                        *balances
                            .entry((acc.address, amount.clone().currency))
                            .or_default() += amount.value;
                    }
                    balances
                },
            )
            .into_iter()
            .filter(|(_, balance)| *balance == 0)
            .collect();

        // Filter out operations that cancel each other
        let filtered_ops: Operations = ops
            .into_iter()
            .filter(|op| {
                if let (Some(acc), Some(amount)) = (&op.account, &op.amount) {
                    !mutually_cancelling_balances
                        .contains_key(&(acc.address, amount.clone().currency))
                } else {
                    true
                }
            })
            .collect();

        Ok(filtered_ops)
    }

    pub async fn try_from_response(
        response: SuiTransactionBlockResponse,
        cache: &CoinMetadataCache,
    ) -> Result<Self, Error> {
        let tx = response
            .transaction
            .ok_or_else(|| anyhow!("Response input should not be empty"))?;
        let sender = *tx.data.sender();
        let effect = response
            .effects
            .ok_or_else(|| anyhow!("Response effects should not be empty"))?;
        let gas_owner = effect.gas_object().owner.get_owner_address()?;
        let gas_summary = effect.gas_cost_summary();
        let gas_used = gas_summary.storage_rebate as i128
            - gas_summary.storage_cost as i128
            - gas_summary.computation_cost as i128;

        let status = Some(effect.into_status().into());
        let ops = Operations::try_from_data(tx.data, status)?;
        let ops = ops.into_iter();

        // We will need to subtract the operation amounts from the actual balance
        // change amount extracted from event to prevent double counting.
        let mut accounted_balances =
            ops.as_ref()
                .iter()
                .fold(HashMap::new(), |mut balances, op| {
                    if let (Some(acc), Some(amount), Some(OperationStatus::Success)) =
                        (&op.account, &op.amount, &op.status)
                    {
                        *balances
                            .entry((acc.address, amount.clone().currency))
                            .or_default() -= amount.value;
                    }
                    balances
                });

        let mut principal_amounts = 0;
        let mut reward_amounts = 0;
        // Extract balance change from unstake events

        if let Some(events) = response.events {
            for event in events.data {
                if is_unstake_event(&event.type_) {
                    let principal_amount = event
                        .parsed_json
                        .pointer("/principal_amount")
                        .and_then(|v| v.as_str())
                        .and_then(|v| i128::from_str(v).ok());
                    let reward_amount = event
                        .parsed_json
                        .pointer("/reward_amount")
                        .and_then(|v| v.as_str())
                        .and_then(|v| i128::from_str(v).ok());
                    if let (Some(principal_amount), Some(reward_amount)) =
                        (principal_amount, reward_amount)
                    {
                        principal_amounts += principal_amount;
                        reward_amounts += reward_amount;
                    }
                }
            }
        }
        let staking_balance = if principal_amounts != 0 {
            *accounted_balances.entry((sender, SUI.clone())).or_default() -= principal_amounts;
            *accounted_balances.entry((sender, SUI.clone())).or_default() -= reward_amounts;
            vec![
                Operation::stake_principle(status, sender, principal_amounts),
                Operation::stake_reward(status, sender, reward_amounts),
            ]
        } else {
            vec![]
        };

        let mut balance_changes = vec![];

        for balance_change in &response
            .balance_changes
            .ok_or_else(|| anyhow!("Response balance changes should not be empty."))?
        {
            if let Ok(currency) = cache.get_currency(&balance_change.coin_type).await {
                if !currency.symbol.is_empty() {
                    balance_changes.push((balance_change.clone(), currency));
                }
            }
        }

        // Extract coin change operations from balance changes
        let coin_change_operations = Self::process_balance_change(
            gas_owner,
            gas_used,
            balance_changes,
            status,
            accounted_balances,
        );

        let ops: Operations = ops
            .into_iter()
            .chain(coin_change_operations)
            .chain(staking_balance)
            .collect();

        // This is a workaround for the payCoin cases that are mistakenly considered to be paySui operations
        // In this case we remove any irrelevant, SUI specific operation entries that sum up to 0 balance changes per address
        // and keep only the actual entries for the right coin type transfers, as they have been extracted from the transaction's
        // balance changes section.
        let mutually_cancelling_balances: HashMap<_, _> = ops
            .clone()
            .into_iter()
            .fold(
                HashMap::new(),
                |mut balances: HashMap<(SuiAddress, Currency), i128>, op| {
                    if let (Some(acc), Some(amount), Some(OperationStatus::Success)) =
                        (&op.account, &op.amount, &op.status)
                    {
                        if op.type_ != OperationType::Gas {
                            *balances
                                .entry((acc.address, amount.clone().currency))
                                .or_default() += amount.value;
                        }
                    }
                    balances
                },
            )
            .into_iter()
            .filter(|balance| {
                let (_, amount) = balance;
                *amount == 0
            })
            .collect();

        let ops: Operations = ops
            .clone()
            .into_iter()
            .filter(|op| {
                if let (Some(acc), Some(amount)) = (&op.account, &op.amount) {
                    return op.type_ == OperationType::Gas
                        || !mutually_cancelling_balances
                            .contains_key(&(acc.address, amount.clone().currency));
                }
                true
            })
            .collect();

        Ok(ops)
    }

    pub async fn try_from_proto_transaction(
        proto_tx: ProtoExecutedTransaction,
        cache: &CoinMetadataCache,
    ) -> Result<Self, Error> {
        // Extract transaction data from BCS
        let tx_data = if let Some(tx) = &proto_tx.transaction {
            if let Some(bcs) = &tx.bcs {
                // Deserialize BCS to get SenderSignedData, then extract transaction data
                let bcs_bytes = &bcs
                    .value
                    .as_deref()
                    .ok_or_else(|| anyhow!("BCS value missing"))?;
                let sender_signed_data: SenderSignedData = bcs::from_bytes(bcs_bytes)
                    .map_err(|e| anyhow!("Failed to deserialize transaction BCS: {}", e))?;
                sender_signed_data.transaction_data().clone()
            } else {
                return Err(anyhow!("Transaction BCS data missing").into());
            }
        } else {
            return Err(anyhow!("Transaction data missing").into());
        };

        // Convert to SuiTransactionBlockData for compatibility with existing logic
        struct NoOpsModuleResolver;
        impl ModuleResolver for NoOpsModuleResolver {
            type Error = Error;
            fn get_module(&self, _id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
                Ok(None)
            }
        }
        let sui_tx_data = SuiTransactionBlockData::try_from_with_module_cache(
            tx_data.clone(),
            &&mut NoOpsModuleResolver,
        )?;

        // Extract transaction effects and determine status
        let effects = proto_tx
            .effects
            .ok_or_else(|| anyhow!("Transaction effects should not be empty"))?;

        let status = effects.status.map(|s| match s.success {
            Some(true) => OperationStatus::Success,
            _ => OperationStatus::Failure,
        });

        // Extract sender and gas information
        let sender = tx_data.sender();
        let gas_owner = effects
            .gas_object
            .as_ref()
            .and_then(|go| go.output_owner.as_ref())
            .and_then(|owner| owner.address.as_ref())
            .and_then(|addr| addr.parse::<SuiAddress>().ok())
            .unwrap_or(sender);

        let gas_used = effects
            .gas_used
            .as_ref()
            .map(|gcs| {
                gcs.storage_rebate.unwrap_or(0) as i128
                    - gcs.storage_cost.unwrap_or(0) as i128
                    - gcs.computation_cost.unwrap_or(0) as i128
            })
            .unwrap_or(0);

        // Extract operations from transaction data using existing logic
        let ops = Operations::try_from_data(sui_tx_data, status)?;
        let ops_iter = ops.into_iter();

        // Convert proto balance changes to JSON-RPC format for compatibility
        let mut proto_balance_changes = vec![];
        for balance_change in &proto_tx.balance_changes {
            if let (Some(address_str), Some(amount_str), Some(coin_type_str)) = (
                balance_change.address.as_ref(),
                balance_change.amount.as_ref(),
                balance_change.coin_type.as_ref(),
            ) {
                if let (Ok(address), Ok(amount), Ok(coin_type)) = (
                    address_str.parse::<SuiAddress>(),
                    amount_str.parse::<i128>(),
                    coin_type_str.parse(),
                ) {
                    let currency = cache.get_currency(&coin_type).await?;

                    let json_balance_change = BalanceChange {
                        owner: Owner::AddressOwner(address),
                        coin_type,
                        amount,
                    };
                    proto_balance_changes.push((json_balance_change, currency));
                }
            }
        }

        // We need to calculate accounted balances from existing operations
        let mut accounted_balances = HashMap::new();
        for op in ops_iter.as_ref().iter() {
            if let (Some(acc), Some(amount), Some(OperationStatus::Success)) =
                (&op.account, &op.amount, &op.status)
            {
                *accounted_balances
                    .entry((acc.address, amount.clone().currency))
                    .or_default() += amount.value;
            }
        }

        // Extract coin change operations from balance changes using existing logic
        let coin_change_operations = Self::process_balance_change(
            gas_owner,
            gas_used,
            proto_balance_changes,
            status,
            accounted_balances,
        );

        let ops: Operations = ops_iter.into_iter().chain(coin_change_operations).collect();

        Ok(ops)
    }
}

fn is_unstake_event(tag: &StructTag) -> bool {
    tag.address == SUI_SYSTEM_ADDRESS
        && tag.module.as_ident_str() == ident_str!("validator")
        && tag.name.as_ident_str() == ident_str!("UnstakingRequestEvent")
}

impl TryFrom<TransactionData> for Operations {
    type Error = Error;
    fn try_from(data: TransactionData) -> Result<Self, Self::Error> {
        struct NoOpsModuleResolver;
        impl ModuleResolver for NoOpsModuleResolver {
            type Error = Error;
            fn get_module(&self, _id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
                Ok(None)
            }
        }
        // Rosetta don't need the call args to be parsed into readable format
        Ok(Operations::try_from_data(
            SuiTransactionBlockData::try_from_with_module_cache(data, &&mut NoOpsModuleResolver)?,
            None,
        )?)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Operation {
    operation_identifier: OperationIdentifier,
    #[serde(rename = "type")]
    pub type_: OperationType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<OperationStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountIdentifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<Amount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coin_change: Option<CoinChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<OperationMetadata>,
}

impl PartialEq for Operation {
    fn eq(&self, other: &Self) -> bool {
        self.operation_identifier == other.operation_identifier
            && self.type_ == other.type_
            && self.account == other.account
            && self.amount == other.amount
            && self.coin_change == other.coin_change
            && self.metadata == other.metadata
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub enum OperationMetadata {
    GenericTransaction(SuiTransactionBlockKind),
    Stake { validator: SuiAddress },
    WithdrawStake { stake_ids: Vec<ObjectID> },
}

impl Operation {
    fn generic_op(
        status: Option<OperationStatus>,
        sender: SuiAddress,
        tx: SuiTransactionBlockKind,
    ) -> Self {
        Operation {
            operation_identifier: Default::default(),
            type_: (&tx).into(),
            status,
            account: Some(sender.into()),
            amount: None,
            coin_change: None,
            metadata: Some(OperationMetadata::GenericTransaction(tx)),
        }
    }

    pub fn genesis(index: u64, sender: SuiAddress, coin: GasCoin) -> Self {
        Operation {
            operation_identifier: index.into(),
            type_: OperationType::Genesis,
            status: Some(OperationStatus::Success),
            account: Some(sender.into()),
            amount: Some(Amount::new(coin.value().into(), None)),
            coin_change: Some(CoinChange {
                coin_identifier: CoinIdentifier {
                    identifier: CoinID {
                        id: *coin.id(),
                        version: SequenceNumber::new(),
                    },
                },
                coin_action: CoinAction::CoinCreated,
            }),
            metadata: None,
        }
    }

    fn pay_sui(status: Option<OperationStatus>, address: SuiAddress, amount: i128) -> Self {
        Operation {
            operation_identifier: Default::default(),
            type_: OperationType::PaySui,
            status,
            account: Some(address.into()),
            amount: Some(Amount::new(amount, None)),
            coin_change: None,
            metadata: None,
        }
    }

    fn pay_coin(
        status: Option<OperationStatus>,
        address: SuiAddress,
        amount: i128,
        currency: Option<Currency>,
    ) -> Self {
        Operation {
            operation_identifier: Default::default(),
            type_: OperationType::PayCoin,
            status,
            account: Some(address.into()),
            amount: Some(Amount::new(amount, currency)),
            coin_change: None,
            metadata: None,
        }
    }

    fn balance_change(
        status: Option<OperationStatus>,
        addr: SuiAddress,
        amount: i128,
        currency: Currency,
    ) -> Self {
        Self {
            operation_identifier: Default::default(),
            type_: OperationType::SuiBalanceChange,
            status,
            account: Some(addr.into()),
            amount: Some(Amount::new(amount, Some(currency))),
            coin_change: None,
            metadata: None,
        }
    }
    fn gas(addr: SuiAddress, amount: i128) -> Self {
        Self {
            operation_identifier: Default::default(),
            type_: OperationType::Gas,
            status: Some(OperationStatus::Success),
            account: Some(addr.into()),
            amount: Some(Amount::new(amount, None)),
            coin_change: None,
            metadata: None,
        }
    }
    fn stake_reward(status: Option<OperationStatus>, addr: SuiAddress, amount: i128) -> Self {
        Self {
            operation_identifier: Default::default(),
            type_: OperationType::StakeReward,
            status,
            account: Some(addr.into()),
            amount: Some(Amount::new(amount, None)),
            coin_change: None,
            metadata: None,
        }
    }
    fn stake_principle(status: Option<OperationStatus>, addr: SuiAddress, amount: i128) -> Self {
        Self {
            operation_identifier: Default::default(),
            type_: OperationType::StakePrinciple,
            status,
            account: Some(addr.into()),
            amount: Some(Amount::new(amount, None)),
            coin_change: None,
            metadata: None,
        }
    }
}
