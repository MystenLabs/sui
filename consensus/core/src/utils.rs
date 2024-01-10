// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::AuthorityRound;

use consensus_config::AuthorityIndex;

fn format_authority_index(i: AuthorityIndex) -> char {
    ('A' as usize + i.value()) as u8 as char
}

pub fn format_authority_round(author_round: &AuthorityRound) -> String {
    if author_round.authority.value() < 26 {
        format!(
            "{}{}",
            format_authority_index(author_round.authority),
            author_round.round
        )
    } else {
        format!("[{:02}]{}", author_round.authority, author_round.round)
    }
}
