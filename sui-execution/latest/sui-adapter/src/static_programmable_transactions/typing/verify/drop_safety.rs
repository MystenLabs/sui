// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{
    env::Env,
    typing::{ast as T, verify::memory_safety::Borrowed},
};
use sui_types::error::ExecutionError;

/// Refines usage of values so that the last `Copy` of a value is a `Move` if it is not borrowed
/// After, it verifies the following
/// - No results without `drop` are unused (all unused non-input values have `drop`)
pub fn refine_and_verify(
    env: &Env,
    borrow_states: Vec<Borrowed>,
    ast: &mut T::Transaction,
) -> Result<(), ExecutionError> {
    // refine::transaction(env, borrow_states, ast)?;
    // verify::transaction(env, &ast)?;
    todo!()
}

// mod refine {
//     use crate::static_programmable_transactions::{
//         env::Env,
//         typing::{
//             ast::{self as T, Type},
//             verify::memory_safety::Borrowed as BorrowedState,
//         },
//     };
//     use std::collections::BTreeSet;

//     struct Borrowed<'a> {
//         command_state: &'a BorrowedState,
//         arguments_acc: BorrowedState,
//     }

//     impl<'a> Borrowed<'a> {
//         fn is_borrowed(&self, loc: &T::Location) -> bool {
//             self.command_state.contains(loc) || self.arguments_acc.contains(loc)
//         }
//     }

//     fn command(
//         used: &mut BTreeSet<(u16, u16)>,
//         borrowed: &BorrowedState,
//         command: &mut T::Command,
//     ) {
//         let args = match command {
//             T::Command::MoveCall(mc) => argument_states(borrowed, mc.arguments.iter_mut()),
//             T::Command::TransferObjects(objects, recipient) => argument_states(
//                 borrowed,
//                 objects.iter_mut().chain(std::iter::once(recipient)),
//             ),
//             T::Command::SplitCoins(_, coin, amounts) => {
//                 argument_states(borrowed, std::iter::once(coin).chain(amounts))
//             }
//             T::Command::MergeCoins(_, target, coins) => {
//                 argument_states(borrowed, std::iter::once(target).chain(coins))
//             }
//             T::Command::MakeMoveVec(_, xs) => argument_states(borrowed, xs),
//             T::Command::Publish(_, _) => vec![],
//             T::Command::Upgrade(_, _, _, x) => argument_states(borrowed, std::iter::once(x)),
//         };
//         arguments(used, args)
//     }

//     fn argument_states<'state, 'arg>(
//         borrowed: &'state BorrowedState,
//         args: impl IntoIterator<Item = &'arg mut T::Argument>,
//     ) -> Vec<(Borrowed<'state>, &'arg mut T::Argument)> {
//         let mut acc = BorrowedState::new();
//         args.into_iter()
//             .map(|arg| {
//                 let mut arguments_acc = acc.clone();
//                 let borrowed = Borrowed {
//                     command_state: borrowed,
//                     arguments_acc: borrowed.clone(),
//                 };
//                 (borrowed, arg)
//             })
//             .collect()
//     }

//     fn arguments(used: &mut BTreeSet<(u16, u16)>, args: Vec<(Borrowed, &mut T::Argument)>) {
//         for (borrowed, arg) in args.into_iter().rev() {
//             argument(used, borrowed, arg)
//         }
//     }

//     fn argument(used: &mut BTreeSet<(u16, u16)>, borrowed: Borrowed, (arg_, ty): &mut T::Argument) {
//         match &arg_ {
//             T::Argument_::Move(T::Location::Result(i, j)) => {
//                 used.insert((*i, *j));
//             }
//             T::Argument_::Copy(T::Location::Result(i, j)) => {
//                 // we are at the last usage of a reference result if it was not yet added to the set
//                 let last_usage = used.insert((*i, *j));
//                 let loc = T::Location::Result(*i, *j);
//                 if last_usage && !borrowed.is_borrowed(&loc) {
//                     // if it was the last usage and is not borrowed,
//                     // we need to change the Copy to a Move
//                     *arg_ = T::Argument_::Move(loc);
//                 }
//             }
//             _ => (),
//         }
//     }
// }
