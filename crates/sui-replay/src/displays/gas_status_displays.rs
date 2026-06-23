// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::displays::Pretty;
use std::fmt::{Display, Formatter};
use sui_types::base_types::ObjectID;
use sui_types::gas::{SuiGasStatus, SuiGasStatusAPI};
use sui_types::gas_model::gas_v2::PerObjectStorage;
use tabled::{
    builder::Builder as TableBuilder,
    settings::{Style as TableStyle, style::HorizontalLine},
};

impl Display for Pretty<'_, SuiGasStatus> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(sui_gas_status) = self;
        display_info(f, *sui_gas_status)?;
        display_per_object_storage_table(f, sui_gas_status.per_object_storage())?;
        Ok(())
    }
}

fn display_per_object_storage_table(
    f: &mut Formatter,
    per_object_storage: &[(ObjectID, PerObjectStorage)],
) -> std::fmt::Result {
    let mut builder = TableBuilder::default();
    builder.push_record(vec!["Object ID", "Bytes", "Old Rebate", "New Rebate"]);
    for (object_id, per_obj_storage) in per_object_storage {
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

fn display_info(f: &mut Formatter<'_>, sui_gas_status: &dyn SuiGasStatusAPI) -> std::fmt::Result {
    let move_gas_status = sui_gas_status.move_gas_status();
    let mut builder = TableBuilder::default();
    builder.push_record(vec!["Gas Info".to_string()]);
    builder.push_record(vec![format!(
        "Reference Gas Price: {}",
        sui_gas_status.reference_gas_price()
    )]);
    builder.push_record(vec![format!("Gas Price: {}", move_gas_status.gas_price())]);

    builder.push_record(vec![format!(
        "Max Gas Stack Height: {}",
        move_gas_status.stack_height_high_water_mark()
    )]);

    builder.push_record(vec![format!(
        "Max Gas Stack Size: {}",
        move_gas_status.stack_size_high_water_mark()
    )]);

    builder.push_record(vec![format!(
        "Number of Bytecode Instructions Executed: {}",
        move_gas_status.instructions_executed()
    )]);

    let mut table = builder.build();
    table.with(TableStyle::rounded());

    write!(f, "\n{}\n", table)
}
