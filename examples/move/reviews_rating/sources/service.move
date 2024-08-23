// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module reviews_rating::service {
    use std::string::String;

    use sui::balance::{Self, Balance};
    use sui::clock::Clock;
    use sui::coin::{Self, Coin};
    use sui::dynamic_field as df;
    use sui::sui::SUI;
    use sui::object_table::{Self, ObjectTable};
    
    use reviews_rating::moderator::{Moderator};
    use reviews_rating::review::{Self, Review};

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
        let service_id = id.to_inner();
        let service = Service {
            id,
            reward: 1000000,
            reward_pool: balance::zero(),
            reviews: object_table::new(ctx),
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
        assert!(poe.service_id == service.id.to_inner(), EInvalidPermission);
        let ProofOfExperience { id, service_id: _ } = poe;
        object::delete(id);
        let review = review::new_review(
            owner,
            service.id.to_inner(),
            content,
            true,
            overall_rate,
            clock,
            ctx
        );
        service.add_review(review, owner, overall_rate);
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
            service.id.to_inner(),
            content,
            false,
            overall_rate,
            clock,
            ctx
        );
        service.add_review(review, owner, overall_rate);
    }

    /// Adds a review to the service
    fun add_review(
        service: &mut Service,
        review: Review,
        owner: address,
        overall_rate: u8
    ) {
        let id = review.get_id();
        let total_score = review.get_total_score();
        let time_issued = review.get_time_issued();
        service.reviews.add(id, review);
        service.update_top_reviews(id, total_score);
        df::add(&mut service.id, id, ReviewRecord { owner, overall_rate, time_issued });
        let overall_rate = (overall_rate as u64);
        service.overall_rate = service.overall_rate + overall_rate;
    }

    /// Returns true if top_reviews should be updated given a total score
    fun should_update_top_reviews(
        service: &Service,
        total_score: u64
    ): bool {
        let len = service.top_reviews.length();
        len < MAX_REVIEWERS_TO_REWARD
            || total_score > service.get_total_score(service.top_reviews[len - 1])
    }

    /// Prunes top_reviews if it exceeds MAX_REVIEWERS_TO_REWARD
    fun prune_top_reviews(
        service: &mut Service
    ) {
        while (service.top_reviews.length() > MAX_REVIEWERS_TO_REWARD) {
            service.top_reviews.pop_back();
        };
    }

    /// Updates top_reviews if necessary
    fun update_top_reviews(
        service: &mut Service,
        review_id: ID,
        total_score: u64
    ) {
        if (service.should_update_top_reviews(total_score)) {
            let idx = service.find_idx(total_score);
            service.top_reviews.insert(review_id, idx);
            service.prune_top_reviews();
        };
    }

    /// Finds the index of a review in top_reviews
    fun find_idx(service: &Service, total_score: u64): u64 {
        let mut i = service.top_reviews.length();
        while (0 < i) {
            let review_id = service.top_reviews[i - 1];
            if (service.get_total_score(review_id) > total_score) {
                break
            };
            i = i - 1;
        };
        i
    }

    /// Gets the total score of a review
    fun get_total_score(service: &Service, review_id: ID): u64 {
        service.reviews[review_id].get_total_score()
    }

    /// Distributes rewards
    public fun distribute_reward(
        cap: &AdminCap,
        service: &mut Service,
        ctx: &mut TxContext
    ) {
        assert!(cap.service_id == service.id.to_inner(), EInvalidPermission);
        // distribute a fixed amount to top MAX_REVIEWERS_TO_REWARD reviewers
        let mut len = service.top_reviews.length();
        if (len > MAX_REVIEWERS_TO_REWARD) {
            len = MAX_REVIEWERS_TO_REWARD;
        };
        // check balance
        assert!(service.reward_pool.value() >= (service.reward * len), ENotEnoughBalance);
        let mut i = 0;
        while (i < len) {
            let sub_balance = service.reward_pool.split(service.reward);
            let reward = coin::from_balance(sub_balance, ctx);
            let review_id = &service.top_reviews[i];
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
        service.reward_pool.join(coin.into_balance());
    }

    /// Mints a proof of experience for a customer
    public fun generate_proof_of_experience(
        cap: &AdminCap,
        service: &Service,
        recipient: address,
        ctx: &mut TxContext
    ) {
        // generate an NFT and transfer it to customer who can use it to write a review with higher score
        assert!(cap.service_id == service.id.to_inner(), EInvalidPermission);
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
        let record: ReviewRecord = df::remove(&mut service.id, review_id);
        service.overall_rate = service.overall_rate - (record.overall_rate as u64);
        let (contains, i) = service.top_reviews.index_of(&review_id);
        if (contains) {
            service.top_reviews.remove(i);
        };
        service.reviews.remove(review_id).delete_review();
    }

    /// Reorder top_reviews after a review is updated
    fun reorder(
        service: &mut Service,
        review_id: ID,
        total_score: u64
    ) {
        let (contains, idx) = service.top_reviews.index_of(&review_id);
        if (!contains) {
            service.update_top_reviews(review_id, total_score);
        } else {
            // remove existing review from vector and insert back
            service.top_reviews.remove(idx);
            let idx = service.find_idx(total_score);
            service.top_reviews.insert(review_id, idx);
        }
    }

    /// Upvotes a review
    public fun upvote(service: &mut Service, review_id: ID) {
        let review = &mut service.reviews[review_id];
        review.upvote();
        service.reorder(review_id, review.get_total_score());
    }
}
