// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::empty_module;
use move_bytecode_source_map::mapping::SourceMapping;
use move_ir_types::location::Spanned;

#[test]
fn test_empty_module() {
    let module = empty_module();
    let location = Spanned::unsafe_no_loc(()).loc;
    SourceMapping::new_without_source_map(&module, location)
        .expect("unable to build source mapping for empty script");
}
