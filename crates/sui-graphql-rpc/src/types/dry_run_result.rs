// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::base64::Base64;
use super::move_type::MoveType;
use super::transaction_block::TransactionBlock;
use super::transaction_block_kind::programmable::TransactionArgument;
use crate::error::Error;
use async_graphql::*;
use sui_json_rpc_types::{DevInspectResults, SuiExecutionResult};
use sui_types::effects::TransactionEffects as NativeTransactionEffects;
use sui_types::event::Event as NativeEvent;
use sui_types::transaction::TransactionData as NativeTransactionData;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct DryRunResult {
    /// The error that occurred during dry run execution, if any.
    pub error: Option<String>,
    /// The intermediate results of the dry run execution.
    pub results: Option<Vec<DryRunEffect>>,

    #[graphql(skip)]
    pub events: Vec<NativeEvent>,

    #[graphql(skip)]
    pub effects: NativeTransactionEffects,

    #[graphql(skip)]
    pub tx_data: NativeTransactionData,
}

#[ComplexObject]
impl DryRunResult {
    /// The transaction block representing the dry run execution.
    pub async fn transaction(&self) -> Option<TransactionBlock> {
        Some(TransactionBlock::DryRun {
            tx_data: self.tx_data.clone(),
            effects: self.effects.clone(),
            events: self.events.clone(),
        })
    }
}

impl TryFrom<DevInspectResults> for DryRunResult {
    type Error = crate::error::Error;
    fn try_from(results: DevInspectResults) -> Result<Self, Self::Error> {
        let execution_results = results
            .results
            .ok_or(Error::Internal(
                "No execution results returned from dev inspect".to_string(),
            ))?
            .into_iter()
            .map(DryRunEffect::try_from)
            .collect::<Result<Vec<_>, Self::Error>>()?;
        let events = results
            .events
            .data
            .into_iter()
            .map(|e| NativeEvent {
                sender: e.sender,
                package_id: e.package_id,
                transaction_module: e.transaction_module,
                type_: e.type_,
                contents: e.bcs,
            })
            .collect();
        let effects: NativeTransactionEffects =
            bcs::from_bytes(&results.raw_effects).map_err(|e| {
                Error::Internal(format!("Unable to deserialize transaction effects: {e}"))
            })?;
        let tx_data: NativeTransactionData = bcs::from_bytes(&results.raw_txn_data)
            .map_err(|e| Error::Internal(format!("Unable to deserialize transaction data: {e}")))?;
        Ok(Self {
            error: results.error,
            results: Some(execution_results),
            events,
            effects,
            tx_data,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct DryRunEffect {
    /// Changes made to arguments that were mutably borrowed by each command in this transaction.
    pub mutated_references: Option<Vec<DryRunMutation>>,

    /// Return results of each command in this transaction.
    pub return_values: Option<Vec<DryRunReturn>>,
}

impl TryFrom<SuiExecutionResult> for DryRunEffect {
    type Error = crate::error::Error;

    fn try_from(result: SuiExecutionResult) -> Result<Self, Self::Error> {
        let mutated_references = result
            .mutable_reference_outputs
            .iter()
            .map(|(argument, bcs, type_)| {
                Ok(DryRunMutation {
                    input: (*argument).into(),
                    type_: MoveType::new(type_.clone().try_into()?),
                    bcs: bcs.into(),
                })
            })
            .collect::<Result<Vec<_>>>()
            .map_err(|e| {
                Error::Internal(format!(
                    "Failed to parse results returned from dev inspect: {:?}",
                    e
                ))
            })?;
        let return_values = result
            .return_values
            .iter()
            .map(|(bcs, type_)| {
                Ok(DryRunReturn {
                    type_: MoveType::new(type_.clone().try_into()?),
                    bcs: bcs.into(),
                })
            })
            .collect::<Result<Vec<_>>>()
            .map_err(|e| {
                Error::Internal(format!(
                    "Failed to parse results returned from dev inspect: {:?}",
                    e
                ))
            })?;
        Ok(Self {
            mutated_references: Some(mutated_references),
            return_values: Some(return_values),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct DryRunMutation {
    pub input: TransactionArgument,

    #[graphql(name = "type")]
    pub type_: MoveType,

    pub bcs: Base64,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct DryRunReturn {
    #[graphql(name = "type")]
    pub type_: MoveType,

    pub bcs: Base64,
}
