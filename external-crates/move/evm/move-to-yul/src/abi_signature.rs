// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This file defines functions for generating JSON-ABI.

use move_ethereum_abi::abi_signature_type::{ABIJsonArg, ABIJsonSignature};

use crate::{
    attributes::FunctionAttribute,
    events::EventSignature,
    solidity_ty::{SoliditySignature, SolidityType},
};

pub(crate) fn from_ty(ty: &SolidityType, name: String) -> ABIJsonArg {
    use SolidityType::*;
    let mut abi_ty_str = ty.to_string();
    let mut components = None;
    if let Struct(_, ty_tuples) = ty {
        let mut comps = vec![];
        abi_ty_str = "tuple".to_string();
        for (_, _, _, para_name, comp_ty) in ty_tuples {
            let t = from_ty(comp_ty, para_name.clone());
            comps.push(t);
        }
        components = Some(comps);
    } else if matches!(ty, DynamicArray(_)) || matches!(ty, StaticArray(_, _)) {
        let mut array_vec = vec![];
        let base_ty = find_inner_ty_from_array(ty, &mut array_vec);
        if let Struct(_, ty_tuples) = base_ty {
            let mut comps = vec![];
            abi_ty_str = "tuple".to_string();
            for dimension in array_vec.into_iter().rev() {
                let dim = if dimension > 0 {
                    dimension.to_string()
                } else {
                    "".to_string()
                };
                abi_ty_str = format!("{}[{}]", abi_ty_str, dim);
            }
            for (_, _, _, para_name, comp_ty) in ty_tuples {
                let t = from_ty(&comp_ty, para_name.clone());
                comps.push(t);
            }
            components = Some(comps);
        }
    }
    ABIJsonArg {
        ty: abi_ty_str,
        indexed: None,
        components,
        name,
    }
}

pub(crate) fn from_event_ty(ty: &SolidityType, indexed: bool, name: String) -> ABIJsonArg {
    let abi = from_ty(ty, name);
    ABIJsonArg {
        ty: abi.ty,
        indexed: Some(indexed),
        components: abi.components,
        name: abi.name,
    }
}

fn find_inner_ty_from_array(ty: &SolidityType, para: &mut Vec<usize>) -> SolidityType {
    use SolidityType::*;
    let mut ret_ty = ty.clone();
    if let DynamicArray(inner_ty) = ty {
        ret_ty = *inner_ty.clone();
        para.push(0);
    } else if let StaticArray(inner_ty, m) = ty {
        ret_ty = *inner_ty.clone();
        para.push(*m);
    }
    if ret_ty.is_array() {
        find_inner_ty_from_array(&ret_ty, para)
    } else {
        ret_ty
    }
}

pub(crate) fn from_solidity_sig(
    sig: &SoliditySignature,
    attr: Option<FunctionAttribute>,
    fun_typ: &str,
) -> ABIJsonSignature {
    let name = sig.sig_name.clone();
    let mut inputs = vec![];
    let mut outputs = vec![];
    for (ty, para_name, _) in &sig.para_types {
        inputs.push(from_ty(ty, para_name.clone()));
    }
    for (ty, _) in &sig.ret_types {
        outputs.push(from_ty(ty, "".to_string()));
    }
    let state_mutability = (if let Some(FunctionAttribute::View) = attr {
        "view"
    } else if let Some(FunctionAttribute::Pure) = attr {
        "pure"
    } else if let Some(FunctionAttribute::Payable) = attr {
        "payable"
    } else {
        "nonpayable"
    })
    .to_string();
    let anonymous = None;
    ABIJsonSignature {
        name,
        ty: fun_typ.to_string(),
        inputs,
        outputs: Some(outputs),
        state_mutability: Some(state_mutability),
        anonymous,
    }
}

pub(crate) fn from_event_sig(sig: &EventSignature) -> ABIJsonSignature {
    let name = sig.event_name.clone();
    let ty = "event".to_string();
    let mut inputs = vec![];
    for (_, ty, _, indexed_flag, ev_name) in &sig.para_types {
        inputs.push(from_event_ty(ty, *indexed_flag, ev_name.clone()));
    }
    ABIJsonSignature {
        name,
        ty,
        inputs,
        outputs: None,
        state_mutability: None,
        anonymous: Some(false),
    }
}
