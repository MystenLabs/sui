// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use once_cell::sync::Lazy;
use proptest::collection::vec;
use proptest::prelude::*;

use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::*;
use sui_types::type_input::TypeInput;
use sui_types::{
    SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION, SUI_FRAMEWORK_PACKAGE_ID,
    SUI_RANDOMNESS_STATE_OBJECT_ID,
};

use super::coin_reservation::{
    coin_reservation_ref, coin_reservation_strategy, coin_reservation_strategy_for,
    valid_coin_reservation_strategy,
};
use super::common::{
    TxFuzzContext, boundary_u64, expiration_strategy, simple_transfer_pt, type_input_strategy,
};

/// Maximum number of valid building blocks composed into a single PT.
const MAX_BLOCKS_PER_PT: usize = 4;

/// Protocol-defined maximum gas price. We probe just above and well above this
/// to exercise the rejection path.
static MAX_GAS_PRICE: Lazy<u64> =
    Lazy::new(|| ProtocolConfig::get_for_max_version_UNSAFE().max_gas_price());
/// Protocol-defined maximum transaction gas budget. Used as a ceiling for "good" budgets.
static MAX_TX_GAS: Lazy<u64> =
    Lazy::new(|| ProtocolConfig::get_for_max_version_UNSAFE().max_tx_gas());

/// Multiplier used to derive a "good" gas budget from a gas price. Picked so that
/// price * budget multiplier ≈ a realistic transaction cost without exceeding `MAX_TX_GAS`.
const GAS_BUDGET_MULTIPLIER: u64 = 50_000;

fn good_budget_for_price(price: u64) -> u64 {
    price.saturating_mul(GAS_BUDGET_MULTIPLIER).min(*MAX_TX_GAS)
}

fn budget_strategy(price: u64) -> BoxedStrategy<u64> {
    let good = good_budget_for_price(price);
    prop_oneof![
        12 => Just(good),
        4 => boundary_u64(),
    ]
    .boxed()
}

fn valid_price_strategy(rgp: u64) -> impl Strategy<Value = u64> {
    rgp..rgp.saturating_add(10_000).max(rgp.saturating_add(1))
}

// --- Gas data strategy arms ---

fn empty_payment_arm(ctx: &TxFuzzContext) -> BoxedStrategy<GasData> {
    let sender = ctx.sender;
    let rgp = ctx.reference_gas_price;
    valid_price_strategy(rgp)
        .prop_flat_map(move |price| {
            budget_strategy(price).prop_map(move |budget| GasData {
                payment: vec![],
                owner: sender,
                price,
                budget,
            })
        })
        .boxed()
}

/// Sponsored gas: `gas_data.owner` is the sponsor (different from sender). The
/// protocol takes the implicit gas withdrawal from the sponsor's address balance.
/// The test driver detects `sender != gas_owner` and dual-signs the transaction.
fn sponsored_payment_arm(ctx: &TxFuzzContext, sponsor: SuiAddress) -> BoxedStrategy<GasData> {
    let rgp = ctx.reference_gas_price;
    valid_price_strategy(rgp)
        .prop_flat_map(move |price| {
            budget_strategy(price).prop_map(move |budget| GasData {
                payment: vec![],
                owner: sponsor,
                price,
                budget,
            })
        })
        .boxed()
}

/// Coin reservation as gas payment, with reservation amount sized to cover the budget
/// (so the transaction can actually pass gas validation).
fn coin_reservation_gas_arm(ctx: &TxFuzzContext) -> BoxedStrategy<GasData> {
    let sender = ctx.sender;
    let rgp = ctx.reference_gas_price;
    let epoch = ctx.epoch;
    let chain = ctx.chain;
    valid_price_strategy(rgp)
        .prop_flat_map(move |price| {
            budget_strategy(price).prop_map(move |budget| {
                let res = coin_reservation_ref(sender, &GAS::type_tag(), epoch, budget, chain);
                GasData {
                    payment: vec![res],
                    owner: sender,
                    price,
                    budget,
                }
            })
        })
        .boxed()
}

/// Coin reservation as gas payment with boundary-biased values — exercises the
/// validation rejection paths (zero amount, wrong epoch, wrong sender, wrong chain).
fn coin_reservation_boundary_gas_arm(ctx: &TxFuzzContext) -> BoxedStrategy<GasData> {
    let sender = ctx.sender;
    let rgp = ctx.reference_gas_price;
    let coin_res =
        coin_reservation_strategy_for(ctx.sender, Arc::new(GAS::type_tag()), ctx.epoch, ctx.chain);
    coin_res
        .prop_flat_map(move |res| {
            valid_price_strategy(rgp).prop_map(move |price| {
                let budget = good_budget_for_price(price);
                GasData {
                    payment: vec![res],
                    owner: sender,
                    price,
                    budget,
                }
            })
        })
        .boxed()
}

fn below_rgp_arm(ctx: &TxFuzzContext) -> BoxedStrategy<GasData> {
    let sender = ctx.sender;
    let rgp = ctx.reference_gas_price;
    Just(GasData {
        payment: vec![],
        owner: sender,
        price: rgp.saturating_sub(1),
        budget: good_budget_for_price(rgp),
    })
    .boxed()
}

fn overflow_price_arm(sender: SuiAddress) -> BoxedStrategy<GasData> {
    Just(GasData {
        payment: vec![],
        owner: sender,
        price: u64::MAX,
        budget: 1,
    })
    .boxed()
}

fn above_max_price_arm(sender: SuiAddress) -> BoxedStrategy<GasData> {
    let above_max = MAX_GAS_PRICE.saturating_add(1);
    Just(GasData {
        payment: vec![],
        owner: sender,
        price: above_max,
        budget: above_max.saturating_mul(GAS_BUDGET_MULTIPLIER),
    })
    .boxed()
}

fn boundary_price_budget_arm(sender: SuiAddress) -> BoxedStrategy<GasData> {
    (boundary_u64(), boundary_u64())
        .prop_map(move |(price, budget)| GasData {
            payment: vec![],
            owner: sender,
            price,
            budget,
        })
        .boxed()
}

fn addr_balance_gas_data_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<GasData> {
    let sender = ctx.sender;
    let mut arms = vec![
        (12u32, empty_payment_arm(ctx)),
        (4, coin_reservation_gas_arm(ctx)),
        (2, coin_reservation_boundary_gas_arm(ctx)),
        (1, below_rgp_arm(ctx)),
        (1, overflow_price_arm(sender)),
        (1, above_max_price_arm(sender)),
        (1, boundary_price_budget_arm(sender)),
    ];
    if let Some(sponsor) = ctx.sponsor {
        arms.push((4, sponsored_payment_arm(ctx, sponsor)));
    }
    proptest::strategy::Union::new_weighted(arms).boxed()
}

// =============================================================================
// VALID PT BUILDING BLOCKS
//
// Each block is a `BlockSpec` describing one or more commands to append to an
// existing PT builder. Blocks are self-contained: they push their own inputs
// (even if duplicated across blocks) and never reference outputs of previous
// blocks. Composing N blocks always yields a valid PT by construction.
//
// These blocks are *expected to reach execution*. Failures from the
// composed-PT path indicate real bugs.
// =============================================================================

#[derive(Debug, Clone)]
enum BlockSpec {
    /// `transfer_sui(recipient, None)` — transfers the gas coin to recipient.
    TransferGas { recipient: SuiAddress },
    /// `MakeMoveVec<address>([addrs...])`.
    MakeVecAddresses { addrs: Vec<SuiAddress> },
    /// References the clock shared object (immutable) and calls `0x2::clock::timestamp_ms`.
    ReadClock,
    /// References the randomness state shared object (immutable) and calls
    /// `0x2::random::new_generator`. Only emitted when randomness is available.
    UseRandomness {
        initial_shared_version: SequenceNumber,
    },
    /// `0x2::balance::zero<T>()` then `0x2::balance::send_funds<T>(b, recipient)`.
    BalanceZeroSend { recipient: SuiAddress },
    /// `FundsWithdrawal` input → `balance::redeem_funds<T>` → `balance::send_funds<T>`.
    WithdrawSend { amount: u64, recipient: SuiAddress },
    /// Coin-reservation `Object(ImmOrOwnedObject(<reservation>))` input →
    /// `coin::send_funds<T>`. The protocol's `convert_withdrawal_to_coin`
    /// layer rewrites the input to a `Coin<T>` at execution time, so calling
    /// `balance::redeem_funds` here would TypeMismatch.
    CoinReservationRedeemSend {
        res: ObjectRef,
        recipient: SuiAddress,
    },
}

fn ident(s: &'static str) -> Identifier {
    Identifier::new(s).unwrap()
}

/// Append a Move call to the sui-framework (`0x2`).
fn fw_call(
    b: &mut ProgrammableTransactionBuilder,
    module: &'static str,
    function: &'static str,
    type_args: Vec<TypeTag>,
    args: Vec<Argument>,
) -> Argument {
    b.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident(module),
        ident(function),
        type_args,
        args,
    )
}

impl BlockSpec {
    /// A "terminal" block must be the last command in the PT:
    /// - `TransferGas` consumes the gas coin so subsequent gas-using blocks fail.
    /// - `UseRandomness` triggers the protocol's post-randomness restriction
    ///   (only `TransferObjects`/`MergeCoins` are allowed after a Random use).
    fn is_terminal(&self) -> bool {
        matches!(
            self,
            BlockSpec::TransferGas { .. } | BlockSpec::UseRandomness { .. }
        )
    }

    fn apply(self, b: &mut ProgrammableTransactionBuilder, fund_type: &TypeTag) {
        match self {
            BlockSpec::TransferGas { recipient } => {
                b.transfer_sui(recipient, None);
            }
            BlockSpec::MakeVecAddresses { addrs } => {
                let elems: Vec<Argument> = addrs.iter().map(|a| b.pure(*a).unwrap()).collect();
                b.command(Command::MakeMoveVec(Some(TypeInput::Address), elems));
            }
            BlockSpec::ReadClock => {
                let clock = b
                    .obj(ObjectArg::SharedObject {
                        id: SUI_CLOCK_OBJECT_ID,
                        initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
                        mutability: SharedObjectMutability::Immutable,
                    })
                    .unwrap();
                fw_call(b, "clock", "timestamp_ms", vec![], vec![clock]);
            }
            BlockSpec::UseRandomness {
                initial_shared_version,
            } => {
                // `0x2::random::new_generator(r: &Random, ctx: &mut TxContext)`
                // takes an immutable reference to Random.
                let r = b
                    .obj(ObjectArg::SharedObject {
                        id: SUI_RANDOMNESS_STATE_OBJECT_ID,
                        initial_shared_version,
                        mutability: SharedObjectMutability::Immutable,
                    })
                    .unwrap();
                fw_call(b, "random", "new_generator", vec![], vec![r]);
            }
            BlockSpec::BalanceZeroSend { recipient } => {
                let ft = fund_type.clone();
                let z = fw_call(b, "balance", "zero", vec![ft.clone()], vec![]);
                let r = b.pure(recipient).unwrap();
                fw_call(b, "balance", "send_funds", vec![ft], vec![z, r]);
            }
            BlockSpec::WithdrawSend { amount, recipient } => {
                let ft = fund_type.clone();
                let w = b
                    .input(CallArg::FundsWithdrawal(
                        FundsWithdrawalArg::balance_from_sender(amount, ft.clone()),
                    ))
                    .unwrap();
                let bal = fw_call(b, "balance", "redeem_funds", vec![ft.clone()], vec![w]);
                let r = b.pure(recipient).unwrap();
                fw_call(b, "balance", "send_funds", vec![ft], vec![bal, r]);
            }
            BlockSpec::CoinReservationRedeemSend { res, recipient } => {
                let ft = fund_type.clone();
                let coin = b
                    .input(CallArg::Object(ObjectArg::ImmOrOwnedObject(res)))
                    .unwrap();
                let r = b.pure(recipient).unwrap();
                fw_call(b, "coin", "send_funds", vec![ft], vec![coin, r]);
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

fn any_block_spec_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<BlockSpec> {
    let coin_res = valid_coin_reservation_strategy(ctx);
    let mut arms: Vec<(u32, BoxedStrategy<BlockSpec>)> = vec![
        (
            3,
            any::<SuiAddress>()
                .prop_map(|recipient| BlockSpec::TransferGas { recipient })
                .boxed(),
        ),
        (
            2,
            vec(any::<SuiAddress>(), 0..4)
                .prop_map(|addrs| BlockSpec::MakeVecAddresses { addrs })
                .boxed(),
        ),
        (2, Just(BlockSpec::ReadClock).boxed()),
        (
            3,
            any::<SuiAddress>()
                .prop_map(|recipient| BlockSpec::BalanceZeroSend { recipient })
                .boxed(),
        ),
        (
            4,
            // Cap amount well under the 1_000 funded per sender so the
            // happy path can actually succeed; insufficient-balance is
            // probed via the garbage strategy's boundary inputs.
            (1u64..=100u64, any::<SuiAddress>())
                .prop_map(|(amount, recipient)| BlockSpec::WithdrawSend { amount, recipient })
                .boxed(),
        ),
        (
            3,
            (coin_res, any::<SuiAddress>())
                .prop_map(|(res, recipient)| BlockSpec::CoinReservationRedeemSend {
                    res,
                    recipient,
                })
                .boxed(),
        ),
    ];
    if let Some(isv) = ctx.randomness_initial_shared_version {
        arms.push((
            2,
            Just(BlockSpec::UseRandomness {
                initial_shared_version: isv,
            })
            .boxed(),
        ));
    }
    proptest::strategy::Union::new_weighted(arms).boxed()
}

fn valid_composed_pt_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<ProgrammableTransaction> {
    let fund_type = ctx.fund_type.clone();
    vec(any_block_spec_strategy(ctx), 1..=MAX_BLOCKS_PER_PT)
        .prop_map(move |raw_blocks| {
            // Coin reservations against the same `(sender, fund_type)` accumulator
            // share an ObjectID but have distinct per-call digests, so the PT
            // builder rejects the second one. Keep only the first.
            let mut seen_coin_res = false;
            // At most one terminal block, and it always goes at the end of the PT.
            let mut first_terminal: Option<BlockSpec> = None;
            let mut blocks: Vec<BlockSpec> = Vec::with_capacity(raw_blocks.len());
            for spec in raw_blocks {
                if matches!(spec, BlockSpec::CoinReservationRedeemSend { .. }) {
                    if seen_coin_res {
                        continue;
                    }
                    seen_coin_res = true;
                }
                if spec.is_terminal() {
                    if first_terminal.is_none() {
                        first_terminal = Some(spec);
                    }
                    continue;
                }
                blocks.push(spec);
            }
            if let Some(t) = first_terminal {
                blocks.push(t);
            }
            // Anchor to a guaranteed-valid PT in case the filter pass left
            // the block list empty.
            if blocks.is_empty() {
                blocks.push(BlockSpec::TransferGas {
                    recipient: SuiAddress::ZERO,
                });
            }
            let mut b = ProgrammableTransactionBuilder::new();
            for spec in blocks {
                spec.apply(&mut b, &fund_type);
            }
            b.finish()
        })
        .boxed()
}

// =============================================================================
// GARBAGE PT STRATEGY
//
// Produces PTs that are *intentionally* invalid: random package addresses,
// random module/function names, random `Argument` indices that almost never
// land in-bounds. The point is to drive the PT validator and command decoders
// through every rejection branch with boundary-shaped data. Effectively 0% of
// these reach execution.
// =============================================================================

fn garbage_move_call_command_strategy() -> impl Strategy<Value = Command> {
    (
        any::<ObjectID>(),
        "[a-z]{1,10}",
        "[a-z]{1,10}",
        vec(type_input_strategy(), 0..3),
        vec(any::<Argument>(), 0..6),
    )
        .prop_map(|(package, module, function, type_arguments, arguments)| {
            Command::MoveCall(Box::new(ProgrammableMoveCall {
                package,
                module,
                function,
                type_arguments,
                arguments,
            }))
        })
}

fn garbage_command_strategy() -> impl Strategy<Value = Command> {
    prop_oneof![
        garbage_move_call_command_strategy(),
        (vec(any::<Argument>(), 1..4), any::<Argument>())
            .prop_map(|(objs, addr)| Command::TransferObjects(objs, addr)),
        (any::<Argument>(), vec(any::<Argument>(), 1..4))
            .prop_map(|(coin, amounts)| Command::SplitCoins(coin, amounts)),
        (any::<Argument>(), vec(any::<Argument>(), 1..4))
            .prop_map(|(target, sources)| Command::MergeCoins(target, sources)),
        (
            proptest::option::of(type_input_strategy()),
            vec(any::<Argument>(), 0..4),
        )
            .prop_map(|(ty, elems)| Command::MakeMoveVec(ty, elems)),
    ]
}

fn garbage_input_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<CallArg> {
    let fund_type = ctx.fund_type.clone();
    let coin_res = coin_reservation_strategy(ctx);
    let ft_sender = fund_type.clone();
    let ft_zero = fund_type.clone();
    let ft_sponsor = fund_type;
    let funds_withdrawal_input = prop_oneof![
        3 => boundary_u64().prop_map(move |amount| {
            CallArg::FundsWithdrawal(FundsWithdrawalArg::balance_from_sender(
                amount.max(1),
                (*ft_sender).clone(),
            ))
        }),
        1 => Just(()).prop_map(move |_| {
            CallArg::FundsWithdrawal(FundsWithdrawalArg::balance_from_sender(
                0,
                (*ft_zero).clone(),
            ))
        }),
        // Sponsor variant — rejected at signing, exercises that path.
        1 => boundary_u64().prop_map(move |amount| {
            CallArg::FundsWithdrawal(FundsWithdrawalArg::balance_from_sponsor(
                amount.max(1),
                (*ft_sponsor).clone(),
            ))
        }),
    ];
    prop_oneof![
        10 => vec(any::<u8>(), 0..64).prop_map(CallArg::Pure),
        5 => any::<ObjectArg>().prop_map(CallArg::Object),
        3 => coin_res.prop_map(|r| CallArg::Object(ObjectArg::ImmOrOwnedObject(r))),
        3 => funds_withdrawal_input,
    ]
    .boxed()
}

pub(super) fn garbage_pt_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<ProgrammableTransaction> {
    (
        vec(garbage_input_strategy(ctx), 0..8),
        vec(garbage_command_strategy(), 1..8),
    )
        .prop_map(|(inputs, commands)| ProgrammableTransaction { inputs, commands })
        .boxed()
}

// =============================================================================
// TOP LEVEL
// =============================================================================

fn addr_balance_pt_strategy(ctx: &TxFuzzContext) -> BoxedStrategy<ProgrammableTransaction> {
    let sender = ctx.sender;
    prop_oneof![
        // Baseline anchor — guarantees a fraction of iterations reach the
        // address-balance accounting code on the simplest possible PT.
        2 => Just(simple_transfer_pt(sender)),
        // Multi-command valid PTs — main vehicle for execution-path coverage.
        6 => valid_composed_pt_strategy(ctx),
        // Validator fuzz — drives the PT validator only, never reaches execution.
        2 => garbage_pt_strategy(ctx),
    ]
    .boxed()
}

pub fn addr_balance_transaction_data_strategy(
    ctx: TxFuzzContext,
) -> BoxedStrategy<TransactionData> {
    let sender = ctx.sender;
    (
        addr_balance_gas_data_strategy(&ctx),
        expiration_strategy(&ctx),
        addr_balance_pt_strategy(&ctx),
    )
        .prop_map(move |(gas_data, expiration, pt)| {
            TransactionData::V1(TransactionDataV1 {
                kind: TransactionKind::ProgrammableTransaction(pt),
                sender,
                gas_data,
                expiration,
            })
        })
        .boxed()
}
