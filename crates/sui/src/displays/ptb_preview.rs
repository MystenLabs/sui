// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    client_ptb::{
        ast::{GAS_BUDGET, GAS_COIN, JSON, SUMMARY, WARN_SHADOWS},
        ptb::PTBPreview,
    },
    sp,
};
use std::fmt::{Display, Formatter};
use tabled::{
    builder::Builder as TableBuilder,
    settings::{style::HorizontalLine, Panel as TablePanel, Style as TableStyle},
};

impl<'a> Display for PTBPreview<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut builder = TableBuilder::default();
        let columns = vec!["command", "values"];
        builder.set_header(columns);
        for sp!(_, cmd) in &self.program.commands {
            if let Some((command, vals)) = cmd.to_string().split_once(' ') {
                builder.push_record([command, vals]);
            }
        }
        if let Some(gas_budget) = self.program_metadata.gas_budget {
            builder.push_record([GAS_BUDGET, gas_budget.value.to_string().as_str()]);
        }
        if let Some(gas_coin_id) = self.program_metadata.gas_object_id {
            builder.push_record([GAS_COIN, gas_coin_id.value.to_string().as_str()]);
        }
        if self.program_metadata.json_set {
            builder.push_record([JSON, "true"]);
        }
        if self.program_metadata.summary_set {
            builder.push_record([SUMMARY, "true"]);
        }
        if self.program.warn_shadows_set {
            builder.push_record([WARN_SHADOWS, "true"]);
        }
        // while theoretically it cannot happen because parsing the PTB requires at least a
        // gas-budget which leads to having at least 1 row,
        // check that there are actual rows in the table
        if builder.count_rows() < 1 {
            return write!(f, "PTB is empty.");
        }
        let mut table = builder.build();
        table.with(TablePanel::header("PTB Preview"));
        table.with(TableStyle::rounded().horizontals([
            HorizontalLine::new(1, TableStyle::modern().get_horizontal()),
            HorizontalLine::new(2, TableStyle::modern().get_horizontal()),
        ]));
        table.with(tabled::settings::style::BorderSpanCorrection);

        write!(f, "{}", table)
    }
}
