// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;
use std::path::PathBuf;

pub trait EnumOrderMap {
    fn order_to_variant_map() -> std::collections::BTreeMap<u64, String>;
}

pub fn check_enum_compat_order<T: EnumOrderMap>(snapshot_file: PathBuf) {
    let new_map = T::order_to_variant_map();

    if let Err(err) = std::fs::read_to_string(snapshot_file.clone()) {
        if err.kind() == std::io::ErrorKind::NotFound {
            // Create the file if not exists
            std::fs::create_dir_all(snapshot_file.parent().unwrap()).unwrap();
            let mut file = std::fs::File::create(snapshot_file).unwrap();
            let content: String = serde_yaml::to_string(&new_map).unwrap();

            write!(file, "{}", content).unwrap();
            return;
        }
        panic!("Error reading file: {:?}: err {:?}", snapshot_file, err);
    }

    let existing_map: std::collections::BTreeMap<u64, String> =
        serde_yaml::from_str(&std::fs::read_to_string(snapshot_file.clone()).unwrap()).unwrap();

    // Check that the new map includes the existing map in order
    for (pos, val) in existing_map {
        match new_map.get(&pos) {
            None => {
                panic!("Enum variant {} has been removed. Not allowed: enum must be backward compatible.", val);
            }
            Some(new_val) if new_val == &val => continue,
            Some(new_val) => {
                panic!("Enum variant {val} has been swapped with {new_val} at position {pos}. Not allowed: enum must be backward compatible.");
            }
        }
    }

    // Update the file
    let mut file = std::fs::File::create(snapshot_file).unwrap();
    let content: String = serde_yaml::to_string(&new_map).unwrap();

    write!(file, "{}", content).unwrap();
}
