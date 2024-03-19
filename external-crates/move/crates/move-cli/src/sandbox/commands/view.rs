// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::sandbox::utils::{
    contains_module, is_bytecode_file, on_disk_state_view::OnDiskStateView,
};

use anyhow::{bail, Result};
use std::path::Path;
/// Print a module or resource stored in `file`
pub fn view(_state: &OnDiskStateView, path: &Path) -> Result<()> {
    if is_bytecode_file(path) {
        let bytecode_opt = if contains_module(path) {
            OnDiskStateView::view_module(path)?
        } else {
            // bytecode extension, but not a module--assume it's a script
            OnDiskStateView::view_script(path)?
        };
        match bytecode_opt {
            Some(bytecode) => println!("{}", bytecode),
            None => println!("Bytecode not found."),
        }
    } else {
        bail!("`move view <file>` must point to a valid file under storage")
    }
    Ok(())
}
