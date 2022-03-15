module Sui::NFT {
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// An ERC721-like non-fungible token with mutable custom data.
    /// The custom data must be provided by a separate module instantiating
    /// this struct with a particular `T`. We will henceforth refer to the author
    /// of this module as "the creator".
    struct NFT<T: store> has key, store {
        id: VersionedID,
        /// Mutable custom metadata to be defined elsewhere
        data: T,
    }

    /// Create a new NFT with the given data.
    /// It is the creator's responsibility to restrict access
    /// to minting. The recommended mechanism for this is to restrict
    /// the ability to mint a value of type `T`--e.g., if the
    /// the creator intends for only 10 `NFT<T>`'s to be minted
    /// the code for creating `T` should maintain a counter to
    /// enforce this.
    public fun mint<T: store>(
        data: T, ctx: &mut TxContext
    ): NFT<T> {
        NFT { id: TxContext::new_id(ctx), data }
    }

    /// Burn `nft` and return its medatada.
    /// As with `mint`, it is the creator's responsibility to
    /// restrict access to burning. The recommended mechanism
    /// for this is to restrict the ability to destroy a value
    /// of type `T`--e.g., if the creator wants to enforce a
    /// burn fee of 10 coins, the code for collecting this
    /// fee should gate the destruction of `T`.
    public fun burn<T: store>(nft: NFT<T>): T {
        let NFT { id, data } = nft;
        ID::delete(id);
        data
    }

    /// Send NFT to `recipient`
    public fun transfer<T: store>(nft: NFT<T>, recipient: address) {
        Transfer::transfer(nft, recipient)
    }

    /// Get an immutable reference to `nft`'s data
    public fun data<T: store>(nft: &NFT<T>): &T {
        &nft.data
    }

    /// Get a mutable reference to `nft`'s data.
    /// If the creator wishes for the data to be immutable or
    /// enforce application-specific mutability policies on the
    /// `T`, the recommended mechanism for this is to
    /// - (1) avoid giving `T` the `drop` ability
    /// - (2) enforce the policy inside the module that defines `T`
    /// on a field-by-field basis.
    public fun data_mut<T: store>(nft: &mut NFT<T>): &mut T {
        &mut nft.data
    }
}
