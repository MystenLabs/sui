// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use num::BigUint;

use crate::{
    ast::{TempIndex, Value},
    model::{DatatypeId, FunctionEnv, GlobalEnv, Loc, NodeId, QualifiedInstId},
    symbol::Symbol,
    ty::Type,
};

/// A trait that defines a generator for `Exp`.
pub trait ExpGenerator<'env> {
    /// Get the functional environment
    fn function_env(&self) -> &FunctionEnv<'env>;

    /// Get the current location
    fn get_current_loc(&self) -> Loc;

    /// Set the current location
    fn set_loc(&mut self, loc: Loc);

    /// Add a local variable with given type, return the local index.
    fn add_local(&mut self, ty: Type) -> TempIndex;

    /// Get the type of a local given at `temp` index
    fn get_local_type(&self, temp: TempIndex) -> Type;

    /// Get the global environment
    fn global_env(&self) -> &'env GlobalEnv {
        self.function_env().module_env.env
    }

    /// Sets the default location from a node id.
    fn set_loc_from_node(&mut self, node_id: NodeId) {
        let loc = self.global_env().get_node_loc(node_id);
        self.set_loc(loc);
    }

    /// Creates a new expression node id, using current default location, provided type,
    /// and optional instantiation.
    fn new_node(&self, ty: Type, inst_opt: Option<Vec<Type>>) -> NodeId {
        let node_id = self.global_env().new_node(self.get_current_loc(), ty);
        if let Some(inst) = inst_opt {
            self.global_env().set_node_instantiation(node_id, inst);
        }
        node_id
    }

    /// Allocates a new temporary.
    fn new_temp(&mut self, ty: Type) -> TempIndex {
        self.add_local(ty)
    }

    /// Make a boolean constant expression.
    fn mk_bool_const(&self, value: bool) -> Value {
        Value::Bool(value)
    }

    /// Make an address constant.
    fn mk_address_const(&self, value: BigUint) -> Value {
        Value::Address(value)
    }

    /// Makes a symbol from a string.
    fn mk_symbol(&self, str: &str) -> Symbol {
        self.global_env().symbol_pool().make(str)
    }

    /// Get's the memory associated with a Call(Global,..) or Call(Exists, ..) node. Crashes
    /// if the node is not typed as expected.
    fn get_memory_of_node(&self, node_id: NodeId) -> QualifiedInstId<DatatypeId> {
        // We do have a call `f<R<..>>` so extract the type from the function instantiation.
        let rty = &self.global_env().get_node_instantiation(node_id)[0];
        let (mid, sid, inst) = rty.require_datatype();
        mid.qualified_inst(sid, inst.to_owned())
    }
}
