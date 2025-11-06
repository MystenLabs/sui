// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::displays::Pretty;
use std::fmt::{Display, Formatter};
use sui_types::gas::GasUsageReport;
use tabled::{
    builder::Builder as TableBuilder,
    settings::{Style as TableStyle, style::HorizontalLine},
};

impl Display for Pretty<'_, GasUsageReport> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(gas_report) = self;
        display_info(f, gas_report)?;
        per_object_storage_table(f, gas_report)?;
        Ok(())
    }
}

fn per_object_storage_table(f: &mut Formatter, gas_report: &GasUsageReport) -> std::fmt::Result {
    let mut builder = TableBuilder::default();
    builder.push_record(vec!["Object ID", "Bytes", "Old Rebate", "New Rebate"]);
    for (object_id, per_obj_storage) in &gas_report.per_object_storage {
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

fn display_info(f: &mut Formatter<'_>, gas_report: &GasUsageReport) -> std::fmt::Result {
    let mut builder = TableBuilder::default();
    macro_rules! record {
        ($($msg:expr),+) => {
            builder.push_record(vec![$($msg.to_string()),+]);
        };
    }
    record!("Gas Info");

    record!("Computation Cost", gas_report.cost_summary.computation_cost);
    record!("Storage Cost", gas_report.cost_summary.storage_cost);
    record!("Storage Rebate", gas_report.cost_summary.storage_rebate);
    record!(
        "Non-Refundable Storage Fee",
        gas_report.cost_summary.non_refundable_storage_fee
    );
    record!("Gas Used", gas_report.gas_used);
    record!("Gas Budget", gas_report.gas_budget);
    record!("Gas Price", gas_report.gas_price);
    record!("Reference Gas Price", gas_report.reference_gas_price);
    record!("Storage Gas Price", gas_report.storage_gas_price);
    record!("Rebate Rate", gas_report.rebate_rate);

    let mut table = builder.build();
    table.with(TableStyle::rounded());

    write!(f, "\n{}\n", table)
}
