/// Example of objects that can be combined to create
/// new objects
module Basics::Sandwich {
    use Sui::Coin::{Self, Coin};
    use Sui::ID::{Self, VersionedID};
    use Sui::GAS::GAS;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    struct Ham has key {
        id: VersionedID
    }

    struct Bread has key {
        id: VersionedID
    }

    struct Sandwich has key {
        id: VersionedID,
    }

    /// Address selling ham, bread, etc
    const GROCERY: address = @0x0;
    /// Price for ham
    const HAM_PRICE: u64 = 10;
    /// Price for bread
    const BREAD_PRICE: u64 = 2;

    /// Not enough funds to pay for the good in question
    const EINSUFFICIENT_FUNDS: u64 = 0;

    /// Exchange `c` for some ham
    public fun buy_ham(c: Coin<GAS>, ctx: &mut TxContext): Ham {
        assert!(Coin::value(&c) == HAM_PRICE, EINSUFFICIENT_FUNDS);
        Transfer::transfer(c, GROCERY);
        Ham { id: TxContext::new_id(ctx) }
    }

    /// Exchange `c` for some bread
    public fun buy_bread(c: Coin<GAS>, ctx: &mut TxContext): Bread {
        assert!(Coin::value(&c) == BREAD_PRICE, EINSUFFICIENT_FUNDS);
        Transfer::transfer(c, GROCERY);
        Bread { id: TxContext::new_id(ctx) }
    }

    /// Combine the `ham` and `bread` into a delicious sandwich
    public fun make_sandwich(
        ham: Ham, bread: Bread, ctx: &mut TxContext
    ) {
        let Ham { id: ham_id } = ham;
        let Bread { id: bread_id } = bread;
        ID::delete(ham_id);
        ID::delete(bread_id);
        Transfer::transfer(Sandwich { id: TxContext::new_id(ctx) }, TxContext::sender(ctx))
    }
}
