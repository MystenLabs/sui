// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// TODO: Consider moving this under `vm/`

pub mod extensions;
pub mod functions;
pub mod move_stdlib;

use functions::NativeFunction;

pub fn make_module_natives(
    natives: impl IntoIterator<Item = (impl Into<String>, NativeFunction)>,
) -> impl Iterator<Item = (String, NativeFunction)> {
    natives
        .into_iter()
        .map(|(func_name, func)| (func_name.into(), func))
}
