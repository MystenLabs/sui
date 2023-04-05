// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_ir_types::location::sp;

use crate::parser::ast as P;

/// A trait that decides whether to include a parsed element in the compilation
pub trait FilterContext {
    /// Attribute-based node removal
    fn should_remove_by_attributes(
        &mut self,
        _attrs: &[P::Attributes],
        _is_source_def: bool,
    ) -> bool {
        false
    }

    fn filter_map_address(
        &mut self,
        address_def: P::AddressDefinition,
        is_source_def: bool,
    ) -> Option<P::AddressDefinition> {
        if self.should_remove_by_attributes(&address_def.attributes, is_source_def) {
            None
        } else {
            Some(address_def)
        }
    }

    fn filter_map_module(
        &mut self,
        module_def: P::ModuleDefinition,
        is_source_def: bool,
    ) -> Option<P::ModuleDefinition> {
        if self.should_remove_by_attributes(&module_def.attributes, is_source_def) {
            None
        } else {
            Some(module_def)
        }
    }

    fn filter_map_script(
        &mut self,
        script_def: P::Script,
        is_source_def: bool,
    ) -> Option<P::Script> {
        if self.should_remove_by_attributes(&script_def.attributes, is_source_def) {
            None
        } else {
            Some(script_def)
        }
    }

    fn filter_map_function(
        &mut self,
        function_def: P::Function,
        is_source_def: bool,
    ) -> Option<P::Function> {
        if self.should_remove_by_attributes(&function_def.attributes, is_source_def) {
            None
        } else {
            Some(function_def)
        }
    }

    fn filter_map_struct(
        &mut self,
        struct_def: P::StructDefinition,
        is_source_def: bool,
    ) -> Option<P::StructDefinition> {
        if self.should_remove_by_attributes(&struct_def.attributes, is_source_def) {
            None
        } else {
            Some(struct_def)
        }
    }

    fn filter_map_spec(
        &mut self,
        spec: P::SpecBlock_,
        is_source_def: bool,
    ) -> Option<P::SpecBlock_> {
        if self.should_remove_by_attributes(&spec.attributes, is_source_def) {
            None
        } else {
            Some(spec)
        }
    }

    fn filter_map_use(&mut self, use_decl: P::UseDecl, is_source_def: bool) -> Option<P::UseDecl> {
        if self.should_remove_by_attributes(&use_decl.attributes, is_source_def) {
            None
        } else {
            Some(use_decl)
        }
    }

    fn filter_map_friend(
        &mut self,
        friend_decl: P::FriendDecl,
        is_source_def: bool,
    ) -> Option<P::FriendDecl> {
        if self.should_remove_by_attributes(&friend_decl.attributes, is_source_def) {
            None
        } else {
            Some(friend_decl)
        }
    }

    fn filter_map_constant(
        &mut self,
        constant: P::Constant,
        is_source_def: bool,
    ) -> Option<P::Constant> {
        if self.should_remove_by_attributes(&constant.attributes, is_source_def) {
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

    let lib_definitions: Vec<_> = lib_definitions
        .into_iter()
        .filter_map(
            |P::PackageDefinition {
                 package,
                 named_address_map,
                 def,
             }| {
                Some(P::PackageDefinition {
                    package,
                    named_address_map,
                    def: filter_definition(context, def, false)?,
                })
            },
        )
        .collect();

    let source_definitions: Vec<_> = source_definitions
        .into_iter()
        .filter_map(
            |P::PackageDefinition {
                 package,
                 named_address_map,
                 def,
             }| {
                Some(P::PackageDefinition {
                    package,
                    named_address_map,
                    def: filter_definition(context, def, true)?,
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
    is_source_def: bool,
) -> Option<P::Definition> {
    match def {
        P::Definition::Module(m) => {
            filter_module(context, m, is_source_def).map(P::Definition::Module)
        }
        P::Definition::Address(a) => {
            filter_address(context, a, is_source_def).map(P::Definition::Address)
        }
        P::Definition::Script(s) => {
            filter_script(context, s, is_source_def).map(P::Definition::Script)
        }
    }
}

fn filter_address<T: FilterContext>(
    context: &mut T,
    address_def: P::AddressDefinition,
    is_source_def: bool,
) -> Option<P::AddressDefinition> {
    let address_def = context.filter_map_address(address_def, is_source_def)?;

    let P::AddressDefinition {
        addr,
        attributes,
        loc,
        modules,
    } = address_def;

    let modules = modules
        .into_iter()
        .filter_map(|m| filter_module(context, m, is_source_def))
        .collect();

    Some(P::AddressDefinition {
        attributes,
        loc,
        addr,
        modules,
    })
}

fn filter_script<T: FilterContext>(
    context: &mut T,
    script_def: P::Script,
    is_source_def: bool,
) -> Option<P::Script> {
    let script_def = context.filter_map_script(script_def, is_source_def)?;

    let P::Script {
        attributes,
        uses,
        constants,
        function,
        specs,
        loc,
    } = script_def;

    // This is a bit weird, if the only function in the script is filtered, we consider
    // the whole script is filtered as well
    let new_function = context.filter_map_function(function, is_source_def)?;

    let new_uses = uses
        .into_iter()
        .filter_map(|use_decl| context.filter_map_use(use_decl, is_source_def))
        .collect();
    let new_constants = constants
        .into_iter()
        .filter_map(|constant| context.filter_map_constant(constant, is_source_def))
        .collect();
    let new_specs = specs
        .into_iter()
        .filter_map(|sp!(spec_loc, spec)| {
            context
                .filter_map_spec(spec, is_source_def)
                .map(|new_spec| sp(spec_loc, new_spec))
        })
        .collect();

    Some(P::Script {
        attributes,
        function: new_function,
        uses: new_uses,
        constants: new_constants,
        specs: new_specs,
        loc,
    })
}

fn filter_module<T: FilterContext>(
    context: &mut T,
    module_def: P::ModuleDefinition,
    is_source_def: bool,
) -> Option<P::ModuleDefinition> {
    let module_def = context.filter_map_module(module_def, is_source_def)?;

    let P::ModuleDefinition {
        attributes,
        loc,
        address,
        name,
        is_spec_module,
        members,
    } = module_def;

    let new_members: Vec<_> = members
        .into_iter()
        .filter_map(|member| filter_module_member(context, member, is_source_def))
        .collect();

    Some(P::ModuleDefinition {
        attributes,
        loc,
        address,
        name,
        is_spec_module,
        members: new_members,
    })
}

fn filter_module_member<T: FilterContext>(
    context: &mut T,
    module_member: P::ModuleMember,
    is_source_def: bool,
) -> Option<P::ModuleMember> {
    use P::ModuleMember as PM;

    match module_member {
        PM::Function(func_def) => context
            .filter_map_function(func_def, is_source_def)
            .map(PM::Function),
        PM::Struct(struct_def) => context
            .filter_map_struct(struct_def, is_source_def)
            .map(PM::Struct),
        PM::Spec(sp!(spec_loc, spec)) => context
            .filter_map_spec(spec, is_source_def)
            .map(|new_spec| PM::Spec(sp(spec_loc, new_spec))),
        PM::Use(use_decl) => context.filter_map_use(use_decl, is_source_def).map(PM::Use),
        PM::Friend(friend_decl) => context
            .filter_map_friend(friend_decl, is_source_def)
            .map(PM::Friend),
        PM::Constant(constant) => context
            .filter_map_constant(constant, is_source_def)
            .map(PM::Constant),
    }
}
