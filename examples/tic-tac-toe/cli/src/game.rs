// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use fastcrypto::encoding::{Base64, Encoding};
use serde::Deserialize;
use sui_types::{
    base_types::{ObjectRef, SequenceNumber},
    digests::ObjectDigest,
    object::Owner,
};

use crate::board::Board;

pub(crate) struct Game {
    pub kind: GameKind,
    pub owner: Owner,
    pub version: SequenceNumber,
    pub digest: ObjectDigest,
    pub winner: Winner,
}

pub(crate) enum GameKind {
    Shared(Shared),
    Owned(Owned),
}

/// Rust representation of a Move `shared::Game`, suitable for deserializing from their BCS
/// representation.
#[derive(Deserialize)]
pub(crate) struct Shared {
    pub board: Board,
}

/// Rust representation of a Move `owned::Game`, suitable for deserializing from their BCS
/// representation.
#[derive(Deserialize)]
pub(crate) struct Owned {
    pub board: Board,
    pub admin: Vec<u8>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Winner {
    None,
    Draw,
    Win,
}

impl Game {
    pub(crate) fn object_ref(&self) -> ObjectRef {
        (self.kind.board().id, self.version, self.digest)
    }
}

impl GameKind {
    fn board(&self) -> &Board {
        match self {
            GameKind::Shared(shared) => &shared.board,
            GameKind::Owned(owned) => &owned.board,
        }
    }
}

impl fmt::Display for Game {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.kind {
            GameKind::Shared(shared) => write!(f, "{shared}"),
            GameKind::Owned(owned) => write!(f, "{owned}"),
        }?;

        match self.winner {
            Winner::None => {}
            Winner::Draw => {
                write!(f, "\n\n{: >34}DRAW!", ' ')?;
            }
            Winner::Win => {
                write!(
                    f,
                    "\n\n{: >33}{} WINS!",
                    ' ',
                    self.kind.board().prev_player()
                )?;
            }
        };

        Ok(())
    }
}

impl fmt::Display for Shared {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.board)
    }
}

impl fmt::Display for Owned {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{}", self.board)?;
        write!(f, "ADMIN: {}", Base64::encode(&self.admin))?;
        Ok(())
    }
}
