// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::displays::Pretty;
use std::fmt::{Display, Formatter};
use sui_types::gas::SuiGasStatus;
use sui_types::gas_model::gas_v2::SuiGasStatus as GasStatusV2;
use tabled::{
    builder::Builder as TableBuilder,
    settings::{style::HorizontalLine, Style as TableStyle},
};

impl<'a> Display for Pretty<'a, SuiGasStatus> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(sui_gas_status) = self;
        match sui_gas_status {
            SuiGasStatus::V2(s) => {
                display_info(f, s)?;
                per_object_storage_table(f, s)?;
            }
        };
        Ok(())
    }
}

fn per_object_storage_table(f: &mut Formatter, sui_gas_status: &GasStatusV2) -> std::fmt::Result {
    let mut builder = TableBuilder::default();
    builder.push_record(vec!["Object ID", "Bytes", "Old Rebate", "New Rebate"]);
    for (object_id, per_obj_storage) in sui_gas_status.per_object_storage() {
        builder.push_record(vec![
            object_id.to_string(),
            per_obj_storage.new_size.to_string(),
            per_obj_storage.storage_rebate.to_string(),
            per_obj_storage.storage_cost.to_string(),
        ]);
    }
    let mut table = builder.build();

    table.with(TableStyle::rounded().horizontals([HorizontalLine::new(
        1,
        TableStyle::modern().get_horizontal(),
    )]));
    write!(f, "\n{}\n", table)
}

fn display_info(f: &mut Formatter<'_>, sui_gas_status: &GasStatusV2) -> std::fmt::Result {
    let mut builder = TableBuilder::default();
    builder.push_record(vec!["Gas Info".to_string()]);
    builder.push_record(vec![format!(
        "Reference Gas Price: {}",
        sui_gas_status.reference_gas_price()
    )]);
    builder.push_record(vec![format!(
        "Gas Price: {}",
        sui_gas_status.gas_status.gas_price()
    )]);

    builder.push_record(vec![format!(
        "Max Gas Stack Height: {}",
        sui_gas_status.gas_status.stack_height_high_water_mark()
    )]);

    builder.push_record(vec![format!(
        "Max Gas Stack Size: {}",
        sui_gas_status.gas_status.stack_size_high_water_mark()
    )]);

    builder.push_record(vec![format!(
        "Number of Bytecode Instructions Executed: {}",
        sui_gas_status.gas_status.instructions_executed()
    )]);

    let mut table = builder.build();
    table.with(TableStyle::rounded());

    write!(f, "\n{}\n", table)
}
