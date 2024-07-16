// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use std::fmt;
use sui_types::base_types::{ObjectID, SuiAddress};

#[derive(Deserialize)]
pub(crate) struct Board {
    pub id: ObjectID,
    pub marks: Vec<u8>,
    pub turn: u8,
    pub x: SuiAddress,
    pub o: SuiAddress,
}

#[derive(Eq, PartialEq)]
pub(crate) enum Player {
    X,
    O,
}

impl Board {
    pub(crate) fn next_player(&self) -> Player {
        if self.turn % 2 == 0 {
            Player::X
        } else {
            Player::O
        }
    }

    pub(crate) fn prev_player(&self) -> Player {
        if self.turn % 2 == 0 {
            Player::O
        } else {
            Player::X
        }
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let m = |i: usize| match self.marks[i] {
            0 => ' ',
            1 => 'X',
            2 => 'O',
            _ => unreachable!(),
        };

        writeln!(f, "{: >31} {} | {} | {}", ' ', m(0), m(1), m(2))?;
        writeln!(f, "{: >31}---+---+---", ' ')?;
        writeln!(f, "{: >31} {} | {} | {}", ' ', m(3), m(4), m(5))?;
        writeln!(f, "{: >31}---+---+---", ' ')?;
        writeln!(f, "{: >31} {} | {} | {}", ' ', m(6), m(7), m(8))?;
        writeln!(f)?;

        use Player as P;
        let next = self.next_player();

        write!(f, "{}", if next == P::X { " -> " } else { "    " })?;
        writeln!(f, "X: {}", self.x)?;

        write!(f, "{}", if next == P::O { " -> " } else { "    " })?;
        writeln!(f, "O: {}", self.o)?;

        let with_prefix = true;
        write!(f, " GAME: {}", self.id.to_canonical_display(with_prefix))?;

        Ok(())
    }
}

impl fmt::Display for Player {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Player::X => write!(f, "X"),
            Player::O => write!(f, "O"),
        }
    }
}
