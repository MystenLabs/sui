// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt::Write};

use anyhow::{bail, Context};
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
    /// `bytes`, and whose type layout is `layout`. Returns a map from field names to their
    /// interpreted values. Errors are returned per-field (rather than returning the first error
    /// encountered).
    pub fn display(
        &self,
        bytes: &[u8],
        layout: &MoveTypeLayout,
    ) -> BTreeMap<String, anyhow::Result<String>> {
        self.fields
            .iter()
            .map(|(name, strands)| (name.to_string(), interpolate(bytes, layout, strands)))
            .collect()
    }
}

/// Interpret a single format string, composed of a sequence of `Strand`s, fetching the values
/// corresponding to any nested field expressions from a Move object, given by `bytes` (its BCS
/// representation) and `layout` (its type layout).
fn interpolate(
    bytes: &[u8],
    layout: &MoveTypeLayout,
    strands: &[Strand<'_>],
) -> anyhow::Result<String> {
    let mut value = String::new();

    for strand in strands {
        match strand {
            Strand::Text(text) => value.push_str(text.as_ref()),
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
                        bail!("'{strand}' is a vector, and is not supported in Display")
                    }

                    SuiMoveValue::Option(opt) => match opt.as_ref() {
                        Some(v) => write!(value, "{v}").unwrap(),
                        None => {}
                    },

                    v => write!(value, "{v}").unwrap(),
                }
            }
        }
    }

    Ok(value)
}
