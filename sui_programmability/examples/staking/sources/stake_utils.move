module staking::stake_utils {
    use sui::sui_system::{Self, SuiSystemState};
    use sui::validator;
    use sui::validator_set;
    use std::vector;

    /// The given validator is not part of the validator set
    const EValidatorNotFound: u64 = 0;

    /// The given rank is larger than the validator set
    const ERankOutOfBounds: u64 = 1;

    /// Total stake info and stake ranking, if the epoch ended right now
    struct ValidatorInfo has store, drop, copy {
        /// Address of the validator
        addr: address,
        /// Current total stake of the validator
        stake: u64,
        /// 0-indexed rank of the validator
        rank: u64,
    }

    /// Return the current stake info and stake rank for `validator`.
    /// This represents what the validator's stake and rank would be if the current epoch ended right now
    public fun next_epoch_info(validator: address, state: &SuiSystemState): (address, u64, u64) {
        let stakes = next_epoch_stakes(state);
        let info = find_validator_info(validator, &stakes);
        (info.addr, info.stake, info.rank)
    }

    /// Return the current stake info and stake rank for the validator with the given `rank`
    /// This represents what the validator's stake and rank would be if the current epoch ended right now
    public fun next_epoch_info_for_rank(rank: u64, state: &SuiSystemState): (address, u64, u64) {
        let stakes = next_epoch_stakes(state);
        assert!(rank < vector::length(&stakes), ERankOutOfBounds);
        let info = vector::borrow(&stakes, rank);
        (info.addr, info.stake, info.rank)
    }

    /// Return live stake info based on pending delegations and withdrawals
    /// Intended for client usage, helpful if you want to see how your assigned
    /// validator is doing in the middle of an epoch
    /// Note: assumes static validator set
    public fun next_epoch_stakes(state: &SuiSystemState): vector<ValidatorInfo> {
        let validators = validator_set::active_validators(sui_system::validators(state));
        let next_epoch_stakes = vector[];
        let i = 0;
        let num_validators = vector::length(validators);
        while (i < num_validators) {
            let validator = vector::borrow(validators, i);
            let addr = validator::sui_address(validator);
            let stake = validator::total_stake(validator) + validator::pending_stake_amount(validator) - validator::pending_withdraw(validator);
            validator_insertion_sort(&mut next_epoch_stakes, ValidatorInfo { addr, stake, rank: 0 });
            i = i + 1
        };

        // now that we have a sorted list, go back and add the rank
        i = 0;
        while (i < num_validators) {
            let validator = vector::borrow_mut(&mut next_epoch_stakes, i);
            validator.rank = i;
            i = i + 1
        };
        next_epoch_stakes
    }

     /// insert `validator` into `v`, maintaining the invariant that `v` is in descending order by stake
    fun validator_insertion_sort(v: &mut vector<ValidatorInfo>, validator: ValidatorInfo) {
        let stake = validator.stake;
        let i = 0;
        let len = vector::length(v);
        while (i < len) {
            if (stake > vector::borrow(v, i).stake) {
                vector::insert(v, validator, i);
                return
            };
            i = i + 1
        };
        vector::push_back(v, validator)
    }

    /// Return the index of the validator with address `addr` in `v`
    fun find_validator_info(validator: address, v: &vector<ValidatorInfo>): ValidatorInfo {
        let i = 0;
        let len = vector::length(v);
        while (i < len) {
            let e = vector::borrow(v, i);
            if (e.addr == validator) {
                return *e
            };
            i = i + 1
        };
        abort(EValidatorNotFound)
    }
}
