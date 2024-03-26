module nft_standard::example_marketplace {

    /// A shared object holding NFT listings
    struct Marketplace {
        /// NFT's listed for sale in this marketplace, indexed by their id's
        // note: Table and ObjectTable will be added by https://github.com/MystenLabs/sui/issues/4203
        listings: Table<ID, Listing>,
        // commission taken on each filled listing. a flat fee, for simplicity.
        commission: u64
    }

    struct Listing has store {
        /// Price of the item in SUI
        price: u64,
        /// Capability to pull the item out of the appropriate `Safe` when making a sale
        transfer_cap: TransferCap
    }

    fun init() {
        // share the marketplace object, set the initial commission
    }

    public fun list(transfer_cap: TransferCap, marketplace: &mut Marketplace) {
        // create a listing from transfer_cap add it to marketplace
    }

    public fun buy<T>(
        royalty: RoyaltyReceipt<T>, coin: &mut Coin<SUI>, id: ID, safe: &mut Safe<T>, marketplace: &mut Marketplace
    ): T {
        // ...extract marketplace.commission from coin
        // ... extract the Listing for ID, get the TransferCap out of it
        safe::buy_nft(transfer_cap, royalty, id, safe);
    }
}
