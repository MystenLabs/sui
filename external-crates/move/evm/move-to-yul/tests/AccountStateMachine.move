#[actor]
/// This is an instance of the Account example using an explicit state machine and one-way message passing.
///
/// In this example, we need to manage rpc state explicitly, remembering any outstanding transfers in the
/// actors state. This creates a little more code, but also is somehow more transparent and true to the
/// transactional semantics of Move. This version implements an additional `cleanup` message which cancels
/// pending transactions over a certain age.
module 0x3::AccountStateMachine {

    use Async::Actor::{self, virtual_time};
    use std::vector;

    const MAX: u64 = 43;
    const MAX_TRANSFER_AGE: u128 = 100000000;

    #[state]
    struct Account {
        value: u64,
        xfer_id_counter: u64,
        pending: vector<PendingTransfer>
    }

    struct PendingTransfer has drop {
       xfer_id: u64,
       amount: u64,
       initiated_at: u128,
    }

    #[init]
    fun init(): Account {
        Account{value: 0, xfer_id_counter: 0, pending: vector::empty()}
    }

    #[message]
    fun deposit(this: &mut Account, v: u64) {
        assert!(MAX - this.value >= v, 1);
        this.value = this.value + v;
    }

    #[message]
    fun withdraw(this: &mut Account, v: u64) {
        assert!(this.value >= v, 2);
        this.value = this.value - v;
    }

    #[message]
    fun xfer(this: &mut Account, dest: address, v: u64) {
        // Do not initiate the transfer if there are not enough funds.
        assert!(this.value >= v, 1);
        let xfer_id = new_xfer_id(this);
        vector::push_back(&mut this.pending, PendingTransfer{xfer_id, amount: v, initiated_at: virtual_time()});
        // Call into a special version of deposit which calls us back once done.
        send_xfer_deposit(dest, v, self(), xfer_id);
    }

    fun new_xfer_id(this: &mut Account): u64 {
        let counter = &mut this.xfer_id_counter;
        let xfer_id = *counter;
        *counter = *counter + 1;
        xfer_id
    }

    #[message]
    fun xfer_deposit(this: &mut Account, v: u64, caller_: address, xfer_id: u64) {
        deposit(this, v);
        send_xfer_finish(caller_, xfer_id);
    }

    #[message]
    fun xfer_finish(this: &mut Account, xfer_id: u64) {
        let i = find_xfer(this, xfer_id);
        let amount = vector::borrow(&this.pending, i).amount;
        vector::remove(&mut this.pending, i);
        withdraw(this, amount)
    }

    fun find_xfer(this: &Account, xfer_id: u64): u64 {
        let pending = &this.pending;
        let i = 0;
        while (i < vector::length(pending) && vector::borrow(pending, i).xfer_id != xfer_id) {
            i = i + 1;
        };
        assert!(i < vector::length(pending), 3);
        i
    }

    #[message]
    /// A periodical cleanup which removes dated pending transfers.
    fun cleanup(this: &mut Account) {
        let pending = &mut this.pending;
        let i = 0;
        while (i < vector::length(pending)) {
            let p = vector::borrow(pending, i);
            if (virtual_time() - p.initiated_at >= MAX_TRANSFER_AGE) {
                vector::remove(pending, i);
            } else {
                i = i + 1;
            }
        }
    }
}
