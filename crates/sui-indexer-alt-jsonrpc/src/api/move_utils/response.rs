// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use move_binary_format::file_format::{Ability, AbilitySet, Visibility};
use sui_json_rpc_types::{
    SuiMoveAbility, SuiMoveAbilitySet, SuiMoveNormalizedFunction, SuiMoveNormalizedType,
    SuiMoveVisibility,
};
use sui_package_resolver::{FunctionDef, OpenSignature, OpenSignatureBody, Reference};
use sui_types::{base_types::ObjectID, Identifier};

use crate::{
    error::{invalid_params, RpcError},
    Context,
};

use super::error::Error;

/// Load information about a function, and convert it into a JSON-RPC response.
pub(super) async fn function(
    ctx: &Context,
    package: ObjectID,
    module: &str,
    name: &str,
) -> Result<SuiMoveNormalizedFunction, RpcError<Error>> {
    use Error as E;

    if !Identifier::is_valid(module) {
        return Err(invalid_params(E::BadIdentifier(module.to_owned())));
    }

    if !Identifier::is_valid(name) {
        return Err(invalid_params(E::BadIdentifier(name.to_owned())));
    }

    let sig = ctx
        .package_resolver()
        .function_signature(*package, module, name)
        .await
        .map_err(|e| {
            use sui_package_resolver::error::Error as PRE;
            match &e {
                // These errors can be triggered by passing a type that doesn't exist for the
                // dynamic field name.
                PRE::NotAPackage(_)
                | PRE::PackageNotFound(_)
                | PRE::ModuleNotFound(_, _)
                | PRE::FunctionNotFound(_, _, _) => invalid_params(E::NotFound(e)),

                // These errors can be triggered by requesting a type whose layout is too large
                // (requires too may resources to resolve)
                PRE::TooManyTypeNodes(_, _)
                | PRE::TooManyTypeParams(_, _)
                | PRE::TypeParamNesting(_, _) => invalid_params(E::ResolutionLimit(e)),

                // The other errors are a form of internal error.
                PRE::Bcs(_)
                | PRE::Store { .. }
                | PRE::DatatypeNotFound(_, _, _)
                | PRE::Deserialize(_)
                | PRE::EmptyPackage(_)
                | PRE::InputTypeConflict(_, _, _)
                | PRE::LinkageNotFound(_)
                | PRE::NoTypeOrigin(_, _, _)
                | PRE::NotAnIdentifier(_)
                | PRE::TypeArityMismatch(_, _)
                | PRE::TypeParamOOB(_, _)
                | PRE::UnexpectedReference
                | PRE::UnexpectedSigner
                | PRE::UnexpectedError(_)
                | PRE::ValueNesting(_) => {
                    RpcError::from(anyhow!(e).context("Failed to resolve type layout"))
                }
            }
        })?;

    Ok(normalized_function(&sig))
}

fn normalized_function(sig: &FunctionDef) -> SuiMoveNormalizedFunction {
    SuiMoveNormalizedFunction {
        visibility: visibility(sig.visibility),
        is_entry: sig.is_entry,
        type_parameters: sig.type_params.iter().map(|a| ability_set(*a)).collect(),
        parameters: sig.parameters.iter().map(normalized_signature).collect(),
        return_: sig.return_.iter().map(normalized_signature).collect(),
    }
}

fn normalized_signature(sig: &OpenSignature) -> SuiMoveNormalizedType {
    use SuiMoveNormalizedType as T;

    let body = normalized_type(&sig.body);
    match sig.ref_ {
        Some(Reference::Immutable) => T::Reference(Box::new(body)),
        Some(Reference::Mutable) => T::MutableReference(Box::new(body)),
        None => body,
    }
}

fn normalized_type(sig: &OpenSignatureBody) -> SuiMoveNormalizedType {
    use OpenSignatureBody as S;
    use SuiMoveNormalizedType as T;
    match sig {
        S::Address => T::Address,
        S::Bool => T::Bool,
        S::U8 => T::U8,
        S::U16 => T::U16,
        S::U32 => T::U32,
        S::U64 => T::U64,
        S::U128 => T::U128,
        S::U256 => T::U256,
        S::Vector(sig) => T::Vector(Box::new(normalized_type(sig))),
        S::Datatype(t, params) => T::Struct {
            address: t.package.to_canonical_string(/* with_prefix */ true),
            module: t.module.to_string(),
            name: t.name.to_string(),
            type_arguments: params.iter().map(normalized_type).collect(),
        },
        S::TypeParameter(ix) => T::TypeParameter(*ix),
    }
}

fn visibility(v: Visibility) -> SuiMoveVisibility {
    match v {
        Visibility::Public => SuiMoveVisibility::Public,
        Visibility::Friend => SuiMoveVisibility::Friend,
        Visibility::Private => SuiMoveVisibility::Private,
    }
}

fn ability_set(a: AbilitySet) -> SuiMoveAbilitySet {
    SuiMoveAbilitySet {
        abilities: a.into_iter().map(ability).collect(),
    }
}

fn ability(a: Ability) -> SuiMoveAbility {
    match a {
        Ability::Copy => SuiMoveAbility::Copy,
        Ability::Drop => SuiMoveAbility::Drop,
        Ability::Store => SuiMoveAbility::Store,
        Ability::Key => SuiMoveAbility::Key,
    }
}
