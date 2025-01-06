use crate::compatibility::InclusionCheck;
use crate::normalized::{Enum, Function, Struct};
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;

pub trait InclusionCheckMode: Default {
    type Error;
    fn module_name_mismatch(&mut self, old_address: &Identifier, new_address: &Identifier);
    fn module_address_mismatch(
        &mut self,
        old_address: &AccountAddress,
        new_address: &AccountAddress,
    );
    fn file_format_version_downgrade(&mut self, old_version: u32, new_version: u32);
    fn struct_new(&mut self, name: &Identifier, new_struct: &Struct);
    fn struct_change(&mut self, name: &Identifier, old_struct: &Struct, new_struct: &Struct);
    fn struct_missing(&mut self, name: &Identifier, old_struct: &Struct);
    fn enum_new(&mut self, name: &Identifier, new_enum: &Enum);
    fn enum_change(&mut self, name: &Identifier, new_enum: &Enum);
    fn enum_missing(&mut self, name: &Identifier, old_enum: &Enum);
    fn function_new(&mut self, name: &Identifier, new_func: &Function);
    fn function_change(&mut self, name: &Identifier, new_func: &Function);
    fn function_missing(&mut self, name: &Identifier, old_func: &Function);

    fn friend_mismatch(&mut self, old_count: usize, new_count: usize);

    fn finish(&self, inclusion: &InclusionCheck) -> Result<(), Self::Error>;
}

pub struct InclusionCheckExecutionMode {
    is_subset: bool,
    is_equal: bool,
}

impl Default for InclusionCheckExecutionMode {
    fn default() -> Self {
        // true until proven otherwise by InclusionChecks.check()
        Self {
            is_subset: true,
            is_equal: true,
        }
    }
}

impl InclusionCheckMode for InclusionCheckExecutionMode {
    type Error = ();

    fn module_name_mismatch(&mut self, _old_address: &Identifier, _new_address: &Identifier) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn module_address_mismatch(
        &mut self,
        _old_address: &AccountAddress,
        _new_address: &AccountAddress,
    ) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn file_format_version_downgrade(&mut self, _old_version: u32, _new_version: u32) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn struct_new(&mut self, _name: &Identifier, _new_struct: &Struct) {
        self.is_equal = false;
    }

    fn struct_change(&mut self, _name: &Identifier, _old_struct: &Struct, _new_struct: &Struct) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn struct_missing(&mut self, _name: &Identifier, _old_struct: &Struct) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn enum_new(&mut self, _name: &Identifier, _new_enum: &Enum) {
        self.is_equal = false;
    }

    fn enum_change(&mut self, _name: &Identifier, _new_enum: &Enum) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn enum_missing(&mut self, _name: &Identifier, _old_enum: &Enum) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn function_new(&mut self, _name: &Identifier, _new_func: &Function) {
        self.is_equal = false;
    }

    fn function_change(&mut self, _name: &Identifier, _new_func: &Function) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn function_missing(&mut self, _name: &Identifier, _old_func: &Function) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn friend_mismatch(&mut self, _old_count: usize, _new_count: usize) {
        self.is_equal = false;
    }

    fn finish(&self, inclusion: &InclusionCheck) -> Result<(), Self::Error> {
        match inclusion {
            InclusionCheck::Subset if !self.is_subset => Err(()),
            InclusionCheck::Equal if !self.is_equal => Err(()),
            _ => Ok(()),
        }
    }
}
