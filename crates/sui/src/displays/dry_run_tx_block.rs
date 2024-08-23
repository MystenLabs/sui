// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{client_commands::estimate_gas_budget_from_gas_cost, displays::Pretty};
use std::fmt::{Display, Formatter};
use sui_json_rpc_types::{
    DryRunTransactionBlockResponse, ObjectChange, SuiTransactionBlockDataAPI,
    SuiTransactionBlockEffectsAPI,
};
use tabled::{
    builder::Builder as TableBuilder,
    settings::{style::HorizontalLine, Panel as TablePanel, Style as TableStyle},
};
impl<'a> Display for Pretty<'a, DryRunTransactionBlockResponse> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(response) = self;

        writeln!(
            f,
            "Dry run completed, execution status: {}",
            response.effects.status()
        )?;

        let mut builder = TableBuilder::default();
        builder.push_record(vec![format!("{}", response.input)]);
        let mut table = builder.build();
        table.with(TablePanel::header("Dry Run Transaction Data"));
        table.with(TableStyle::rounded().horizontals([HorizontalLine::new(
            1,
            TableStyle::modern().get_horizontal(),
        )]));
        writeln!(f, "{}", table)?;
        writeln!(f, "{}", response.effects)?;
        write!(f, "{}", response.events)?;

        if response.object_changes.is_empty() {
            writeln!(f, "╭─────────────────────────────╮")?;
            writeln!(f, "│ No object changes           │")?;
            writeln!(f, "╰─────────────────────────────╯")?;
        } else {
            let mut builder = TableBuilder::default();
            let (
                mut created,
                mut deleted,
                mut mutated,
                mut published,
                mut transferred,
                mut wrapped,
            ) = (vec![], vec![], vec![], vec![], vec![], vec![]);
            for obj in &response.object_changes {
                match obj {
                    ObjectChange::Created { .. } => created.push(obj),
                    ObjectChange::Deleted { .. } => deleted.push(obj),
                    ObjectChange::Mutated { .. } => mutated.push(obj),
                    ObjectChange::Published { .. } => published.push(obj),
                    ObjectChange::Transferred { .. } => transferred.push(obj),
                    ObjectChange::Wrapped { .. } => wrapped.push(obj),
                };
            }

            write_obj_changes(created, "Created", &mut builder)?;
            write_obj_changes(deleted, "Deleted", &mut builder)?;
            write_obj_changes(mutated, "Mutated", &mut builder)?;
            write_obj_changes(published, "Published", &mut builder)?;
            write_obj_changes(transferred, "Transferred", &mut builder)?;
            write_obj_changes(wrapped, "Wrapped", &mut builder)?;

            let mut table = builder.build();
            table.with(TablePanel::header("Object Changes"));
            table.with(TableStyle::rounded().horizontals([HorizontalLine::new(
                1,
                TableStyle::modern().get_horizontal(),
            )]));
            writeln!(f, "{}", table)?;
        }
        if response.balance_changes.is_empty() {
            writeln!(f, "╭─────────────────────────────╮")?;
            writeln!(f, "│ No balance changes          │")?;
            writeln!(f, "╰─────────────────────────────╯")?;
        } else {
            let mut builder = TableBuilder::default();
            for balance in &response.balance_changes {
                builder.push_record(vec![format!("{}", balance)]);
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
            "Dry run completed, execution status: {}",
            response.effects.status()
        )?;
        writeln!(
            f,
            "Estimated gas cost (includes a small buffer): {} MIST",
            estimate_gas_budget_from_gas_cost(
                response.effects.gas_cost_summary(),
                response.input.gas_data().price
            )
        )
    }
}

fn write_obj_changes<T: Display>(
    values: Vec<T>,
    output_string: &str,
    builder: &mut TableBuilder,
) -> std::fmt::Result {
    if !values.is_empty() {
        builder.push_record(vec![format!("{} Objects: ", output_string)]);
        for obj in values {
            builder.push_record(vec![format!("{}", obj)]);
        }
    }
    Ok(())
}
