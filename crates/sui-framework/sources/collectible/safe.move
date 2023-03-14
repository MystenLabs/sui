/// An entity uses their `&UID` as a token.
/// Based on this token the safe owner grants redeem rights for specific NFT.
/// An entity that has been granted redeem rights can call `get_nft`.
module sui::nft_safe {
    use std::type_name::{Self, TypeName};
    use std::option::{Self, Option};

    use sui::dynamic_object_field::{Self as dof};
    use sui::object::{Self, ID, UID};
    use sui::package::{Self, Publisher};
    use sui::tx_context::TxContext;
    use sui::vec_map::{Self, VecMap};
    use sui::vec_set::{Self, VecSet};

    // === Errors ===

    /// Incorrect owner for the given Safe
    const ESafeOwnerMismatch: u64 = 0;
    /// Safe does not contain the NFT
    const ESafeDoesNotContainNft: u64 = 1;
    /// NFT is already exclusively listed
    const ENftAlreadyExclusivelyListed: u64 = 2;
    /// NFT is already listed
    const ENftAlreadyListed: u64 = 3;
    /// The logic requires that no NFTs are stored in the safe.
    const EMustBeEmpty: u64 = 4;

    // === Structs ===

    /// Whoever owns this object can perform some admin actions against the
    /// `NftSafe` shared object with the corresponding id.
    struct OwnerCap has key, store {
        id: UID,
        safe: ID,
    }

    struct NftSafe has key, store {
        id: UID,
        /// Accounting for deposited NFTs.
        /// Each dynamic object field NFT is represented in this map.
        refs: VecMap<ID, NftRef>,
    }

    /// Holds info about NFT listing which is used to determine if an entity
    /// is allowed to redeem the NFT.
    struct NftRef has store, drop {
        /// Entities which can use their `&UID` to redeem the NFT.
        /// 
        /// We also configure min listing price.
        /// The item must be bought by the entity by _at least_ this many SUI.
        listed_with: VecMap<ID, u64>,
        /// If set to true, then `listed_with` must have length of 1
        is_exclusively_listed: bool,
        /// Cannot be `Some` if exclusively listed.
        listed_for: Option<u64>,
    }

    // === Events ===

    public fun new(ctx: &mut TxContext): (NftSafe, OwnerCap) {
        let safe = NftSafe {
            id: object::new(ctx),
            refs: vec_map::empty(),
        };

        let cap = OwnerCap {
            id: object::new(ctx),
            safe: object::id(&safe),
        };

        (safe, cap)
    }

    /// Given object is added to the safe and can be listed from now on.
    public fun deposit_nft<NFT: key + store>(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        nft: NFT,
    ) {
        let nft_id = object::id(&nft);

        vec_map::insert(&mut self.refs, nft_id, NftRef {
            listed_with: vec_map::empty(),
            is_exclusively_listed: false,
            listed_for: option::none(),
        });

        dof::add(&mut self.id, nft_id, nft);
    }

    /// TODO
    /// 
    /// # Aborts
    /// * If the NFT has already given exclusive redeem rights.
    public fun list_nft(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        nft_id: ID,
        min_price: u64,
    ) {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_ref_not_exclusively_listed(ref);

        option::fill(&mut ref.listed_for, min_price);
    }

    /// Multiples entities can have redeem rights for the same NFT.
    /// Additionally, the owner can remove redeem rights for a specific entity
    /// at any time.
    /// 
    /// # Aborts
    /// * If the NFT has already given exclusive redeem rights.
    public fun auth_entity_for_nft_transfer(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        entity_id: ID,
        nft_id: ID,
        min_price: u64,
    ) {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_ref_not_exclusively_listed(ref);

        vec_map::insert(&mut ref.listed_with, entity_id, min_price);
    }

    /// One only entity can have exclusive redeem rights for the same NFT.
    /// Only the same entity can then give up their rights.
    /// Use carefully, if the entity is malicious, they can lock the NFT. 
    /// 
    /// # Note
    /// Unlike with `auth_entity_for_nft_transfer`, we require that the entity gives us
    /// their `&UID`.
    /// This gives the owner some sort of warranty that the implementation of 
    /// the entity took into account the exclusive listing.
    /// 
    /// # Aborts
    /// * If the NFT already has given up redeem rights (not necessarily exclusive)
    public fun auth_entity_for_exclusive_nft_transfer(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        entity_id: &UID,
        nft_id: ID,
        min_price: u64,
    ) {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_not_listed(ref);

        vec_map::insert(
            &mut ref.listed_with, object::uid_to_inner(entity_id), min_price
        );
        ref.is_exclusively_listed = true;
    }

    /// An entity uses the `&UID` as a token which has been granted a permission 
    /// for transfer of the specific NFT.
    /// With this token, a transfer can be performed.
    /// TODO: add min price + hot pot
    public fun get_nft<NFT: key + store>(
        self: &mut NftSafe,
        entity_id: &UID,
        nft_id: ID,
    ): NFT {
        assert_has_nft(self, &nft_id);

        // NFT is being transferred - destroy the ref
        let (_, ref) = vec_map::remove(&mut self.refs, &nft_id);
        // aborts if entity is not included in the map
        let entity_auth = object::uid_to_inner(entity_id);
        vec_map::remove(&mut ref.listed_with, &entity_auth);

        dof::remove<ID, NFT>(&mut self.id, nft_id)
    }

    /// Get an NFT out of the safe as the owner.
    public fun get_nft_as_owner<NFT: key + store>(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        nft_id: ID,
    ): NFT {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        // NFT is being transferred - destroy the ref
        let (_, ref) = vec_map::remove(&mut self.refs, &nft_id);
        assert_ref_not_exclusively_listed(&ref);

        dof::remove<ID, NFT>(&mut self.id, nft_id)
    }

    /// An entity can remove itself from accessing (ie. delist) an NFT.
    /// 
    /// This method is the only way an exclusive listing can be delisted.
    /// 
    /// # Aborts
    /// * If the entity is not listed as an auth for this NFT.
    public fun remove_entity_from_nft_listing(
        self: &mut NftSafe,
        entity_id: &UID,
        nft_id: &ID,
    ) {
        assert_has_nft(self, nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        // aborts if the entity is not in the map
        let entity_auth = object::uid_to_inner(entity_id);
        vec_map::remove(&mut ref.listed_with, &entity_auth);
        ref.is_exclusively_listed = false; // no-op unless it was exclusive
    }

    /// The safe owner can remove an entity from accessing an NFT unless
    /// it's listed exclusively.
    /// An exclusive listing can be canceled only via
    /// `remove_auth_from_nft_listing`.
    /// 
    /// # Aborts
    /// * If the NFT is exclusively listed.
    /// * If the entity is not listed as an auth for this NFT.
    public fun remove_entity_from_nft_listing_as_owner(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        entity_id: &ID,
        nft_id: &ID,
    ) {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        assert_ref_not_exclusively_listed(ref);
        // aborts if the entity is not in the map
        vec_map::remove(&mut ref.listed_with, entity_id);
    }

    /// Removes all access to an NFT.
    /// An exclusive listing can be canceled only via
    /// `remove_auth_from_nft_listing`.
    /// 
    /// # Aborts
    /// * If the NFT is exclusively listed.
   public fun delist_nft(
        self: &mut NftSafe,
        owner_cap: &OwnerCap,
        nft_id: &ID,
    ) {
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        assert_ref_not_exclusively_listed(ref);
        ref.listed_with = vec_map::empty();
    }

    /// If there are no deposited NFTs in the safe, the safe is destroyed.
    /// Only works for non-shared safes.
    public fun destroy_empty(self: NftSafe, owner_cap: OwnerCap) {
        assert_owner_cap(&self, &owner_cap);
        assert!(vec_map::is_empty(&self.refs), EMustBeEmpty);

        let NftSafe { id, refs } = self;
        let OwnerCap { id: cap_id, safe: _ } = owner_cap;
        vec_map::destroy_empty(refs);
        object::delete(id);
        object::delete(cap_id);
    }

    // === Getters ===

    public fun nfts_count(self: &NftSafe): u64 {
        vec_map::size(&self.refs)
    }

    public fun borrow_nft<NFT: key + store>(
        self: &NftSafe, nft_id: ID,
    ): &NFT {
        assert_has_nft(self, &nft_id);
        dof::borrow<ID, NFT>(&self.id, nft_id)
    }

    public fun has_nft<NFT: key + store>(
        self: &NftSafe, nft_id: ID,
    ): bool {
        dof::exists_with_type<ID, NFT>(&self.id, nft_id)
    }

    public fun owner_cap_safe(cap: &OwnerCap): ID { cap.safe }

    // === Assertions ===

    public fun assert_owner_cap(self: &NftSafe, cap: &OwnerCap) {
        assert!(cap.safe == object::id(self), ESafeOwnerMismatch);
    }

    public fun assert_has_nft(self: &NftSafe, nft: &ID) {
        assert!(vec_map::contains(&self.refs, nft), ESafeDoesNotContainNft);
    }

    public fun assert_not_exclusively_listed(
        self: &NftSafe, nft: &ID
    ) {
        let ref = vec_map::get(&self.refs, nft);
        assert_ref_not_exclusively_listed(ref);
    }

    fun assert_ref_not_exclusively_listed(ref: &NftRef) {
        assert!(!ref.is_exclusively_listed, ENftAlreadyExclusivelyListed);
    }

    fun assert_not_listed(ref: &NftRef) {
        assert!(vec_map::size(&ref.listed_with) == 0, ENftAlreadyListed);
        assert!(option::is_none(&ref.listed_for), ENftAlreadyListed);
    }
}
