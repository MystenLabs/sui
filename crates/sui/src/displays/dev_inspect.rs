// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::displays::Pretty;
use std::fmt::{Display, Formatter};
use sui_json_rpc_types::{DevInspectResults, SuiTransactionBlockEffectsAPI};

impl<'a> Display for Pretty<'a, DevInspectResults> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(response) = self;

        if let Some(error) = &response.error {
            writeln!(f, "Dev inspect failed: {}", error)?;
            return Ok(());
        }

        writeln!(
            f,
            "Dev inspect completed, execution status: {}",
            response.effects.status()
        )?;

        writeln!(f, "{}", response.effects)?;
        write!(f, "{}", response.events)?;

        if let Some(results) = &response.results {
            if results.is_empty() {
                writeln!(f, "No execution results")?;
                return Ok(());
            }

            writeln!(f, "Execution Result")?;
            for result in results {
                if !result.mutable_reference_outputs.is_empty() {
                    writeln!(f, "  Mutable Reference Outputs")?;
                    for m in result.mutable_reference_outputs.iter() {
                        writeln!(f, "    Sui Argument: {}", m.0)?;
                        writeln!(f, "    Sui TypeTag: {:?}", m.2)?;
                        writeln!(f, "    Bytes: {:?}", m.1)?;
                    }
                }

                if !result.return_values.is_empty() {
                    writeln!(f, "  Return values")?;

                    for val in result.return_values.iter() {
                        writeln!(f, "    Sui TypeTag: {:?}", val.1)?;
                        writeln!(f, "    Bytes: {:?}", val.0)?;
                    }
                }
            }
        }

        Ok(())
    }
}
