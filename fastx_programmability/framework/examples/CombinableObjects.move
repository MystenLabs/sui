/// Example of objects that can be combined to create
/// new objects
module Examples::CombinableObjects {
    use Examples::TrustedCoin::EXAMPLE;
    use FastX::Authenticator::{Self, Authenticator};
    use FastX::Coin::{Self, Coin};
    use FastX::ID::ID;
    use FastX::Transfer;
    use FastX::TxContext::{Self, TxContext};

    struct Ham has key {
        id: ID
    }

    struct Bread has key {
        id: ID
    }

    struct Sandwich has key {
        id: ID
    }

    /// Address selling ham, bread, etc
    const GROCERY: vector<u8> = b"";
    /// Price for ham
    const HAM_PRICE: u64 = 10;
    /// Price for bread
    const BREAD_PRICE: u64 = 2;

    /// Not enough funds to pay for the good in question
    const EINSUFFICIENT_FUNDS: u64 = 0;

    /// Exchange `c` for some ham
    public fun buy_ham(c: Coin<EXAMPLE>, ctx: &mut TxContext): Ham {
        assert!(Coin::value(&c) == HAM_PRICE, EINSUFFICIENT_FUNDS);
        Transfer::transfer(c, admin());
        Ham { id: TxContext::new_id(ctx) }
    }

    /// Exchange `c` for some bread
    public fun buy_bread(c: Coin<EXAMPLE>, ctx: &mut TxContext): Bread {
        assert!(Coin::value(&c) == BREAD_PRICE, EINSUFFICIENT_FUNDS);
        Transfer::transfer(c, admin());
        Bread { id: TxContext::new_id(ctx) }
    }

    /// Combine the `ham` and `bread` into a delicious sandwich
    public fun make_sandwich(
        ham: Ham, bread: Bread, ctx: &mut TxContext
    ): Sandwich {
        let Ham { id: _ } = ham;
        let Bread { id: _ } = bread;
        Sandwich { id: TxContext::new_id(ctx) }
    }

    fun admin(): Authenticator {
        Authenticator::new(GROCERY)
    }
}
