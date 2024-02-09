// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod gas_cost_summary;
mod status;
mod summary;

use crate::ptb::ptb::PTBGas;
use crate::ptb::ptb::PTBPreview;
use std::fmt::{Display, Formatter};
use tabled::{
    builder::Builder as TableBuilder,
    settings::{style::HorizontalLine, Panel as TablePanel, Style as TableStyle},
};

pub struct Pretty<'a, T>(pub &'a T);

impl Display for PTBPreview {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut builder = TableBuilder::default();
        let columns = vec!["command", "from", "value(s)"];
        builder.set_header(columns);
        let mut from = "console";
        for cmd in &self.cmds {
            if cmd.name == "file-include-start" {
                from = cmd.values.first().unwrap();
                continue;
            } else if cmd.name == "file-include-end" {
                from = "console";
                continue;
            } else if cmd.is_preview_false() || cmd.is_warn_shadows_false() {
                continue;
            }
            builder.push_record([
                cmd.name.to_string(),
                from.to_string(),
                cmd.values.join(" ").to_string(),
            ]);
        }
        let mut table = builder.build();
        table.with(TablePanel::header("PTB Preview"));
        table.with(TableStyle::rounded().horizontals([
            HorizontalLine::new(1, TableStyle::modern().get_horizontal()),
            HorizontalLine::new(2, TableStyle::modern().get_horizontal()),
            HorizontalLine::new(2, TableStyle::modern().get_horizontal()),
        ]));
        table.with(tabled::settings::style::BorderSpanCorrection);

        write!(f, "{}", table)
    }
}

impl Display for PTBGas {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let r = match self {
            PTBGas::Min => "min",
            PTBGas::Max => "max",
            PTBGas::Sum => "sum",
        };
        write!(f, "{}", r)
    }
}
