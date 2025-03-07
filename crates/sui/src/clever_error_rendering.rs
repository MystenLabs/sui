// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module provides a function to render a Move abort status string into a more human-readable
//! error message using the clever error rendering logic.
//!
//! The logic in this file is largely a stop-gap to provide Clever Error rendering in the CLI while
//! it still uses the JSON-RPC API. The new GraphQL API already rendered Clever Errors on the server
//! side in a much more robust and efficient way.
//!
//! Once the CLI is updated to use the GraphQL API, this file can be removed, and the GraphQL-based
//! rendering logic for Clever Errors should be used instead.

use fastcrypto::encoding::{Base64, Encoding};
use move_binary_format::{
    binary_config::BinaryConfig, file_format::SignatureToken, CompiledModule,
};
use move_command_line_common::{
    display::{try_render_constant, RenderResult},
    error_bitset::ErrorBitset,
};
use move_core_types::account_address::AccountAddress;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiRawData};
use sui_sdk::apis::ReadApi;
use sui_types::{base_types::ObjectID, Identifier};

/// Take a Move abort status string and render it into a more human-readable error message using
/// by parsing the string (as best we can) and seeing if the abort code is a Clever Error abort
/// code. If it is, we attempt to render the error in a more huma-readable manner using the Read
/// API and decoding the Clever Error encoding in the abort code.
///
/// This function is used to render Clever Errors for on-chain errors only within the Sui CLI. This
/// function is _not_ used at all for off-chain errors or Move unit tests. You should only use this
/// function within this crate.
pub(crate) async fn render_clever_error_opt(
    error_string: &str,
    read_api: &ReadApi,
) -> Option<String> {
    let (address, module_name, function_name, instruction, abort_code, command_index) =
        parse_abort_status_string(error_string).ok()?;

    let error = 'error: {
        let Some(error_bitset) = ErrorBitset::from_u64(abort_code) else {
            break 'error format!(
                "function '{}::{}::{}' at instruction {} with code {}",
                address.to_canonical_display(true),
                module_name,
                function_name,
                instruction,
                abort_code
            );
        };

        let line_number = error_bitset.line_number()?;

        if error_bitset.constant_index().is_none() && error_bitset.identifier_index().is_none() {
            break 'error format!(
                "function '{}::{}::{}' at line {}",
                address.to_canonical_display(true),
                module_name,
                function_name,
                line_number
            );
        }

        let SuiRawData::Package(package) = read_api
            .get_object_with_options(
                ObjectID::from_address(address),
                SuiObjectDataOptions::bcs_lossless(),
            )
            .await
            .ok()?
            .into_object()
            .ok()?
            .bcs?
        else {
            return None;
        };

        let module = package.module_map.get(module_name.as_str())?;
        let module =
            CompiledModule::deserialize_with_config(module, &BinaryConfig::standard()).ok()?;

        let error_identifier_constant = module
            .constant_pool()
            .get(error_bitset.identifier_index()? as usize)?;
        let error_value_constant = module
            .constant_pool()
            .get(error_bitset.constant_index()? as usize)?;

        if !matches!(&error_identifier_constant.type_, SignatureToken::Vector(x) if x.as_ref() == &SignatureToken::U8)
        {
            return None;
        };

        let error_identifier = bcs::from_bytes::<Vec<u8>>(&error_identifier_constant.data)
            .ok()
            .and_then(|x| String::from_utf8(x).ok())?;

        let const_str = match try_render_constant(error_value_constant) {
            RenderResult::NotRendered => {
                format!("'{}'", Base64::encode(&error_value_constant.data))
            }
            RenderResult::AsString(s) => format!("'{s}'"),
            RenderResult::AsValue(v_str) => v_str,
        };

        format!(
            "function '{}::{}::{}' at line {}. Aborted with '{}' -- {}",
            address.to_canonical_display(true),
            module_name,
            function_name,
            line_number,
            error_identifier,
            const_str
        )
    };

    // Convert the command index into an ordinal.
    let command = command_index + 1;
    let suffix = match command % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    };

    Some(format!("{command}{suffix} command aborted within {error}"))
}

/// Parsing the error with a regex is not great, but it's the best we can do with the current
/// JSON-RPC API since we only get error messages as strings. This function attempts to parse a
/// Move abort status string into its different parts, and then parses it back into the structured
/// format that we can then use to render a Clever Error.
///
/// If we are able to parse the string, we return a tuple with the address, module name, function
/// name, instruction, abort code, and command index. If we are unable to parse the string, we
/// return `Err`.
///
/// You should delete this function with glee once the CLI is updated to use the GraphQL API.
fn parse_abort_status_string(
    s: &str,
) -> Result<(AccountAddress, Identifier, Identifier, u16, u64, u16), anyhow::Error> {
    use regex::Regex;
    let re = Regex::new(r#"MoveAbort.*address:\s*(.*?),.* name:.*Identifier\((.*?)\).*instruction:\s+(\d+),.*function_name:.*Some\((.*?)\).*},\s*(\d+).*in command\s*(\d+)"#).unwrap();
    let Some(captures) = re.captures(s) else {
        anyhow::bail!(
            "Cannot parse abort status string: {} as a move abort string",
            s
        );
    };

    // Remove any escape characters from the string if present.
    let clean_string = |s: &str| s.replace(['\\', '\"'], "");

    let address = AccountAddress::from_hex(&captures[1])?;
    let module_name = Identifier::new(clean_string(&captures[2]))?;
    let instruction = captures[3].parse::<u16>()?;
    let function_name = Identifier::new(clean_string(&captures[4]))?;
    let abort_code = captures[5].parse::<u64>()?;
    let command_index = captures[6].parse::<u16>()?;
    Ok((
        address,
        module_name,
        function_name,
        instruction,
        abort_code,
        command_index,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_abort_status_string() {
        let corpus = vec![
            r#"Failure { error: "MoveAbort(MoveLocation { module: ModuleId { address: 60197a0c146e31dd12689e890208767fe2fefb2f726710b4a9fa0b857f7a2c20, name: Identifier(\"clever_errors\") }, function: 0, instruction: 1, function_name: Some(\"aborter\") }, 0) in command 0" }"#,
            r#"Failure { error: "MoveAbort(MoveLocation { module: ModuleId { address: 60197a0c146e31dd12689e890208767fe2fefb2f726710b4a9fa0b857f7a2c20, name: Identifier(\"clever_errors\") }, function: 1, instruction: 1, function_name: Some(\"aborter_line_no\") }, 9223372105574252543) in command 0" }"#,
            r#"Failure { error: "MoveAbort(MoveLocation { module: ModuleId { address: 60197a0c146e31dd12689e890208767fe2fefb2f726710b4a9fa0b857f7a2c20, name: Identifier(\"clever_errors\") }, function: 2, instruction: 1, function_name: Some(\"clever_aborter\") }, 9223372118459154433) in command 0" }"#,
            r#"Failure { error: "MoveAbort(MoveLocation { module: ModuleId { address: 60197a0c146e31dd12689e890208767fe2fefb2f726710b4a9fa0b857f7a2c20, name: Identifier(\"clever_errors\") }, function: 3, instruction: 1, function_name: Some(\"clever_aborter_not_a_string\") }, 9223372135639154691) in command 0" }"#,
            r#"MoveAbort(MoveLocation { module: ModuleId { address: 24bf9e624820625ac1e38076901421d2630b2b225b638aaf0b85264b857a608b, name: Identifier(\"tester\") }, function: 0, instruction: 1, function_name: Some(\"test\") }, 9223372071214514177) in command 0"#,
            r#"MoveAbort(MoveLocation { module: ModuleId { address: 24bf9e624820625ac1e38076901421d2630b2b225b638aaf0b85264b857a608b, name: Identifier("tester") }, function: 0, instruction: 1, function_name: Some("test") }, 9223372071214514177) in command 0"#,
        ];
        let parsed: Vec<_> = corpus.into_iter().map(|c| {
            let (address, module_name, function_name, instruction, abort_code, command_index) =
                parse_abort_status_string(c).unwrap();
            format!("original abort message: {}\n address: {}\n module_name: {}\n function_name: {}\n instruction: {}\n abort_code: {}\n command_index: {}", c, address, module_name, function_name, instruction, abort_code, command_index)
        }).collect();
        insta::assert_snapshot!(parsed.join("\n------\n"));
    }
}
