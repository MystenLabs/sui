module nft_standard::example_nft {

    /// The NFT itself.
    struct ExampleNFT has key, store {
        id: UID,
        /// Proof that this NFT belongs to `collection`
        collection: CollectionProof,
        // metadata understood by the "display" standards goes here.
        name: String,
        url: Url,
        // .. more fields
        // ..., or alternatively, we could put the "standard" metadata in a common structure like
        // https://github.com/Origin-Byte/nft-protocol/blob/main/sources/nft/std_nft.move#L22 and
        // ask folks to use this
    }

    /// The mint policy. This is a shared object that allows someone to mint an NFT by paying the required fee.
    /// In all likelihood, there would be libraries for common drop schemes, auctions, etc. that NFT creators can leverage.
    /// But a creator can also define a totally custom policy using arbitrary Move code (e.g., only someone that owns a
    /// JellyNFT can mint a PeanutButterNFT, ...)
    struct MintPolicy has key {
        id: UID,
        mint_cap: MintCap,
        /// Price of the NFT, in SUI
        price: u64,
    }

    /// The royalty policy. This is a shared object that allows a buyer to complete a sale by
    /// complying with it.
    /// In all likelihood, there would be libraries for common policies that NFT creators can
    /// leverage. But a creator can also define a totally custom policy using arbitrary Move code
    /// (e.g., the royalty payment must be half SUI and half DOGE, ...)
    struct RoyaltyPolicy has key {
        id: UID,
        royalty_cap: RoyaltyCap<T>,
        /// Amount to collect on each sale. Use a fixed price here for simplicity, almost
        /// certainly a fraction in practice
        royalty_amount: u64,
        /// total value collected in royalties
        total_collected: u64,
        /// total number of NFT's sold
        total_sales: u64,
        /// these addresses split each royalty payment that comes in
        beneficiaries: vector<address>,
    }

    struct Witness {}

    fun init(ctx: &mut TxContext) {
        let name = x"Example";
        let total_supply = 50;
        let price = 1000;
        let (collection, mint_cap, royalty_cap) = collection::create(&Witness{}, name, total_supply);
        transfer::share(MintPolicy { id: object::new(ctx), mint_cap, price });
        transfer::share(RoyaltyPolicy { id: object::new(ctx), royalty_cap, beneficiaries: [tx_context::sender()] });
        transfer::freeze(collection);
    }

    public fun mint(
        policy: &mut MintPolicy<ExampleNFT>, payment: &mut Coin<SUI>, collection: &Collection<ExampleNFT>, ctx: &mut TxContext
    ): ExampleNFT {
        ExampleNFT {
            id: object::new(ctx),
            collection: collection::mint(&mut policy.mint_cap),
            // ...deduct policy.price from `payment
            // ...derive name and Url from collection
        }
    }

    public fun buy(policy: &mut RoyaltyPolicy<ExampleNFT>, payment: &mut Coin<SUI>, ctx: &mut TxContext): RoyaltyReceipt<ExampleNFT> {
        let receipt = collection::create_receipt(&policy.royalty_cap);
        // deduct policy.amount from payment, split, send to policy.benificiaries
        receipt
    }
}
