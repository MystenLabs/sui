module poc::service {
    use std::string::String;

    use sui::balance::{Self, Balance};
    use sui::clock::Clock;
    use sui::coin::{Self, Coin};
    use sui::dynamic_field as df;
    use sui::sui::SUI;
    use sui::object_table::{Self, ObjectTable};
    
    use poc::moderator::{Moderator};
    use poc::review::{Self, Review};

    const EInvalidPermission: u64 = 1;
    const ENotEnoughBalance: u64 = 2;
    const ENotExists: u64 = 3;

    const MAX_REVIEWERS_TO_REWARD: u64 = 10;

    /// A capability that can be used to perform admin operations on a service
    public struct AdminCap has key, store {
        id: UID,
        service_id: ID
    }

    /// Represents a service
    public struct Service has key, store {
        id: UID,
        reward_pool: Balance<SUI>,
        reward: u64,
        top_reviews: vector<ID>,
        reviews: ObjectTable<ID, Review>,
        overall_rate: u64,
        name: String
    }

    /// Represents a proof of experience that can be used to write a review with higher score
    public struct ProofOfExperience has key {
        id: UID,
        service_id: ID,
    }

    /// Represents a review record
    public struct ReviewRecord has store, drop {
        owner: address,
        overall_rate: u8,
        time_issued: u64,
    }

    #[allow(lint(self_transfer))]
    /// Creates a new service
    public fun create_service(
        name: String,
        ctx: &mut TxContext,
    ): ID {
        let id = object::new(ctx);
        let service_id = object::uid_to_inner(&id);
        let service = Service {
            id,
            reward: 1000000,
            reward_pool: balance::zero(),
            reviews: object_table::new<ID, Review>(ctx),
            top_reviews: vector[],
            overall_rate: 0,
            name
        };

        let admin_cap = AdminCap {
            id: object::new(ctx),
            service_id
        };

        transfer::share_object(service);
        transfer::public_transfer(admin_cap, tx_context::sender(ctx));
        service_id
    }

    /// Writes a new review
    public fun write_new_review(
        service: &mut Service,
        owner: address,
        content: String,
        overall_rate: u8,
        clock: &Clock,
        poe: ProofOfExperience,
        ctx: &mut TxContext
    ) {
        assert!(poe.service_id == object::uid_to_inner(&service.id), EInvalidPermission);
        let ProofOfExperience { id, service_id: _ } = poe;
        object::delete(id);
        let review = review::new_review(
            owner,
            object::uid_to_inner(&service.id),
            content,
            true,
            overall_rate,
            clock,
            ctx
        );
        add_review(service, review, owner, overall_rate);
    }

    /// Writes a new review without proof of experience
    public fun write_new_review_without_poe(
        service: &mut Service,
        owner: address,
        content: String,
        overall_rate: u8,
        clock: &Clock,
        ctx: &mut TxContext
    ) {
        let review = review::new_review(
            owner,
            object::uid_to_inner(&service.id),
            content,
            false,
            overall_rate,
            clock,
            ctx
        );
        add_review(service, review, owner, overall_rate);
    }

    /// Adds a review to the service
    fun add_review(
        service: &mut Service,
        review: Review,
        owner: address,
        overall_rate: u8
    ) {
        let id = review::get_id(&review);
        let total_score = review::get_total_score(&review);
        let time_issued = review::get_time_issued(&review);
        object_table::add(&mut service.reviews, id, review);
        update_top_reviews(service, id, total_score);
        df::add<ID, ReviewRecord>(&mut service.id, id, ReviewRecord { owner, overall_rate, time_issued });
        let overall_rate = (overall_rate as u64);
        service.overall_rate = service.overall_rate + overall_rate;
    }

    /// Returns true if top_reviews should be updated given a total score
    fun should_update_top_reviews(
        service: &Service,
        total_score: u64
    ): bool {
        let len = vector::length(&service.top_reviews);
        if (len < MAX_REVIEWERS_TO_REWARD) {
            return true
        };
        let review_id = vector::borrow(&service.top_reviews, len - 1);
        if (total_score > get_total_score(service, *review_id)) {
            return true
        };
        false
    }

    /// Prunes top_reviews if it exceeds MAX_REVIEWERS_TO_REWARD
    fun prune_top_reviews(
        service: &mut Service
    ) {
        let len = service.top_reviews.length();
        if (len > MAX_REVIEWERS_TO_REWARD) {
            service.top_reviews.remove(len - 1);
        };
    }

    /// Updates top_reviews if necessary
    fun update_top_reviews(
        service: &mut Service,
        review_id: ID,
        total_score: u64
    ) {
        if (should_update_top_reviews(service, total_score)) {
            let idx = find_idx(service, total_score);
            service.top_reviews.insert(review_id, idx);
            prune_top_reviews(service);
        };
    }

    /// Finds the index of a review in top_reviews
    fun find_idx(service: &Service, total_score: u64): u64 {
        let len = service.top_reviews.length();
        let mut i = 0;
        let mut idx = 0;
        while (i < len) {
            idx = len - i - 1;
            let review_id = service.top_reviews[idx];
            if (get_total_score(service, review_id) > total_score) {
                return idx + 1
            };
            i = i + 1;
        };
        idx
    }

    /// Gets the total score of a review
    fun get_total_score(service: &Service, review_id: ID): u64 {
        let review = object_table::borrow(&service.reviews, review_id);
        review::get_total_score(review)
    }

    /// Distributes rewards
    public fun distribute_reward(
        cap: &AdminCap,
        service: &mut Service,
        ctx: &mut TxContext
    ) {
        assert!(cap.service_id == object::uid_to_inner(&service.id), EInvalidPermission);
        // distribute a fixed amount to top MAX_REVIEWERS_TO_REWARD reviewers
        let mut len = vector::length(&service.top_reviews);
        if (len > MAX_REVIEWERS_TO_REWARD) {
            len = MAX_REVIEWERS_TO_REWARD;
        };
        // check balance
        assert!(balance::value(&service.reward_pool) >= (service.reward * len), ENotEnoughBalance);
        let mut i = 0;
        while (i < len) {
            let sub_balance = balance::split(&mut service.reward_pool, service.reward);
            let reward = coin::from_balance(sub_balance, ctx);
            let review_id = vector::borrow(&service.top_reviews, i);
            let record = df::borrow<ID, ReviewRecord>(&service.id, *review_id);
            transfer::public_transfer(reward, record.owner);
            i = i + 1;
        };
    }

    /// Adds coins to reward pool
    public fun top_up_reward(
        service: &mut Service,
        coin: Coin<SUI>
    ) {
        balance::join(&mut service.reward_pool, coin::into_balance(coin));
    }

    /// Mints a proof of experience for a customer
    public fun generate_proof_of_experience(
        cap: &AdminCap,
        service: &Service,
        recipient: address,
        ctx: &mut TxContext
    ) {
        // generate an NFT and transfer it to customer who can use it to write a review with higher score
        assert!(cap.service_id == object::uid_to_inner(&service.id), EInvalidPermission);
        let poe = ProofOfExperience {
            id: object::new(ctx),
            service_id: cap.service_id
        };
        transfer::transfer(poe, recipient);
    }

    /// Removes a review (only moderators can do this)
    public fun remove_review(
        _: &Moderator,
        service: &mut Service,
        review_id: ID,
    ) {
        assert!(service.reviews.contains(review_id), ENotExists);
        let record = df::remove<ID, ReviewRecord>(&mut service.id, review_id);
        service.overall_rate = service.overall_rate - (record.overall_rate as u64);
        let (contains, i) = service.top_reviews.index_of(&review_id);
        if (contains) {
            service.top_reviews.remove(i);
        };
        let review = object_table::remove(&mut service.reviews, review_id);
        review::delete_review(review);
    }

    /// Reorder top_reviews after a review is updated
    fun reorder(
        service: &mut Service,
        review_id: ID,
        total_score: u64
    ) {
        let (contains, idx) = service.top_reviews.index_of(&review_id);
        if (!contains) {
            update_top_reviews(service, review_id, total_score);
        } else {
            // remove existing review from vector and insert back
            service.top_reviews.remove(idx);
            let idx = find_idx(service, total_score);
            service.top_reviews.insert(review_id, idx);
        }
    }

    /// Upvotes a review
    public fun upvote(service: &mut Service, review_id: ID) {
        let review = service.reviews.borrow_mut(review_id);
        review.upvote();
        service.reorder(review_id, review.get_total_score());
    }

    /// Downvotes a review
    public fun downvote(service: &mut Service, review_id: ID) {
        let review = service.reviews.borrow_mut(review_id);
        review.downvote();
        service.reorder(review_id, review.get_total_score());
    }
}
