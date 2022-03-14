/// A flash loan that works for any Coin type
module DeFi::FlashLender {
    use Sui::Coin::{Self, Coin};
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// A shared object offering flash loans to any buyer willing to pay `fee`.
    struct FlashLender<phantom T> has key {
        id: VersionedID,
        /// Coins available to be lent to prospective borrowers
        to_lend: Coin<T>,
        /// Number of `Coin<T>`'s that will be charged for the loan.
        /// In practice, this would probably be a percentage, but
        /// we use a flat fee here for simplicity.
        fee: u64,
    }

    /// A "hot potato" struct recording the number of `Coin<T>`'s that
    /// were borrowed. Because this struct does not have the `key` or
    /// `store` ability, it cannot be transferred or otherwise placed in
    /// persistent storage. Because it does not have the `drop` ability,
    /// it cannot be discarded. Thus, the only way to get rid of this
    /// struct is to call `repay` sometime during the transaction that created it,
    /// which is exactly what we want from a flash loan.
    struct Receipt<phantom T> {
        /// ID of the flash lender object the debt holder borrowed from
        flash_lender_id: ID,
        /// Total amount of funds the borrower must repay: amount borrowed + the fee
        repay_amount: u64
    }

    /// An object conveying the privilege to withdraw funds from and deposit funds to the 
    /// `FlashLender` instance with ID `flash_lender_id`. Initially granted to the creator 
    /// of the `FlashLender`, and only one `AdminCap` per lender exists.
    struct AdminCap has key, store {
        id: VersionedID,
        flash_lender_id: ID,
    }

    /// Attempted to borrow more than the `FlashLender` has.
    /// Try borrowing a smaller amount.
    const ELOAN_TOO_LARGE: u64 = 0;

    /// Tried to repay an amount other than `repay_amount` (i.e., the amount borrowed + the fee).
    /// Try repaying the proper amount.
    const EINVALID_REPAYMENT_AMOUNT: u64 = 1;

    /// Attempted to repay a `FlashLender` that was not the source of this particular debt.
    /// Try repaying the correct lender.
    const EREPAY_TO_WRONG_LENDER: u64 = 2;

    /// Attempted to perform an admin-only operation without valid permissions
    /// Try using the correct `AdminCap`
    const EADMIN_ONLY: u64 = 3;

    /// Attempted to withdraw more than the `FlashLender` has.
    /// Try withdrawing a smaller amount.
    const EWITHDRAW_TOO_LARGE: u64 = 4;

    // === Creating a flash lender ===

    /// Create a shared `FlashLender` object that makes `to_lend` available for borrowing.
    /// Any borrower will need to repay the borrowed amount and `fee` by the end of the
    /// current transaction.
    public fun new<T>(to_lend: Coin<T>, fee: u64, ctx: &mut TxContext): AdminCap {
        let id = TxContext::new_id(ctx);
        let flash_lender_id = *ID::inner(&id);
        let flash_lender = FlashLender { id, to_lend, fee };
        // make the `FlashLender` a shared object so anyone can request loans
        Transfer::share_object(flash_lender);
        // give the creator admin permissions
        AdminCap { id: TxContext::new_id(ctx), flash_lender_id }
    }

    /// Same as `new`, but transfer `WithdrawCap` to the transaction sender
    public fun create<T>(to_lend: Coin<T>, fee: u64, ctx: &mut TxContext) {
        let withdraw_cap = new(to_lend, fee, ctx);
        Transfer::transfer(withdraw_cap, TxContext::sender(ctx))
    }

    // === Core functionality: requesting a loan and repaying it ===

    /// Request a loan of `amount` from `lender`. The returned `Receipt<T>` "hot potato" ensures
    /// that the borrower will call `repay(lender, ...)` later on in this tx. 
    /// Aborts if `amount` is greater that the amount that `lender` has available for lending.
    public fun loan<T>(
        self: &mut FlashLender<T>, amount: u64, ctx: &mut TxContext
    ): (Coin<T>, Receipt<T>) {
        let to_lend = &mut self.to_lend;
        assert!(Coin::value(to_lend) >= amount, ELOAN_TOO_LARGE);
        let loan = Coin::withdraw(to_lend, amount, ctx);

        let repay_amount = amount + self.fee;        
        let receipt = Receipt { flash_lender_id: *ID::id(self), repay_amount };
        (loan, receipt)
    }

    /// Repay the loan recorded by `receipt` to `lender` with `payment`.
    /// Aborts if the repayment amount is incorrect or `lender` is not the `FlashLender`
    /// that issued the original loan. 
    public fun repay<T>(self: &mut FlashLender<T>, payment: Coin<T>, receipt: Receipt<T>) {
        let Receipt { flash_lender_id, repay_amount } = receipt;
        assert!(ID::id(self) == &flash_lender_id, EREPAY_TO_WRONG_LENDER);
        assert!(Coin::value(&payment) == repay_amount, EINVALID_REPAYMENT_AMOUNT);

        Coin::join(&mut self.to_lend, payment)
    }

    // === Admin-only functionality ===

    /// Allow admin for `self` to withdraw funds.
    public fun withdraw<T>(
        self: &mut FlashLender<T>, 
        admin_cap: &AdminCap,
        amount: u64, 
        ctx: &mut TxContext
    ): Coin<T> {
        // only the holder of the `AdminCap` for `self` can withdraw funds
        check_admin(self, admin_cap);

        let to_lend = &mut self.to_lend;
        assert!(Coin::value(to_lend) >= amount, EWITHDRAW_TOO_LARGE);
        Coin::withdraw(to_lend, amount, ctx)
    }

    /// Allow admin to add more funds to `self`
    public fun deposit<T>(
        self: &mut FlashLender<T>, admin_cap: &AdminCap, coin: Coin<T>, _ctx: &mut TxContext
    ) {
        // only the holder of the `AdminCap` for `self` can deposit funds
        check_admin(self, admin_cap);

        Coin::join(&mut self.to_lend, coin)
    }

    /// Allow admin to update the fee for `self`
    public fun update_fee<T>(
        self: &mut FlashLender<T>, admin_cap: &AdminCap, new_fee: u64, _ctx: &mut TxContext
    ) {
        // only the holder of the `AdminCap` for `self` can update the fee
        check_admin(self, admin_cap);

        self.fee = new_fee
    }

    fun check_admin<T>(self: &FlashLender<T>, admin_cap: &AdminCap) {
        assert!(ID::id(self) == &admin_cap.flash_lender_id, EADMIN_ONLY);
    }

    // === Reads ===

    /// Return the current fee for `self`
    public fun fee<T>(self: &FlashLender<T>): u64 {
        self.fee
    }

    /// Return the maximum amount available for borrowing
    public fun max_loan<T>(self: &FlashLender<T>): u64 {
        Coin::value(&self.to_lend)
    }

    /// Return the amount that the holder of `self` must repay
    public fun repay_amount<T>(self: &Receipt<T>): u64 {
        self.repay_amount
    }

    /// Return the amount that the holder of `self` must repay
    public fun flash_lender_id<T>(self: &Receipt<T>): ID {
        self.flash_lender_id
    }
}