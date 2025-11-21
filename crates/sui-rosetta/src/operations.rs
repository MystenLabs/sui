// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::ops::Not;
use std::str::FromStr;
use std::vec;

use anyhow::anyhow;
use move_core_types::ident_str;
use move_core_types::language_storage::StructTag;
use prost_types::value::Kind;
use serde::Deserialize;
use serde::Serialize;
use tracing::warn;

use sui_rpc::proto::sui::rpc::v2::Argument;
use sui_rpc::proto::sui::rpc::v2::BalanceChange;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::Input;
use sui_rpc::proto::sui::rpc::v2::MoveCall;
use sui_rpc::proto::sui::rpc::v2::ProgrammableTransaction;
use sui_rpc::proto::sui::rpc::v2::TransactionKind;
use sui_rpc::proto::sui::rpc::v2::argument::ArgumentKind;
use sui_rpc::proto::sui::rpc::v2::command::Command;
use sui_rpc::proto::sui::rpc::v2::input::InputKind;
use sui_rpc::proto::sui::rpc::v2::transaction_kind::Data as TransactionKindData;
use sui_rpc::proto::sui::rpc::v2::transaction_kind::Kind::ProgrammableTransaction as ProgrammableTransactionKind;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::gas_coin::GasCoin;
use sui_types::governance::{ADD_STAKE_FUN_NAME, WITHDRAW_STAKE_FUN_NAME};
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::{SUI_SYSTEM_ADDRESS, SUI_SYSTEM_PACKAGE_ID};

use crate::types::internal_operation::{PayCoin, PaySui, Stake, WithdrawStake};
use crate::types::{
    AccountIdentifier, Amount, CoinAction, CoinChange, CoinID, CoinIdentifier, Currency,
    InternalOperation, OperationIdentifier, OperationStatus, OperationType,
};
use crate::{CoinMetadataCache, Error, SUI};

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
        Ok(InternalOperation::PaySui(PaySui {
            sender,
            recipients,
            amounts,
        }))
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
        Ok(InternalOperation::PayCoin(PayCoin {
            sender,
            recipients,
            amounts,
            currency,
        }))
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

        Ok(InternalOperation::Stake(Stake {
            sender,
            validator,
            amount,
        }))
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

        Ok(InternalOperation::WithdrawStake(WithdrawStake {
            sender,
            stake_ids,
        }))
    }

    pub fn from_transaction(
        tx: TransactionKind,
        sender: SuiAddress,
        status: Option<OperationStatus>,
    ) -> Result<Vec<Operation>, Error> {
        let TransactionKind { data, kind, .. } = tx;
        Ok(match data {
            Some(TransactionKindData::ProgrammableTransaction(pt))
                if status != Some(OperationStatus::Failure) =>
            {
                Self::parse_programmable_transaction(sender, status, pt)?
            }
            data => {
                let mut tx = TransactionKind::default();
                tx.data = data;
                tx.kind = kind;
                vec![Operation::generic_op(status, sender, tx)]
            }
        })
    }

    fn parse_programmable_transaction(
        sender: SuiAddress,
        status: Option<OperationStatus>,
        pt: ProgrammableTransaction,
    ) -> Result<Vec<Operation>, Error> {
        #[derive(Debug)]
        enum KnownValue {
            GasCoin(u64),
        }
        fn resolve_result(
            known_results: &[Vec<KnownValue>],
            i: u32,
            j: u32,
        ) -> Option<&KnownValue> {
            known_results
                .get(i as usize)
                .and_then(|inner| inner.get(j as usize))
        }
        fn split_coins(
            inputs: &[Input],
            known_results: &[Vec<KnownValue>],
            coin: &Argument,
            amounts: &[Argument],
        ) -> Option<Vec<KnownValue>> {
            match coin.kind() {
                ArgumentKind::Gas => (),
                ArgumentKind::Result => {
                    let i = coin.result?;
                    let subresult_idx = coin.subresult.unwrap_or(0);
                    let KnownValue::GasCoin(_) = resolve_result(known_results, i, subresult_idx)?;
                }
                // Might not be a SUI coin
                ArgumentKind::Input => (),
                _ => return None,
            };

            let amounts = amounts
                .iter()
                .map(|amount| {
                    let value: u64 = match amount.kind() {
                        ArgumentKind::Input => {
                            let input_idx = amount.input() as usize;
                            let input = inputs.get(input_idx)?;
                            match input.kind() {
                                InputKind::Pure => {
                                    let bytes = input.pure();
                                    bcs::from_bytes(bytes).ok()?
                                }
                                _ => return None,
                            }
                        }
                        _ => return None,
                    };
                    Some(KnownValue::GasCoin(value))
                })
                .collect::<Option<_>>()?;
            Some(amounts)
        }
        fn transfer_object(
            aggregated_recipients: &mut HashMap<SuiAddress, u64>,
            inputs: &[Input],
            known_results: &[Vec<KnownValue>],
            objs: &[Argument],
            recipient: &Argument,
        ) -> Option<Vec<KnownValue>> {
            let addr = match recipient.kind() {
                ArgumentKind::Input => {
                    let input_idx = recipient.input() as usize;
                    let input = inputs.get(input_idx)?;
                    match input.kind() {
                        InputKind::Pure => {
                            let bytes = input.pure();
                            bcs::from_bytes::<SuiAddress>(bytes).ok()?
                        }
                        _ => return None,
                    }
                }
                _ => return None,
            };
            for obj in objs {
                let i = match obj.kind() {
                    ArgumentKind::Result => obj.result(),
                    _ => return None,
                };

                let subresult_idx = obj.subresult.unwrap_or(0);
                let KnownValue::GasCoin(value) = resolve_result(known_results, i, subresult_idx)?;

                let aggregate = aggregated_recipients.entry(addr).or_default();
                *aggregate += value;
            }
            Some(vec![])
        }
        fn stake_call(
            inputs: &[Input],
            known_results: &[Vec<KnownValue>],
            call: &MoveCall,
        ) -> Result<Option<(Option<u64>, SuiAddress)>, Error> {
            let arguments = &call.arguments;
            let (amount, validator) = match &arguments[..] {
                [_, coin, validator] => {
                    let amount = match coin.kind() {
                        ArgumentKind::Result => {
                            let i = coin
                                .result
                                .ok_or_else(|| anyhow!("Result argument missing index"))?;
                            let KnownValue::GasCoin(value) = resolve_result(known_results, i, 0)
                                .ok_or_else(|| {
                                    anyhow!("Cannot resolve Gas coin value at Result({i})")
                                })?;
                            value
                        }
                        _ => return Ok(None),
                    };
                    let (some_amount, validator) = match validator.kind() {
                        // [WORKAROUND] - this is a hack to work out if the staking ops is for a selected amount or None amount (whole wallet).
                        // We use the position of the validator arg as a indicator of if the rosetta stake
                        // transaction is staking the whole wallet or not, if staking whole wallet,
                        // we have to omit the amount value in the final operation output.
                        ArgumentKind::Input => {
                            let i = validator.input();
                            let validator_addr = match inputs.get(i as usize) {
                                Some(input) if input.kind() == InputKind::Pure => {
                                    bcs::from_bytes::<SuiAddress>(input.pure()).ok()
                                }
                                _ => None,
                            };
                            (i == 1, Ok(validator_addr))
                        }
                        _ => return Ok(None),
                    };
                    (some_amount.then_some(*amount), validator)
                }
                _ => Err(anyhow!(
                    "Error encountered when extracting arguments from move call, expecting 3 elements, got {}",
                    arguments.len()
                ))?,
            };
            validator.map(|v| v.map(|v| (amount, v)))
        }

        fn unstake_call(inputs: &[Input], call: &MoveCall) -> Result<Option<ObjectID>, Error> {
            let arguments = &call.arguments;
            let id = match &arguments[..] {
                [_, stake_id] => match stake_id.kind() {
                    ArgumentKind::Input => {
                        let i = stake_id.input();
                        let id = match inputs.get(i as usize) {
                            Some(input) if input.kind() == InputKind::ImmutableOrOwned => input
                                .object_id
                                .as_ref()
                                .and_then(|oid| ObjectID::from_str(oid).ok()),
                            _ => None,
                        }
                        .ok_or_else(|| anyhow!("Cannot find stake id from input args."))?;
                        // [WORKAROUND] - this is a hack to work out if the withdraw stake ops is for a selected stake or None (all stakes).
                        // this hack is similar to the one in stake_call.
                        let some_id = i % 2 == 1;
                        some_id.then_some(id)
                    }
                    _ => None,
                },
                _ => Err(anyhow!(
                    "Error encountered when extracting arguments from move call, expecting 2 elements, got {}",
                    arguments.len()
                ))?,
            };
            Ok(id)
        }
        let inputs = &pt.inputs;
        let commands = &pt.commands;
        let mut known_results: Vec<Vec<KnownValue>> = vec![];
        let mut aggregated_recipients: HashMap<SuiAddress, u64> = HashMap::new();
        let mut needs_generic = false;
        let mut operations = vec![];
        let mut stake_ids = vec![];
        let mut currency: Option<Currency> = None;

        for command in commands {
            let result = match &command.command {
                Some(Command::SplitCoins(split)) => {
                    let coin = split.coin();
                    split_coins(inputs, &known_results, coin, &split.amounts)
                }
                Some(Command::TransferObjects(transfer)) => {
                    let addr = transfer.address();
                    transfer_object(
                        &mut aggregated_recipients,
                        inputs,
                        &known_results,
                        &transfer.objects,
                        addr,
                    )
                }
                Some(Command::MoveCall(m)) if Self::is_stake_call(m) => {
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
                Some(Command::MoveCall(m)) if Self::is_unstake_call(m) => {
                    let stake_id = unstake_call(inputs, m)?;
                    stake_ids.push(stake_id);
                    Some(vec![])
                }
                Some(Command::MergeCoins(_)) => {
                    // We don't care about merge-coins, we can just skip it.
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
                        currency = inputs.iter().last().and_then(|input| {
                            if input.kind() == InputKind::Pure {
                                let bytes = input.pure();
                                bcs::from_bytes::<String>(bytes).ok().and_then(|json_str| {
                                    serde_json::from_str::<Currency>(&json_str).ok()
                                })
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
            let tx_kind = TransactionKind::default()
                .with_kind(ProgrammableTransactionKind)
                .with_programmable_transaction(pt);
            operations.push(Operation::generic_op(status, sender, tx_kind))
        }
        Ok(operations)
    }

    fn is_stake_call(tx: &MoveCall) -> bool {
        let package_id = match ObjectID::from_str(tx.package()) {
            Ok(id) => id,
            Err(e) => {
                warn!(
                    package = tx.package(),
                    error = %e,
                    "Failed to parse package ID for MoveCall"
                );
                return false;
            }
        };

        package_id == SUI_SYSTEM_PACKAGE_ID
            && tx.module() == SUI_SYSTEM_MODULE_NAME.as_str()
            && tx.function() == ADD_STAKE_FUN_NAME.as_str()
    }

    fn is_unstake_call(tx: &MoveCall) -> bool {
        let package_id = match ObjectID::from_str(tx.package()) {
            Ok(id) => id,
            Err(e) => {
                warn!(
                    package = tx.package(),
                    error = %e,
                    "Failed to parse package ID for MoveCall"
                );
                return false;
            }
        };

        package_id == SUI_SYSTEM_PACKAGE_ID
            && tx.module() == SUI_SYSTEM_MODULE_NAME.as_str()
            && tx.function() == WITHDRAW_STAKE_FUN_NAME.as_str()
    }

    fn process_balance_change(
        gas_owner: SuiAddress,
        gas_used: i128,
        balance_changes: &[(BalanceChange, Currency)],
        status: Option<OperationStatus>,
        balances: HashMap<(SuiAddress, Currency), i128>,
    ) -> impl Iterator<Item = Operation> {
        let mut balances =
            balance_changes
                .iter()
                .fold(balances, |mut balances, (balance_change, ccy)| {
                    if let (Some(addr_str), Some(amount_str)) =
                        (&balance_change.address, &balance_change.amount)
                        && let (Ok(owner), Ok(amount)) =
                            (SuiAddress::from_str(addr_str), i128::from_str(amount_str))
                    {
                        *balances.entry((owner, ccy.clone())).or_default() += amount;
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

    /// Checks to see if transferObjects is used on GasCoin
    fn is_gascoin_transfer(tx: &TransactionKind) -> bool {
        if let Some(TransactionKindData::ProgrammableTransaction(pt)) = &tx.data {
            return pt.commands.iter().any(|command| {
                if let Some(Command::TransferObjects(transfer)) = &command.command {
                    transfer
                        .objects
                        .iter()
                        .any(|arg| arg.kind() == ArgumentKind::Gas)
                } else {
                    false
                }
            });
        }
        false
    }

    /// Add balance-change with zero amount if the gas owner does not have an entry.
    /// An entry is required for gas owner because the balance would be adjusted.
    fn add_missing_gas_owner(operations: &mut Vec<Operation>, gas_owner: SuiAddress) {
        if !operations.iter().any(|operation| {
            if let Some(amount) = &operation.amount
                && let Some(account) = &operation.account
                && account.address == gas_owner
                && amount.currency == *SUI
            {
                return true;
            }
            false
        }) {
            operations.push(Operation::balance_change(
                Some(OperationStatus::Success),
                gas_owner,
                0,
                SUI.clone(),
            ));
        }
    }

    /// Compare initial balance_changes to new_operations and make sure
    /// the balance-changes stay the same after updating the operations
    fn validate_operations(
        initial_balance_changes: &[(BalanceChange, Currency)],
        new_operations: &[Operation],
    ) -> Result<(), anyhow::Error> {
        let balances: HashMap<(SuiAddress, Currency), i128> = HashMap::new();
        let mut initial_balances =
            initial_balance_changes
                .iter()
                .fold(balances, |mut balances, (balance_change, ccy)| {
                    if let (Some(addr_str), Some(amount_str)) =
                        (&balance_change.address, &balance_change.amount)
                        && let (Ok(owner), Ok(amount)) =
                            (SuiAddress::from_str(addr_str), i128::from_str(amount_str))
                    {
                        *balances.entry((owner, ccy.clone())).or_default() += amount;
                    }
                    balances
                });

        let mut new_balances = HashMap::new();
        for op in new_operations {
            if let Some(Amount {
                currency, value, ..
            }) = &op.amount
            {
                if let Some(account) = &op.account {
                    let balance_change = new_balances
                        .remove(&(account.address, currency.clone()))
                        .unwrap_or(0)
                        + value;
                    new_balances.insert((account.address, currency.clone()), balance_change);
                } else {
                    return Err(anyhow!("Missing account for a balance-change"));
                }
            }
        }

        for ((address, currency), amount_expected) in new_balances {
            let new_amount = initial_balances.remove(&(address, currency)).unwrap_or(0);
            if new_amount != amount_expected {
                return Err(anyhow!(
                    "Expected {} balance-change for {} but got {}",
                    amount_expected,
                    address,
                    new_amount
                ));
            }
        }
        if !initial_balances.is_empty() {
            return Err(anyhow!(
                "Expected every item in initial_balances to be mapped"
            ));
        }
        Ok(())
    }

    /// If GasCoin is transferred as a part of transferObjects, operations need to be
    /// updated such that:
    /// 1) gas owner needs to be assigned back to the previous owner
    /// 2) balances of previous and new gas owners need to be adjusted for the gas
    fn process_gascoin_transfer(
        coin_change_operations: &mut impl Iterator<Item = Operation>,
        is_gascoin_transfer: bool,
        prev_gas_owner: SuiAddress,
        new_gas_owner: SuiAddress,
        gas_used: i128,
        initial_balance_changes: &[(BalanceChange, Currency)],
    ) -> Result<Vec<Operation>, anyhow::Error> {
        let mut operations = vec![];
        if is_gascoin_transfer && prev_gas_owner != new_gas_owner {
            operations = coin_change_operations.collect();
            Self::add_missing_gas_owner(&mut operations, prev_gas_owner);
            Self::add_missing_gas_owner(&mut operations, new_gas_owner);
            for operation in &mut operations {
                match operation.type_ {
                    OperationType::Gas => {
                        // change gas account back to the previous owner as it is the one
                        // who paid for the txn (this is the format Rosetta wants to process)
                        operation.account = Some(prev_gas_owner.into())
                    }
                    OperationType::SuiBalanceChange => {
                        let account = operation
                            .account
                            .as_ref()
                            .ok_or_else(|| anyhow!("Missing account for a balance-change"))?;
                        let amount = operation
                            .amount
                            .as_mut()
                            .ok_or_else(|| anyhow!("Missing amount for a balance-change"))?;
                        // adjust the balances for previous and new gas_owners
                        if account.address == prev_gas_owner && amount.currency == *SUI {
                            amount.value -= gas_used;
                        } else if account.address == new_gas_owner && amount.currency == *SUI {
                            amount.value += gas_used;
                        }
                    }
                    _ => {
                        return Err(anyhow!(
                            "Discarding unsupported operation type {:?}",
                            operation.type_
                        ));
                    }
                }
            }
            Self::validate_operations(initial_balance_changes, &operations)?;
        }
        Ok(operations)
    }
}

impl Operations {
    pub async fn try_from_executed_transaction(
        executed_tx: ExecutedTransaction,
        cache: &CoinMetadataCache,
    ) -> Result<Self, Error> {
        let ExecutedTransaction {
            transaction,
            effects,
            events,
            balance_changes,
            ..
        } = executed_tx;

        let transaction = transaction.ok_or_else(|| {
            Error::DataError("ExecutedTransaction missing transaction".to_string())
        })?;
        let effects = effects
            .ok_or_else(|| Error::DataError("ExecutedTransaction missing effects".to_string()))?;

        let sender = SuiAddress::from_str(transaction.sender())?;

        let gas_owner = if effects.gas_object.is_some() {
            let gas_object = effects.gas_object();
            let owner = gas_object.output_owner();
            SuiAddress::from_str(owner.address())?
        } else if sender == SuiAddress::ZERO {
            // System transactions don't have a gas_object.
            sender
        } else {
            return Err(Error::DataError(format!(
                "Non-system transaction (sender={}) missing gas_object: {}",
                sender,
                transaction.digest()
            )));
        };

        let gas_summary = effects.gas_used();
        let gas_used = gas_summary.storage_rebate_opt().unwrap_or(0) as i128
            - gas_summary.storage_cost_opt().unwrap_or(0) as i128
            - gas_summary.computation_cost_opt().unwrap_or(0) as i128;

        let status = Some(effects.status().into());

        let prev_gas_owner = SuiAddress::from_str(transaction.gas_payment().owner())?;

        let tx_kind = transaction
            .kind
            .ok_or_else(|| Error::DataError("Transaction missing kind".to_string()))?;
        let is_gascoin_transfer = Self::is_gascoin_transfer(&tx_kind);
        let ops = Self::new(Self::from_transaction(tx_kind, sender, status)?);
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
        let events = events.as_ref().map(|e| e.events.as_slice()).unwrap_or(&[]);
        for event in events {
            let event_type = event.event_type();
            if let Ok(type_tag) = StructTag::from_str(event_type)
                && is_unstake_event(&type_tag)
                && let Some(json) = &event.json
                && let Some(Kind::StructValue(struct_val)) = &json.kind
            {
                if let Some(principal_field) = struct_val.fields.get("principal_amount")
                    && let Some(Kind::StringValue(s)) = &principal_field.kind
                    && let Ok(amount) = i128::from_str(s)
                {
                    principal_amounts += amount;
                }
                if let Some(reward_field) = struct_val.fields.get("reward_amount")
                    && let Some(Kind::StringValue(s)) = &reward_field.kind
                    && let Ok(amount) = i128::from_str(s)
                {
                    reward_amounts += amount;
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

        let mut balance_changes_with_currency = vec![];

        for balance_change in &balance_changes {
            let coin_type = balance_change.coin_type();
            let type_tag = sui_types::TypeTag::from_str(coin_type)
                .map_err(|e| anyhow!("Invalid coin type: {}", e))?;

            if let Ok(currency) = cache.get_currency(&type_tag).await
                && !currency.symbol.is_empty()
            {
                balance_changes_with_currency.push((balance_change.clone(), currency));
            }
        }

        // Extract coin change operations from balance changes
        let mut coin_change_operations = Self::process_balance_change(
            gas_owner,
            gas_used,
            &balance_changes_with_currency,
            status,
            accounted_balances.clone(),
        );

        // Take {gas, previous gas owner, new gas owner} out of coin_change_operations
        // and convert BalanceChange to PaySui when GasCoin is transferred
        let gascoin_transfer_operations = Self::process_gascoin_transfer(
            &mut coin_change_operations,
            is_gascoin_transfer,
            prev_gas_owner,
            gas_owner,
            gas_used,
            &balance_changes_with_currency,
        )?;

        let ops: Operations = ops
            .into_iter()
            .chain(coin_change_operations)
            .chain(gascoin_transfer_operations)
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
                        && op.type_ != OperationType::Gas
                    {
                        *balances
                            .entry((acc.address, amount.clone().currency))
                            .or_default() += amount.value;
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
}

fn is_unstake_event(tag: &StructTag) -> bool {
    tag.address == SUI_SYSTEM_ADDRESS
        && tag.module.as_ident_str() == ident_str!("validator")
        && tag.name.as_ident_str() == ident_str!("UnstakingRequestEvent")
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

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub enum OperationMetadata {
    GenericTransaction(TransactionKind),
    Stake { validator: SuiAddress },
    WithdrawStake { stake_ids: Vec<ObjectID> },
}

impl Operation {
    fn generic_op(
        status: Option<OperationStatus>,
        sender: SuiAddress,
        tx: TransactionKind,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SUI;
    use crate::types::ConstructionMetadata;
    use sui_rpc::proto::sui::rpc::v2::Transaction;
    use sui_types::base_types::{ObjectDigest, ObjectID, SequenceNumber, SuiAddress};
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::{TEST_ONLY_GAS_UNIT_FOR_TRANSFER, TransactionData};

    #[tokio::test]
    async fn test_operation_data_parsing_pay_sui() -> Result<(), anyhow::Error> {
        let gas = (
            ObjectID::random(),
            SequenceNumber::new(),
            ObjectDigest::random(),
        );

        let sender = SuiAddress::random_for_testing_only();

        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder
                .pay_sui(vec![SuiAddress::random_for_testing_only()], vec![10000])
                .unwrap();
            builder.finish()
        };
        let gas_price = 10;
        let data = TransactionData::new_programmable(
            sender,
            vec![gas],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            gas_price,
        );

        let proto_tx: Transaction = data.clone().into();
        let ops = Operations::new(Operations::from_transaction(
            proto_tx
                .kind
                .ok_or_else(|| Error::DataError("Transaction missing kind".to_string()))?,
            sender,
            None,
        )?);
        ops.0
            .iter()
            .for_each(|op| assert_eq!(op.type_, OperationType::PaySui));
        let metadata = ConstructionMetadata {
            sender,
            gas_coins: vec![gas],
            extra_gas_coins: vec![],
            objects: vec![],
            party_objects: vec![],
            total_coin_value: 0,
            gas_price,
            budget: TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            currency: None,
        };
        let parsed_data = ops.into_internal()?.try_into_data(metadata)?;
        assert_eq!(data, parsed_data);

        Ok(())
    }

    #[tokio::test]
    async fn test_operation_data_parsing_pay_coin() -> Result<(), anyhow::Error> {
        let gas = (
            ObjectID::random(),
            SequenceNumber::new(),
            ObjectDigest::random(),
        );

        let coin = (
            ObjectID::random(),
            SequenceNumber::new(),
            ObjectDigest::random(),
        );

        let sender = SuiAddress::random_for_testing_only();

        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder
                .pay(
                    vec![coin],
                    vec![SuiAddress::random_for_testing_only()],
                    vec![10000],
                )
                .unwrap();
            // the following is important in order to be able to transfer the coin type info between the various flow steps
            builder.pure(serde_json::to_string(&SUI.clone())?)?;
            builder.finish()
        };
        let gas_price = 10;
        let data = TransactionData::new_programmable(
            sender,
            vec![gas],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            gas_price,
        );

        let proto_tx: Transaction = data.clone().into();
        let ops = Operations::new(Operations::from_transaction(
            proto_tx
                .kind
                .ok_or_else(|| Error::DataError("Transaction missing kind".to_string()))?,
            sender,
            None,
        )?);
        ops.0
            .iter()
            .for_each(|op| assert_eq!(op.type_, OperationType::PayCoin));
        let metadata = ConstructionMetadata {
            sender,
            gas_coins: vec![gas],
            extra_gas_coins: vec![],
            objects: vec![coin],
            party_objects: vec![],
            total_coin_value: 0,
            gas_price,
            budget: TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
            currency: Some(SUI.clone()),
        };
        let parsed_data = ops.into_internal()?.try_into_data(metadata)?;
        assert_eq!(data, parsed_data);

        Ok(())
    }
}
