module paywalrus::feed;

use 0x1::string::String;
use paywalrus::policy::{Policy, PolicyAdminCap};
use sui::balance::{Self, Balance};
use sui::coin::{Self, Coin};
use sui::dynamic_object_field;
use sui::sui::SUI;
use sui::table_vec::{Self, TableVec};

const ENotAuthorized: u64 = 0;
const EInvalidVersion: u64 = 1;
const EWrongPolicy: u64 = 2;
const EPurchasingNotEnabled: u64 = 3;
const EAmountIncorrect: u64 = 4;
const EAlreadyAuthorized: u64 = 5;

const VERSION: u8 = 1;

public struct FeedAdminCap has key, store {
    id: UID,
    feed: ID,
}

public struct PolicyCapKey() has copy, store, drop;

public struct Feed has key {
    id: UID,
    version: u8,
    publish_policy: ID,
    access_policy: ID,
    content: TableVec<FeedContentOption>,
    price: u64,
    balance: Balance<SUI>,
    title: String,
    description: String,
}

public struct BlobId(u256) has copy, store, drop;

public enum FeedContentOption has store, drop {
    Some(FeedContent),
    None,
}

public struct FeedContent has store, drop {
    content: BlobId,
    author: address,
    sub_feed: Option<ID>,
}

public fun create_feed(
    publish_policy: &mut Policy,
    access_policy: &mut Policy,
    title: String,
    description: String,
    ctx: &mut TxContext,
): (Feed, FeedAdminCap) {
    let feed = Feed {
        id: object::new(ctx),
        version: VERSION,
        publish_policy: publish_policy.id(),
        access_policy: access_policy.id(),
        content: table_vec::empty(ctx),
        price: 0,
        balance: balance::zero(),
        title,
        description,
    };

    let cap = FeedAdminCap {
        id: object::new(ctx),
        feed: feed.id.to_inner(),
    };

    (feed, cap)
}

public fun create_comment_feed(
    policy: &mut Policy,
    ctx: &mut TxContext,
): (Feed, FeedAdminCap) {
    let feed = Feed {
        id: object::new(ctx),
        version: VERSION,
        publish_policy: policy.id(),
        access_policy: policy.id(),
        content: table_vec::empty(ctx),
        price: 0,
        balance: balance::zero(),
        title: b"".to_string(),
        description: b"".to_string(),
    };

    let cap = FeedAdminCap {
        id: object::new(ctx),
        feed: feed.id.to_inner(),
    };

    (feed, cap)
}

public fun id(feed: &Feed): ID {
    feed.id.to_inner()
}

public fun add_content(
    feed: &mut Feed,
    policy: &Policy,
    content: u256,
    ctx: &TxContext,
) {
    feed.validate_version();
    feed.validate_publish_policy(policy, ctx);

    feed
        .content
        .push_back(
            FeedContentOption::Some(FeedContent {
                content: BlobId(content),
                author: ctx.sender(),
                sub_feed: option::none(),
            }),
        );
}

public fun add_content_with_subfeed(
    feed: &mut Feed,
    policy: &Policy,
    content: u256,
    sub_feed: &Feed,
    ctx: &TxContext,
) {
    feed.validate_version();
    feed.validate_publish_policy(policy, ctx);

    feed
        .content
        .push_back(
            FeedContentOption::Some(FeedContent {
                content: BlobId(content),
                author: ctx.sender(),
                sub_feed: option::some(sub_feed.id()),
            }),
        );
}

public fun remove_content(feed: &mut Feed, idx: u64, cap: &FeedAdminCap) {
    feed.validate_version();
    feed.validate_cap(cap);

    feed.content.push_back(FeedContentOption::None);
    feed.content.swap_remove(idx);
}

public fun remove_own_content(feed: &mut Feed, idx: u64, ctx: &TxContext) {
    feed.validate_version();

    match (feed.content.borrow(idx)) {
        FeedContentOption::Some(content) => {
            assert!(content.author == ctx.sender(), ENotAuthorized);
            feed.content.push_back(FeedContentOption::None);
            feed.content.swap_remove(idx);
        },
        FeedContentOption::None => {
            assert!(false, ENotAuthorized);
        },
    }
}

public fun set_price(
    feed: &mut Feed,
    cap: &FeedAdminCap,
    policy: &Policy,
    policy_cap: &PolicyAdminCap,
    price: u64,
    ctx: &mut TxContext,
) {
    feed.validate_cap(cap);
    feed.validate_version();
    assert!(policy_cap.policy_id() == feed.access_policy, EWrongPolicy);

    if (
        dynamic_object_field::exists_with_type<PolicyCapKey, PolicyAdminCap>(
            &feed.id,
            PolicyCapKey(),
        )
    ) {
        let old_cap = dynamic_object_field::remove<
            PolicyCapKey,
            PolicyAdminCap,
        >(&mut feed.id, PolicyCapKey());
        old_cap.destroy_cap();
    };

    dynamic_object_field::add(
        &mut feed.id,
        PolicyCapKey(),
        policy.create_cap(policy_cap, ctx),
    );
    feed.price = price;
}

public fun purchase_access(
    feed: &mut Feed,
    policy: &mut Policy,
    payment: Coin<SUI>,
    ctx: &mut TxContext,
) {
    feed.validate_version();
    assert!(feed.access_policy == policy.id(), EWrongPolicy);

    assert!(
        dynamic_object_field::exists_with_type<PolicyCapKey, PolicyAdminCap>(
            &feed.id,
            PolicyCapKey(),
        ),
        EPurchasingNotEnabled,
    );
    assert!(feed.price > 0, EPurchasingNotEnabled);
    assert!(feed.price == coin::value(&payment), EAmountIncorrect);
    assert!(!policy.is_authorized(ctx.sender()), EAlreadyAuthorized);

    let cap = dynamic_object_field::borrow<PolicyCapKey, PolicyAdminCap>(
        &feed.id,
        PolicyCapKey(),
    );
    policy.authorize(cap, ctx.sender());

    coin::put(&mut feed.balance, payment);
}

public fun withdraw_balance(
    feed: &mut Feed,
    cap: &FeedAdminCap,
    ctx: &mut TxContext,
): Coin<SUI> {
    feed.validate_cap(cap);
    feed.validate_version();

    feed.balance.withdraw_all().into_coin(ctx)
}

#[allow(lint(share_owned))]
public fun share(feed: Feed) {
    feed.validate_version();
    transfer::share_object(feed);
}

fun validate_version(feed: &Feed) {
    assert!(feed.version == VERSION, EInvalidVersion);
}

public fun validate_publish_policy(
    feed: &Feed,
    policy: &Policy,
    ctx: &TxContext,
) {
    assert!(
        feed.publish_policy == policy.id() || feed.access_policy == policy.id(),
        EWrongPolicy,
    );
    assert!(policy.is_authorized(ctx.sender()), ENotAuthorized);
}

public fun validate_access_policy(
    feed: &Feed,
    policy: &Policy,
    ctx: &TxContext,
) {
    assert!(feed.access_policy == policy.id(), EWrongPolicy);
    assert!(policy.is_authorized(ctx.sender()), ENotAuthorized);
}

fun validate_cap(feed: &Feed, cap: &FeedAdminCap) {
    assert!(cap.feed == object::borrow_id(feed), ENotAuthorized);
}
