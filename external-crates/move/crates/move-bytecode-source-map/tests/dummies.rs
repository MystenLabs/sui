// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{binary_views::BinaryIndexedView, file_format::empty_script};
use move_bytecode_source_map::mapping::SourceMapping;
use move_ir_types::location::Spanned;

#[test]
fn test_empty_script() {
    let script = empty_script();
    let view = BinaryIndexedView::Script(&script);
    let location = Spanned::unsafe_no_loc(()).loc;
    SourceMapping::new_from_view(view, location)
        .expect("unable to build source mapping for empty script");
}
