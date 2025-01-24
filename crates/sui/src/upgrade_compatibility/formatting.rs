// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Error};
use move_binary_format::normalized::{Field, Type};
use move_bytecode_source_map::source_map::SourceName;
use move_core_types::identifier::Identifier;
use move_ir_types::location::Loc;
use regex::Regex;
use std::fmt;
use std::sync::LazyLock;

pub(super) struct FormattedType<'f> {
    type_: &'f Type,
    type_params: &'f [SourceName],
}

impl<'f> fmt::Display for FormattedType<'f> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            write!(
                f,
                "{}",
                format_param(self.type_, self.type_params.to_vec(), &mut Vec::new())
                    .map_err(|_| fmt::Error)?,
            )
        } else {
            write!(
                f,
                "'{}'",
                format_param(self.type_, self.type_params.to_vec(), &mut Vec::new())
                    .map_err(|_| fmt::Error)?,
            )
        }
    }
}

pub(super) enum FormattedIdentifier<'f> {
    Positional(usize),
    Named(&'f Identifier),
}

pub(super) struct FormattedField<'f> {
    pub(super) identifier: FormattedIdentifier<'f>,
    pub(super) type_: FormattedType<'f>,
}

impl<'f> FormattedField<'f> {
    pub(super) fn new(f: &'f Field, type_params: &'f [SourceName]) -> Self {
        static RE_POS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^pos(\d+)$").unwrap());
        let identifier = if let Some(ix) = RE_POS
            .captures(f.name.as_str())
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<usize>().ok())
        {
            FormattedIdentifier::Positional(ix)
        } else {
            FormattedIdentifier::Named(&f.name)
        };

        FormattedField {
            identifier,
            type_: FormattedType {
                type_: &f.type_,
                type_params,
            },
        }
    }
}

impl<'f> fmt::Display for FormattedField<'f> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use FormattedIdentifier as FI;
        match self.identifier {
            FI::Positional(_) if f.alternate() => write!(f, "a positional field"),
            FI::Named(name) if f.alternate() => write!(f, "'{name}'"),

            FI::Positional(ix) => write!(f, "'{:#}' at position {ix}", self.type_),
            FI::Named(name) => write!(f, "'{name}: {:#}'", self.type_),
        }
    }
}

/// Returns a string representation of a parameter and updates its secondary label to include its location.
pub(super) fn format_param(
    param: &Type,
    type_params: Vec<SourceName>,
    secondary: &mut Vec<(Loc, String)>,
) -> Result<String, Error> {
    Ok(match param {
        Type::TypeParameter(t) => {
            let type_param = type_params
                .get(*t as usize)
                .context("Unable to get type param location")?;

            secondary.push((
                type_param.1,
                format!("Type parameter '{}' is defined here", &type_param.0),
            ));
            type_param.0.to_string()
        }
        Type::Vector(t) => {
            format!("vector<{}>", format_param(t, type_params, secondary)?)
        }
        Type::MutableReference(t) => {
            format!("&mut {}", format_param(t, type_params, secondary)?)
        }
        Type::Reference(t) => {
            format!("&{}", format_param(t, type_params, secondary)?)
        }
        Type::Struct {
            address,
            module,
            name,
            type_arguments,
            ..
        } if !type_arguments.is_empty() => {
            format!(
                "{}::{}::{}<{}>",
                address.to_hex_literal(),
                module,
                name,
                type_arguments
                    .iter()
                    .map(|t| format_param(t, type_params.clone(), secondary))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ")
            )
        }
        _ => format!("{}", param),
    })
}

/// Format a list of items into a human-readable string.
pub(super) fn format_list(
    items: impl IntoIterator<Item = impl std::fmt::Display>,
    noun_singular_plural: Option<(&str, &str)>,
) -> String {
    let items: Vec<_> = items.into_iter().map(|i| i.to_string()).collect();
    let items_string = match items.len() {
        0 => "none".to_string(),
        1 => items[0].to_string(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => {
            let all_but_last = &items[..items.len() - 1].join(", ");
            let last = items.last().expect("unexpected empty list");
            format!("{}, and {}", all_but_last, last)
        }
    };
    if let Some((singular, plural)) = noun_singular_plural {
        format!(
            "{}: {}",
            singular_or_plural(items.len(), singular, plural),
            items_string,
        )
    } else {
        items_string
    }
}

/// Returns a string with the singular or plural form of a word based on a count.
pub(super) fn singular_or_plural(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        singular.to_string()
    } else {
        plural.to_string()
    }
}
