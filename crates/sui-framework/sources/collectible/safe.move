/// An entity uses their `&UID` as a token.
/// Based on this token the safe owner grants redeem rights for specific NFT.
/// An entity that has been granted redeem rights can call `get_nft`.
module sui::nft_safe {
    use std::ascii;
    use std::option::{Self, Option};
    use std::type_name::{Self, TypeName};

    use sui::dynamic_object_field::{Self as dof};
    use sui::event;
    use sui::object::{Self, ID, UID};
    use sui::package::{Self, Publisher};
    use sui::tx_context::TxContext;
    use sui::vec_map::{Self, VecMap};
    use sui::vec_set::{Self, VecSet};

    // === Errors ===

    /// NFT type is not what the user expected
    const ENftTypeMismatch: u64 = 0;
    /// Incorrect owner for the given Safe
    const ESafeOwnerMismatch: u64 = 1;
    /// Safe does not contain the NFT
    const ESafeDoesNotContainNft: u64 = 2;
    /// Entity not authorized to transfer the given NFT
    const EEntityNotAuthorizedForTransfer: u64 = 3;
    /// NFT is already exclusively listed
    const ENftAlreadyExclusivelyListed: u64 = 4;
    /// NFT is already listed
    const ENftAlreadyListed: u64 = 5;
    /// The provided `Publisher` must match the package of the inner type.
    const EPublisherInnerTypeMismatch: u64 = 6;

    // === Structs ===

    struct NftSafe<I: store> has key, store {
        id: UID,
        /// Accounting for deposited NFTs.
        /// Each dynamic object field NFT is represented in this map.
        refs: VecMap<ID, NftRef>,
        /// Constrains how the NFTs are listed and redeemed.
        inner: I,
    }

    struct NftRef has store, drop {
        auths: VecSet<ID>,
        exclusive_auth: Option<ID>,
        object_type: TypeName,
    }

    /// Whoever owns this object can perform some admin actions against the
    /// `NftSafe` shared object with the corresponding id.
    struct OwnerCap has key, store {
        id: UID,
        safe: ID,
    }

    // === Events ===

    struct DepositEvent has copy, drop {
        safe: ID,
        nft: ID,
        /// The type of the transferred object. 
        nft_type: ascii::String,
    }

    struct TransferEvent has copy, drop {
        safe: ID,
        nft: ID,
        /// The type of the transferred object. 
        nft_type: ascii::String,
        /// Entity which authorized the transfer.
        /// If None then by the owner of the safe.
        by_entity: Option<ID>,
    }

    public fun new<I: store>(
        inner: I,
        ctx: &mut TxContext
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

    /// Multiples entities can have redeem rights for the same NFT.
    /// Additionally, the owner can remove redeem rights for a specific entity
    /// at any time.
    /// 
    /// # Aborts
    /// * If the NFT has already given exclusive redeem rights.
    public fun grant_redeem_rights<I: store>(
        self: &mut NftSafe<I>,
        inner_publisher: &Publisher,
        owner_cap: &OwnerCap,
        entity_id: ID,
        nft_id: ID,
    ) {
        assert_from_package<I>(inner_publisher);
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_ref_not_exclusively_listed(ref);

        vec_set::insert(&mut ref.auths, entity_id);
    }

    /// One only entity can have exclusive redeem rights for the same NFT.
    /// Only the same entity can then give up their rights.
    /// Use carefully, if the entity is malicious, they can lock the NFT. 
    /// 
    /// # Note
    /// Unlike with `grant_redeem_rights`, we require that the entity gives us
    /// their `&UID`.
    /// This gives the owner some sort of warranty that the implementation of 
    /// the entity took into account the exclusive listing.
    /// 
    /// # Aborts
    /// * If the NFT already has given up redeem rights (not necessarily exclusive)
    public fun grant_exclusive_redeem_rights<I: store>(
        self: &mut NftSafe<I>,
        owner_cap: &OwnerCap,
        inner_publisher: &Publisher,
        entity_id: &UID,
        nft_id: ID,
    ) {
        assert_from_package<I>(inner_publisher);
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_not_listed(ref);

        option::fill(&mut ref.exclusive_auth, object::uid_to_inner(entity_id));
    }

    /// Given object is added to the safe and can be listed from now on.
    public fun deposit_nft<I: store, NFT: key + store>(
        self: &mut NftSafe<I>,
        inner_publisher: &Publisher,
        nft: NFT,
    ) {
        assert_from_package<I>(inner_publisher);

        let nft_id = object::id(&nft);
        let object_type = type_name::get<NFT>();

        // aborts if key already exists
        vec_map::insert(&mut self.refs, nft_id, NftRef {
            auths: vec_set::empty(),
            exclusive_auth: option::none(),
            object_type,
        });

        event::emit(
            DepositEvent {
                safe: object::id(self),
                nft: nft_id,
                nft_type: type_name::into_string(object_type),
            }
        );

        dof::add(&mut self.id, nft_id, nft);
    }

    /// An entity uses the `UID` as a token which has been granted a permission 
    /// for transfer of the specific NFT.
    /// With this token, a transfer can be performed.
    public fun get_nft<I: store, NFT: key + store>(
        self: &mut NftSafe<I>,
        inner_publisher: &Publisher,
        entity_id: &UID,
        nft_id: ID,
    ): NFT {
        assert_from_package<I>(inner_publisher);
        get_nft_<I, NFT>(self, entity_id, nft_id)
    }

    /// An entity can remove itself from accessing (ie. delist) an NFT.
    /// 
    /// # Aborts
    /// * If the entity is not listed as an auth for this NFT.
    public fun remove_entity_from_nft_listing<I: store>(
        self: &mut NftSafe<I>,
        inner_publisher: &Publisher,
        entity_id: &UID,
        nft_id: &ID,
    ) {
        assert_from_package<I>(inner_publisher);
        assert_has_nft(self, nft_id);

        let entity_auth = object::uid_to_inner(entity_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        if (option::is_some(&ref.exclusive_auth)) {
            // aborts if the entity is not the exclusive auth
            let exclusive_auth = option::extract(&mut ref.exclusive_auth);
            assert!(exclusive_auth == entity_auth, 0);
        } else {
            // aborts if the entity is not in the set
            vec_set::remove(&mut ref.auths, &entity_auth);
        };
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
        self: &mut NftSafe<I>,
        inner_publisher: &Publisher,
        owner_cap: &OwnerCap,
        entity_id: &ID,
        nft_id: &ID,
    ) {
        assert_from_package<I>(inner_publisher);
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        if (option::is_some(&ref.exclusive_auth)) {
            abort(0)
        } else {
            // aborts if the entity is not in the set
            vec_set::remove(&mut ref.auths, entity_id);
        };
    }

    /// Removes all access to an NFT.
    /// An exclusive listing can be canceled only via
    /// `remove_auth_from_nft_listing`.
    /// 
    /// # Aborts
    /// * If the NFT is exclusively listed.
   public fun delist_nft<I: store>(
        self: &mut NftSafe<I>,
        inner_publisher: &Publisher,
        owner_cap: &OwnerCap,
        nft_id: &ID,
    ) {
        assert_from_package<I>(inner_publisher);
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, nft_id);
        if (option::is_some(&ref.exclusive_auth)) {
            abort(0)
        } else {
            ref.auths = vec_set::empty();
        };
    }

    // === Private functions ===

    /// Assumes all authorization for this call has been properly done by the 
    /// caller.
    fun get_nft_<I: store, NFT: key + store>(
        self: &mut NftSafe<I>,
        entity_id: &UID,
        nft_id: ID,
    ): NFT {
        assert_has_nft(self, &nft_id);

        let entity_auth = object::uid_to_inner(entity_id);

        // NFT is being transferred - destroy the ref
        let (_, ref) = vec_map::remove(&mut self.refs, &nft_id);
        if (option::is_some(&ref.exclusive_auth)) {
            let exclusive_auth = option::extract(&mut ref.exclusive_auth);
            assert!(exclusive_auth == entity_auth, 0); // TODO
        } else {
            // aborts if entity is not included in the set
            vec_set::remove(&mut ref.auths, &entity_auth); 
        };

        event::emit(
            TransferEvent {
                safe: object::id(self),
                nft: nft_id,
                nft_type: type_name::into_string(ref.object_type),
                by_entity: option::some(entity_auth),
            }
        );

        dof::remove<ID, NFT>(&mut self.id, nft_id)
    }

    // === Getters ===

    public fun borrow_nft<I: store, NFT: key + store>(
        self: &NftSafe<I>,
        nft_id: ID,
    ): &NFT {
        assert_has_nft(self, &nft_id);
        dof::borrow<ID, NFT>(&self.id, nft_id)
    }

    public fun has_nft<I: store, NFT: key + store>(
        self: &NftSafe<I>,
        nft_id: ID,
    ): bool {
        dof::exists_with_type<ID, NFT>(&self.id, nft_id)
    }

    public fun owner_cap_safe(cap: &OwnerCap): ID {
        cap.safe
    }

    public fun nft_object_type<I: store>(
        self: &NftSafe<I>,
        nft_id: ID,
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
        assert!(!option::is_some(&ref.exclusive_auth), ENftAlreadyExclusivelyListed);
    }

    fun assert_not_listed(ref: &NftRef) {
        assert!(vec_set::size(&ref.auths) == 0, ENftAlreadyListed);

        assert_ref_not_exclusively_listed(ref);
    }

    fun assert_from_package<I>(inner_publisher: &Publisher) {
        assert!(package::from_package<I>(inner_publisher), EPublisherInnerTypeMismatch);
    }
}