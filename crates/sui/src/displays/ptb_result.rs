// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::client_commands::SuiClientCommandResult::PTB;
use crate::client_ptb::ptb::Summary;
use crate::displays::Pretty;
use std::fmt::{Display, Formatter};

use tabled::{
    builder::Builder as TableBuilder,
    settings::{style::HorizontalLine, Panel as TablePanel, Style as TableStyle},
};
impl<'a> Display for Pretty<'a, PTB<'a>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some((transaction_response, program_metadata)) = self.0 {
            if let Some(effects) = transaction_response.effects.as_ref() {
                if effects.status().is_err() {
                    return Err(anyhow!(
                        "PTB execution {}. Transaction digest is: {}",
                        Pretty(effects.status()),
                        effects.transaction_digest()
                    ));
                }
            }
            let summary = {
                let effects = transaction_response.effects.as_ref().ok_or_else(|| {
                    anyhow!("Internal error: no transaction effects after PTB was executed.")
                })?;
                Summary {
                    digest: transaction_response.digest,
                    status: effects.status().clone(),
                    gas_cost: effects.gas_cost_summary().clone(),
                }
            };

            if program_metadata.json_set {
                let json_string = if program_metadata.summary_set {
                    serde_json::to_string_pretty(&serde_json::json!(summary))
                        .map_err(|_| anyhow!("Cannot serialize PTB result to json"))?
                } else {
                    serde_json::to_string_pretty(&serde_json::json!(transaction_response))
                        .map_err(|_| anyhow!("Cannot serialize PTB result to json"))?
                };
                writeln!(f, "{}", json_string)
            } else if program_metadata.summary_set {
                writeln!(f, "{}", Pretty(&summary))
            } else {
                writeln!(f, "{}", transaction_response)
            }
        } else {
            Ok(())
        }
    }
}
