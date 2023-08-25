/// # Admin
///
/// Module defining `AdminCap`, the capability held by the game admin,
/// which is used to drive the game simulation.
///
/// `AdminCap`s can be `copy_`-ed, to allow issuing concurrent
/// transactions (each would need its own `AdminCap`).
module turnip_town::admin {
    use sui::object::{Self, ID, UID};
    use sui::tx_context::TxContext;

    friend turnip_town::game;

    struct AdminCap has key, store {
        id: UID,
        game: ID,
    }

    /// AdminCap does not have permissions to update the game.
    const ENotAuthorized: u64 = 0;

    /// Only the `game` module can create brand new `AdminCap`s.
    public(friend) fun mint(game: ID, ctx: &mut TxContext): AdminCap {
        AdminCap { id: object::new(ctx), game }
    }

    /// Create an identical copy of `cap` (with the same privileges).
    /// This is useful to support issuing concurrent transactions that
    /// need admin authorization.
    public fun copy_(cap: &AdminCap, ctx: &mut TxContext): AdminCap {
        AdminCap { id: object::new(ctx), game: cap.game }
    }

    public fun authorize(cap: &AdminCap, game: ID) {
        assert!(cap.game == game, ENotAuthorized)
    }

    public fun burn(cap: AdminCap) {
        let AdminCap { id, game: _ } = cap;
        object::delete(id);
    }
}
