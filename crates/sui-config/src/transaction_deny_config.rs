// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use sui_types::base_types::{ObjectID, SuiAddress};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TransactionDenyConfig {
    /// A list of object IDs that are not allowed to be accessed/used in transactions.
    /// Note that since this is checked during transaction signing, only root object ids
    /// are supported here (i.e. no child-objects).
    /// Similarly this does not apply to wrapped objects as they are not directly accessible.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    object_deny_list: Vec<ObjectID>,

    /// A list of package object IDs that are not allowed to be called into in transactions,
    /// either directly or indirectly through transitive dependencies.
    /// Note that this does not apply to type arguments.
    /// Also since we only compare the deny list against the upgraded package ID of each dependency
    /// in the used package, when a package ID is denied, newer versions of that package are
    /// still allowed. If we want to deny the entire upgrade family of a package, we need to
    /// explicitly specify all the package IDs in the deny list.
    /// TODO: We could consider making this more flexible, e.g. whether to check in type args,
    /// whether to block entire upgrade family, whether to allow upgrade and etc.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    package_deny_list: Vec<ObjectID>,

    /// A list of sui addresses that are not allowed to be used as the sender or sponsor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    address_deny_list: Vec<SuiAddress>,

    /// Whether publishing new packages is disabled.
    #[serde(default)]
    package_publish_disabled: bool,

    /// Whether upgrading existing packages is disabled.
    #[serde(default)]
    package_upgrade_disabled: bool,

    /// Whether usage of shared objects is disabled.
    #[serde(default)]
    shared_object_disabled: bool,

    /// Whether user transactions are disabled (i.e. only system transactions are allowed).
    /// This is essentially a kill switch for transactions processing to a degree.
    #[serde(default)]
    user_transaction_disabled: bool,

    /// In-memory maps for faster lookup of various lists.
    #[serde(skip)]
    object_deny_map: OnceCell<HashSet<ObjectID>>,

    #[serde(skip)]
    package_deny_map: OnceCell<HashSet<ObjectID>>,

    #[serde(skip)]
    sender_deny_map: OnceCell<HashSet<SuiAddress>>,
    // TODO: We could consider add a deny list for types that we want to disable public transfer.
    // TODO: We could also consider disable more types of commands, such as transfer, split and etc.
}

impl TransactionDenyConfig {
    pub fn get_object_deny_map(&self) -> &HashSet<ObjectID> {
        self.object_deny_map
            .get_or_init(|| self.object_deny_list.iter().cloned().collect())
    }

    pub fn get_package_deny_map(&self) -> &HashSet<ObjectID> {
        self.package_deny_map
            .get_or_init(|| self.package_deny_list.iter().cloned().collect())
    }

    pub fn get_address_deny_map(&self) -> &HashSet<SuiAddress> {
        self.sender_deny_map
            .get_or_init(|| self.address_deny_list.iter().cloned().collect())
    }

    pub fn package_publish_disabled(&self) -> bool {
        self.package_publish_disabled
    }

    pub fn package_upgrade_disabled(&self) -> bool {
        self.package_upgrade_disabled
    }

    pub fn shared_object_disabled(&self) -> bool {
        self.shared_object_disabled
    }

    pub fn user_transaction_disabled(&self) -> bool {
        self.user_transaction_disabled
    }
}

#[derive(Default)]
pub struct TransactionDenyConfigBuilder {
    config: TransactionDenyConfig,
}

impl TransactionDenyConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(self) -> TransactionDenyConfig {
        self.config
    }

    pub fn disable_user_transaction(mut self) -> Self {
        self.config.user_transaction_disabled = true;
        self
    }

    pub fn disable_shared_object_transaction(mut self) -> Self {
        self.config.shared_object_disabled = true;
        self
    }

    pub fn disable_package_publish(mut self) -> Self {
        self.config.package_publish_disabled = true;
        self
    }

    pub fn disable_package_upgrade(mut self) -> Self {
        self.config.package_upgrade_disabled = true;
        self
    }

    pub fn add_denied_object(mut self, id: ObjectID) -> Self {
        self.config.object_deny_list.push(id);
        self
    }

    pub fn add_denied_address(mut self, address: SuiAddress) -> Self {
        self.config.address_deny_list.push(address);
        self
    }

    pub fn add_denied_package(mut self, id: ObjectID) -> Self {
        self.config.package_deny_list.push(id);
        self
    }
}
