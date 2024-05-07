// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::sandbox::utils::{is_bytecode_file, on_disk_state_view::OnDiskStateView};

use anyhow::{bail, Result};
use std::path::Path;
/// Print a module or resource stored in `file`
pub fn view(_state: &OnDiskStateView, path: &Path) -> Result<()> {
    if is_bytecode_file(path) {
        let bytecode_opt = OnDiskStateView::view_module(path)?;
        match bytecode_opt {
            Some(bytecode) => println!("{}", bytecode),
            None => println!("Bytecode not found."),
        }
    } else {
        bail!("`move view <file>` must point to a valid file under storage")
    }
    Ok(())
}
