// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use crate::parser::ast as P;

// TODO we should really do this after expansion so that its done after attribute resolution. But
// that can only really be done if we move most of expansion into naming.

/// A trait that decides whether to include a parsed element in the compilation
pub trait FilterContext {
    fn set_current_package(&mut self, package: Option<Symbol>);
    fn set_is_source_def(&mut self, is_source_def: bool);

    /// Attribute-based node removal
    fn should_remove_by_attributes(&mut self, _attrs: &[P::Attributes]) -> bool;

    fn filter_map_address(
        &mut self,
        address_def: P::AddressDefinition,
    ) -> Option<P::AddressDefinition> {
        if self.should_remove_by_attributes(&address_def.attributes) {
            None
        } else {
            Some(address_def)
        }
    }

    fn filter_map_module(
        &mut self,
        module_def: P::ModuleDefinition,
    ) -> Option<P::ModuleDefinition> {
        if self.should_remove_by_attributes(&module_def.attributes) {
            None
        } else {
            Some(module_def)
        }
    }

    fn filter_map_function(&mut self, function_def: P::Function) -> Option<P::Function> {
        if self.should_remove_by_attributes(&function_def.attributes) {
            None
        } else {
            Some(function_def)
        }
    }

    fn filter_map_struct(
        &mut self,
        struct_def: P::StructDefinition,
    ) -> Option<P::StructDefinition> {
        if self.should_remove_by_attributes(&struct_def.attributes) {
            None
        } else {
            Some(struct_def)
        }
    }

    fn filter_map_enum(&mut self, enum_def: P::EnumDefinition) -> Option<P::EnumDefinition> {
        if self.should_remove_by_attributes(&enum_def.attributes) {
            None
        } else {
            Some(enum_def)
        }
    }

    fn filter_map_use(&mut self, use_decl: P::UseDecl) -> Option<P::UseDecl> {
        if self.should_remove_by_attributes(&use_decl.attributes) {
            None
        } else {
            Some(use_decl)
        }
    }

    fn filter_map_friend(&mut self, friend_decl: P::FriendDecl) -> Option<P::FriendDecl> {
        if self.should_remove_by_attributes(&friend_decl.attributes) {
            None
        } else {
            Some(friend_decl)
        }
    }

    fn filter_map_constant(&mut self, constant: P::Constant) -> Option<P::Constant> {
        if self.should_remove_by_attributes(&constant.attributes) {
            None
        } else {
            Some(constant)
        }
    }
}

/// This filters out module member from `prog` based on supplied `FilterContext` implementation
pub fn filter_program<T: FilterContext>(context: &mut T, prog: P::Program) -> P::Program {
    let P::Program {
        named_address_maps,
        source_definitions,
        lib_definitions,
    } = prog;

    context.set_is_source_def(false);
    let lib_definitions: Vec<_> = lib_definitions
        .into_iter()
        .filter_map(
            |P::PackageDefinition {
                 package,
                 named_address_map,
                 def,
                 target_kind: pkg_def_kind,
             }| {
                context.set_current_package(package);
                Some(P::PackageDefinition {
                    package,
                    named_address_map,
                    def: filter_definition(context, def)?,
                    target_kind: pkg_def_kind,
                })
            },
        )
        .collect();

    context.set_is_source_def(true);
    let source_definitions: Vec<_> = source_definitions
        .into_iter()
        .filter_map(
            |P::PackageDefinition {
                 package,
                 named_address_map,
                 def,
                 target_kind: pkg_def_kind,
             }| {
                context.set_current_package(package);
                Some(P::PackageDefinition {
                    package,
                    named_address_map,
                    def: filter_definition(context, def)?,
                    target_kind: pkg_def_kind,
                })
            },
        )
        .collect();

    P::Program {
        named_address_maps,
        source_definitions,
        lib_definitions,
    }
}

fn filter_definition<T: FilterContext>(
    context: &mut T,
    def: P::Definition,
) -> Option<P::Definition> {
    match def {
        P::Definition::Module(m) => filter_module(context, m).map(P::Definition::Module),
        P::Definition::Address(a) => filter_address(context, a).map(P::Definition::Address),
    }
}

fn filter_address<T: FilterContext>(
    context: &mut T,
    address_def: P::AddressDefinition,
) -> Option<P::AddressDefinition> {
    let address_def = context.filter_map_address(address_def)?;

    let P::AddressDefinition {
        addr,
        attributes,
        loc,
        modules,
    } = address_def;

    let modules = modules
        .into_iter()
        .filter_map(|m| filter_module(context, m))
        .collect();

    Some(P::AddressDefinition {
        attributes,
        loc,
        addr,
        modules,
    })
}

fn filter_module<T: FilterContext>(
    context: &mut T,
    module_def: P::ModuleDefinition,
) -> Option<P::ModuleDefinition> {
    let module_def = context.filter_map_module(module_def)?;

    let P::ModuleDefinition {
        doc,
        attributes,
        loc,
        address,
        name,
        is_spec_module,
        members,
        definition_mode,
    } = module_def;

    let new_members: Vec<_> = members
        .into_iter()
        .filter_map(|member| filter_module_member(context, member))
        .collect();

    Some(P::ModuleDefinition {
        doc,
        attributes,
        loc,
        address,
        name,
        is_spec_module,
        members: new_members,
        definition_mode,
    })
}

fn filter_module_member<T: FilterContext>(
    context: &mut T,
    module_member: P::ModuleMember,
) -> Option<P::ModuleMember> {
    use P::ModuleMember as PM;

    match module_member {
        PM::Function(func_def) => context.filter_map_function(func_def).map(PM::Function),
        PM::Struct(struct_def) => context.filter_map_struct(struct_def).map(PM::Struct),
        x @ PM::Spec(_) => Some(x),
        PM::Enum(enum_def) => context.filter_map_enum(enum_def).map(PM::Enum),
        PM::Use(use_decl) => context.filter_map_use(use_decl).map(PM::Use),
        PM::Friend(friend_decl) => context.filter_map_friend(friend_decl).map(PM::Friend),
        PM::Constant(constant) => context.filter_map_constant(constant).map(PM::Constant),
    }
}
