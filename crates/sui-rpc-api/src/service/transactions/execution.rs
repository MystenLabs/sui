// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::SimulateTransactionQueryParameters;
use crate::types::TransactionSimulationResponse;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_sdk_types::framework::Coin;
use sui_sdk_types::Address;
use sui_sdk_types::BalanceChange;
use sui_sdk_types::Object;
use sui_sdk_types::Owner;
use sui_sdk_types::Transaction;
use sui_sdk_types::TransactionEffects;
use sui_types::transaction_executor::SimulateTransactionResult;
use tap::Pipe;

impl RpcService {
    pub fn simulate_transaction(
        &self,
        parameters: &SimulateTransactionQueryParameters,
        transaction: Transaction,
    ) -> Result<TransactionSimulationResponse> {
        let executor = self
            .executor
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No Transaction Executor"))?;

        if transaction.gas_payment.objects.is_empty() {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                "no gas payment provided",
            ));
        }

        let SimulateTransactionResult {
            input_objects,
            output_objects,
            events,
            effects,
            mock_gas_id,
        } = executor
            .simulate_transaction(transaction.try_into()?)
            .map_err(anyhow::Error::from)?;

        if mock_gas_id.is_some() {
            return Err(RpcError::new(
                tonic::Code::Internal,
                "simulate unexpectedly used a mock gas payment",
            ));
        }

        let events = events.map(TryInto::try_into).transpose()?;
        let effects = effects.try_into()?;

        let input_objects = input_objects
            .into_values()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        let output_objects = output_objects
            .into_values()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        let balance_changes = derive_balance_changes(&effects, &input_objects, &output_objects);

        TransactionSimulationResponse {
            events,
            effects,
            balance_changes: parameters.balance_changes.then_some(balance_changes),
            input_objects: parameters.input_objects.then_some(input_objects),
            output_objects: parameters.output_objects.then_some(output_objects),
        }
        .pipe(Ok)
    }
}

fn coins(objects: &[Object]) -> impl Iterator<Item = (&Address, Coin<'_>)> + '_ {
    objects.iter().filter_map(|object| {
        let address = match object.owner() {
            Owner::Address(address) => address,
            Owner::Object(object_id) => object_id.as_address(),
            Owner::Shared { .. } | Owner::Immutable => return None,
        };
        let coin = Coin::try_from_object(object)?;
        Some((address, coin))
    })
}

pub(crate) fn derive_balance_changes(
    _effects: &TransactionEffects,
    input_objects: &[Object],
    output_objects: &[Object],
) -> Vec<BalanceChange> {
    // 1. subtract all input coins
    let balances = coins(input_objects).fold(
        std::collections::BTreeMap::<_, i128>::new(),
        |mut acc, (address, coin)| {
            *acc.entry((address, coin.coin_type().to_owned()))
                .or_default() -= coin.balance() as i128;
            acc
        },
    );

    // 2. add all mutated coins
    let balances = coins(output_objects).fold(balances, |mut acc, (address, coin)| {
        *acc.entry((address, coin.coin_type().to_owned()))
            .or_default() += coin.balance() as i128;
        acc
    });

    balances
        .into_iter()
        .filter_map(|((address, coin_type), amount)| {
            if amount == 0 {
                return None;
            }

            Some(BalanceChange {
                address: *address,
                coin_type,
                amount,
            })
        })
        .collect()
}
