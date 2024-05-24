// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

pub async fn render_clever_error_opt(error_string: &str, read_api: &ReadApi) -> Option<String> {
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

fn parse_abort_status_string(
    s: &str,
) -> Result<(AccountAddress, Identifier, Identifier, u16, u64, u16), anyhow::Error> {
    use regex::Regex;
    let re = Regex::new(r"MoveAbort.*address:\s*(.*?),.*Identifier...(.*?)\\.*instruction:\s+(\d+),.*function_name:\s*Some...(\w+?)\\.*},\s*(\d+).*in command\s*(\d+)").unwrap();
    let captures = re.captures(s).unwrap();

    let address = AccountAddress::from_hex(captures.get(1).unwrap().as_str())?;
    let module_name = Identifier::new(captures.get(2).unwrap().as_str())?;
    let instruction = captures.get(3).unwrap().as_str().parse::<u16>()?;
    let function_name = Identifier::new(captures.get(4).unwrap().as_str())?;
    let abort_code = captures.get(5).unwrap().as_str().parse::<u64>()?;
    let command_index = captures.get(6).unwrap().as_str().parse::<u16>()?;
    Ok((
        address,
        module_name,
        function_name,
        instruction,
        abort_code,
        command_index,
    ))
}
