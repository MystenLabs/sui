// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::{Constant, SignatureToken};
use move_core_types::{account_address::AccountAddress, u256::U256};

pub enum RenderResult {
    AsString(String),
    AsValue(String),
    NotRendered,
}

pub fn try_render_constant(constant: &Constant) -> RenderResult {
    let bytes = &constant.data;
    match &constant.type_ {
        SignatureToken::Vector(inner_ty) if inner_ty.as_ref() == &SignatureToken::U8 => {
            bcs::from_bytes::<Vec<u8>>(bytes)
                .ok()
                .and_then(|x| String::from_utf8(x).ok())
                .map_or(RenderResult::NotRendered, RenderResult::AsString)
        }
        SignatureToken::U8 => bcs::from_bytes::<u8>(bytes)
            .ok()
            .map(|x| x.to_string())
            .map_or(RenderResult::NotRendered, RenderResult::AsValue),
        SignatureToken::U16 => bcs::from_bytes::<u16>(bytes)
            .ok()
            .map(|x| x.to_string())
            .map_or(RenderResult::NotRendered, RenderResult::AsValue),
        SignatureToken::U32 => bcs::from_bytes::<u32>(bytes)
            .ok()
            .map(|x| x.to_string())
            .map_or(RenderResult::NotRendered, RenderResult::AsValue),
        SignatureToken::U64 => bcs::from_bytes::<u64>(bytes)
            .ok()
            .map(|x| x.to_string())
            .map_or(RenderResult::NotRendered, RenderResult::AsValue),
        SignatureToken::U128 => bcs::from_bytes::<u128>(bytes)
            .ok()
            .map(|x| x.to_string())
            .map_or(RenderResult::NotRendered, RenderResult::AsValue),
        SignatureToken::U256 => bcs::from_bytes::<U256>(bytes)
            .ok()
            .map(|x| x.to_string())
            .map_or(RenderResult::NotRendered, RenderResult::AsValue),
        SignatureToken::Address => bcs::from_bytes::<AccountAddress>(bytes)
            .ok()
            .map(|x| x.to_canonical_string(true))
            .map_or(RenderResult::NotRendered, RenderResult::AsValue),
        SignatureToken::Bool => bcs::from_bytes::<bool>(bytes)
            .ok()
            .map(|x| x.to_string())
            .map_or(RenderResult::NotRendered, RenderResult::AsValue),

        SignatureToken::Signer
        | SignatureToken::Vector(_)
        | SignatureToken::Datatype(_)
        | SignatureToken::DatatypeInstantiation(_)
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_)
        | SignatureToken::TypeParameter(_) => RenderResult::NotRendered,
    }
}
