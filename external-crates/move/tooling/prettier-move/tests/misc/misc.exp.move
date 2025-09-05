// options:
// printWidth: 80
// autoGroupImports: package
// useModuleLabel: true

module prettier::misc;

use std::{string::String, type_name::{Self, TypeName}};
use sui::{
    clock::Clock,
    coin::Coin,
    dynamic_field as df,
    sui::SUI,
    table::{Self, Table}
};

fun calculate_pending_rewards<StakeCoin, RewardCoin>(
    acc: &Account<StakeCoin, RewardCoin>,
    an_acc: &mut Account<StakeCoin, RewardCoin>,
    stake_factor: u64,
    accrued_rewards_per_share: u256,
): u64 {
    (
        (
            ((acc.amount as u256) * accrued_rewards_per_share / (stake_factor as u256)) - acc.reward_debt,
        ) as u64,
    )
}

// sui-system/validator_set.move
fun compute_reward_adjustments(
    mut slashed_validator_indices: vector<u64>,
    reward_slashing_rate: u64,
    unadjusted_staking_reward_amounts: &vector<u64>,
    unadjusted_storage_fund_reward_amounts: &vector<u64>,
): (
    u64, // sum of staking reward adjustments
    VecMap<u64, u64>, // mapping of individual validator's staking reward adjustment from index -> amount
    u64, // sum of storage fund reward adjustments
    VecMap<u64, u64>, // mapping of individual validator's storage fund reward adjustment from index -> amount
) {
    let unadjusted_storage_fund_reward_amount = unadjusted_storage_fund_reward_amounts[
        i,
    ];
    let adjusted_storage_fund_reward_amount = // If the validator is one of the slashed ones, then subtract the adjustment.
    if (individual_storage_fund_reward_adjustments.contains(&i)) {
        let adjustment = individual_storage_fund_reward_adjustments[&i];
        unadjusted_storage_fund_reward_amount - adjustment
    } else {
        // Otherwise the slashed rewards should be equally distributed among the unslashed validators.
        let adjustment =
            total_storage_fund_reward_adjustment / num_unslashed_validators;
        unadjusted_storage_fund_reward_amount + adjustment
    };

    adjusted_storage_fund_reward_amounts.push_back(
        adjusted_storage_fund_reward_amount,
    );
}

// === Imports ===

public fun lock<T: store>(
    data: T,
    c: &Clock,
    unlock_time: u64,
    ctx: &mut TxContext,
): Timelock<T> {
    // It makes no sense to lock in the past
    assert!(unlock_time > c.timestamp_ms(), EInvalidTime);
}

public fun propose<DaoWitness: drop>(
    dao: &mut Dao<DaoWitness>,
    c: &Clock,
    authorized_witness: Option<TypeName>,
    capability_id: Option<ID>,
    action_delay: u64,
    quorum_votes: u64,
    hash: String,
    // hash proposal title/content
    ctx: &mut TxContext,
): Proposal<DaoWitness> {
    assert!(action_delay >= dao.min_action_delay, EActionDelayTooShort);
    assert!(quorum_votes >= dao.min_quorum_votes, EMinQuorumVotesTooSmall);
    assert!(hash.length() != 0, EEmptyHash);

    let start_time = c.timestamp_ms() + dao.voting_delay;

    let proposal = Proposal {
        id: object::new(ctx),
        proposer: ctx.sender(),
        start_time,
        end_time: start_time + dao.voting_period,
        for_votes: 0,
        against_votes: 0,
        eta: 0,
        action_delay,
        quorum_votes,
        voting_quorum_rate: dao.voting_quorum_rate,
        hash,
        authorized_witness,
        capability_id,
        coin_type: dao.coin_type,
    };

    emit(NewProposal<DaoWitness> {
        proposal_id: object::id(&proposal),
        proposer: proposal.proposer,
    });

    proposal
}

public fun inline_fun(): u128 { 1000 }

public fun unlock_time<T: store>(self: &Timelock<T>): u64 {
    self.unlock_time
}

public fun lock<T: store>(
    data: T,
    c: &Clock,
    unlock_time: u64,
    ctx: &mut TxContext,
): Timelock<T> {
    // It makes no sense to lock in the past
    assert!(unlock_time > c.timestamp_ms(), EInvalidTime);

    Timelock { id: object::new(ctx), data, unlock_time }
}

public fun unlock<T: store>(self: Timelock<T>, c: &Clock): T {
    let Timelock { id, data, unlock_time } = self;

    assert!(c.timestamp_ms() >= unlock_time, ETooEarly);
    id.delete();
    data
}

/// Print the container as an `SVG` element.
public fun to_string(container: &Container): String {
    let (name, attributes, elements) = match (container) {
        // Desc is a special case, it's just a list of descriptions.
        Container::Desc(tags) => {
            return (*tags).fold!(b"".to_string(), |mut svg, tag| {
                svg.append(tag.to_string());
                svg
            })
        },
        // Root is a special case, we append all elements directly.
        Container::Root(shapes) => {
            return (*shapes).fold!(b"".to_string(), |mut svg, shape| {
                svg.append(shape.to_string());
                svg
            })
        },
        Container::Defs(shapes) => (
            b"defs",
            vec_map::empty(),
            shapes.map_ref!(|shape| shape.to_string()),
        ),
        Container::A(_href, attrs) => (b"a", *attrs, vector[]),
        Container::G(shapes, attrs) => (
            b"g",
            *attrs,
            shapes.map_ref!(|shape| shape.to_string()),
        ),
        _ => abort ENotImplemented,
    };

    print::print(name.to_string(), attributes, option::some(elements))
}

fun content() {
    expression
        // disappearing_comment_1
        .div(50) // trailing_comment_1
        // disappearing_comment_2
        .mul(50); // trailing_comment_2

    svg.add_root(vector[
        {
            let mut shape = shape::text(str.to_string(), 100, 50);
            shape
        },
        shape::circle(10, 10, 5),
        {
            let mut rect = shape::rect(10, 10, 20, 20);
            rect
        },
        shape::ellipse(30, 30, 10, 5),
    ]);
}

public fun withdraw<T>(
    vault: &mut Vault<T>,
    key: key::Key,
    ctx: &mut TxContext,
): coin::Coin<T> {
    assert_valid_key_code(vault, &key);
    key.delete();

    let new_coin = coin::from_balance(
        balance::split(
            &mut vault.balance,
            vault.withdrawal_amount,
        ),
        ctx,
    );
    new_coin
}

fun staking() {
    let unadjusted_staking_reward_amount = unadjusted_staking_reward_amounts[i];
    let adjusted_staking_reward_amount // If the validator is one of the slashed ones, then subtract the adjustment.
     = if (individual_staking_reward_adjustments.contains(&i)) {
        let adjustment = individual_staking_reward_adjustments[&i];
        unadjusted_staking_reward_amount - adjustment
    } else {
        // Otherwise the slashed rewards should be distributed among the unslashed
        // validators so add the corresponding adjustment.
        let adjustment =
            total_staking_reward_adjustment as u128 * voting_power / (total_unslashed_validator_voting_power as u128);
        unadjusted_staking_reward_amount + (adjustment as u64)
    };
    adjusted_staking_reward_amounts.push_back(adjusted_staking_reward_amount);
}
