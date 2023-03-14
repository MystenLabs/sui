/// Module of `NftSafe` type.
///
/// `NftSafe` is an abstraction meant to hold NFTs in it.
///
/// We are defining here an NFT as any owned non-fungible object type that has
/// `key + store` ability, however in practice the `NftSafe` is generic enough
/// to hold any object with any degree of fungibility as long as the object type
/// has the aforementioned abilities and is Single-Writer.
///
/// A user that transfers its NFTs to its Safe is able to delegate the power
/// of transferability.
///
/// The ownership model of the `NftSafe` relies on the object `OwnerCap` whose
/// holder is the effective owner of the `NftSafe` and subsequently the owner of
/// the assets within it.
///
/// The `NftSafe` solves for the following problems:
///
/// 1. Discoverability:
///
/// One typical issue with on-chain trading is that by sending one's assets
/// to a shared object (the trading primitive), one looses the ability to
/// see them in their wallet, even though one has still technical ownership
/// of such assets, until a trade is effectively executed.
///
/// By holding NFTs in the `NftSafe`, users can list them for sale and still
/// be able to see them in their wallets until the point that they're
/// effectively sold and transferred out.
///
/// Instead of transferring the assets to the shared object (trading primitive),
/// the `NftSafe` registers a Transfer Authorisation that allows for the trading
/// primitive to withdraw the NFT at a later stage when the settlement is executed.
/// The settlement can occur immediately after the trade execution, in the same
/// transaction, or at a later stage, it's up to the trading primitive.
///
/// The registration of transfer authorisations is done through `NftRef`, namely
/// in the fields `auths` and `exclusive_auth`. This structure represents an
/// accounting item, and as a whole the `NftSafe` maintains a coherent accounting
/// of the NFTs it owns and their respective transfer authorisations
///
/// Transfer authorisations are registered in `NftRef`s which function as the
/// `NftSafe` accounting items. When a transfer occurs, all the `TransferAuth`s for
/// the respective NFT get cleared.
///
/// 2. Isomorphic control over transfers:
///
/// Objects with `key + store` ability have access to polymorphic transfer
/// functions, making these objects freely transferrable. Whilst this is useful
/// in a great deal of use-cases, creators often want build custom
/// transferrability rules (e.g. Royalty protection mechanisms, NFT with
/// expiration dates, among others).
///
/// `NftSafe` has a generic inner type `I` which regulates access to the outer
/// type. We guarantee this by having the parameter `inner_witness: IW` in
/// the funtion signatures and by calling
/// `assert_same_module_as_witness<I, IW>()`, where `IW` is a witness struct
/// defined in the inner safe module.
///
/// In effect, this allows creators and developers to create `NftSafe`s
/// with custom transferrability rules.
///
/// 3. Mutable access to NFTs:
///
/// The inner safe patter described above also allows for creators and developers
/// to define custom NFT write access rules. This is a usefule feature for
/// dynamic NFTs.
///
///
/// This module uses the following witnesses:
/// I: Inner `NftSafe` type
/// IW: Inner Witness type
/// E: Entinty Witness type of the entity requesting transfer authorisation
/// NFT: NFT type of a given NFT in the `NftSafe`
module sui::nft_safe {
    use std::type_name::{Self, TypeName};

    use sui::event;
    use sui::types;
    use sui::tx_context::TxContext;
    use sui::vec_set::{Self, VecSet};
    use sui::vec_map::{Self, VecMap};
    use sui::object::{Self, ID, UID};
    use sui::transfer::share_object;
    use sui::dynamic_object_field::{Self as dof};

    // === Errors ===

    /// NFT type is not what the user expected
    const ENftTypeMismatch: u64 = 0;

    /// Incorrect owner for the given Safe
    const ESafeOwnerMismatch: u64 = 1;

    /// Safe does not containt the NFT
    const ESafeDoesNotContainNft: u64 = 2;

    /// Entity not authotised to transfer the given NFT
    const EEntityNotAuthorisedForTransfer: u64 = 3;

    /// NFT is already exclusively listed
    const ENftAlreadyExclusivelyListed: u64 = 4;

    /// NFT is already listed
    const ENftAlreadyListed: u64 = 5;

    /// The logic requires that no NFTs are stored in the safe.
    const EMustBeEmpty: u64 = 5;


    struct NftSafe<I: key + store> has key, store {
        id: UID,
        /// Accounting for deposited NFTs. Each NFT in the object bag is
        /// represented in this map.
        refs: VecMap<ID, NftRef>,
        inner: I,
    }

    /// Holds info about NFT listing which is used to determine if an entity
    /// is allowed to redeem the NFT.
    struct NftRef has store, drop {
        /// Entities which can use their `&UID` to redeem the NFT.
        listed_with: VecSet<ID>,
        /// If set to true, then `listed_with` must have length of 1.
        /// Certain trading primitives, such as orderbooks, require exclusive
        /// auths, since heuristic transfer access to NFTs would render these
        /// primivies unusable. When traders interact with an orderbook, they
        /// expect NFTs sold it in to be available for transfer.
        is_exclusively_listed: bool,
        object_type: TypeName,
    }

    /// Whoever owns this object can perform some admin actions against the
    /// `NftSafe` shared object with the corresponding id.
    struct OwnerCap has key, store {
        id: UID,
        safe: ID,
    }

    struct DepositEvent has copy, drop {
        safe: ID,
        nft: ID,
    }

    struct TransferEvent has copy, drop {
        safe: ID,
        nft: ID,
    }

    public fun new<I: key + store>(
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

    /// Creates a new `NftSafe` shared object and returns the authority
    /// capability that grants authority over this safe.
    public fun create_safe<I: key + store>(
        inner: I,
        ctx: &mut TxContext
    ): OwnerCap {
        let (safe, cap) = new<I>(inner, ctx);
        share_object(safe);

        cap
    }

    /// Multiple entities can have redeem rights for the same NFT.
    /// Additionally, the owner can remove redeem rights for a specific entity
    /// at any time.
    /// 
    /// # Aborts
    /// * If the NFT has already given exclusive redeem rights.
    public fun list_nft<I: key + store, IW: drop>(
        self: &mut NftSafe<I>,
        owner_cap: &OwnerCap,
        nft_id: ID,
        entity_id: ID,
        _inner_witness: IW,
    ) {
        types::assert_same_module<I, IW>();
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
    public fun exclusively_list_nft<I: key + store, IW: drop>(
        self: &mut NftSafe<I>,
        owner_cap: &OwnerCap,
        entity_id: &UID,
        nft_id: ID,
        _inner_witness: IW,
    ) {
        types::assert_same_module<I, IW>();
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_not_listed(ref);

        vec_set::insert(&mut ref.listed_with, object::uid_to_inner(entity_id));
        ref.is_exclusively_listed = true;
    }

    /// Transfer an NFT into the `NftSafe`.
    public fun deposit_nft<I: key + store, IW: drop, NFT: key + store>(
        self: &mut NftSafe<I>,
        nft: NFT,
        _inner_witness: IW,
    ) {
        types::assert_same_module<I, IW>();

        let nft_id = object::id(&nft);
        let object_type = type_name::get<NFT>();

        // aborts if key already exists
        vec_map::insert(&mut self.refs, nft_id, NftRef {
            listed_with: vec_set::empty(),
            is_exclusively_listed: false,
            object_type,
        });

        dof::add(&mut self.id, nft_id, nft);

        event::emit(
            DepositEvent {
                safe: object::id(self),
                nft: nft_id,
            }
        );
    }

    /// An entity uses the `&UID` as a token which has been granted a permission 
    /// for transfer of the specific NFT.
    /// With this token, a transfer can be performed.
    public fun get_nft<I: key + store, IW: drop, NFT: key + store>(
        self: &mut NftSafe<I>,
        nft_id: ID,
        entity_id: &UID,
        _inner_witness: IW,
    ): NFT {
        types::assert_same_module<I, IW>();
        assert_has_nft(self, &nft_id);

        // NFT is being transferred - destroy the ref
        let (_, ref) = vec_map::remove(&mut self.refs, &nft_id);
        // aborts if entity is not included in the set
        let entity_auth = object::uid_to_inner(entity_id);
        vec_set::remove(&mut ref.listed_with, &entity_auth);

        dof::remove<ID, NFT>(&mut self.id, nft_id)
    }

    /// Get an NFT out of the safe as the owner.
    public fun get_nft_as_owner<I: key + store, IW: drop, NFT: key + store>(
        self: &mut NftSafe<I>,
        owner_cap: &OwnerCap,
        nft_id: ID,
        _inner_witness: IW,
    ): NFT {
        types::assert_same_module<I, IW>();
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);

        // NFT is being transferred - destroy the ref
        let (_, ref) = vec_map::remove(&mut self.refs, &nft_id);
        assert_ref_not_exclusively_listed(&ref);

        dof::remove<ID, NFT>(&mut self.id, nft_id)
    }

    /// An entity uses the `&UID` as a token which has been granted a permission 
    /// for transfer of the specific NFT.
    /// With this token, a transfer can be performed.
    public fun get_nft_to_inner_entity<I: key + store, IW: drop, NFT: key + store>(
        self: &mut NftSafe<I>,
        nft_id: ID,
        _inner_witness: IW,
    ): NFT {
        types::assert_same_module<I, IW>();
        assert_has_nft(self, &nft_id);

        // NFT is being transferred - destroy the ref
        let (_, ref) = vec_map::remove(&mut self.refs, &nft_id);
        // aborts if entity is not included in the set
        let entity_auth = object::id(&self.inner);
        vec_set::remove(&mut ref.listed_with, &entity_auth);

        dof::remove<ID, NFT>(&mut self.id, nft_id)
    }

    /// Removes all access to an NFT.
    /// An exclusive listing can be canceled only via
    /// `remove_auth_from_nft_listing`.
    /// 
    /// # Aborts
    /// * If the NFT is exclusively listed.
    public fun delist_nft<I: key + store, IW: drop>(
        self: &mut NftSafe<I>,
        owner_cap: &OwnerCap,
        nft_id: ID,
        _inner_witness: IW,
    ) {
        types::assert_same_module<I, IW>();
        assert_owner_cap(self, owner_cap);
        assert_has_nft(self, &nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_ref_not_exclusively_listed(ref);
        ref.listed_with = vec_set::empty();
    }

    /// An entity can remove itself from accessing (ie. delist) an NFT.
    /// 
    /// This method is the only way an exclusive listing can be delisted.
    /// 
    /// # Aborts
    /// * If the entity is not listed as an auth for this NFT.
    public fun remove_entity_from_nft_listing<I: key + store, IW: drop>(
        self: &mut NftSafe<I>,
        nft_id: ID,
        entity_id: &UID,
        _inner_witness: IW,
    ) {
        types::assert_same_module<I, IW>();
        assert_has_nft(self, &nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
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
    public fun remove_entity_from_nft_listing_as_owner<I: key + store, IW: drop>(
        self: &mut NftSafe<I>,
        owner: &OwnerCap,
        nft_id: ID,
        entity_id: &ID,
        _inner_witness: IW,
    ) {
        types::assert_same_module<I, IW>();
        assert_owner_cap(self, owner);
        assert_has_nft(self, &nft_id);
        
        let ref = vec_map::get_mut(&mut self.refs, &nft_id);
        assert_ref_not_exclusively_listed(ref);
        // aborts if the entity is not in the set
        vec_set::remove(&mut ref.listed_with, entity_id);
    }

    /// If there are no deposited NFTs in the safe, the safe is destroyed.
    /// Only works for non-shared safes.
    public fun destroy_empty<I: key + store, IW: drop>(
        self: NftSafe<I>,
        owner_cap: OwnerCap,
        _inner_witness: IW,
    ): I {
        types::assert_same_module<I, IW>();
        assert_owner_cap(&self, &owner_cap);
        assert!(vec_map::is_empty(&self.refs), EMustBeEmpty);

        let NftSafe { id, refs, inner } = self;
        let OwnerCap { id: cap_id, safe: _ } = owner_cap;
        vec_map::destroy_empty(refs);
        object::delete(id);
        object::delete(cap_id);

        inner
    }

    // // === Getters ===

    public fun borrow_nft<I: key + store, NFT: key + store>(
        self: &NftSafe<I>,
        nft_id: ID,
    ): &NFT {
        dof::borrow<ID, NFT>(&self.id, nft_id)
    }

    public fun has_nft<I: key + store, NFT: key + store>(
        self: &NftSafe<I>,
        nft_id: ID,
    ): bool {
        dof::exists_with_type<ID, NFT>(&self.id, nft_id)
    }

    // Borrow Inner `I` type immutably from `NftSafe<I>`
    public fun borrow_inner<I: key + store>(self: &NftSafe<I>): &I {
        &self.inner
    }

    // Borrow Inner `I` type mutably from `NftSafe<I>`
    public fun borrow_inner_mut<I: key + store>(self: &mut NftSafe<I>): &mut I {
        &mut self.inner
    }

    // Getter for OwnerCap's Safe ID
    public fun owner_cap_safe(cap: &OwnerCap): ID {
        cap.safe
    }

    public fun nft_object_type<I: key + store>(
        self: &NftSafe<I>,
        nft_id: ID,
    ): TypeName {
        let ref = vec_map::get(&self.refs, &nft_id);
        ref.object_type
    }

    // === Assertions ===

    public fun assert_owner_cap<I: key + store>(self: &NftSafe<I>, cap: &OwnerCap) {
        assert!(cap.safe == object::id(self), ESafeOwnerMismatch);
    }

    public fun assert_has_nft<I: key + store>(self: &NftSafe<I>, nft: &ID) {
        assert!(vec_map::contains(&self.refs, nft), ESafeDoesNotContainNft);
    }

    public fun assert_not_exclusively_listed<I: key + store>(
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
}
