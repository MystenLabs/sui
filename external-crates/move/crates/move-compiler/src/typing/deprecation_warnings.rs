// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::{DiagnosticReporter, Diagnostics},
    expansion::ast::{self as E, ModuleIdent},
    ice,
    shared::{
        known_attributes::{AttributePosition, DeprecationAttribute, KnownAttribute},
        program_info::NamingProgramInfo,
        CompilationEnv, Name,
    },
};
use move_ir_types::location::Loc;
use std::collections::HashMap;

const NOTE_STR: &str = "note";

#[derive(Debug, Clone)]
pub struct Deprecation {
    // The source location of the deprecation attribute
    #[allow(unused)]
    pub source_location: Loc,
    // The type of the member that is deprecated (function, constant, etc.)
    pub location: AttributePosition,
    // The module that the deprecated member belongs to. This is used in part to make sure we don't
    // register deprecation warnings for members within a given deprecated module calling within
    // that module.
    pub module_ident: ModuleIdent,
    // Information about the deprecation information depending on the deprecation attribute.
    // #[deprecated]  -- if None
    // #[deprecated(note = b"message")] -- if Some(message)
    pub deprecation_note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Deprecations {
    // Name = None -- deprecation on Module
    // Name = Some(Name) -- deprecation on Module::Name member
    pub deprecated_members: HashMap<(ModuleIdent, Option<Name>), Deprecation>,
}

impl Deprecations {
    /// Index the modules and their members for deprecation attributes and register each
    /// deprecation attribute for use later on.
    pub fn new(env: &CompilationEnv, info: &NamingProgramInfo) -> Self {
        let mut deprecated_members = HashMap::new();
        let reporter = env.diagnostic_reporter_at_top_level();

        for (mident, module_info) in info.modules.key_cloned_iter() {
            if let Some(deprecation) = deprecations(
                &reporter,
                AttributePosition::Module,
                &module_info.attributes,
                mident.loc,
                mident,
            ) {
                deprecated_members.insert((mident, None), deprecation);
            }

            for (name, constant) in module_info.constants.key_cloned_iter() {
                if let Some(deprecation) = deprecations(
                    &reporter,
                    AttributePosition::Constant,
                    &constant.attributes,
                    name.0.loc,
                    mident,
                ) {
                    deprecated_members.insert((mident, Some(name.0)), deprecation);
                }
            }

            for (name, function) in module_info.functions.key_cloned_iter() {
                if let Some(deprecation) = deprecations(
                    &reporter,
                    AttributePosition::Function,
                    &function.attributes,
                    name.0.loc,
                    mident,
                ) {
                    deprecated_members.insert((mident, Some(name.0)), deprecation);
                }
            }

            for (name, datatype) in module_info.structs.key_cloned_iter() {
                if let Some(deprecation) = deprecations(
                    &reporter,
                    AttributePosition::Struct,
                    &datatype.attributes,
                    name.0.loc,
                    mident,
                ) {
                    deprecated_members.insert((mident, Some(name.0)), deprecation);
                }
            }

            for (name, datatype) in module_info.enums.key_cloned_iter() {
                if let Some(deprecation) = deprecations(
                    &reporter,
                    AttributePosition::Enum,
                    &datatype.attributes,
                    name.0.loc,
                    mident,
                ) {
                    deprecated_members.insert((mident, Some(name.0)), deprecation);
                }
            }
        }

        Self { deprecated_members }
    }

    /// Return the deprecation for the specific module member if present, otherwise return the
    /// deprecation for the module itself.
    pub fn get_deprecation(&self, mident: ModuleIdent, member_name: Name) -> Option<&Deprecation> {
        self.deprecated_members
            .get(&(mident, Some(member_name)))
            .or_else(|| self.deprecated_members.get(&(mident, None)))
    }
}

impl Deprecation {
    /// Emit a warning for the deprecation of a module member.
    pub fn deprecation_warnings(&self, member_name: Name, method_opt: Option<Name>) -> Diagnostics {
        let mident_string = self.module_ident.to_string();
        let location_string = match (self.location, method_opt) {
            (AttributePosition::Module, None) => {
                format!(
                    "The '{mident_string}::{member_name}' member of the {} '{mident_string}' is deprecated. \
                    It is deprecated since its whole module is marked deprecated",
                    AttributePosition::Module
                )
            }
            (AttributePosition::Module, Some(method)) => {
                format!(
                    "The method '{method}' resolves to the function '{mident_string}::{member_name}' in the {} '{mident_string}' which is deprecated. \
                    This function, and the method are deprecated since the whole module is marked deprecated",
                    AttributePosition::Module
                )
            }
            (position, None) => {
                format!("The {position} '{mident_string}::{member_name}' is deprecated")
            }
            (position, Some(method)) => {
                format!(
                    "The method '{method}' resolves to the {position} '{mident_string}::{member_name}' which is deprecated"
                )
            }
        };

        let message = match &self.deprecation_note {
            None => location_string,
            Some(note) => format!("{location_string}: {note}"),
        };

        let location = method_opt.map_or(member_name.loc, |method| method.loc);

        Diagnostics::from(vec![diag!(
            TypeSafety::DeprecatedUsage,
            (location, message)
        )])
    }
}

// Process the deprecation attributes for a given member (module, constant, function, etc.) and
// return `Optiong<Deprecation>` if there is a #[deprecated] attribute. If there are invalid
// #[deprecated] attributes (malformed, or multiple on the member), add an error diagnostic to
// `env` and return None.
fn deprecations(
    reporter: &DiagnosticReporter,
    attr_position: AttributePosition,
    attrs: &E::Attributes,
    source_location: Loc,
    mident: ModuleIdent,
) -> Option<Deprecation> {
    let deprecations: Vec<_> = attrs
        .iter()
        .filter(|(_, v, _)| matches!(v, KnownAttribute::Deprecation(_)))
        .collect();

    if deprecations.is_empty() {
        return None;
    }

    if deprecations.len() != 1 {
        reporter.add_diag(ice!((
            source_location,
            "ICE: verified that there is at least one deprecation attribute above, \
            and expansion should have failed if there were multiple deprecation attributes."
        )));
        return None;
    }

    let (loc, _, attr) = deprecations
        .last()
        .expect("Verified deprecations is not empty above");

    let make_invalid_deprecation_diag = || {
        let mut diag = diag!(
            Attributes::InvalidUsage,
            (
                *loc,
                format!("Invalid '{}' attribute", DeprecationAttribute.name())
            )
        );
        let note = format!(
            "Deprecation attributes must be written as `#[{0}]` or `#[{0}(note = b\"message\")]`",
            DeprecationAttribute.name()
        );
        diag.add_note(note);
        reporter.add_diag(diag);
        None
    };

    match &attr.value {
        E::Attribute_::Name(_) => Some(Deprecation {
            source_location,
            location: attr_position,
            deprecation_note: None,
            module_ident: mident,
        }),
        E::Attribute_::Parameterized(_, assigns) if assigns.len() == 1 => {
            let param = assigns.key_cloned_iter().next().unwrap().1;
            match param {
                sp!(_, E::Attribute_::Assigned(sp!(_, name), attr_val))
                    if name.as_str() == NOTE_STR
                        && matches!(
                            &attr_val.value,
                            E::AttributeValue_::Value(sp!(_, E::Value_::Bytearray(_)))
                        ) =>
                {
                    let E::AttributeValue_::Value(sp!(_, E::Value_::Bytearray(b))) =
                        &attr_val.value
                    else {
                        unreachable!()
                    };
                    let msg = std::str::from_utf8(b).unwrap().to_string();
                    Some(Deprecation {
                        source_location,
                        location: attr_position,
                        deprecation_note: Some(msg),
                        module_ident: mident,
                    })
                }
                _ => make_invalid_deprecation_diag(),
            }
        }
        E::Attribute_::Assigned(_, _) | E::Attribute_::Parameterized(_, _) => {
            make_invalid_deprecation_diag()
        }
    }
}
