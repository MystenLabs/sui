// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::SuiAddress;
use crate::{
    base_types::{ObjectID, ObjectRef, SequenceNumber},
    error::SuiResult,
    event::Event,
    object::Object,
    SUI_FRAMEWORK_OBJECT_ID,
};
use move_core_types::ident_str;
use move_core_types::identifier::{IdentStr, Identifier};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum WriteKind {
    /// The object was in storage already but has been modified
    Mutate,
    /// The object was created in this transaction
    Create,
    /// The object was previously wrapped in another object, but has been restored to storage
    Unwrap,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum DeleteKind {
    /// An object is provided in the call input, and gets deleted.
    Normal,
    /// An object is not provided in the call input, but gets unwrapped
    /// from another object, and then gets deleted.
    UnwrapThenDelete,
    /// An object is provided in the call input, and gets wrapped into another object.
    Wrap,
}

pub enum ObjectChange {
    Write(InnerTxContext, Object, WriteKind),
    Delete(InnerTxContext, SequenceNumber, DeleteKind),
}

#[derive(Clone)]
pub struct InnerTxContext {
    pub package_id: ObjectID,
    pub transaction_module: Identifier,
    pub sender: SuiAddress,
}

impl InnerTxContext {
    pub fn transfer_sui(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("transfer_sui"), sender)
    }
    pub fn transfer_object(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("transfer_object"), sender)
    }
    pub fn native_transaction(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("native"), sender)
    }
    pub fn pay(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("pay"), sender)
    }
    pub fn unused_input(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("unused_input_object"), sender)
    }
    pub fn publish(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("publish"), sender)
    }
    pub fn gas(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("gas"), sender)
    }
    fn sui_transaction(ident: &IdentStr, sender: SuiAddress) -> Self {
        Self {
            package_id: SUI_FRAMEWORK_OBJECT_ID,
            transaction_module: Identifier::from(ident),
            sender,
        }
    }
}

/// An abstraction of the (possibly distributed) store for objects. This
/// API only allows for the retrieval of objects, not any state changes
pub trait ChildObjectResolver {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>>;
}

/// An abstraction of the (possibly distributed) store for objects, and (soon) events and transactions
pub trait Storage {
    fn reset(&mut self);

    /// Record an event that happened during execution
    fn log_event(&mut self, event: Event);

    fn read_object(&self, id: &ObjectID) -> Option<&Object>;

    fn apply_object_changes(&mut self, changes: BTreeMap<ObjectID, ObjectChange>);
}

pub trait BackingPackageStore {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>>;
}

impl<S: BackingPackageStore> BackingPackageStore for std::sync::Arc<S> {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        BackingPackageStore::get_package(self.as_ref(), package_id)
    }
}

impl<S: BackingPackageStore> BackingPackageStore for &S {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        BackingPackageStore::get_package(*self, package_id)
    }
}

impl<S: BackingPackageStore> BackingPackageStore for &mut S {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        BackingPackageStore::get_package(*self, package_id)
    }
}

pub trait ParentSync {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>>;
}

impl<S: ParentSync> ParentSync for std::sync::Arc<S> {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        ParentSync::get_latest_parent_entry_ref(self.as_ref(), object_id)
    }
}

impl<S: ParentSync> ParentSync for &S {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        ParentSync::get_latest_parent_entry_ref(*self, object_id)
    }
}

impl<S: ParentSync> ParentSync for &mut S {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        ParentSync::get_latest_parent_entry_ref(*self, object_id)
    }
}

impl<S: ChildObjectResolver> ChildObjectResolver for std::sync::Arc<S> {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        ChildObjectResolver::read_child_object(self.as_ref(), parent, child)
    }
}

impl<S: ChildObjectResolver> ChildObjectResolver for &S {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        ChildObjectResolver::read_child_object(*self, parent, child)
    }
}
