// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub(crate) fn comma_separated<T: std::fmt::Display>(items: &[T]) -> String {
    items
        .iter()
        .map(|item| format!("{}", item))
        .collect::<Vec<_>>()
        .join(", ")
}
