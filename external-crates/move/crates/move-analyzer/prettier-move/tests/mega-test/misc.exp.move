// options:
// printWidth: 80

/*
 * @title Timelock
 *
 * @notice Locks any object with the store ability for a specific amount of time.
 *
 * @dev We do not provide a function to read the data inside the {Timelock<T>} to prevent capabilities from being used.
 */
module suitears::timelock {
    fun calculate_pending_rewards<StakeCoin, RewardCoin>(
        acc: &Account<StakeCoin, RewardCoin>,
        stake_factor: u64,
        accrued_rewards_per_share: u256,
    ): u64 {
        (
            (
                (
                    (acc.amount as u256) * accrued_rewards_per_share /
                    (stake_factor as u256),
                ) -
                acc.reward_debt,
            ) as u64,
        )
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

    // === Public View Function ===

    /*
     * @notice Returns the unlock time in milliseconds.
     *
     * @param self A {Timelock<T>}
     * @return u64. The `self.unlock_time`.
     */
    public fun unlock_time<T: store>(self: &Timelock<T>): u64 {
        self.unlock_time
    }

    // === Public Mutative Function ===

    /*
     * @notice Locks the `data` for `unlock_time` milliseconds.
     *
     * @param data An object with the store ability.
     * @param c The shared `sui::clock::Clock` object.
     * @patam unlock_time The lock period in milliseconds.
     * @return {Timelock<T>}.
     *
     * aborts-if
     * - `unlock_time` is in the past.
     */
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

    /*
     * @notice Unlocks a {Timelock<T>} and returns the locked resource `T`.
     *
     * @param self A {Timelock<T>}
     * @param c The shared `sui::clock::Clock` object.
     * @return `T`. An object with the store ability.
     *
     * aborts-if
     * - `unlock_time` has not passed.
     */
    public fun unlock<T: store>(self: Timelock<T>, c: &Clock): T {
        let Timelock { id, data, unlock_time } = self;

        assert!(c.timestamp_ms() >= unlock_time, ETooEarly);
        id.delete();
        data
    }
}
