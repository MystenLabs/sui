// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::file_format::{Constant, SignatureToken};

use move_core_types::{
    compressed::runtime::{
        BackendBuilder as _, LayoutHandle, MoveLayoutView, MoveTypeLayout, MoveTypeLayoutBuilder,
        MoveTypeLayoutRef,
    },
    runtime_value::MoveValue,
};

pub fn constant_sig_token_to_layout(sig: &SignatureToken) -> Option<MoveTypeLayout> {
    fn sig_to_ty_internal(
        builder: &mut MoveTypeLayoutBuilder,
        sig: &SignatureToken,
    ) -> Option<LayoutHandle> {
        Some(match sig {
            SignatureToken::Address => builder.address(),
            SignatureToken::Bool => builder.bool(),
            SignatureToken::U8 => builder.u8(),
            SignatureToken::U16 => builder.u16(),
            SignatureToken::U32 => builder.u32(),
            SignatureToken::U64 => builder.u64(),
            SignatureToken::U128 => builder.u128(),
            SignatureToken::U256 => builder.u256(),
            SignatureToken::Vector(v) => {
                let inner = sig_to_ty_internal(builder, v.as_ref())?;
                builder.vector(inner).ok()?
            }
            SignatureToken::Signer
            | SignatureToken::Reference(_)
            | SignatureToken::MutableReference(_)
            | SignatureToken::Datatype(_)
            | SignatureToken::TypeParameter(_)
            | SignatureToken::DatatypeInstantiation(_) => return None,
        })
    }

    let mut builder = MoveTypeLayoutBuilder::new();
    sig_to_ty_internal(&mut builder, sig).map(|handle| builder.build(handle))
}

fn ty_to_sig(ty: MoveTypeLayoutRef<'_>) -> Option<SignatureToken> {
    match ty.as_view() {
        MoveLayoutView::Address => Some(SignatureToken::Address),
        MoveLayoutView::U8 => Some(SignatureToken::U8),
        MoveLayoutView::U16 => Some(SignatureToken::U16),
        MoveLayoutView::U32 => Some(SignatureToken::U32),
        MoveLayoutView::U64 => Some(SignatureToken::U64),
        MoveLayoutView::U128 => Some(SignatureToken::U128),
        MoveLayoutView::U256 => Some(SignatureToken::U256),
        MoveLayoutView::Vector(v) => Some(SignatureToken::Vector(Box::new(ty_to_sig(v)?))),
        MoveLayoutView::Bool => Some(SignatureToken::Bool),
        MoveLayoutView::Signer | MoveLayoutView::Struct(_) | MoveLayoutView::Enum(_) => None,
    }
}

impl Constant {
    pub fn serialize_constant(ty: MoveTypeLayoutRef<'_>, v: &MoveValue) -> Option<Self> {
        Some(Self {
            type_: ty_to_sig(ty)?,
            data: v.simple_serialize()?,
        })
    }

    pub fn deserialize_constant(&self) -> Option<MoveValue> {
        let ty = constant_sig_token_to_layout(&self.type_)?;
        MoveValue::simple_deserialize_compressed(&self.data, &ty).ok()
    }
}
