module deepbook::constants {
    const CURRENT_VERSION: u64 = 1;
    const POOL_CREATION_FEE: u64 = 10000 * 1_000_000; // 10000 DEEP
    const FLOAT_SCALING: u64 = 1_000_000_000;
    const MAX_U64: u64 = (1u128 << 64 - 1) as u64;
    const MIN_PRICE: u64 = 1;
    const MAX_PRICE: u64 = (1u128 << 63 - 1) as u64;
    const DEFAULT_STAKE_REQUIRED: u64 = 100_000_000; // 100 DEEP
    const HALF: u64 = 500_000_000;
    const DEEP_UNIT: u64 = 1_000_000;

    // Restrictions on limit orders.
    // No restriction on the order.
    const NO_RESTRICTION: u8 = 0;
    // Mandates that whatever amount of an order that can be executed in the current transaction, be filled and then the rest of the order canceled.
    const IMMEDIATE_OR_CANCEL: u8 = 1;
    // Mandates that the entire order size be filled in the current transaction. Otherwise, the order is canceled.
    const FILL_OR_KILL: u8 = 2;
    // Mandates that the entire order be passive. Otherwise, cancel the order.
    const POST_ONLY: u8 = 3;
    // Maximum restriction value.
    const MAX_RESTRICTION: u8 = 3;

    // Self matching types.
    // Self matching is allowed.
    const SELF_MATCHING_ALLOWED: u8 = 0;
    // Cancel the taker order.
    const CANCEL_TAKER: u8 = 1;
    // Cancel the maker order.
    const CANCEL_MAKER: u8 = 2;

    // Order statuses.
    const LIVE: u8 = 0;
    const PARTIALLY_FILLED: u8 = 1;
    const FILLED: u8 = 2;
    const CANCELED: u8 = 3;
    const EXPIRED: u8 = 4;

    // History constants
    const PHASE_OUT_EPOCHS: u64 = 28;

    // Fee type constants
    const FEE_IS_DEEP: bool = true;

    // Constants for testing
    #[test_only]
    const MAKER_FEE: u64 = 500000;
    #[test_only]
    const TAKER_FEE: u64 = 1000000;
    #[test_only]
    const STABLE_MAKER_FEE: u64 = 50000;
    #[test_only]
    const STABLE_TAKER_FEE: u64 = 100000;
    #[test_only]
    const TICK_SIZE: u64 = 1000;
    #[test_only]
    const LOT_SIZE: u64 = 1000;
    #[test_only]
    const MIN_SIZE: u64 = 10000;
    #[test_only]
    const DEEP_MULTIPLIER: u64 = 100 * FLOAT_SCALING;
    #[test_only]
    const TAKER_DISCOUNT: u64 = 500_000_000;
    #[test_only]
    const USDC_UNIT: u64 = 1_000_000;
    #[test_only]
    const SUI_UNIT: u64 = 1_000_000_000;

    // Testing error codes
    #[test_only]
    const EOrderInfoMismatch: u64 = 0;
    #[test_only]
    const EBookOrderMismatch: u64 = 1;
    #[test_only]
    const EIncorrectMidPrice: u64 = 2;
    #[test_only]
    const EIncorrectPoolId: u64 = 3;

    public fun current_version(): u64 {
        CURRENT_VERSION
    }

    public fun pool_creation_fee(): u64 {
        POOL_CREATION_FEE
    }

    public fun float_scaling(): u64 {
        FLOAT_SCALING
    }

    public fun max_u64(): u64 {
        MAX_U64
    }

    public fun no_restriction(): u8 {
        NO_RESTRICTION
    }

    public fun immediate_or_cancel(): u8 {
        IMMEDIATE_OR_CANCEL
    }

    public fun fill_or_kill(): u8 {
        FILL_OR_KILL
    }

    public fun post_only(): u8 {
        POST_ONLY
    }

    public fun max_restriction(): u8 {
        MAX_RESTRICTION
    }

    public fun live(): u8 {
        LIVE
    }

    public fun partially_filled(): u8 {
        PARTIALLY_FILLED
    }

    public fun filled(): u8 {
        FILLED
    }

    public fun canceled(): u8 {
        CANCELED
    }

    public fun expired(): u8 {
        EXPIRED
    }

    public fun self_matching_allowed(): u8 {
        SELF_MATCHING_ALLOWED
    }

    public fun cancel_taker(): u8 {
        CANCEL_TAKER
    }

    public fun cancel_maker(): u8 {
        CANCEL_MAKER
    }

    public fun min_price(): u64 {
        MIN_PRICE
    }

    public fun max_price(): u64 {
        MAX_PRICE
    }

    public fun phase_out_epochs(): u64 {
        PHASE_OUT_EPOCHS
    }

    public fun default_stake_required(): u64 {
        DEFAULT_STAKE_REQUIRED
    }

    public fun half(): u64 {
        HALF
    }

    public fun fee_is_deep(): bool {
        FEE_IS_DEEP
    }

    public fun deep_unit(): u64 {
        DEEP_UNIT
    }

    #[test_only]
    public fun maker_fee(): u64 {
        MAKER_FEE
    }

    #[test_only]
    public fun taker_fee(): u64 {
        TAKER_FEE
    }

    #[test_only]
    public fun stable_maker_fee(): u64 {
        STABLE_MAKER_FEE
    }

    #[test_only]
    public fun stable_taker_fee(): u64 {
        STABLE_TAKER_FEE
    }

    #[test_only]
    public fun tick_size(): u64 {
        TICK_SIZE
    }

    #[test_only]
    public fun lot_size(): u64 {
        LOT_SIZE
    }

    #[test_only]
    public fun min_size(): u64 {
        MIN_SIZE
    }

    #[test_only]
    public fun deep_multiplier(): u64 {
        DEEP_MULTIPLIER
    }

    #[test_only]
    public fun taker_discount(): u64 {
        TAKER_DISCOUNT
    }

    #[test_only]
    public fun e_order_info_mismatch(): u64 {
        EOrderInfoMismatch
    }

    #[test_only]
    public fun e_book_order_mismatch(): u64 {
        EBookOrderMismatch
    }

    #[test_only]
    public fun e_incorrect_mid_price(): u64 {
        EIncorrectMidPrice
    }

    #[test_only]
    public fun usdc_unit(): u64 {
        USDC_UNIT
    }

    #[test_only]
    public fun sui_unit(): u64 {
        SUI_UNIT
    }

    #[test_only]
    public fun e_incorrect_pool_id(): u64 {
        EIncorrectPoolId
    }
}
