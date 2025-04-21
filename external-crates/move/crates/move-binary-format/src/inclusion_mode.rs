// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::compatibility::{Enum, Function, InclusionCheck, Struct};
use std::rc::Rc;

use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::{IdentStr, Identifier};

pub trait InclusionCheckMode: Default {
    type Error;
    fn module_id_mismatch(
        &mut self,
        old_address: &AccountAddress,
        old_name: &IdentStr,
        new_address: &AccountAddress,
        new_name: &IdentStr,
    );
    fn file_format_version_downgrade(&mut self, old_version: u32, new_version: u32);
    fn struct_new(&mut self, name: &Identifier, new_struct: &Rc<Struct>);
    fn struct_change(
        &mut self,
        name: &Identifier,
        old_struct: &Rc<Struct>,
        new_struct: &Rc<Struct>,
    );
    fn struct_missing(&mut self, name: &Identifier, old_struct: &Rc<Struct>);
    fn enum_new(&mut self, name: &Identifier, new_enum: &Rc<Enum>);
    fn enum_change(&mut self, name: &Identifier, new_enum: &Rc<Enum>);
    fn enum_missing(&mut self, name: &Identifier, old_enum: &Rc<Enum>);
    fn function_new(&mut self, name: &Identifier, new_func: &Rc<Function>);
    fn function_change(
        &mut self,
        name: &Identifier,
        old_func: &Rc<Function>,
        new_func: &Rc<Function>,
    );
    fn function_missing(&mut self, name: &Identifier, old_func: &Rc<Function>);

    fn friend_mismatch(&mut self, old_count: usize, new_count: usize);

    fn finish(self, inclusion: &InclusionCheck) -> Result<(), Self::Error>;
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

    fn module_id_mismatch(
        &mut self,
        _old_address: &AccountAddress,
        _old_name: &IdentStr,
        _new_address: &AccountAddress,
        _new_name: &IdentStr,
    ) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn file_format_version_downgrade(&mut self, _old_version: u32, _new_version: u32) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn struct_new(&mut self, _name: &Identifier, _new_struct: &Rc<Struct>) {
        self.is_equal = false;
    }

    fn struct_change(
        &mut self,
        _name: &Identifier,
        _old_struct: &Rc<Struct>,
        _new_struct: &Rc<Struct>,
    ) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn struct_missing(&mut self, _name: &Identifier, _old_struct: &Rc<Struct>) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn enum_new(&mut self, _name: &Identifier, _new_enum: &Rc<Enum>) {
        self.is_equal = false;
    }

    fn enum_change(&mut self, _name: &Identifier, _new_enum: &Rc<Enum>) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn enum_missing(&mut self, _name: &Identifier, _old_enum: &Rc<Enum>) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn function_new(&mut self, _name: &Identifier, _new_func: &Rc<Function>) {
        self.is_equal = false;
    }

    fn function_change(
        &mut self,
        _name: &Identifier,
        _old_func: &Rc<Function>,
        _new_func: &Rc<Function>,
    ) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn function_missing(&mut self, _name: &Identifier, _old_func: &Rc<Function>) {
        self.is_subset = false;
        self.is_equal = false;
    }

    fn friend_mismatch(&mut self, _old_count: usize, _new_count: usize) {
        self.is_equal = false;
    }

    fn finish(self, inclusion: &InclusionCheck) -> Result<(), Self::Error> {
        match inclusion {
            InclusionCheck::Subset if !self.is_subset => Err(()),
            InclusionCheck::Equal if !self.is_equal => Err(()),
            _ => Ok(()),
        }
    }
}
