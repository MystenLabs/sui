// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block::Slot;

use consensus_config::AuthorityIndex;

pub(crate) fn format_authority_round(author_round: &Slot) -> String {
    format!(
        "{}{}",
        format_authority_index(author_round.authority),
        author_round.round
    )
}

fn format_authority_index(i: AuthorityIndex) -> String {
    if i.value() < 26 {
        let c = (b'A' + i.value() as u8) as char;
        c.to_string()
    } else {
        format!("[{:02}]", i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_authority_index() {
        let index = AuthorityIndex::new_for_test(0);
        assert_eq!(format_authority_index(index), "A");

        let index = AuthorityIndex::new_for_test(150);
        assert_eq!(format_authority_index(index), "[150]");
    }
}
