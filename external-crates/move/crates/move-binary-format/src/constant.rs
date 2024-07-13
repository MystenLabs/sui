// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::file_format::{Constant, SignatureToken};

use move_core_types::runtime_value::{MoveTypeLayout, MoveValue};

fn sig_to_ty(sig: &SignatureToken) -> Option<MoveTypeLayout> {
    match sig {
        SignatureToken::Signer => Some(MoveTypeLayout::Signer),
        SignatureToken::Address => Some(MoveTypeLayout::Address),
        SignatureToken::Bool => Some(MoveTypeLayout::Bool),
        SignatureToken::U8 => Some(MoveTypeLayout::U8),
        SignatureToken::U16 => Some(MoveTypeLayout::U16),
        SignatureToken::U32 => Some(MoveTypeLayout::U32),
        SignatureToken::U64 => Some(MoveTypeLayout::U64),
        SignatureToken::U128 => Some(MoveTypeLayout::U128),
        SignatureToken::U256 => Some(MoveTypeLayout::U256),
        SignatureToken::Vector(v) => Some(MoveTypeLayout::Vector(Box::new(sig_to_ty(v.as_ref())?))),
        SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_)
        | SignatureToken::Datatype(_)
        | SignatureToken::TypeParameter(_)
        | SignatureToken::DatatypeInstantiation(_) => None,
    }
}

fn ty_to_sig(ty: &MoveTypeLayout) -> Option<SignatureToken> {
    match ty {
        MoveTypeLayout::Address => Some(SignatureToken::Address),
        MoveTypeLayout::Signer => Some(SignatureToken::Signer),
        MoveTypeLayout::U8 => Some(SignatureToken::U8),
        MoveTypeLayout::U16 => Some(SignatureToken::U16),
        MoveTypeLayout::U32 => Some(SignatureToken::U32),
        MoveTypeLayout::U64 => Some(SignatureToken::U64),
        MoveTypeLayout::U128 => Some(SignatureToken::U128),
        MoveTypeLayout::U256 => Some(SignatureToken::U256),
        MoveTypeLayout::Vector(v) => Some(SignatureToken::Vector(Box::new(ty_to_sig(v.as_ref())?))),
        MoveTypeLayout::Struct(_) => None,
        MoveTypeLayout::Enum(_) => None,
        MoveTypeLayout::Bool => Some(SignatureToken::Bool),
    }
}

impl Constant {
    pub fn serialize_constant(ty: &MoveTypeLayout, v: &MoveValue) -> Option<Self> {
        Some(Self {
            type_: ty_to_sig(ty)?,
            data: v.simple_serialize()?,
        })
    }

    pub fn deserialize_constant(&self) -> Option<MoveValue> {
        let ty = sig_to_ty(&self.type_)?;
        MoveValue::simple_deserialize(&self.data, &ty).ok()
    }
}
