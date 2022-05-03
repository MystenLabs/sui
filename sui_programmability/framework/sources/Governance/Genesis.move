// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::Genesis {
    use Std::Vector;

    use Sui::Coin;
    use Sui::SUI;
    use Sui::SuiSystem;
    use Sui::TxContext::TxContext;
    use Sui::Validator;

    /// The initial amount of SUI locked in the storage fund.
    /// 10^14, an arbitrary number.
    const INIT_STORAGE_FUND: u64 = 100000000000000;

    /// Initial value of the lower-bound on the amount of stake required to become a validator.
    const INIT_MIN_VALIDATOR_STAKE: u64 = 100000000000000;

    /// Initial value of the upper-bound on the amount of stake allowed to become a validator.
    const INIT_MAX_VALIDATOR_STAKE: u64 = 100000000000000000;

    /// Initial value of the upper-bound on the number of validators.
    const INIT_MAX_VALIDATOR_COUNT: u64 = 100;

    /// Basic information of Validator1, as an example, all dummy values.
    const VALIDATOR1_SUI_ADDRESS: address = @0x1234;
    const VALIDATOR1_NAME: vector<u8> = b"Validator1";
    const VALIDATOR1_IP_ADDRESS: vector<u8> = x"00FF00FF";
    const VALIDATOR1_STAKE: u64 = 100000000000000;

    /// This is a module initializer that runs during module publishing.
    /// It will create a singleton SuiSystemState object, which contains
    /// all the information we need in the system.
    fun init(ctx: &mut TxContext) {
        let treasury_cap = SUI::new(ctx);
        let storage_fund = Coin::mint_balance(INIT_STORAGE_FUND, &mut treasury_cap);
        let validators = Vector::empty();
        Vector::push_back(&mut validators, Validator::new(
            VALIDATOR1_SUI_ADDRESS,
            VALIDATOR1_NAME,
            VALIDATOR1_IP_ADDRESS,
            Coin::mint_balance(VALIDATOR1_STAKE, &mut treasury_cap),
        ));
        SuiSystem::create(
            validators,
            treasury_cap,
            storage_fund,
            INIT_MAX_VALIDATOR_COUNT,
            INIT_MIN_VALIDATOR_STAKE,
            INIT_MAX_VALIDATOR_STAKE,
            ctx,
        );
    }
}
