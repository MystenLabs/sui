// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use move_core_types::annotated_value::{MoveStruct, MoveValue};
use sui_types::collection_types::VecMap;

use crate::error::Error;
use sui_json_rpc_types::SuiMoveValue;

use super::{json::Json, move_value::try_to_json_value};

#[derive(Debug, SimpleObject)]
pub(crate) struct DisplayEntry {
    pub key: String,
    pub value: String,
}

pub(crate) fn get_rendered_fields(
    fields: VecMap<String, String>,
    move_struct: &MoveStruct,
) -> Result<Vec<DisplayEntry>, Error> {
    let mut rendered_fields: Vec<DisplayEntry> = vec![];

    for entry in fields.contents.iter() {
        let rendered_value = parse_template(&entry.value, move_struct)?;
        rendered_fields.push(DisplayEntry {
            key: entry.key.clone(),
            value: rendered_value,
        });
    }

    Ok(rendered_fields)
}

// handles the PART = '{' CHAIN '}'
fn parse_template(template: &str, move_struct: &MoveStruct) -> Result<String, Error> {
    let mut output = template.to_string();
    let mut var_name = String::new();
    let mut in_braces = false;
    let mut escaped = false;

    for ch in template.chars() {
        match ch {
            '\\' => {
                escaped = true;
                continue;
            }
            '{' if !escaped => {
                in_braces = true;
                var_name.clear();
            }
            '}' if !escaped => {
                in_braces = false;
                let value = get_value_from_move_struct(move_struct, &var_name)?;
                output = output.replace(&format!("{{{}}}", var_name), &format!("{}", value));
            }
            _ if !escaped => {
                if in_braces {
                    var_name.push(ch);
                }
            }
            _ => {}
        }
        escaped = false;
    }

    Ok(output.replace('\\', ""))
}

pub(crate) fn get_value_from_move_struct(
    move_struct: &MoveStruct,
    var_name: &str,
) -> Result<Json, Error> {
    // Supports CHAIN . IDENT today
    let parts: Vec<&str> = var_name.split('.').collect();
    if parts.is_empty() {
        // todo: custom error
        Err(Error::Internal(
            "Display template value cannot be empty".to_string(),
        ))?;
    }
    // todo: new limit on config
    if parts.len() > 10 {
        Err(Error::Internal(format!(
            "Display template value nested depth cannot exist {}",
            10
        )))?;
    }

    // update this as we iterate through the parts
    let start_value = &MoveValue::Struct(move_struct.clone());

    let result = parts
        .iter()
        .try_fold(start_value, |current_value, part| match current_value {
            MoveValue::Struct(s) => s
                .fields
                .iter()
                .find_map(|(id, value)| {
                    if id.to_string() == *part {
                        Some(value)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| Error::Internal(format!("Field '{}' not found", part))),
            _ => Err(Error::Internal("Unexpected MoveValue".to_string())),
        })?;

    // TODO: implement Display for MoveData and use that instead
    match result {
        MoveValue::Vector(_) => Err(Error::Internal(format!(
            "Vector is not supported as a Display value {}",
            var_name
        )))?,
        _ => Ok(try_to_json_value(result.clone())?.into()),
    }

    // let sui_move_value: SuiMoveValue = result.clone().into();
    // // MoveValue::json_impl(&self, layout: A::MoveTypeLayout)

    // match sui_move_value {
    //     SuiMoveValue::Option(move_option) => match move_option.as_ref() {
    //         Some(move_value) => Ok(move_value.to_string()),
    //         None => Ok("".to_string()),
    //     },
    //     SuiMoveValue::Vector(_) => Err(Error::Internal(format!(
    //         "Vector is not supported as a Display value {}",
    //         var_name
    //     )))?,

    //     _ => Ok(sui_move_value.to_string()),
}
