// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use proptest::collection::vec;
use proptest::prelude::*;

use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::*;

use super::addr_balance::garbage_pt_strategy;
use super::coin_reservation::valid_coin_reservation_strategy;
use super::common::{TxFuzzContext, expiration_strategy};

/// Maximum number of valid building blocks composed into a single gasless PT.
const MAX_BLOCKS_PER_PT: usize = 4;

/// Per-sender funded balance of the custom coin in the gasless test setup.
/// Withdrawal amounts must stay well below this so multiple withdrawals in a
/// single composed PT can succeed without depleting the balance.
const FUNDED_BALANCE: u64 = 1_000;
const MAX_WITHDRAW: u64 = FUNDED_BALANCE / 10;
/// Amount used by `BalanceRedeemSplitSend` for the `balance::split` operation.
const SPLIT_AMOUNT: u64 = 1;

// =============================================================================
// VALID GASLESS BUILDING BLOCKS
//
// Each block calls only allow-listed sui-framework functions and is structured
// so that every value it produces is consumed (sent or destroyed) by the same
// block. This makes blocks composable: chaining N blocks always yields a valid
// gasless PT by construction.
//
// Gasless PTs have `budget = 0`, no `Argument::GasCoin`, and the protocol
// charges execution against the funds withdrawn by the PT itself.
// =============================================================================

#[derive(Debug, Clone)]
enum GaslessBlockSpec {
    /// `FundsWithdrawal → balance::redeem_funds → balance::send_funds`.
    BalanceRedeemSend { amount: u64, recipient: SuiAddress },
    /// `redeem → balance::split → send_funds (split) → send_funds (remainder)`.
    /// Tests `balance::split`.
    BalanceRedeemSplitSend {
        amount: u64,
        split: u64,
        recipient: SuiAddress,
    },
    /// `FundsWithdrawal → coin::redeem_funds → coin::send_funds`.
    CoinRedeemSend { amount: u64, recipient: SuiAddress },
    /// `coin::redeem_funds → coin::into_balance → balance::send_funds`.
    /// Tests `coin::into_balance`.
    CoinIntoBalanceSend { amount: u64, recipient: SuiAddress },
    /// `balance::zero → balance::send_funds`. No inputs at all — exercises
    /// `balance::zero` and the empty-PT happy path.
    BalanceZeroSend { recipient: SuiAddress },
    /// `coin::redeem_funds → balance::zero → coin::put → balance::send_funds`.
    /// Tests `coin::put`.
    CoinPut { amount: u64, recipient: SuiAddress },
    /// `funds_accumulator::withdrawal_split → redeem each → send each`.
    /// Tests `funds_accumulator::withdrawal_split`.
    WithdrawalSplitSend { amount: u64, recipient: SuiAddress },
    /// Coin-reservation `Object(ImmOrOwnedObject(<reservation>))` input →
    /// `coin::send_funds<T>`. See `BlockSpec::CoinReservationRedeemSend`
    /// in `addr_balance.rs` for why this can't use `balance::redeem_funds`.
    CoinReservationSend {
        res: ObjectRef,
        recipient: SuiAddress,
    },
}

fn ident(s: &'static str) -> Identifier {
    Identifier::new(s).unwrap()
}

/// Append a Move call to the sui-framework (`0x2`) parameterized by a single
/// type argument.
fn fw_call(
    b: &mut ProgrammableTransactionBuilder,
    module: &'static str,
    function: &'static str,
    ty: &TypeTag,
    args: Vec<Argument>,
) -> Argument {
    b.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident(module),
        ident(function),
        vec![ty.clone()],
        args,
    )
}

/// Push a `FundsWithdrawal<Balance<T>>` from the sender.
fn push_withdrawal(b: &mut ProgrammableTransactionBuilder, amount: u64, ty: &TypeTag) -> Argument {
    b.input(CallArg::FundsWithdrawal(
        FundsWithdrawalArg::balance_from_sender(amount, ty.clone()),
    ))
    .unwrap()
}

/// `balance::redeem_funds<T>(withdrawal) -> Balance<T>`.
fn redeem_to_balance(
    b: &mut ProgrammableTransactionBuilder,
    w: Argument,
    ty: &TypeTag,
) -> Argument {
    fw_call(b, "balance", "redeem_funds", ty, vec![w])
}

/// `coin::redeem_funds<T>(withdrawal) -> Coin<T>`.
fn redeem_to_coin(b: &mut ProgrammableTransactionBuilder, w: Argument, ty: &TypeTag) -> Argument {
    fw_call(b, "coin", "redeem_funds", ty, vec![w])
}

/// `balance::send_funds<T>(bal, recipient)`.
fn send_balance(
    b: &mut ProgrammableTransactionBuilder,
    bal: Argument,
    recipient: SuiAddress,
    ty: &TypeTag,
) {
    let r = b.pure(recipient).unwrap();
    fw_call(b, "balance", "send_funds", ty, vec![bal, r]);
}

/// `coin::send_funds<T>(coin, recipient)`.
fn send_coin(
    b: &mut ProgrammableTransactionBuilder,
    coin: Argument,
    recipient: SuiAddress,
    ty: &TypeTag,
) {
    let r = b.pure(recipient).unwrap();
    fw_call(b, "coin", "send_funds", ty, vec![coin, r]);
}

impl GaslessBlockSpec {
    fn apply(self, b: &mut ProgrammableTransactionBuilder, ft: &TypeTag) {
        match self {
            GaslessBlockSpec::BalanceRedeemSend { amount, recipient } => {
                let w = push_withdrawal(b, amount, ft);
                let bal = redeem_to_balance(b, w, ft);
                send_balance(b, bal, recipient, ft);
            }
            GaslessBlockSpec::BalanceRedeemSplitSend {
                amount,
                split,
                recipient,
            } => {
                let w = push_withdrawal(b, amount, ft);
                let bal = redeem_to_balance(b, w, ft);
                let split_amt = b.pure(split).unwrap();
                let split_bal = fw_call(b, "balance", "split", ft, vec![bal, split_amt]);
                send_balance(b, split_bal, recipient, ft);
                send_balance(b, bal, recipient, ft);
            }
            GaslessBlockSpec::CoinRedeemSend { amount, recipient } => {
                let w = push_withdrawal(b, amount, ft);
                let coin = redeem_to_coin(b, w, ft);
                send_coin(b, coin, recipient, ft);
            }
            GaslessBlockSpec::CoinIntoBalanceSend { amount, recipient } => {
                let w = push_withdrawal(b, amount, ft);
                let coin = redeem_to_coin(b, w, ft);
                let bal = fw_call(b, "coin", "into_balance", ft, vec![coin]);
                send_balance(b, bal, recipient, ft);
            }
            GaslessBlockSpec::BalanceZeroSend { recipient } => {
                let z = fw_call(b, "balance", "zero", ft, vec![]);
                send_balance(b, z, recipient, ft);
            }
            GaslessBlockSpec::CoinPut { amount, recipient } => {
                let w = push_withdrawal(b, amount, ft);
                let coin = redeem_to_coin(b, w, ft);
                let zero = fw_call(b, "balance", "zero", ft, vec![]);
                // coin::put<T>(&mut Balance<T>, Coin<T>) — PT handles &mut implicitly.
                fw_call(b, "coin", "put", ft, vec![zero, coin]);
                send_balance(b, zero, recipient, ft);
            }
            GaslessBlockSpec::WithdrawalSplitSend { amount, recipient } => {
                let w = push_withdrawal(b, amount, ft);
                let sub_limit = b.pure(move_core_types::u256::U256::from(1u64)).unwrap();
                // withdrawal_split takes &mut Withdrawal<T>, returns a fresh Withdrawal<T>.
                // Type arg is `Balance<T>`, not `T`.
                let balance_ty = sui_types::balance::Balance::type_tag(ft.clone());
                let split_w = fw_call(
                    b,
                    "funds_accumulator",
                    "withdrawal_split",
                    &balance_ty,
                    vec![w, sub_limit],
                );
                let b1 = redeem_to_balance(b, w, ft);
                let b2 = redeem_to_balance(b, split_w, ft);
                send_balance(b, b1, recipient, ft);
                send_balance(b, b2, recipient, ft);
            }
            GaslessBlockSpec::CoinReservationSend { res, recipient } => {
                let coin = b
                    .input(CallArg::Object(ObjectArg::ImmOrOwnedObject(res)))
                    .unwrap();
                send_coin(b, coin, recipient, ft);
            }
        }
    }
}

// =============================================================================
// VALID PT COMPOSITION
//
// Picks 1..=MAX_BLOCKS_PER_PT building blocks (with replacement, weighted) and
// chains them into a single PT. Always valid by construction.
// =============================================================================

/// `(amount, recipient)` strategy used by every withdraw-shaped block.
fn amount_and_recipient() -> impl Strategy<Value = (u64, SuiAddress)> {
    // `SPLIT_AMOUNT + 1` ensures `BalanceRedeemSplitSend` always has enough to split.
    (SPLIT_AMOUNT + 1..=MAX_WITHDRAW, any::<SuiAddress>())
}

fn any_gasless_block_spec_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<GaslessBlockSpec> {
    let coin_res = valid_coin_reservation_strategy(ctx);
    let arms: Vec<(u32, BoxedStrategy<GaslessBlockSpec>)> = vec![
        (
            4,
            amount_and_recipient()
                .prop_map(|(amount, recipient)| GaslessBlockSpec::BalanceRedeemSend {
                    amount,
                    recipient,
                })
                .boxed(),
        ),
        (
            3,
            amount_and_recipient()
                .prop_map(
                    |(amount, recipient)| GaslessBlockSpec::BalanceRedeemSplitSend {
                        amount,
                        split: SPLIT_AMOUNT,
                        recipient,
                    },
                )
                .boxed(),
        ),
        (
            3,
            amount_and_recipient()
                .prop_map(|(amount, recipient)| GaslessBlockSpec::CoinRedeemSend {
                    amount,
                    recipient,
                })
                .boxed(),
        ),
        (
            2,
            amount_and_recipient()
                .prop_map(
                    |(amount, recipient)| GaslessBlockSpec::CoinIntoBalanceSend {
                        amount,
                        recipient,
                    },
                )
                .boxed(),
        ),
        (
            2,
            any::<SuiAddress>()
                .prop_map(|recipient| GaslessBlockSpec::BalanceZeroSend { recipient })
                .boxed(),
        ),
        (
            2,
            amount_and_recipient()
                .prop_map(|(amount, recipient)| GaslessBlockSpec::CoinPut { amount, recipient })
                .boxed(),
        ),
        (
            2,
            amount_and_recipient()
                .prop_map(
                    |(amount, recipient)| GaslessBlockSpec::WithdrawalSplitSend {
                        amount,
                        recipient,
                    },
                )
                .boxed(),
        ),
        (
            3,
            (coin_res, any::<SuiAddress>())
                .prop_map(|(res, recipient)| GaslessBlockSpec::CoinReservationSend {
                    res,
                    recipient,
                })
                .boxed(),
        ),
    ];
    proptest::strategy::Union::new_weighted(arms).boxed()
}

fn gasless_composed_pt_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<ProgrammableTransaction> {
    let fund_type = ctx.fund_type.clone();
    vec(any_gasless_block_spec_strategy(ctx), 1..=MAX_BLOCKS_PER_PT)
        .prop_map(move |raw_blocks| {
            // Coin reservations against the same `(sender, fund_type)` accumulator
            // share an ObjectID but have distinct per-call digests, so the PT
            // builder rejects the second one. Keep only the first.
            let mut seen_coin_res = false;
            let blocks: Vec<GaslessBlockSpec> = raw_blocks
                .into_iter()
                .filter(|spec| {
                    if matches!(spec, GaslessBlockSpec::CoinReservationSend { .. }) {
                        if seen_coin_res {
                            return false;
                        }
                        seen_coin_res = true;
                    }
                    true
                })
                .collect();
            let mut b = ProgrammableTransactionBuilder::new();
            for spec in blocks {
                spec.apply(&mut b, &fund_type);
            }
            b.finish()
        })
        .boxed()
}

// =============================================================================
// TOP LEVEL
// =============================================================================

fn gasless_pt_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<ProgrammableTransaction> {
    prop_oneof![
        // Composed PTs of allow-listed block calls — main vehicle for execution-path coverage.
        8 => gasless_composed_pt_strategy(ctx),
        // Validator fuzz — random package/function/argument PTs that should be
        // rejected by the gasless validator. Catches rejection-path bugs.
        2 => garbage_pt_strategy(ctx),
    ]
    .boxed()
}

pub fn gasless_transaction_data_strategy(ctx: TxFuzzContext) -> BoxedStrategy<TransactionData> {
    let sender = ctx.sender;
    let sponsor = ctx.sponsor;
    // 1-in-4 chance of producing a sponsored gasless tx (dual-signed), if a sponsor
    // is configured. Sponsored gasless still has budget=0 — no implicit gas
    // withdrawal — but exercises the dual-signature path.
    let gas_owner_strategy: BoxedStrategy<SuiAddress> = match sponsor {
        Some(sp) => prop_oneof![
            3 => Just(sender),
            1 => Just(sp),
        ]
        .boxed(),
        None => Just(sender).boxed(),
    };
    (
        gasless_pt_strategy(&ctx),
        expiration_strategy(&ctx),
        gas_owner_strategy,
    )
        .prop_map(move |(pt, expiration, gas_owner)| {
            TransactionData::V1(TransactionDataV1 {
                kind: TransactionKind::ProgrammableTransaction(pt),
                sender,
                gas_data: GasData {
                    payment: vec![],
                    owner: gas_owner,
                    price: 0,
                    budget: 0,
                },
                expiration,
            })
        })
        .boxed()
}
