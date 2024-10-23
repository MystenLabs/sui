use crate::compatibility::Compatibility;
use crate::file_format::Visibility;
use crate::normalized::{Enum, Function, Struct};
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::{IdentStr, Identifier};

/// A trait which will allow accumulating the information necessary for checking upgrade compatibility between two modules,
/// while allowing flexibility in the error type that is returned.
/// Gathers the errors and accumulates them into a single error.
/// The [`Compatibility`] struct's flags are used to determine the compatibility checks that are needed.
pub trait CompatibilityMode: Default {
    /// The error type that will be returned when [`CompatibilityMode::finish`] is called, returning the accumulated result.
    type Error;

    /// The module id mismatch error occurs when the module id of the old and new modules do not match.
    fn module_id_mismatch(
        &mut self,
        old_addr: &AccountAddress,
        old_name: &IdentStr,
        new_addr: &AccountAddress,
        new_name: &IdentStr,
    );

    /// The struct missing error occurs when a struct is present in the old module but not in the new module.
    fn struct_missing(&mut self, name: &Identifier, old_struct: &Struct);

    /// The struct ability mismatch error occurs when the abilities of a struct are outside of the
    /// allowed new abilities. Adding an ability is fine as long as it's not in the disallowed_new_abilities set.
    fn struct_ability_mismatch(
        &mut self,
        name: &Identifier,
        old_struct: &Struct,
        new_struct: &Struct,
    );

    /// Struct type parameters mismatch error occurs when the type parameters of a struct are not the same.
    fn struct_type_param_mismatch(
        &mut self,
        name: &Identifier,
        old_struct: &Struct,
        new_struct: &Struct,
    );

    /// Struct field mismatch error occurs when the fields of a struct are not the same.
    fn struct_field_mismatch(
        &mut self,
        name: &Identifier,
        old_struct: &Struct,
        new_struct: &Struct,
    );

    /// Enum missing error occurs when an enum is present in the old module but not in the new module.
    fn enum_missing(&mut self, name: &Identifier, old_enum: &Enum);

    /// Enum ability mismatch error occurs when the abilities of an enum are outside of the
    /// allowed new abilities. Adding an ability is fine as long as it's not in the disallowed_new_abilities set.
    fn enum_ability_mismatch(&mut self, name: &Identifier, old_enum: &Enum, new_enum: &Enum);

    /// Enum type parameters mismatch error occurs when the type parameters of an enum are not the same.
    fn enum_type_param_mismatch(&mut self, name: &Identifier, old_enum: &Enum, new_enum: &Enum);

    /// Enum new variant error occurs when a new variant is added to an enum.
    fn enum_new_variant(&mut self, name: &Identifier, old_enum: &Enum, new_enum: &Enum);

    /// Enum variant missing error occurs when a variant is present in the old enum but not in the new enum.
    fn enum_variant_missing(&mut self, name: &Identifier, old_enum: &Enum, tag: usize);

    /// Enum variant mismatch error occurs when a variant is present in the old enum but not in the new enum.
    fn enum_variant_mismatch(
        &mut self,
        name: &Identifier,
        old_enum: &Enum,
        new_enum: &Enum,
        tag: usize,
    );

    /// Function missing public error occurs when a public function is present in the old module but not in the new module.
    fn function_missing_public(&mut self, name: &Identifier, old_func: &Function);

    /// Function missing entry error occurs when an entry function is present in the old module but not in the new module.
    fn function_missing_entry(&mut self, name: &Identifier, old_func: &Function);

    /// Function signature mismatch error occurs when the signature of a function changes.
    fn function_signature_mismatch(
        &mut self,
        name: &Identifier,
        old_func: &Function,
        new_func: &Function,
    );

    /// Function lost public visibility error occurs when a function loses its public visibility.
    fn function_lost_public_visibility(&mut self, name: &Identifier, old_func: &Function);

    /// Function entry compatibility error occurs when an entry function is not compatible.
    fn function_entry_compatibility(
        &mut self,
        name: &Identifier,
        old_func: &Function,
        new_func: &Function,
    );

    /// Finish the compatibility check and return the error if one has been accumulated from individual errors.
    fn finish(self, _: &Compatibility) -> Result<(), Self::Error>;
}

/// Compatibility mode impl for execution compatibility checks.
/// These flags are set when a type safety check is violated. see [`Compatibility`] for more information.
pub struct ExecutionCompatibilityMode {
    /// This can never be overridden with a flag, and thus has no associated [`Compatibility`] flag.
    /// In other words public linking can never be broken. all other flags
    datatype_and_function_linking: bool,
    datatype_layout: bool,
    entry_linking: bool,
    no_new_variants: bool,
}

impl Default for ExecutionCompatibilityMode {
    fn default() -> Self {
        Self {
            datatype_and_function_linking: true,
            datatype_layout: true,
            entry_linking: true,
            no_new_variants: true,
        }
    }
}

impl CompatibilityMode for ExecutionCompatibilityMode {
    /// Unit error type for execution compatibility mode.
    /// We only need to know if an error has occurred.
    type Error = ();

    fn module_id_mismatch(
        &mut self,
        _old_addr: &AccountAddress,
        _old_name: &IdentStr,
        _new_addr: &AccountAddress,
        _new_name: &IdentStr,
    ) {
        self.datatype_and_function_linking = false;
    }

    fn struct_missing(&mut self, _name: &Identifier, _old_struct: &Struct) {
        self.datatype_and_function_linking = false;
        self.datatype_layout = false;
    }

    fn struct_ability_mismatch(
        &mut self,
        _name: &Identifier,
        _old_struct: &Struct,
        _new_struct: &Struct,
    ) {
        self.datatype_and_function_linking = false;
    }

    fn struct_type_param_mismatch(
        &mut self,
        _name: &Identifier,
        _old_struct: &Struct,
        _new_struct: &Struct,
    ) {
        self.datatype_and_function_linking = false;
    }

    fn struct_field_mismatch(
        &mut self,
        _name: &Identifier,
        _old_struct: &Struct,
        _new_struct: &Struct,
    ) {
        self.datatype_layout = false;
    }

    fn enum_missing(&mut self, _name: &Identifier, _old_enum: &Enum) {
        self.datatype_and_function_linking = false;
        self.datatype_layout = false;
    }

    fn enum_ability_mismatch(&mut self, _name: &Identifier, _old_enum: &Enum, _new_enum: &Enum) {
        self.datatype_and_function_linking = false;
    }

    fn enum_type_param_mismatch(&mut self, _name: &Identifier, _old_enum: &Enum, _new_enum: &Enum) {
        self.datatype_and_function_linking = false;
    }

    fn enum_new_variant(&mut self, _name: &Identifier, _old_enum: &Enum, _new_enum: &Enum) {
        self.no_new_variants = false;
    }

    fn enum_variant_missing(&mut self, _name: &Identifier, _old_enum: &Enum, _tag: usize) {
        self.datatype_layout = false;
    }

    fn enum_variant_mismatch(
        &mut self,
        _name: &Identifier,
        _old_enum: &Enum,
        _new_enum: &Enum,
        _tag: usize,
    ) {
        self.datatype_layout = false;
    }

    fn function_missing_public(&mut self, _name: &Identifier, _old_func: &Function) {
        self.datatype_and_function_linking = false;
    }

    fn function_missing_entry(&mut self, _name: &Identifier, _old_func: &Function) {
        self.entry_linking = false;
    }

    fn function_signature_mismatch(
        &mut self,
        _name: &Identifier,
        old_func: &Function,
        _new_func: &Function,
    ) {
        if old_func.visibility == Visibility::Public {
            self.datatype_and_function_linking = false;
        }

        if old_func.is_entry {
            self.entry_linking = false;
        }
    }

    fn function_lost_public_visibility(&mut self, _name: &Identifier, _old_func: &Function) {
        self.datatype_and_function_linking = false;
    }

    fn function_entry_compatibility(
        &mut self,
        _name: &Identifier,
        _old_func: &Function,
        _new_func: &Function,
    ) {
        self.entry_linking = false;
    }

    /// Finish by comparing against the compatibility flags.
    fn finish(self, compatability: &Compatibility) -> Result<(), ()> {
        if !self.datatype_and_function_linking {
            return Err(());
        }
        if compatability.check_datatype_layout && !self.datatype_layout {
            return Err(());
        }
        if compatability.check_private_entry_linking && !self.entry_linking {
            return Err(());
        }
        if compatability.check_datatype_layout && !self.no_new_variants {
            return Err(());
        }

        Ok(())
    }
}
