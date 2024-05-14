use crate::{
    parser::ast as P,
    expansion::ast::{self as E, Address},
    naming::translate::DefnContext, shared::{NamedAddressMap, Name}, ice, diagnostics::Diagnostic, diag
};

use move_command_line_common::address::NumericalAddress;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;

// Access a top level address as declared, not affected by any aliasing/shadowing
pub(super) fn top_level_address(
    context: &mut DefnContext,
    suggest_declaration: bool,
    ln: P::LeadingNameAccess,
) -> Address {
    top_level_address_(
        context,
        context.named_address_mapping.as_ref().unwrap(),
        suggest_declaration,
        ln,
    )
}

pub(super) fn top_level_address_(
    context: &mut DefnContext,
    named_address_mapping: &NamedAddressMap,
    suggest_declaration: bool,
    ln: P::LeadingNameAccess,
) -> Address {
    let sp!(loc, ln_) = ln;
    match ln_ {
        P::LeadingNameAccess_::AnonymousAddress(bytes) => {
            Address::anonymous(loc, bytes)
        }
        // This should have been handled elsewhere in alias resolution for user-provided paths, and
        // should never occur in compiler-generated ones.
        P::LeadingNameAccess_::GlobalAddress(name) => {
            context.env.add_diag(ice!((
                loc,
                "Found an address in top-level address position that uses a global name"
            )));
            Address::NamedUnassigned(name)
        }
        P::LeadingNameAccess_::Name(name) => {
            match named_address_mapping.get(&name.value).copied() {
                Some(addr) => make_address(context, name, loc, addr),
                None => {
                    context.env.add_diag(address_without_value_error(
                        suggest_declaration,
                        loc,
                        &name,
                    ));
                    Address::NamedUnassigned(name)
                }
            }
        }
    }
}

pub(super) fn top_level_address_opt(
    context: &mut DefnContext,
    ln: P::LeadingNameAccess,
) -> Option<Address> {
    let named_address_mapping = context.named_address_mapping.as_ref().unwrap();
    let sp!(loc, ln_) = ln;
    match ln_ {
        P::LeadingNameAccess_::AnonymousAddress(bytes) => {
            Some(Address::anonymous(loc, bytes))
        }
        // This should have been handled elsewhere in alias resolution for user-provided paths, and
        // should never occur in compiler-generated ones.
        P::LeadingNameAccess_::GlobalAddress(_) => {
            context.env.add_diag(ice!((
                loc,
                "Found an address in top-level address position that uses a global name"
            )));
            None
        }
        P::LeadingNameAccess_::Name(name) => {
            let addr = named_address_mapping.get(&name.value).copied()?;
            Some(make_address(context, name, loc, addr))
        }
    }
}

fn maybe_make_well_known_address(context: &mut DefnContext, loc: Loc, name: Symbol) -> Option<Address> {
    let named_address_mapping = context.named_address_mapping.as_ref().unwrap();
    let addr = named_address_mapping.get(&name).copied()?;
    Some(make_address(
        context,
        sp(loc, name),
        loc,
        addr,
    ))
}

fn address_without_value_error(suggest_declaration: bool, loc: Loc, n: &Name) -> Diagnostic {
    let mut msg = format!("address '{}' is not assigned a value", n);
    if suggest_declaration {
        msg = format!(
            "{}. Try assigning it a value when calling the compiler",
            msg,
        )
    }
    diag!(NameResolution::AddressWithoutValue, (loc, msg))
}

pub(super) fn make_address(
    context: &mut DefnContext,
    name: Name,
    loc: Loc,
    value: NumericalAddress,
) -> Address {
    Address::Numerical {
        name: Some(name),
        value: sp(loc, value),
        name_conflict: context.address_conflicts.contains(&name.value),
    }
}

pub(super) fn module_ident(
    context: &mut DefnContext,
    sp!(loc, mident_): P::ModuleIdent,
) -> E::ModuleIdent {
    let P::ModuleIdent_ {
        address: ln,
        module,
    } = mident_;
    let addr = top_level_address(context, /* suggest_declaration */ false, ln);
    sp(loc, E::ModuleIdent_::new(addr, module))
}

fn check_module_address(
    context: &mut DefnContext,
    loc: Loc,
    addr: Address,
    m: &mut P::ModuleDefinition,
) -> Spanned<Address> {
    let module_address = std::mem::take(&mut m.address);
    match module_address {
        Some(other_paddr) => {
            let other_loc = other_paddr.loc;
            let other_addr = top_level_address(
                context,
                /* suggest_declaration */ true,
                other_paddr,
            );
            let msg = if addr == other_addr {
                "Redundant address specification"
            } else {
                "Multiple addresses specified for module"
            };
            context.env.add_diag(diag!(
                Declarations::DuplicateItem,
                (other_loc, msg),
                (loc, "Address previously specified here")
            ));
            sp(other_loc, other_addr)
        }
        None => sp(loc, addr),
    }
}

