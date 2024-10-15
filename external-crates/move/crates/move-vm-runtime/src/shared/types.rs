// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

pub type DefiningTypeId = AccountAddress;

/// On-chain storage ID for the package we are linking account (e.g., v0 and v1 will use different
/// Packge Storage IDs).
pub type PackageStorageId = AccountAddress;

/// Runtime ID: An ID used at runtime. This is consistent between versions (e.g., v0 and v1 will
/// use the same Runtime Package ID).
pub type RuntimePackageId = AccountAddress;
