/// # Turnip
///
/// This module defines the `Turnip` NFT, and a transfer policy
/// configured with a royalty.
///
/// Any owner of a turnip can query its properties, but only the
/// `field` module can modify those properties (size and freshness).
module turnip_town::turnip {
    use sui::object::{Self, UID};
    use sui::package;
    use sui::transfer;
    use sui::transfer_policy;
    use sui::tx_context::{Self, TxContext};

    use kiosk::royalty_rule;

    friend turnip_town::field;

    struct TURNIP has drop {}

    struct Turnip has key, store {
        id: UID,
        /// Size is measured in its own units
        size: u64,
        /// Freshness is measured in basis points.
        freshness: u16,
    }

    /// Turnip is too small to harvest
    const ETooSmall: u64 = 0;

    /// 1% commission, in basis points
    const COMMISSION_BP: u16 = 1_00;

    /// Be paid at least 1 MIST for each transaction.
    const MIN_ROYALTY: u64 = 1;

    /// The smallest size that a plant can be harvested at to still
    /// get a turnip.
    const MIN_SIZE: u64 = 100;

    /// Initially, turnips start out maximally fresh.
    const MAX_FRESHNESS_BP: u16 = 100_00;

    fun init(otw: TURNIP, ctx: &mut TxContext) {
        let publisher = package::claim(otw, ctx);
        let (policy, cap) = transfer_policy::new<Turnip>(&publisher, ctx);

        royalty_rule::add(&mut policy, &cap, COMMISSION_BP, MIN_ROYALTY);
        transfer::public_share_object(policy);
        transfer::public_transfer(cap, tx_context::sender(ctx));
        package::burn_publisher(publisher);
    }

    /// Turnips that are below the minimum size cannot be harvested.
    public fun assert_harvest(turnip: &Turnip) {
        assert!(turnip.size >= MIN_SIZE, ETooSmall)
    }

    public fun size(turnip: &Turnip): u64 {
        turnip.size
    }

    public fun freshness(turnip: &Turnip): u16 {
        turnip.freshness
    }

    public fun is_fresh(turnip: &Turnip): bool {
        turnip.freshness > 0
    }

    public fun burn(turnip: Turnip) {
        let Turnip { id, size: _, freshness: _ } = turnip;
        object::delete(id);
    }

    /** Protected Functions ***************************************************/

    /// A brand new turnip (only the `field` module can create these).
    public(friend) fun fresh(ctx: &mut TxContext): Turnip {
        Turnip {
            id: object::new(ctx),
            size: 0,
            freshness: 100_00,
        }
    }

    /// Protected function used by `field` module to increase its size.
    public(friend) fun grow(turnip: &mut Turnip, growth: u64) {
        turnip.size = turnip.size + growth;
    }

    /// Protected function used by `field` module to increase
    /// freshness, up to a maximum of 100%.
    public(friend) fun credit_freshness(turnip: &mut Turnip) {
        turnip.freshness = turnip.freshness + 5_00;
        if (turnip.freshness > 100_00) {
            turnip.freshness = 100_00;
        }
    }

    /// Protected function used by `field` module to halve freshness.
    public(friend) fun debit_freshness(turnip: &mut Turnip) {
        turnip.freshness = turnip.freshness / 2;
    }

    /* Tests ******************************************************************/

    #[test_only]
    public fun prepare_for_harvest(turnip: &mut Turnip) {
        grow(turnip, MIN_SIZE + 1)
    }
}
