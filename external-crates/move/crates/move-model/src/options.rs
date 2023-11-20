// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::simplifier::SimplificationPass;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelBuilderOptions {
    /// Ignore the "opaque" pragma on internal function (i.e., functions with no unknown callers)
    /// specs when possible. The opaque can be ignored as long as the function spec has no property
    /// marked as `[concrete]` or `[abstract]`.
    pub ignore_pragma_opaque_internal_only: bool,

    /// Ignore the "opaque" pragma on all function specs when possible. The opaque can be ignored
    /// as long as the function spec has no property marked as `[concrete]` or `[abstract]`.
    pub ignore_pragma_opaque_when_possible: bool,

    /// List of simplification passes and the order each pass to be executed
    pub simplification_pipeline: Vec<SimplificationPass>,
}
