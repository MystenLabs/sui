// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod gas_cost_summary;
mod status;
mod summary;

use crate::{client_ptb::ptb::PTBPreview, sp};
use std::fmt::{Display, Formatter};
use tabled::{
    builder::Builder as TableBuilder,
    settings::{style::HorizontalLine, Panel as TablePanel, Style as TableStyle},
};

pub struct Pretty<'a, T>(pub &'a T);

impl<'a> Display for PTBPreview<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut builder = TableBuilder::default();
        let columns = vec!["command", "from"];
        builder.set_header(columns);
        for sp!(loc, cmd) in &self.program.commands {
            builder.push_record([
                cmd.to_string(),
                loc.file_scope.name.to_string(),
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
