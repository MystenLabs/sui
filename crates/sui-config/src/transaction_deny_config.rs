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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    object_deny_list: Vec<ObjectID>,

    /// A list of package object IDs that are not allowed to be called into in transactions,
    /// either directly or indirectly through transitive dependencies.
    /// Note that this does not apply to type arguments.
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
