// Copyright (c) Verichains, 2023

use std::{cell::RefCell, collections::HashSet, rc::Rc};

use move_model::ty::Type;

fn default_display(ty: &Type, _: &Naming) -> String {
    format!("{:?}", ty)
}

pub struct Naming<'a> {
    arg_count: usize,
    type_display: Rc<RefCell<dyn Fn(&Type, &Naming) -> String + 'a>>,
    referenced_vairables: Option<HashSet<usize>>,
}

impl Clone for Naming<'_> {
    fn clone(&self) -> Self {
        Naming {
            arg_count: self.arg_count,
            type_display: self.type_display.clone(),
            referenced_vairables: self.referenced_vairables.clone(),
        }
    }
}

impl<'a> Naming<'a> {
    pub fn new() -> Self {
        Naming {
            arg_count: 0,
            type_display: Rc::new(RefCell::new(default_display)),
            referenced_vairables: None,
        }
    }

    pub fn with_arg_count<'b>(&self, arg_count: usize) -> Naming<'b>
    where
        'a: 'b,
    {
        Naming {
            arg_count,
            type_display: self.type_display.clone(),
            ..self.clone()
        }
    }

    pub fn with_type_display<'b, T>(&self, type_display: T) -> Naming<'b>
    where
        T: Fn(&Type, &Naming) -> String + 'b,
    {
        Naming {
            type_display: Rc::new(RefCell::new(type_display)),
            ..self.clone()
        }
    }

    pub fn with_referenced_variables<'b>(
        &self,
        referenced_vairables: &'b HashSet<usize>,
    ) -> Naming<'b>
    where
        'a: 'b,
    {
        Naming::<'b> {
            referenced_vairables: Some(referenced_vairables.clone()),
            type_display: self.type_display.clone(),
            arg_count: self.arg_count,
        }
    }

    pub fn templated_type(&self, idx: usize) -> String {
        format!("T{}", idx)
    }

    pub fn place_holder(&self) -> String {
        "_".to_string()
    }

    pub fn variable(&self, idx: usize) -> String {
        if let Some(referenced_vairables) = &self.referenced_vairables {
            if !referenced_vairables.contains(&idx) {
                return self.place_holder();
            }
        }
        if idx < self.arg_count {
            self.argument(idx)
        } else {
            self.local(idx - self.arg_count)
        }
    }

    pub fn argument(&self, idx: usize) -> String {
        format!("arg{}", idx)
    }

    fn local(&self, idx: usize) -> String {
        format!("v{}", idx)
    }

    pub fn ty(&self, ty: &Type) -> String {
        (self.type_display.borrow())(ty, &self)
    }
}
