
fn unnecessary_alias_errors(context: &mut Context, unnecessaries: Vec<UnnecessaryAlias>) {
    for unnecessary in unnecessaries {
        unnecessary_alias_error(context, unnecessary)
    }
}

fn unnecessary_alias_error(context: &mut Context, unnecessary: UnnecessaryAlias) {
    let UnnecessaryAlias { entry, prev } = unnecessary;
    let loc = entry.loc();
    let is_default = prev == Loc::invalid();
    let (alias, entry_case) = match entry {
        AliasEntry::Address(_, _) => {
            debug_assert!(false, "ICE cannot manually make address aliases");
            return;
        }
        AliasEntry::TypeParam(_) => {
            debug_assert!(
                false,
                "ICE cannot manually make type param aliases. \
                We do not have nested TypeParam scopes"
            );
            return;
        }
        AliasEntry::Module(n, m) => (n, format!(" for module '{m}'")),
        AliasEntry::Member(n, m, mem) => (n, format!(" for module member '{m}::{mem}'")),
    };
    let decl_case = if is_default {
        "This alias is provided by default"
    } else {
        "It was already in scope"
    };
    let msg = format!("Unnecessary alias '{alias}'{entry_case}. {decl_case}");
    let mut diag = diag!(Declarations::DuplicateAlias, (loc, msg));
    if prev != Loc::invalid() {
        // nothing to point to for the default case
        diag.add_secondary_label((prev, "The same alias was previously declared here"))
    }
    context.env().add_diag(diag);
}

// Implicit aliases for the Move Stdlib:
// use std::vector;
// use std::option::{Self, Option};
const IMPLICIT_STD_MODULES: &[Symbol] = &[symbol!("option"), symbol!("vector")];
const IMPLICIT_STD_MEMBERS: &[(Symbol, Symbol, ModuleMemberKind)] = &[(
    symbol!("option"),
    symbol!("Option"),
    ModuleMemberKind::Struct,
)];

// Implicit aliases for Sui mode:
// use sui::object::{Self, ID, UID};
// use sui::transfer;
// use sui::tx_context::{Self, TxContext};
const IMPLICIT_SUI_MODULES: &[Symbol] = &[
    symbol!("object"),
    symbol!("transfer"),
    symbol!("tx_context"),
];
const IMPLICIT_SUI_MEMBERS: &[(Symbol, Symbol, ModuleMemberKind)] = &[
    (symbol!("object"), symbol!("ID"), ModuleMemberKind::Struct),
    (symbol!("object"), symbol!("UID"), ModuleMemberKind::Struct),
    (
        symbol!("tx_context"),
        symbol!("TxContext"),
        ModuleMemberKind::Struct,
    ),
];

fn default_aliases(context: &mut Context) -> AliasMapBuilder {
    let current_package = context.current_package();
    let mut builder = context.new_alias_map_builder();
    if !context
        .env()
        .supports_feature(current_package, FeatureGate::Move2024Paths)
    {
        return builder;
    }
    // Unused loc since these will not conflict and are implicit so no warnings are given
    let loc = Loc::invalid();
    let std_address = maybe_make_well_known_address(context, loc, symbol!("std"));
    let sui_address = maybe_make_well_known_address(context, loc, symbol!("sui"));
    let mut modules: Vec<(Address, Symbol)> = vec![];
    let mut members: Vec<(Address, Symbol, Symbol, ModuleMemberKind)> = vec![];
    // if std is defined, add implicit std aliases
    if let Some(std_address) = std_address {
        modules.extend(
            IMPLICIT_STD_MODULES
                .iter()
                .copied()
                .map(|m| (std_address, m)),
        );
        members.extend(
            IMPLICIT_STD_MEMBERS
                .iter()
                .copied()
                .map(|(m, mem, k)| (std_address, m, mem, k)),
        );
    }
    // if sui is defined and the current package is in Sui mode, add implicit sui aliases
    if sui_address.is_some() && context.env().package_config(current_package).flavor == Flavor::Sui
    {
        let sui_address = sui_address.unwrap();
        modules.extend(
            IMPLICIT_SUI_MODULES
                .iter()
                .copied()
                .map(|m| (sui_address, m)),
        );
        members.extend(
            IMPLICIT_SUI_MEMBERS
                .iter()
                .copied()
                .map(|(m, mem, k)| (sui_address, m, mem, k)),
        );
    }
    for (addr, module) in modules {
        let alias = sp(loc, module);
        let mident = sp(loc, ModuleIdent_::new(addr, ModuleName(sp(loc, module))));
        builder.add_implicit_module_alias(alias, mident).unwrap();
    }
    for (addr, module, member, kind) in members {
        let alias = sp(loc, member);
        let mident = sp(loc, ModuleIdent_::new(addr, ModuleName(sp(loc, module))));
        let name = sp(loc, member);
        builder
            .add_implicit_member_alias(alias, mident, name, kind)
            .unwrap();
    }
    builder
}

