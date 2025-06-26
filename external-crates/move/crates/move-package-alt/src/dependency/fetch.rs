// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Git dependencies are cached in `~/.move`. Each dependency has a sparse, shallow checkout
//! in the directory `~/.move/<remote>_<sha>` (see [crate::git::format_repo_to_fs_path])

use crate::package::paths::PackagePath;

use super::Dependency;

/// Once a dependency has been fetched, it is simply represented by a [PackagePath]
type Fetched = PackagePath;

pub struct FetchedDependency(pub(super) Dependency<Fetched>);
