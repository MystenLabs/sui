/// An entity uses their `&UID` as a token.
/// Based on this token the safe owner grants redeem rights for specific NFT.
/// An entity that has been granted redeem rights can call `get_nft`.
module sui::nft_safe {
    use std::type_name::{Self, TypeName};

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
    /// The provided `Publisher` must match the package of the inner type.
    const EPublisherInnerTypeMismatch: u64 = 4;
    /// The logic requires that no NFTs are stored in the safe.
    const EMustBeEmpty: u64 = 5;

    // === Structs ===

    struct NftSafe<I: store> has key, store {
        id: UID,
        /// Accounting for deposited NFTs.
        /// Each dynamic object field NFT is represented in this map.
        refs: VecMap<ID, NftRef>,
        /// Constrains how the NFTs are listed and redeemed.
        inner: I,
    }

    /// Holds info about NFT listing which is used to determine if an entity
    /// is allowed to redeem the NFT.
    struct NftRef has store, drop {
        /// Entities which can use their `&UID` to redeem the NFT.
        listed_with: VecSet<ID>,
        /// If set to true, then `listed_with` must have length of 1
        is_exclusively_listed: bool,
        object_type: TypeName,
    }

    /// Whoever owns this object can perform some admin actions against the
    /// `NftSafe` shared object with the corresponding id.
    struct OwnerCap has key, store {
        id: UID,
        safe: ID,
    }

    // === Events ===

    public fun new<I: store>(
        inner: I, ctx: &mut TxContext
    ): (NftSafe<I>, OwnerCap) {
        let safe = NftSafe {
            id: object::new(ctx),
            refs: vec_map::empty(),
            inner,
        };

        let cap = OwnerCap {
            id: object::new(ctx),
            safe: object::id(&safe),
        };

        (safe, cap)
    }

    /// Given object is added to the safe and can be listed from now on.
    public fun deposit_nft<I: store, NFT: key + store>(
        inner_publisher: &Publisher,
        self: &mut NftSafe<I>,
        nft: NFT,
    ) {
        assert_from_package<I>(inner_publisher);

        let nft_id = object::id(&nft);
        let object_type = type_name::get<NFT>();

        // aborts if key already exists
        vec_map::insert(&mut self.refs, nft_id, NftRef {
            listed_with: vec_set::empty(),
            is_exclusively_listed: false,
            object_type,
        });

        dof::add(&mut self.id, nft_id, nft);
    }


    /// Multiples entities can have redeem rights for the same NFT.
    /// Additionally, the owner can remove redeem rights for a specific entity
    /// at any time.
    /// 
    /// # Aborts
    /// * If the NFT has already given exclusive redeem rights.
    public fun list_nft<I: store>(
        inner_publisher: &Publisher,
        self: &mut NftSafe<I>,
        owner_cap: &OwnerCap,
        entity_id: ID,
        nft_id: ID,
    ) {
        assert_from_package<I>(inner_publisher);
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_ref_not_exclusively_listed(ref);

        vec_set::insert(&mut ref.listed_with, entity_id);
    }

    /// One only entity can have exclusive redeem rights for the same NFT.
    /// Only the same entity can then give up their rights.
    /// Use carefully, if the entity is malicious, they can lock the NFT. 
    /// 
    /// # Note
    /// Unlike with `list_nft`, we require that the entity gives us
    /// their `&UID`.
    /// This gives the owner some sort of warranty that the implementation of 
    /// the entity took into account the exclusive listing.
    /// 
    /// # Aborts
    /// * If the NFT already has given up redeem rights (not necessarily exclusive)
    public fun exclusively_list_nft<I: store>(
        inner_publisher: &Publisher,
        self: &mut NftSafe<I>,
        owner_cap: &OwnerCap,
        entity_id: &UID,
        nft_id: ID,
    ) {
        assert_from_package<I>(inner_publisher);
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_not_listed(ref);

        vec_set::insert(&mut ref.listed_with, object::uid_to_inner(entity_id));
        ref.is_exclusively_listed = true;
    }

    /// An entity uses the `&UID` as a token which has been granted a permission 
    /// for transfer of the specific NFT.
    /// With this token, a transfer can be performed.
    public fun get_nft<I: store, NFT: key + store>(
        inner_publisher: &Publisher,
        self: &mut NftSafe<I>,
        entity_id: &UID,
        nft_id: ID,
    ): NFT {
        assert_from_package<I>(inner_publisher);
        assert_has_nft(self, &nft_id);

        // NFT is being transferred - destroy the ref
        let (_, ref) = vec_map::remove(&mut self.refs, &nft_id);
        // aborts if entity is not included in the set
        let entity_auth = object::uid_to_inner(entity_id);
        vec_set::remove(&mut ref.listed_with, &entity_auth);

        dof::remove<ID, NFT>(&mut self.id, nft_id)
    }

    /// Get an NFT out of the safe as the owner.
    public fun get_nft_as_owner<I: store, NFT: key + store>(
        inner_publisher: &Publisher,
        self: &mut NftSafe<I>,
        owner_cap: &OwnerCap,
        nft_id: ID,
    ): NFT {
        assert_from_package<I>(inner_publisher);
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
    public fun remove_entity_from_nft_listing<I: store>(
        inner_publisher: &Publisher,
        self: &mut NftSafe<I>,
        entity_id: &UID,
        nft_id: &ID,
    ) {
        assert_from_package<I>(inner_publisher);
        assert_has_nft(self, nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        // aborts if the entity is not in the set
        let entity_auth = object::uid_to_inner(entity_id);
        vec_set::remove(&mut ref.listed_with, &entity_auth);
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
    public fun remove_entity_from_nft_listing_as_owner<I: store>(
        inner_publisher: &Publisher,
        self: &mut NftSafe<I>,
        owner_cap: &OwnerCap,
        entity_id: &ID,
        nft_id: &ID,
    ) {
        assert_from_package<I>(inner_publisher);
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        assert_ref_not_exclusively_listed(ref);
        // aborts if the entity is not in the set
        vec_set::remove(&mut ref.listed_with, entity_id);
    }

    /// Removes all access to an NFT.
    /// An exclusive listing can be canceled only via
    /// `remove_auth_from_nft_listing`.
    /// 
    /// # Aborts
    /// * If the NFT is exclusively listed.
   public fun delist_nft<I: store>(
        inner_publisher: &Publisher,
        self: &mut NftSafe<I>,
        owner_cap: &OwnerCap,
        nft_id: &ID,
    ) {
        assert_from_package<I>(inner_publisher);
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        assert_ref_not_exclusively_listed(ref);
        ref.listed_with = vec_set::empty();
    }

    /// If there are no deposited NFTs in the safe, the safe is destroyed.
    /// Only works for non-shared safes.
    public fun destroy_empty<I: store>(
        inner_publisher: &Publisher,
        self: NftSafe<I>,
        owner_cap: OwnerCap,
    ): I {
        assert_from_package<I>(inner_publisher);
        assert_owner_cap(&self, &owner_cap);
        assert!(vec_map::is_empty(&self.refs), EMustBeEmpty);

        let NftSafe { id, refs, inner } = self;
        let OwnerCap { id: cap_id, safe: _ } = owner_cap;
        vec_map::destroy_empty(refs);
        object::delete(id);
        object::delete(cap_id);

        inner
    }

    // === Getters ===

    public fun borrow_inner<I: store>(self: &NftSafe<I>): &I { &self.inner }

    public fun borrow_inner_mut<I: store>(self: &mut NftSafe<I>): &mut I {
        &mut self.inner
    }

    public fun nfts_count<I: store>(self: &NftSafe<I>): u64 {
        vec_map::size(&self.refs)
    }

    public fun borrow_nft<I: store, NFT: key + store>(
        self: &NftSafe<I>, nft_id: ID,
    ): &NFT {
        assert_has_nft(self, &nft_id);
        dof::borrow<ID, NFT>(&self.id, nft_id)
    }

    public fun has_nft<I: store, NFT: key + store>(
        self: &NftSafe<I>, nft_id: ID,
    ): bool {
        dof::exists_with_type<ID, NFT>(&self.id, nft_id)
    }

    public fun owner_cap_safe(cap: &OwnerCap): ID { cap.safe }

    public fun nft_object_type<I: store>(
        self: &NftSafe<I>, nft_id: ID,
    ): TypeName {
        assert_has_nft(self, &nft_id);
        let ref = vec_map::get(&self.refs, &nft_id);
        ref.object_type
    }

    // === Assertions ===

    public fun assert_owner_cap<I: store>(self: &NftSafe<I>, cap: &OwnerCap) {
        assert!(cap.safe == object::id(self), ESafeOwnerMismatch);
    }

    public fun assert_has_nft<I: store>(self: &NftSafe<I>, nft: &ID) {
        assert!(vec_map::contains(&self.refs, nft), ESafeDoesNotContainNft);
    }

    public fun assert_not_exclusively_listed<I: store>(
        self: &NftSafe<I>, nft: &ID
    ) {
        let ref = vec_map::get(&self.refs, nft);
        assert_ref_not_exclusively_listed(ref);
    }

    fun assert_ref_not_exclusively_listed(ref: &NftRef) {
        assert!(!ref.is_exclusively_listed, ENftAlreadyExclusivelyListed);
    }

    fun assert_not_listed(ref: &NftRef) {
        assert!(vec_set::size(&ref.listed_with) == 0, ENftAlreadyListed);
    }

    fun assert_from_package<I>(inner_publisher: &Publisher) {
        assert!(package::from_package<I>(inner_publisher), EPublisherInnerTypeMismatch);
    }
}
