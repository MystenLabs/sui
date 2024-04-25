module amm::pool {
    use std::type_name::{Self, TypeName};
    use std::vector;
    use sui::object::{Self, UID, ID};
    use sui::balance::{Self, Balance, Supply};
    use sui::balance::{create_supply};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::event;
    use sui::math;
    use sui::table::{Self, Table};

    /* ================= errors ================= */

    /// The pool balance differs from the acceptable.
    const EExcessiveSlippage: u64 = 0;
    /// The input amount is zero.
    const EZeroInput: u64 = 1;
    /// The pool ID doesn't match the required.
    const EInvalidPoolID: u64 = 2;
    /// There's no liquidity in the pool.
    const ENoLiquidity: u64 = 3;
    /// Fee parameter is not valid.
    const EInvalidFeeParam: u64 = 4;
    /// The provided admin capability doesn't belong to this pool.
    const EInvalidAdminCap: u64 = 5;
    /// Pool pair coin types must be ordered alphabetically (`A` < `B`) and mustn't be equal.
    const EInvalidPair: u64 = 6;
    /// Pool for this pair already exists.
    const EPoolAlreadyExists: u64 = 7;

    /* ================= events ================= */

    struct PoolCreationEvent has copy, drop {
        pool_id: ID,
    }

    /* ================= constants ================= */

    /// The number of basis points in 100%.
    const BPS_IN_100_PCT: u64 = 100 * 100;

    /* ================= LP ================= */

    /// Pool LP token witness.
    struct LP<phantom A, phantom B> has drop { }

    /* ================= Pool ================= */

    /// Pool represents an AMM Pool.
    struct Pool<phantom A, phantom B> has key {
        id: UID,
        balance_a: Balance<A>,
        balance_b: Balance<B>,
        lp_supply: Supply<LP<A, B>>,
        /// The liquidity provider fees expressed in basis points (1 bps is 0.01%)
        lp_fee_bps: u64,
        /// Admin fees are calculated as a percentage of liquidity provider fees.
        admin_fee_pct: u64,
        /// Admin fees are deposited into this balance. They can be colleced by
        /// this pool's PoolAdminCap bearer.
        admin_fee_balance: Balance<LP<A, B>>
    }

    /// Returns the balances of token A and B present in the pool and the total
    /// supply of LP coins.
    public fun pool_values<A, B>(pool: &Pool<A, B>): (u64, u64, u64) {
        (
            balance::value(&pool.balance_a),
            balance::value(&pool.balance_b),
            balance::supply_value(&pool.lp_supply)
        )
    }

    /// Returns the pool fee info.
    public fun pool_fees<A, B>(pool: &Pool<A, B>): (u64, u64) {
        (pool.lp_fee_bps, pool.admin_fee_pct)
    }

    /// Returns the value of collected admin fees stored in the pool.
    public fun pool_admin_fee_value<A, B>(pool: &Pool<A, B>): u64 {
        balance::value(&pool.admin_fee_balance)
    }

    /* ================= PoolRegistry ================= */

    /// `PoolRegistry` stores a table of all pools created which is used to guarantee
    /// that only one pool per currency pair can exist.
    struct PoolRegistry has key, store {
        id: UID,
        table: Table<PoolRegistryItem, bool>,
    }

    /// An item in the `PoolRegistry` table. Represents a pool's currency pair.
    struct PoolRegistryItem has copy, drop, store  {
        a: TypeName,
        b: TypeName
    }

    /// Creat a new empty `PoolRegistry`.
    fun new_registry(ctx: &mut TxContext): PoolRegistry {
        PoolRegistry { 
            id: object::new(ctx),
            table: table::new(ctx)
        }
    }

    // returns:
    //    0 if a < b,
    //    1 if a == b,
    //    2 if a > b
    public fun cmp_type_names(a: &TypeName, b: &TypeName): u8 {
        let bytes_a = std::ascii::as_bytes(type_name::borrow_string(a));
        let bytes_b = std::ascii::as_bytes(type_name::borrow_string(b));

        let len_a = vector::length(bytes_a);
        let len_b = vector::length(bytes_b);

        let i = 0;
        let n = math::min(len_a, len_b);
        while (i < n) {
            let a = *vector::borrow(bytes_a, i);
            let b = *vector::borrow(bytes_b, i);

            if (a < b) {
                return 0
            };
            if (a > b) {
                return 2
            };
            i = i + 1;
        };

        if (len_a == len_b) {
            return 1
        };

        return if (len_a < len_b) {
            0
        } else {
            2
        }
    }

    /// Add a new coin type tuple (`A`, `B`) to the registry. Types must be sorted alphabetically (ASCII ordered)
    /// such that `A` < `B`. They also cannot be equal.
    /// Aborts when coin types are the same.
    /// Aborts when coin types are not in order (type `A` must come before `B` alphabetically).
    /// Aborts when coin type tuple is already in the registry.
    fun registry_add<A, B>(self: &mut PoolRegistry) {
        let a = type_name::get<A>();
        let b = type_name::get<B>();
        assert!(cmp_type_names(&a, &b) == 0, EInvalidPair);

        let item = PoolRegistryItem{ a, b };
        assert!(table::contains(&self.table, item) == false, EPoolAlreadyExists);

        table::add(&mut self.table, item, true)
    }

    /* ================= AdminCap ================= */

    /// Capability allowing the bearer to execute admin operations on the pools
    /// (e.g. withdraw admin fees). There's only one `AdminCap` created during module
    /// initialization that's valid for all pools.
    struct AdminCap has key, store {
        id: UID,
    }

    /* ================= math ================= */

    /// Calculates (a * b) / c. Errors if result doesn't fit into u64.
    fun muldiv(a: u64, b: u64, c: u64): u64 {
        ((((a as u128) * (b as u128)) / (c as u128)) as u64)
    }

    /// Calculates ceil_div((a * b), c). Errors if result doesn't fit into u64.
    fun ceil_muldiv(a: u64, b: u64, c: u64): u64 {
        (ceil_div_u128((a as u128) * (b as u128), (c as u128)) as u64)
    }

    /// Calculates sqrt(a * b).
    fun mulsqrt(a: u64, b: u64): u64 {
        (math::sqrt_u128((a as u128) * (b as u128)) as u64)
    }

    /// Calculates (a * b) / c for u128. Errors if result doesn't fit into u128.
    fun muldiv_u128(a: u128, b: u128, c: u128): u128 {
        ((((a as u256) * (b as u256)) / (c as u256)) as u128)
    }

    /// Calculates ceil(a / b).
    fun ceil_div_u128(a: u128, b: u128): u128 {
        if (a == 0) 0 else (a - 1) / b + 1
    }

    /* ================= main logic ================= */

    /// Initializes the `PoolRegistry` objects and shares it, and transfers `AdminCap` to sender.
    fun init(ctx: &mut TxContext) {
        transfer::share_object(new_registry(ctx));
        transfer::transfer(
            AdminCap{ id: object::new(ctx) },
            tx_context::sender(ctx)
        )
    }

    /// Creates a new Pool with provided initial balances. Returns the initial LP coins.
    public fun create<A, B>(
        registry: &mut PoolRegistry,
        init_a: Balance<A>,
        init_b: Balance<B>,
        lp_fee_bps: u64,
        admin_fee_pct: u64,
        ctx: &mut TxContext,
    ): Balance<LP<A, B>> {
        // sanity checks
        assert!(balance::value(&init_a) > 0 && balance::value(&init_b) > 0, EZeroInput);
        assert!(lp_fee_bps < BPS_IN_100_PCT, EInvalidFeeParam);
        assert!(admin_fee_pct <= 100, EInvalidFeeParam);

        // add to registry (guarantees that there's only one pool per currency pair)
        registry_add<A, B>(registry);

        // create pool
        let pool = Pool<A, B> {
            id: object::new(ctx),
            balance_a: init_a,
            balance_b: init_b,
            lp_supply: create_supply(LP<A, B> {}),
            lp_fee_bps,
            admin_fee_pct,
            admin_fee_balance: balance::zero<LP<A, B>>()
        };

        // mint initial lp tokens
        let lp_amt = mulsqrt(balance::value(&pool.balance_a), balance::value(&pool.balance_b));
        let lp_balance = balance::increase_supply(&mut pool.lp_supply, lp_amt);

        event::emit(PoolCreationEvent { pool_id: object::id(&pool) });
        transfer::share_object(pool);

        lp_balance
    }

    /// Deposit liquidity into pool. The deposit will use up the maximum amount of
    /// the provided balances possible depending on the current pool ratio. Usually
    /// this means that all of either `input_a` or `input_b` will be fully used, while
    /// the other only partially. Otherwise, both input values will be fully used.
    /// Returns the remaining input amounts (if any) and LP Coin of appropriate value.
    /// Fails if the value of the issued LP Coin is smaller than `min_lp_out`. 
    public fun deposit<A, B>(
        pool: &mut Pool<A, B>,
        input_a: Balance<A>,
        input_b: Balance<B>,
        min_lp_out: u64
    ): (Balance<A>, Balance<B>, Balance<LP<A, B>>) {
        // sanity checks
        assert!(balance::value(&input_a) > 0, EZeroInput);
        assert!(balance::value(&input_b) > 0, EZeroInput);

        // calculate the deposit amounts
        let dab: u128 = (balance::value(&input_a) as u128) * (balance::value(&pool.balance_b) as u128);
        let dba: u128 = (balance::value(&input_b) as u128) * (balance::value(&pool.balance_a) as u128);

        let deposit_a: u64;
        let deposit_b: u64;
        let lp_to_issue: u64;
        if (dab > dba) {
            deposit_b = balance::value(&input_b);
            deposit_a = (ceil_div_u128(
                dba,
                (balance::value(&pool.balance_b) as u128),
            ) as u64);
            lp_to_issue = muldiv(
                deposit_b,
                balance::supply_value(&pool.lp_supply),
                balance::value(&pool.balance_b)
            );
        } else if (dab < dba) {
            deposit_a = balance::value(&input_a);
            deposit_b = (ceil_div_u128(
                dab,
                (balance::value(&pool.balance_a) as u128),
            ) as u64);
            lp_to_issue = muldiv(
                deposit_a,
                balance::supply_value(&pool.lp_supply),
                balance::value(&pool.balance_a)
            );
        } else {
            deposit_a = balance::value(&input_a);
            deposit_b = balance::value(&input_b);
            if (balance::supply_value(&pool.lp_supply) == 0) {
                // in this case both pool balances are 0 and lp supply is 0
                lp_to_issue = mulsqrt(deposit_a, deposit_b);
            } else {
                // the ratio of input a and b matches the ratio of pool balances
                lp_to_issue = muldiv(
                    deposit_a,
                    balance::supply_value(&pool.lp_supply),
                    balance::value(&pool.balance_a)
                );
            }
        };

        // deposit amounts into pool 
        balance::join(
            &mut pool.balance_a,
            balance::split(&mut input_a, deposit_a)
        );
        balance::join(
            &mut pool.balance_b,
            balance::split(&mut input_b, deposit_b)
        );

        // mint lp coin
        assert!(lp_to_issue >= min_lp_out, EExcessiveSlippage);
        let lp = balance::increase_supply(&mut pool.lp_supply, lp_to_issue);

        // return
        (input_a, input_b, lp)
    }

    /// Burns the provided LP Coin and withdraws corresponding pool balances.
    /// Fails if the withdrawn balances are smaller than `min_a_out` and `min_b_out`
    /// respectively.
    public fun withdraw<A, B>(
        pool: &mut Pool<A, B>,
        lp_in: Balance<LP<A, B>>,
        min_a_out: u64,
        min_b_out: u64,
    ): (Balance<A>, Balance<B>) {
        // sanity checks
        assert!(balance::value(&lp_in) > 0, EZeroInput);

        // calculate output amounts
        let lp_in_value = balance::value(&lp_in);
        let pool_a_value = balance::value(&pool.balance_a);
        let pool_b_value = balance::value(&pool.balance_b);
        let pool_lp_value = balance::supply_value(&pool.lp_supply);

        let a_out = muldiv(lp_in_value, pool_a_value, pool_lp_value);
        let b_out = muldiv(lp_in_value, pool_b_value, pool_lp_value);
        assert!(a_out >= min_a_out, EExcessiveSlippage);
        assert!(b_out >= min_b_out, EExcessiveSlippage);

        // burn lp tokens
        balance::decrease_supply(&mut pool.lp_supply, lp_in);

        // return amounts
        (
            balance::split(&mut pool.balance_a, a_out),
            balance::split(&mut pool.balance_b, b_out)
        )
    }

    /// Calclates swap result and fees based on the input amount and current pool state.
    fun calc_swap_result(
        i_value: u64,
        i_pool_value: u64,
        o_pool_value: u64,
        pool_lp_value: u64,
        lp_fee_bps: u64,
        admin_fee_pct: u64
    ): (u64, u64) {
        // calc out value
        let lp_fee_value = ceil_muldiv(i_value, lp_fee_bps, BPS_IN_100_PCT);
        let in_after_lp_fee = i_value - lp_fee_value;
        let out_value = muldiv(in_after_lp_fee, o_pool_value, i_pool_value + in_after_lp_fee);

        // calc admin fee
        let admin_fee_value = muldiv(lp_fee_value, admin_fee_pct, 100);
        // dL = L * sqrt((A + dA) / A) - L = sqrt(L^2(A + dA) / A) - L
        let admin_fee_in_lp = (math::sqrt_u128(
            muldiv_u128(
                (pool_lp_value as u128) * (pool_lp_value as u128),
                ((i_pool_value + i_value) as u128),
                ((i_pool_value + i_value - admin_fee_value) as u128)
            )
        ) as u64) - pool_lp_value;

        (out_value, admin_fee_in_lp)
    }

    /// Swaps the provided amount of A for B. Fails if the resulting amount of B
    /// is smaller than `min_out`.
    public fun swap_a<A, B>(
        pool: &mut Pool<A, B>, input: Balance<A>, min_out: u64,
    ): Balance<B> {
        // sanity checks
        assert!(balance::value(&input) > 0, EZeroInput);
        assert!(
            balance::value(&pool.balance_a) > 0 && balance::value(&pool.balance_b) > 0,
            ENoLiquidity
        );

        // calculate swap result
        let i_value = balance::value(&input);
        let i_pool_value = balance::value(&pool.balance_a);
        let o_pool_value = balance::value(&pool.balance_b);
        let pool_lp_value = balance::supply_value(&pool.lp_supply);

        let (out_value, admin_fee_in_lp) = calc_swap_result(
            i_value, i_pool_value, o_pool_value, pool_lp_value, pool.lp_fee_bps, pool.admin_fee_pct
        );

        assert!(out_value >= min_out, EExcessiveSlippage);

        // deposit admin fee
        balance::join(
            &mut pool.admin_fee_balance,
            balance::increase_supply(&mut pool.lp_supply, admin_fee_in_lp)
        );

        // deposit input
        balance::join(&mut pool.balance_a, input);

        // return output
        balance::split(&mut pool.balance_b, out_value)
    }

    /// Swaps the provided amount of B for A. Fails if the resulting amount of A
    /// is smaller than `min_out`.
    public fun swap_b<A, B>(
        pool: &mut Pool<A, B>, input: Balance<B>, min_out: u64
    ): Balance<A> {
        // sanity checks
        assert!(balance::value(&input) > 0, EZeroInput);
        assert!(
            balance::value(&pool.balance_a) > 0 && balance::value(&pool.balance_b) > 0,
            ENoLiquidity
        );

        // calculate swap result
        let i_value = balance::value(&input);
        let i_pool_value = balance::value(&pool.balance_b);
        let o_pool_value = balance::value(&pool.balance_a);
        let pool_lp_value = balance::supply_value(&pool.lp_supply);

        let (out_value, admin_fee_in_lp) = calc_swap_result(
            i_value, i_pool_value, o_pool_value, pool_lp_value, pool.lp_fee_bps, pool.admin_fee_pct
        );

        assert!(out_value >= min_out, EExcessiveSlippage);

        // deposit admin fee
        balance::join(
            &mut pool.admin_fee_balance,
            balance::increase_supply(&mut pool.lp_supply, admin_fee_in_lp)
        );

        // deposit input
        balance::join(&mut pool.balance_b, input);

        // return output
        balance::split(&mut pool.balance_a, out_value)
    }

    /// Withdraw `amount` of collected admin fees by providing pool's PoolAdminCap.
    /// When `amount` is set to 0, it will withdraw all available fees.
    public fun admin_withdraw_fees<A, B>(
        pool: &mut Pool<A, B>,
        _: &AdminCap, 
        amount: u64
    ): Balance<LP<A, B>> {
        if (amount == 0) amount = balance::value(&pool.admin_fee_balance);
        balance::split(&mut pool.admin_fee_balance, amount)
    }

    /* ================= test only ================= */

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(ctx)
    }

    /* ================= tests ================= */

    #[test_only]
    struct BAR has drop {}
    #[test_only]
    struct FOO has drop {}
    #[test_only]
    struct FOOD has drop {}
    #[test_only]
    struct FOOd has drop {}

    #[test]
    fun test_cmp_type() {
        assert!(cmp_type_names(&type_name::get<BAR>(), &type_name::get<FOO>()) == 0, 0);
        assert!(cmp_type_names(&type_name::get<FOO>(), &type_name::get<FOO>()) == 1, 0);
        assert!(cmp_type_names(&type_name::get<FOO>(), &type_name::get<BAR>()) == 2, 0);

        assert!(cmp_type_names(&type_name::get<FOO>(), &type_name::get<FOOd>()) == 0, 0);
        assert!(cmp_type_names(&type_name::get<FOOd>(), &type_name::get<FOO>()) == 2, 0);

        assert!(cmp_type_names(&type_name::get<FOOD>(), &type_name::get<FOOd>()) == 0, 0);
        assert!(cmp_type_names(&type_name::get<FOOd>(), &type_name::get<FOOD>()) == 2, 0);
    }

    #[test_only]
    fun destroy_empty_registry_for_testing(registry: PoolRegistry) {
        let PoolRegistry { id, table } = registry;
        object::delete(id);
        table::destroy_empty(table);
    }

    #[test_only]
    fun remove_for_testing<A, B>(registry: &mut PoolRegistry) {
        let a = type_name::get<A>();
        let b = type_name::get<B>();
        table::remove(&mut registry.table, PoolRegistryItem{ a, b });
    }

    #[test]
    fun test_pool_registry_add() {
        let ctx = &mut tx_context::dummy();
        let registry = new_registry(ctx);

        registry_add<BAR, FOO>(&mut registry);
        registry_add<FOO, FOOd>(&mut registry);

        remove_for_testing<BAR, FOO>(&mut registry);
        remove_for_testing<FOO, FOOd>(&mut registry);
        destroy_empty_registry_for_testing(registry);
    }

    #[test]
    #[expected_failure(abort_code = EInvalidPair)]
    fun test_pool_registry_add_aborts_when_wrong_order() {
        let ctx = &mut tx_context::dummy();
        let registry = new_registry(ctx);

        registry_add<FOO, BAR>(&mut registry);

        remove_for_testing<FOO, BAR>(&mut registry);
        destroy_empty_registry_for_testing(registry);
    }

    #[test]
    #[expected_failure(abort_code = EInvalidPair)]
    fun test_pool_registry_add_aborts_when_equal() {
        let ctx = &mut tx_context::dummy();
        let registry = new_registry(ctx);

        registry_add<FOO, FOO>(&mut registry);

        remove_for_testing<FOO, FOO>(&mut registry);
        destroy_empty_registry_for_testing(registry);
    }

    #[test]
    #[expected_failure(abort_code = EPoolAlreadyExists)]
    fun test_pool_registry_add_aborts_when_already_exists() {
        let ctx = &mut tx_context::dummy();
        let registry = new_registry(ctx);

        registry_add<BAR, FOO>(&mut registry);
        registry_add<BAR, FOO>(&mut registry); // aborts here

        remove_for_testing<BAR, FOO>(&mut registry);
        destroy_empty_registry_for_testing(registry);
    }
}
