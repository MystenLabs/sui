// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt::Write};

use anyhow::{anyhow, bail, Context};
use move_core_types::{
    annotated_extractor::Extractor,
    annotated_value::{MoveTypeLayout, MoveValue},
};
use parser::{Parser, Strand};
use sui_json_rpc_types::SuiMoveValue;
use sui_types::{
    collection_types::{Entry, VecMap},
    object::bounded_visitor::BoundedVisitor,
};

pub(crate) mod lexer;
pub(crate) mod parser;

/// Format strings extracted from a `Display` object or `DisplayVersionUpdated` event on-chain.
pub struct Format<'s> {
    fields: BTreeMap<&'s str, Vec<Strand<'s>>>,
}

/// A writer that tracks an output budget (measured in bytes) and fails when that budget is hit
/// (and from there on out).
struct BoundedWriter<'b> {
    output: String,
    budget: &'b mut usize,
}

/// Internal error type that distinguishes output budget overflow as a distinct error case.
#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    Error(#[from] anyhow::Error),

    #[error("Output budget exceeded")]
    OutputBudgetExceeded,
}

impl<'s> Format<'s> {
    /// Convert the contents of a `Display` object or `DisplayVersionUpdated` event into a
    /// `Format` string by parsing each of its fields' format strings.
    ///
    /// `max_depth` controls how deeply nested a field access expression can be before it is
    /// considered an error.
    pub fn parse(
        max_depth: usize,
        display_fields: &'s VecMap<String, String>,
    ) -> anyhow::Result<Self> {
        let mut fields = BTreeMap::new();

        for Entry { key, value } in &display_fields.contents {
            let name = key.as_str();
            let parser = Parser::new(max_depth, value);
            let strands = parser
                .parse_format()
                .with_context(|| format!("Failed to parse format for display field {name:?}"))?;

            fields.insert(name, strands);
        }

        Ok(Self { fields })
    }

    /// Interpret the fields of this `Format` structure for the object whose BCS representation is
    /// `bytes`, and whose type layout is `layout`. The `output_budget` limits how big the display
    /// output can be overall (it limits the size of fields and values).
    ///
    /// Returns a map from field names to their interpreted values. Errors are returned per-field
    /// (rather than returning the first error encountered), but the function can fail overall if
    /// the output budget is exceeded.
    pub fn display(
        &self,
        max_output_size: usize,
        bytes: &[u8],
        layout: &MoveTypeLayout,
    ) -> anyhow::Result<BTreeMap<String, anyhow::Result<String>>> {
        let mut output = BTreeMap::new();

        let mut output_budget = max_output_size;
        for (name, strands) in &self.fields {
            match interpolate(&mut output_budget, bytes, layout, strands) {
                Ok(value) if name.len() <= output_budget => {
                    output_budget -= name.len();
                    output.insert(name.to_string(), Ok(value));
                }

                Err(Error::Error(e)) => {
                    output.insert(name.to_string(), Err(e));
                }

                _ => {
                    bail!("Display output too large");
                }
            }
        }

        Ok(output)
    }
}

/// Interpret a single format string, composed of a sequence of `Strand`s, fetching the values
/// corresponding to any nested field expressions from a Move object, given by `bytes` (its BCS
/// representation) and `layout` (its type layout).
fn interpolate(
    output_budget: &mut usize,
    bytes: &[u8],
    layout: &MoveTypeLayout,
    strands: &[Strand<'_>],
) -> Result<String, Error> {
    let mut writer = BoundedWriter::new(output_budget);

    for strand in strands {
        let res = match strand {
            Strand::Text(text) => writer.write_str(text.as_ref()),
            Strand::Expr(path) => {
                let mut visitor = BoundedVisitor::default();
                let mut extractor = Extractor::new(&mut visitor, path);
                let extracted: SuiMoveValue =
                    MoveValue::visit_deserialize(bytes, layout, &mut extractor)
                        .with_context(|| format!("Failed to extract '{strand}'"))?
                        .with_context(|| format!("'{strand}' not found in object"))?
                        .into();

                match extracted {
                    SuiMoveValue::Vector(_) => {
                        return Err(Error::Error(anyhow!(
                            "'{strand}' is a vector, and is not supported in Display"
                        )));
                    }

                    SuiMoveValue::Option(opt) => match opt.as_ref() {
                        Some(v) => write!(writer, "{v}"),
                        None => Ok(()),
                    },

                    v => write!(writer, "{v}"),
                }
            }
        };

        if res.is_err() {
            return Err(Error::OutputBudgetExceeded);
        }
    }

    Ok(writer.finish())
}

impl<'b> BoundedWriter<'b> {
    fn new(budget: &'b mut usize) -> Self {
        Self {
            output: String::new(),
            budget,
        }
    }

    fn finish(self) -> String {
        self.output
    }
}

impl Write for BoundedWriter<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if s.len() > *self.budget {
            return Err(std::fmt::Error);
        }

        self.output.push_str(s);
        *self.budget -= s.len();
        Ok(())
    }
}
