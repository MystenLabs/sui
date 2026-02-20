// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{client_commands::estimate_gas_budget_from_gas_cost, displays::Pretty};
use std::fmt::{Display, Formatter};
use sui_rpc_api::client::SimulateTransactionResponse;
use sui_types::{
    effects::TransactionEffectsAPI,
    execution_status::{ExecutionFailure, ExecutionStatus},
    transaction::TransactionDataAPI,
};
use tabled::{
    builder::Builder as TableBuilder,
    settings::{Panel as TablePanel, Style as TableStyle, style::HorizontalLine},
};

impl Display for Pretty<'_, SimulateTransactionResponse> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(response) = self;

        write!(f, "Dry run completed, execution status: ",)?;
        match response.transaction.effects.status() {
            ExecutionStatus::Success => writeln!(f, "success")?,
            ExecutionStatus::Failure(ExecutionFailure { error, command }) => {
                writeln!(f, "failure")?;
                if let Some(command) = command {
                    writeln!(f, "{error:?} in command {command}")?;
                } else {
                    writeln!(f, "{error:?}")?;
                }
            }
        }

        let mut builder = TableBuilder::default();
        builder.push_record(vec![format!("{:?}", response.transaction.transaction)]);
        let mut table = builder.build();
        table.with(TablePanel::header("Dry Run Transaction Data"));
        table.with(TableStyle::rounded().horizontals([HorizontalLine::new(
            1,
            TableStyle::modern().get_horizontal(),
        )]));
        writeln!(f, "{}", table)?;
        writeln!(f, "{:?}", response.transaction.effects)?;
        write!(f, "{:?}", response.transaction.events)?;

        if response.transaction.changed_objects.is_empty() {
            writeln!(f, "╭─────────────────────────────╮")?;
            writeln!(f, "│ No object changes           │")?;
            writeln!(f, "╰─────────────────────────────╯")?;
        } else {
            writeln!(f, "{:#?}", response.transaction.changed_objects)?;
        }
        if response.transaction.balance_changes.is_empty() {
            writeln!(f, "╭─────────────────────────────╮")?;
            writeln!(f, "│ No balance changes          │")?;
            writeln!(f, "╰─────────────────────────────╯")?;
        } else {
            let mut builder = TableBuilder::default();
            for balance in &response.transaction.balance_changes {
                builder.push_record(vec![format!("{:#?}", balance)]);
            }
            let mut table = builder.build();
            table.with(TablePanel::header("Balance Changes"));
            table.with(TableStyle::rounded().horizontals([HorizontalLine::new(
                1,
                TableStyle::modern().get_horizontal(),
            )]));
            writeln!(f, "{}", table)?;
        }
        writeln!(
            f,
            "Estimated gas cost (includes a small buffer): {} MIST",
            estimate_gas_budget_from_gas_cost(
                response.transaction.effects.gas_cost_summary(),
                response.transaction.transaction.gas_data().price
            )
        )?;

        if let Some(clever_error) = &response.transaction.clever_error {
            writeln!(f, "Execution error: {clever_error:#?}")?;
        }

        Ok(())
    }
}
