// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    sp,
    static_programmable_transactions::{
        metering::translation_meter::TranslationMeter, typing::ast as T,
    },
};
use mysten_common::ZipDebugEqIteratorExt;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::TxContextKind, error::ExecutionErrorTrait, execution_status::ExecutionErrorKind,
};

/// Tracks the references live at any point in the transaction, and the total returned
struct Context<'pc> {
    protocol_config: &'pc ProtocolConfig,
    live: u64,
    total_returned: u64,
}

/// Charges for the references live at each command, and checks three limits:
/// - The number of references live at any point
/// - The number of references returned by any command
/// - The number of references returned over the whole transaction
pub fn meter<E: ExecutionErrorTrait>(
    meter: &mut TranslationMeter,
    protocol_config: &ProtocolConfig,
    transaction: &T::Transaction,
) -> Result<(), E> {
    let mut context = Context {
        protocol_config,
        live: 0,
        total_returned: 0,
    };
    for (idx, c) in transaction.commands.iter().enumerate() {
        command::<E>(&mut context, meter, c).map_err(|e| e.with_command_index(idx))?;
    }
    Ok(())
}

fn command<E: ExecutionErrorTrait>(
    context: &mut Context,
    meter: &mut TranslationMeter,
    sp!(_, c): &T::Command,
) -> Result<(), E> {
    // The references created by the arguments are held by the command until the return values
    // are fully processed
    let held = arguments::<E>(context, c.command.arguments())?;

    let returned = c.result_type.iter().filter(|ty| ty.is_reference()).count() as u64;
    context.returned_n::<E>(returned)?;
    // The references held for the arguments are consumed once the command has its return values
    context.free_n::<E>(held)?;

    // Unused results are dropped at the end of the command
    let dropped = {
        assert_invariant!(
            c.drop_values.len() == c.result_type.len(),
            "command drop_values length does not match result_type length"
        );
        c.drop_values
            .iter()
            .zip_debug_eq(&c.result_type)
            .filter(|(drop, ty)| **drop && ty.is_reference())
            .count() as u64
    };
    context.free_n::<E>(dropped)?;
    meter.charge_num_live_references(context.live)
}

/// Returns the number of references for the arguments, excluding the TxContext.
/// For each argument, appropriately tracks the creation/freeing of references
fn arguments<'a, E: ExecutionErrorTrait>(
    context: &mut Context,
    args: impl IntoIterator<Item = &'a T::Argument>,
) -> Result<u64, E> {
    args.into_iter().try_fold(0u64, |held, arg| {
        let is_ref = argument::<E>(context, arg)?;
        let incr = if is_ref { 1 } else { 0 };
        Ok(held.saturating_add(incr))
    })
}

/// Returns true iff the argument is a reference type, excluding the TxContext.
/// Counts the creation/freeing of references for the argument and locations/usage
fn argument<E: ExecutionErrorTrait>(
    context: &mut Context,
    sp!(_, (arg, ty)): &T::Argument,
) -> Result</* is ref */ bool, E> {
    if ty.is_tx_context() != TxContextKind::None {
        // TxContext is excluded from reference safety
        return Ok(false);
    }
    Ok(match arg {
        T::Argument__::Borrow(_, _location) => {
            context.create::<E>()?;
            true
        }

        T::Argument__::Freeze(u) => {
            let usage_is_ref = usage(context, u, ty)?;
            assert_invariant!(usage_is_ref, "freeze argument must be a reference type");
            context.free::<E>()?;
            context.create::<E>()?;
            true
        }
        T::Argument__::Read(u) => {
            let usage_is_ref = usage::<E>(context, u, ty)?;
            assert_invariant!(usage_is_ref, "read argument must be a reference type");
            context.free::<E>()?;
            false
        }
        T::Argument__::Use(u) => usage::<E>(context, u, ty)?,
    })
}

/// Returns true iff the argument is a reference type
/// Counts the creation of a new reference for `Copy`
fn usage<E: ExecutionErrorTrait>(
    context: &mut Context,
    u: &T::Usage,
    ty: &T::Type,
) -> Result<bool, E> {
    if !ty.is_reference() {
        return Ok(false);
    }
    match u {
        T::Usage::Move(_) => Ok(true),
        T::Usage::Copy { .. } => {
            context.create::<E>()?;
            Ok(true)
        }
    }
}

impl Context<'_> {
    fn create<E: ExecutionErrorTrait>(&mut self) -> Result<(), E> {
        self.create_n::<E>(1)
    }

    fn create_n<E: ExecutionErrorTrait>(&mut self, n: u64) -> Result<(), E> {
        self.live = self.live.saturating_add(n);
        let max_live = self.protocol_config.max_ptb_live_references();
        if self.live > max_live {
            return Err(E::new_with_source(
                // TODO introduce an ExecutionErrorKind for limits
                ExecutionErrorKind::InsufficientGas,
                format!(
                    "Command has {} live references, exceeding the maximum of {max_live}",
                    self.live
                ),
            ));
        }
        Ok(())
    }

    /// Creates the `n` references returned by a command, checking both the per-command limit and
    /// the transaction wide total. The total bounds the length of any chain of returned
    /// references, which other analyses are sensitive to beyond the number live at once.
    fn returned_n<E: ExecutionErrorTrait>(&mut self, n: u64) -> Result<(), E> {
        let max_per_command = self.protocol_config.max_ptb_returned_references();
        if n > max_per_command {
            return Err(E::new_with_source(
                // TODO introduce an ExecutionErrorKind for limits
                ExecutionErrorKind::InsufficientGas,
                format!(
                    "Command returns {n} references, exceeding the maximum of {max_per_command}"
                ),
            ));
        }
        self.total_returned = self.total_returned.saturating_add(n);
        let max_total = self.protocol_config.max_ptb_total_returned_references();
        if self.total_returned > max_total {
            return Err(E::new_with_source(
                // TODO introduce an ExecutionErrorKind for limits
                ExecutionErrorKind::InsufficientGas,
                format!(
                    "Transaction returns {} references, exceeding the maximum of {max_total}",
                    self.total_returned
                ),
            ));
        }
        self.create_n::<E>(n)
    }

    fn free<E: ExecutionErrorTrait>(&mut self) -> Result<(), E> {
        self.free_n::<E>(1)
    }

    fn free_n<E: ExecutionErrorTrait>(&mut self, n: u64) -> Result<(), E> {
        let Some(rem) = self.live.checked_sub(n) else {
            invariant_violation!("freeing {n} reference(s) when {} are live", self.live)
        };
        self.live = rem;
        Ok(())
    }
}
