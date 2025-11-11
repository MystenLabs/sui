// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod block;
mod bloom;

pub(crate) use block::query_blocked_blooms;
pub(crate) use bloom::candidate_cp_blooms;

pub(super) fn bit_checks<T: AsRef<[usize]>>(
    keys_positions: &[T],
    column: &str,
    fixed_size_bytes: Option<usize>,
) -> String {
    keys_positions
        .iter()
        .map(|positions| {
            let checks: Vec<_> = positions
                .as_ref()
                .iter()
                .map(|&pos| bit_check(pos, column, fixed_size_bytes))
                .collect();
            format!("({})", checks.join(" AND "))
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn bit_check(pos: usize, column: &str, fixed_size_bytes: Option<usize>) -> String {
    let mask = 1 << (pos % 8);
    match fixed_size_bytes {
        Some(size) => {
            let byte_idx = (pos % (size * 8)) / 8;
            format!("(get_byte({column}, {byte_idx}) & {mask}) != 0")
        }
        None => {
            format!("(get_byte({column}, ({pos} % (length({column}) * 8)) / 8) & {mask}) != 0")
        }
    }
}
