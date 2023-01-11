// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use sui_types::base_types::TransactionDigest;
use sui_types::messages::TransactionEffects;

pub struct CasualOrder {
    not_seen: BTreeMap<TransactionDigest, TransactionEffects>,
    output: Vec<TransactionEffects>,
}

impl CasualOrder {
    /// Casually sort given vector of effects
    ///
    /// Returned list has effects that
    ///
    /// (a) Casually sorted
    /// (b) Have deterministic order between transactions that are not casually dependent
    ///
    /// The order of result list does not depend on order of effects in the supplied vector
    pub fn casual_sort(effects: Vec<TransactionEffects>) -> Vec<TransactionEffects> {
        let mut this = Self::from_vec(effects);
        while let Some(item) = this.pop_first() {
            this.insert(item);
        }
        this.into_list()
    }

    fn from_vec(effects: Vec<TransactionEffects>) -> Self {
        let output = Vec::with_capacity(effects.len() * 2);
        let not_seen = effects
            .into_iter()
            .map(|e| (e.transaction_digest, e))
            .collect();
        Self { not_seen, output }
    }

    fn pop_first(&mut self) -> Option<TransactionEffects> {
        // Once map_first_last is stabilized this function can be rewritten as this:
        // self.not_seen.pop_first()
        let key = *self.not_seen.keys().next()?;
        Some(self.not_seen.remove(key.as_ref()).unwrap())
    }

    // effect is already removed from self.not_seen at this point
    fn insert(&mut self, effect: TransactionEffects) {
        let initial_state = InsertState::new(effect);
        let mut states = vec![initial_state];

        while let Some(state) = states.last_mut() {
            if let Some(new_state) = state.process(self) {
                // This is essentially a 'recursive call' but using heap instead of stack to store state
                states.push(new_state);
            } else {
                // Done with current state, remove it
                states.pop().expect("Should contain an element");
            }
        }
    }

    fn into_list(self) -> Vec<TransactionEffects> {
        self.output
    }
}

struct InsertState {
    dependencies: Vec<TransactionDigest>,
    effect: Option<TransactionEffects>,
}

impl InsertState {
    pub fn new(effect: TransactionEffects) -> Self {
        Self {
            dependencies: effect.dependencies.clone(),
            effect: Some(effect),
        }
    }

    pub fn process(&mut self, casual_order: &mut CasualOrder) -> Option<InsertState> {
        while let Some(dep) = self.dependencies.pop() {
            if let Some(dep_effect) = casual_order.not_seen.remove(dep.as_ref()) {
                return Some(InsertState::new(dep_effect));
            }
        }
        let effect = self
            .effect
            .take()
            .expect("Can't use InsertState after it is finished");
        casual_order.output.push(effect);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_casual_order() {
        let e1 = e(d(1), vec![d(2), d(3)]);
        let e2 = e(d(2), vec![d(3), d(4)]);
        let e3 = e(d(3), vec![]);
        let e4 = e(d(4), vec![]);

        let r = extract(CasualOrder::casual_sort(vec![
            e1.clone(),
            e2,
            e3,
            e4.clone(),
        ]));
        assert_eq!(r, vec![3, 4, 2, 1]);

        // e1 and e4 are not (directly) casually dependent - ordered lexicographically
        let r = extract(CasualOrder::casual_sort(vec![e1.clone(), e4.clone()]));
        assert_eq!(r, vec![1, 4]);
        let r = extract(CasualOrder::casual_sort(vec![e4, e1]));
        assert_eq!(r, vec![1, 4]);
    }

    fn extract(e: Vec<TransactionEffects>) -> Vec<u8> {
        e.into_iter()
            .map(|e| e.transaction_digest.as_ref()[0])
            .collect()
    }

    fn d(i: u8) -> TransactionDigest {
        let mut bytes: [u8; 32] = Default::default();
        bytes[0] = i;
        TransactionDigest::new(bytes)
    }

    fn e(
        transaction_digest: TransactionDigest,
        dependencies: Vec<TransactionDigest>,
    ) -> TransactionEffects {
        TransactionEffects {
            transaction_digest,
            dependencies,
            ..Default::default()
        }
    }
}
