// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::Round;

use consensus_config::AuthorityIndex;

pub fn format_authority_index(i: AuthorityIndex) -> char {
    ('A' as usize + i.value()) as u8 as char
}

pub fn format_authority_round(i: AuthorityIndex, r: Round) -> String {
    format!("{}{}", format_authority_index(i), r)
}
