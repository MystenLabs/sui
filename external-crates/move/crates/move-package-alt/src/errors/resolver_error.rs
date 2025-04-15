// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

use crate::package::PackageName;

#[derive(Error, Debug)]
pub enum ResolverError {
    #[error(
        "resolver {resolver} didn't resolve dependency {dep}; it returned an invalid dependency"
    )]
    ResolverReturnedUnresolved { resolver: String, dep: PackageName },
}
