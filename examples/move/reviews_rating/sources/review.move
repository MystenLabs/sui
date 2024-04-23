module poc::review {
    use std::string::String;

    use sui::clock::{Self, Clock};
    use sui::math;

    /* friend poc::service; */

    const EMaxDownvoteReached: u64 = 1;
    const EInvalidContentLen: u64 = 2;

    const MIN_REVIEW_CONTENT_LEN: u64 = 5;
    const MAX_REVIEW_CONTENT_LEN: u64 = 1000;

    /// Represents a review of a service
    public struct Review has key, store {
        id: UID,
        owner: address,
        service_id: ID,
        content: String,
        // intrinsic score
        len: u64,
        // extrinsic score
        votes: u64,
        time_issued: u64,
        // proof of experience
        has_poe: bool,
        // total score
        total_score: u64,
        // overall rating value; max=5
        overall_rate: u8,
    }

    /// Creates a new review
    public(package) fun new_review(
        owner: address,
        service_id: ID,
        content: String,
        has_poe: bool,
        overall_rate: u8,
        clock: &Clock,
        ctx: &mut TxContext
    ): Review {
        let len = std::string::length(&content);
        assert!(len > MIN_REVIEW_CONTENT_LEN && len <= MAX_REVIEW_CONTENT_LEN, EInvalidContentLen);
        let mut new_review = Review {
            id: object::new(ctx),
            owner,
            service_id,
            content,
            len,
            votes: 10, // start with 10, can go down to 0
            time_issued: clock::timestamp_ms(clock),
            has_poe,
            total_score: 0,
            overall_rate,
        };
        new_review.total_score = calculate_total_score(&new_review);
        new_review
    }

    /// Deletes a review
    public(package) fun delete_review(rev: Review) {
        let Review {
            id, owner: _, service_id: _, content: _, len: _, votes: _, time_issued: _,
            has_poe: _, total_score: _, overall_rate: _
        } = rev;
        object::delete(id);
    }

    /// Calculates the total score of a review
    fun calculate_total_score(rev: &Review): u64 {
        let mut intrinsic_score: u64 = rev.len;
        intrinsic_score = math::min(intrinsic_score, 150);
        let extrinsic_score: u64 = 10 * rev.votes;
        let vm: u64 = if (rev.has_poe) { 2 } else { 1 };
        (intrinsic_score + extrinsic_score) * vm
    }

    /// Updates the total score of a review
    fun update_total_score(rev: &mut Review) {
        rev.total_score = calculate_total_score(rev);
    }

    /// Upvotes a review
    public fun upvote(rev: &mut Review) {
        rev.votes = rev.votes + 1;
        update_total_score(rev);
    }

    /// Downvotes a review
    public fun downvote(rev: &mut Review) {
        assert!(rev.votes > 0, EMaxDownvoteReached);
        rev.votes = rev.votes - 1;
        update_total_score(rev);
    }

    public fun get_id(rev: &Review): ID {
        object::uid_to_inner(&rev.id)
    }

    public fun get_total_score(rev: &Review): u64 {
        rev.total_score
    }

    public fun get_time_issued(rev: &Review): u64 {
        rev.time_issued
    }
}
