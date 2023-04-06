// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{cfgir::absint::*, hlir::ast::Var, shared::unique_map::UniqueMap};
use move_ir_types::location::*;

//**************************************************************************************************
// Abstract state
//**************************************************************************************************

#[derive(Debug, Clone, Copy)]
pub enum UnavailableReason {
    Unassigned,
    Moved,
}

#[derive(Clone, Debug)]
pub enum LocalState {
    // Local does not have a value
    Unavailable(Loc, UnavailableReason),
    // Local has a value
    Available(Loc),
    // Available in some branches but not all. If it is a resource, cannot be assigned
    MaybeUnavailable {
        available: Loc,
        unavailable: Loc,
        unavailable_reason: UnavailableReason,
    },
}

impl LocalState {
    pub fn is_available(&self) -> bool {
        match self {
            LocalState::Available(_) => true,
            LocalState::Unavailable(_, _) | LocalState::MaybeUnavailable { .. } => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LocalStates {
    local_states: UniqueMap<Var, LocalState>,
}

impl LocalStates {
    pub fn initial<T>(function_arguments: &[(Var, T)], local_types: &UniqueMap<Var, T>) -> Self {
        let mut states = LocalStates {
            local_states: UniqueMap::new(),
        };
        for (var, _) in local_types.key_cloned_iter() {
            let local_state = LocalState::Unavailable(var.loc(), UnavailableReason::Unassigned);
            states.set_state(var, local_state)
        }
        for (var, _) in function_arguments {
            let local_state = LocalState::Available(var.loc());
            states.set_state(*var, local_state)
        }
        states
    }

    pub fn get_state(&self, local: &Var) -> &LocalState {
        self.local_states
            .get(local)
            .unwrap_or_else(|| panic!("ICE: Unable to get state for local {:#?}", local))
    }

    pub fn set_state(&mut self, local: Var, state: LocalState) {
        self.local_states.remove(&local);
        self.local_states.add(local, state).unwrap();
    }

    pub fn iter(&self) -> impl Iterator<Item = (Var, &LocalState)> {
        self.local_states.key_cloned_iter()
    }

    #[allow(dead_code)]
    pub fn debug(&self) {
        use LocalState as L;
        for (var, state) in self.iter() {
            print!("{}: ", var.0);
            match state {
                L::Unavailable(_, _) => println!("Unavailable"),
                L::Available(_) => println!("Available"),
                L::MaybeUnavailable { .. } => println!("MaybeUnavailable"),
            }
        }
    }
}

impl AbstractDomain for LocalStates {
    fn join(&mut self, other: &Self) -> JoinResult {
        use LocalState as L;
        let mut result = JoinResult::Unchanged;
        for (local, other_state) in other.local_states.key_cloned_iter() {
            match (self.get_state(&local), other_state) {
                // equal so nothing to do
                (L::Unavailable(_, _), L::Unavailable(_, _))
                | (L::Available(_), L::Available(_))
                | (L::MaybeUnavailable { .. }, L::MaybeUnavailable { .. }) => (),
                // if its partially assigned, stays partially assigned
                (L::MaybeUnavailable { .. }, _) => (),

                // if was partially assigned in other, its now partially assigned
                (_, L::MaybeUnavailable { .. }) => {
                    result = JoinResult::Changed;
                    let state = other_state.clone();
                    self.set_state(local, state)
                }

                // Available in one but not the other, so maybe unavailable
                (L::Available(available), L::Unavailable(unavailable, reason))
                | (L::Unavailable(unavailable, reason), L::Available(available)) => {
                    result = JoinResult::Changed;
                    let available = *available;
                    let unavailable = *unavailable;
                    let state = L::MaybeUnavailable {
                        available,
                        unavailable,
                        unavailable_reason: *reason,
                    };

                    self.set_state(local, state)
                }
            }
        }

        result
    }
}
