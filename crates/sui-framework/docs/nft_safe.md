
<a name="0x2_nft_safe"></a>

# Module `0x2::nft_safe`

This modules exports <code><a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a></code> and transfer related primitives.


<a name="@Listing_0"></a>

## Listing


<code><a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a></code> is a storage for NFTs which can be traded.
There are three ways NFTs can be listed:
1. Publicly - anyone can buy the NFT for a given price.
2. Privately - a specific entity can buy the NFT for a given price.
3. Exclusively - only a specific entity can buy the NFT for a given price.

Exclusive listing cannot be revoked by the owner of the safe without
approval of the entity that has been granted exclusive listing rights.

An entity uses their <code>&UID</code> as a token.
Based on this token the safe owner grants redeem rights for specific NFT.
An entity that has been granted redeem rights can call <code>get_nft</code>.


<a name="@Transfer_rules_1"></a>

## Transfer rules


Using <code><a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a>&lt;T&gt;</code> and <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a>&lt;T&gt;</code> objects, a creator can
establish conditions upon which NFTs of their collection can be traded.

Simplest <code><a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a>&lt;T&gt;</code> will require 0 holders of <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a>&lt;T&gt;</code>
to sign a <code><a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt;</code>.
This is useful for collections which don't require any special conditions
such as royalties.

A royalty focused <code><a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a>&lt;T&gt;</code> will require 1 holder of
<code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a>&lt;T&gt;</code> to sign.
For example, that can be <code>sui::royalty::RoyaltyPolicy</code>.

With the pattern of <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a>&lt;T&gt;</code> signing, a pipeline of independent
<code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a>&lt;T&gt;</code> holders can be chained together.
For example, a <code>sui::royalty::RoyaltyPolicy</code> can be chained with an
allowlist of sorts to enable only certain entities to trade the NFT.


-  [Listing](#@Listing_0)
-  [Transfer rules](#@Transfer_rules_1)
-  [Struct `TransferRequest`](#0x2_nft_safe_TransferRequest)
-  [Resource `TransferPolicy`](#0x2_nft_safe_TransferPolicy)
-  [Resource `TransferCap`](#0x2_nft_safe_TransferCap)
-  [Resource `OwnerCap`](#0x2_nft_safe_OwnerCap)
-  [Resource `NftSafe`](#0x2_nft_safe_NftSafe)
-  [Struct `NftRef`](#0x2_nft_safe_NftRef)
-  [Struct `NftPubliclyListedEvent`](#0x2_nft_safe_NftPubliclyListedEvent)
-  [Constants](#@Constants_2)
-  [Function `new_transfer_policy`](#0x2_nft_safe_new_transfer_policy)
-  [Function `destroy_transfer_policy`](#0x2_nft_safe_destroy_transfer_policy)
-  [Function `set_transfer_policy_required_signatures`](#0x2_nft_safe_set_transfer_policy_required_signatures)
-  [Function `new_transfer_cap`](#0x2_nft_safe_new_transfer_cap)
-  [Function `destroy_transfer_cap`](#0x2_nft_safe_destroy_transfer_cap)
-  [Function `sign_transfer`](#0x2_nft_safe_sign_transfer)
-  [Function `allow_transfer`](#0x2_nft_safe_allow_transfer)
-  [Function `new`](#0x2_nft_safe_new)
-  [Function `new_in_ecosystem`](#0x2_nft_safe_new_in_ecosystem)
-  [Function `deposit_nft`](#0x2_nft_safe_deposit_nft)
-  [Function `list_nft`](#0x2_nft_safe_list_nft)
    -  [Aborts](#@Aborts_3)
-  [Function `purchase`](#0x2_nft_safe_purchase)
    -  [Aborts](#@Aborts_4)
-  [Function `auth_entity_for_nft_transfer`](#0x2_nft_safe_auth_entity_for_nft_transfer)
    -  [Aborts](#@Aborts_5)
-  [Function `auth_entity_for_exclusive_nft_transfer`](#0x2_nft_safe_auth_entity_for_exclusive_nft_transfer)
    -  [Note](#@Note_6)
    -  [Aborts](#@Aborts_7)
-  [Function `purchase_as_entity`](#0x2_nft_safe_purchase_as_entity)
-  [Function `get_nft_as_owner`](#0x2_nft_safe_get_nft_as_owner)
-  [Function `remove_entity_from_nft_listing`](#0x2_nft_safe_remove_entity_from_nft_listing)
    -  [Aborts](#@Aborts_8)
-  [Function `remove_entity_from_nft_listing_as_owner`](#0x2_nft_safe_remove_entity_from_nft_listing_as_owner)
    -  [Aborts](#@Aborts_9)
-  [Function `delist_nft`](#0x2_nft_safe_delist_nft)
    -  [Aborts](#@Aborts_10)
-  [Function `destroy_empty`](#0x2_nft_safe_destroy_empty)
-  [Function `withdraw_profits`](#0x2_nft_safe_withdraw_profits)
-  [Function `ecosystem`](#0x2_nft_safe_ecosystem)
-  [Function `nfts_count`](#0x2_nft_safe_nfts_count)
-  [Function `borrow_nft`](#0x2_nft_safe_borrow_nft)
-  [Function `has_nft`](#0x2_nft_safe_has_nft)
-  [Function `owner_cap_safe`](#0x2_nft_safe_owner_cap_safe)
-  [Function `transfer_request_paid`](#0x2_nft_safe_transfer_request_paid)
-  [Function `transfer_request_safe`](#0x2_nft_safe_transfer_request_safe)
-  [Function `transfer_request_entity`](#0x2_nft_safe_transfer_request_entity)
-  [Function `transfer_request_signatures`](#0x2_nft_safe_transfer_request_signatures)
-  [Function `assert_owner_cap`](#0x2_nft_safe_assert_owner_cap)
-  [Function `assert_has_nft`](#0x2_nft_safe_assert_has_nft)
-  [Function `assert_not_exclusively_listed`](#0x2_nft_safe_assert_not_exclusively_listed)
-  [Function `assert_ref_not_exclusively_listed`](#0x2_nft_safe_assert_ref_not_exclusively_listed)
-  [Function `assert_not_listed`](#0x2_nft_safe_assert_not_listed)


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
<b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field">0x2::dynamic_object_field</a>;
<b>use</b> <a href="event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="package.md#0x2_package">0x2::package</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="vec_map.md#0x2_vec_map">0x2::vec_map</a>;
<b>use</b> <a href="vec_set.md#0x2_vec_set">0x2::vec_set</a>;
</code></pre>



<a name="0x2_nft_safe_TransferRequest"></a>

## Struct `TransferRequest`

A "Hot Potato" forcing the buyer to get a transfer permission
from the item type (<code>T</code>) owner on purchase attempt.


<pre><code><b>struct</b> <a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>paid: u64</code>
</dt>
<dd>
 Amount of SUI paid for the item. Can be used to
 calculate the fee / transfer policy enforcement.
</dd>
<dt>
<code><a href="safe.md#0x2_safe">safe</a>: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>
 The ID of the <code><a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a></code> the object is being sold from.
</dd>
<dt>
<code>entity: <a href="_Option">option::Option</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>&gt;</code>
</dt>
<dd>
 Is some if the item was bought through redeem right specific to a
 trading contract (entity.)
 Is none if the item was bought directly from the safe.
</dd>
<dt>
<code>signatures: <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>&gt;</code>
</dt>
<dd>
 IDs of <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a></code> objects which allowed the transfer.

 Must be at least <code>TransferPolicy::required_signatures</code> to be
 consumed.
</dd>
</dl>


</details>

<a name="0x2_nft_safe_TransferPolicy"></a>

## Resource `TransferPolicy`

A unique objects which defines how many unique <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a>&lt;T&gt;</code>
objects must sign an NFT transfer.

<code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a></code> objects work like middleware and therefore can create
pipelines of different types which must approve a <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a></code>.

Can only be created with the <code>Publisher</code> object.


<pre><code><b>struct</b> <a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a>&lt;T&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>required_signatures: u64</code>
</dt>
<dd>
 A <code><a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a></code> is only consumed if it has at least this many
 signatures collected from <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a></code> owners.
</dd>
</dl>


</details>

<a name="0x2_nft_safe_TransferCap"></a>

## Resource `TransferCap`

A capability handed off to middleware.
The creator (access to the <code>Publisher</code> object) can define how many
unique <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a></code> objects must sign a <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a></code> before it can
be consumed with <code>allow_transfer</code>.

Can only be created with the <code>Publisher</code> object.


<pre><code><b>struct</b> <a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a>&lt;T&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_nft_safe_OwnerCap"></a>

## Resource `OwnerCap`

Whoever owns this object can perform some admin actions against the
<code><a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a></code> object with the corresponding id.


<pre><code><b>struct</b> <a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code><a href="safe.md#0x2_safe">safe</a>: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_nft_safe_NftSafe"></a>

## Resource `NftSafe`



<pre><code><b>struct</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>refs: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>, <a href="nft_safe.md#0x2_nft_safe_NftRef">nft_safe::NftRef</a>&gt;</code>
</dt>
<dd>
 Accounting for deposited NFTs.
 Each dynamic object field NFT is represented in this map.
</dd>
<dt>
<code>profits: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>
 TBD: This could be a dynamic field and hence allow for generic
 tokens to be stored.
</dd>
<dt>
<code>ecosystem: <a href="_Option">option::Option</a>&lt;<a href="_String">ascii::String</a>&gt;</code>
</dt>
<dd>
 We can ensure that the safe went through creation procedure in given
 contract by assigning its package ID to the safe's property
 <code>ecosystem</code>.
 The package ID is gotten from <code><a href="package.md#0x2_package_published_package">package::published_package</a></code>.

 This enables assertions for use cases where the owner cap should be
 wrapped to amend certain actions.
</dd>
<dt>
<code>owner_cap_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>
 Discoverability purposes
</dd>
</dl>


</details>

<a name="0x2_nft_safe_NftRef"></a>

## Struct `NftRef`

Inner accounting type.

Holds info about NFT listing which is used to determine if an entity
is allowed to redeem the NFT.


<pre><code><b>struct</b> <a href="nft_safe.md#0x2_nft_safe_NftRef">NftRef</a> <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>listed_with: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>, u64&gt;</code>
</dt>
<dd>
 Entities which can use their <code>&UID</code> to redeem the NFT.

 We also configure min listing price.
 The item must be bought by the entity by _at least_ this many SUI.
</dd>
<dt>
<code>is_exclusively_listed: bool</code>
</dt>
<dd>
 If set to true, then <code>listed_with</code> must have length of 1 and
 listed_for must be "none".
</dd>
<dt>
<code>listed_for: <a href="_Option">option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
 How much is the NFT _publicly_ listed for.

 Anyone can come to the safe and buy the NFT for this price.
</dd>
</dl>


</details>

<a name="0x2_nft_safe_NftPubliclyListedEvent"></a>

## Struct `NftPubliclyListedEvent`



<pre><code><b>struct</b> <a href="nft_safe.md#0x2_nft_safe_NftPubliclyListedEvent">NftPubliclyListedEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="safe.md#0x2_safe">safe</a>: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>nft: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>price: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_2"></a>

## Constants


<a name="0x2_nft_safe_ENotEnough"></a>

The amount provided is not enough.


<pre><code><b>const</b> <a href="nft_safe.md#0x2_nft_safe_ENotEnough">ENotEnough</a>: u64 = 6;
</code></pre>



<a name="0x2_nft_safe_EMustBeEmpty"></a>

The logic requires that no NFTs are stored in the safe.


<pre><code><b>const</b> <a href="nft_safe.md#0x2_nft_safe_EMustBeEmpty">EMustBeEmpty</a>: u64 = 4;
</code></pre>



<a name="0x2_nft_safe_ENftAlreadyExclusivelyListed"></a>

NFT is already exclusively listed


<pre><code><b>const</b> <a href="nft_safe.md#0x2_nft_safe_ENftAlreadyExclusivelyListed">ENftAlreadyExclusivelyListed</a>: u64 = 2;
</code></pre>



<a name="0x2_nft_safe_ENftAlreadyListed"></a>

NFT is already listed


<pre><code><b>const</b> <a href="nft_safe.md#0x2_nft_safe_ENftAlreadyListed">ENftAlreadyListed</a>: u64 = 3;
</code></pre>



<a name="0x2_nft_safe_ENotEnoughSignatures"></a>

The <code><a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a></code> has not been signed by enough <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a></code>s.


<pre><code><b>const</b> <a href="nft_safe.md#0x2_nft_safe_ENotEnoughSignatures">ENotEnoughSignatures</a>: u64 = 7;
</code></pre>



<a name="0x2_nft_safe_EPublisherMismatch"></a>

Publisher does not match the expected type.


<pre><code><b>const</b> <a href="nft_safe.md#0x2_nft_safe_EPublisherMismatch">EPublisherMismatch</a>: u64 = 5;
</code></pre>



<a name="0x2_nft_safe_ESafeDoesNotContainNft"></a>

Safe does not contain the NFT


<pre><code><b>const</b> <a href="nft_safe.md#0x2_nft_safe_ESafeDoesNotContainNft">ESafeDoesNotContainNft</a>: u64 = 1;
</code></pre>



<a name="0x2_nft_safe_ESafeOwnerMismatch"></a>

Incorrect owner for the given Safe


<pre><code><b>const</b> <a href="nft_safe.md#0x2_nft_safe_ESafeOwnerMismatch">ESafeOwnerMismatch</a>: u64 = 0;
</code></pre>



<a name="0x2_nft_safe_new_transfer_policy"></a>

## Function `new_transfer_policy`

Register a type in the <code><a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a></code> system and receive an<code><a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a></code>
which is required to confirm <code><a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a></code> deals for the <code>T</code>.
If there's no <code><a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a></code> available for use, the type can not be
traded in <code><a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a></code>s.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_new_transfer_policy">new_transfer_policy</a>&lt;T: store, key&gt;(publisher: &<a href="package.md#0x2_package_Publisher">package::Publisher</a>, required_signatures: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="nft_safe.md#0x2_nft_safe_TransferPolicy">nft_safe::TransferPolicy</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_new_transfer_policy">new_transfer_policy</a>&lt;T: key + store&gt;(
    publisher: &Publisher, required_signatures: u64, ctx: &<b>mut</b> TxContext,
): <a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a>&lt;T&gt; {
    <b>assert</b>!(<a href="package.md#0x2_package_from_package">package::from_package</a>&lt;T&gt;(publisher), <a href="nft_safe.md#0x2_nft_safe_EPublisherMismatch">EPublisherMismatch</a>);
    <b>let</b> id = <a href="object.md#0x2_object_new">object::new</a>(ctx);
    <a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a> { id, required_signatures }
}
</code></pre>



</details>

<a name="0x2_nft_safe_destroy_transfer_policy"></a>

## Function `destroy_transfer_policy`

Destroy a <code><a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_destroy_transfer_policy">destroy_transfer_policy</a>&lt;T: store, key&gt;(policy: <a href="nft_safe.md#0x2_nft_safe_TransferPolicy">nft_safe::TransferPolicy</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_destroy_transfer_policy">destroy_transfer_policy</a>&lt;T: key + store&gt;(policy: <a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a>&lt;T&gt;) {
    <b>let</b> <a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a> { id, required_signatures: _ } = policy;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
}
</code></pre>



</details>

<a name="0x2_nft_safe_set_transfer_policy_required_signatures"></a>

## Function `set_transfer_policy_required_signatures`

Changes how many unique <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a></code> signatures are necessary to
consume <code><a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_set_transfer_policy_required_signatures">set_transfer_policy_required_signatures</a>&lt;T: store, key&gt;(publisher: &<a href="package.md#0x2_package_Publisher">package::Publisher</a>, policy: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_TransferPolicy">nft_safe::TransferPolicy</a>&lt;T&gt;, required_signatures: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_set_transfer_policy_required_signatures">set_transfer_policy_required_signatures</a>&lt;T: key + store&gt;(
    publisher: &Publisher, policy: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a>&lt;T&gt;, required_signatures: u64,
) {
    <b>assert</b>!(<a href="package.md#0x2_package_from_package">package::from_package</a>&lt;T&gt;(publisher), <a href="nft_safe.md#0x2_nft_safe_EPublisherMismatch">EPublisherMismatch</a>);
    policy.required_signatures = required_signatures;
}
</code></pre>



</details>

<a name="0x2_nft_safe_new_transfer_cap"></a>

## Function `new_transfer_cap`

Register a type in the <code><a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a></code> system and receive an<code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a></code>
which is required to confirm <code><a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a></code> deals for the <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_new_transfer_cap">new_transfer_cap</a>&lt;T: store, key&gt;(publisher: &<a href="package.md#0x2_package_Publisher">package::Publisher</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="nft_safe.md#0x2_nft_safe_TransferCap">nft_safe::TransferCap</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_new_transfer_cap">new_transfer_cap</a>&lt;T: key + store&gt;(
    publisher: &Publisher, ctx: &<b>mut</b> TxContext,
): <a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a>&lt;T&gt; {
    <b>assert</b>!(<a href="package.md#0x2_package_from_package">package::from_package</a>&lt;T&gt;(publisher), <a href="nft_safe.md#0x2_nft_safe_EPublisherMismatch">EPublisherMismatch</a>);
    <b>let</b> id = <a href="object.md#0x2_object_new">object::new</a>(ctx);
    <a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a> { id }
}
</code></pre>



</details>

<a name="0x2_nft_safe_destroy_transfer_cap"></a>

## Function `destroy_transfer_cap`

Destroy a <code><a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_destroy_transfer_cap">destroy_transfer_cap</a>&lt;T: store, key&gt;(cap: <a href="nft_safe.md#0x2_nft_safe_TransferCap">nft_safe::TransferCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_destroy_transfer_cap">destroy_transfer_cap</a>&lt;T: key + store&gt;(cap: <a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a>&lt;T&gt;) {
    <b>let</b> <a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a> { id } = cap;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
}
</code></pre>



</details>

<a name="0x2_nft_safe_sign_transfer"></a>

## Function `sign_transfer`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_sign_transfer">sign_transfer</a>&lt;T: store, key&gt;(cap: &<a href="nft_safe.md#0x2_nft_safe_TransferCap">nft_safe::TransferCap</a>&lt;T&gt;, req: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_TransferRequest">nft_safe::TransferRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_sign_transfer">sign_transfer</a>&lt;T: key + store&gt;(
    cap: &<a href="nft_safe.md#0x2_nft_safe_TransferCap">TransferCap</a>&lt;T&gt;, req: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt;,
) {
    <a href="vec_set.md#0x2_vec_set_insert">vec_set::insert</a>(&<b>mut</b> req.signatures, <a href="object.md#0x2_object_id">object::id</a>(cap));
}
</code></pre>



</details>

<a name="0x2_nft_safe_allow_transfer"></a>

## Function `allow_transfer`

Allow a <code><a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a></code> for the type <code>T</code>.
The call is protected by the type constraint, as only the publisher of
the <code>T</code> can get <code><a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a>&lt;T&gt;</code>.

Note: unless there's a policy for <code>T</code> to allow transfers, trades will
not be possible.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_allow_transfer">allow_transfer</a>&lt;T: store, key&gt;(policy: &<a href="nft_safe.md#0x2_nft_safe_TransferPolicy">nft_safe::TransferPolicy</a>&lt;T&gt;, req: <a href="nft_safe.md#0x2_nft_safe_TransferRequest">nft_safe::TransferRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_allow_transfer">allow_transfer</a>&lt;T: key + store&gt;(
    policy: &<a href="nft_safe.md#0x2_nft_safe_TransferPolicy">TransferPolicy</a>&lt;T&gt;, req: <a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt;,
) {
    <b>let</b> <a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a> {
        paid: _, <a href="safe.md#0x2_safe">safe</a>: _, entity: _, signatures,
    } = req;

    <b>assert</b>!(
        <a href="vec_set.md#0x2_vec_set_size">vec_set::size</a>(&signatures) &gt;= policy.required_signatures,
        <a href="nft_safe.md#0x2_nft_safe_ENotEnoughSignatures">ENotEnoughSignatures</a>,
    );
}
</code></pre>



</details>

<a name="0x2_nft_safe_new"></a>

## Function `new`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_new">new</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, <a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_new">new</a>(ctx: &<b>mut</b> TxContext): (<a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>, <a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>) {
    <b>let</b> cap_uid = <a href="object.md#0x2_object_new">object::new</a>(ctx);
    <b>let</b> <a href="safe.md#0x2_safe">safe</a> = <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        refs: <a href="vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        profits: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
        ecosystem: <a href="_none">option::none</a>(),
        owner_cap_id: <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&cap_uid),
    };
    <b>let</b> cap = <a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a> {
        id: cap_uid,
        <a href="safe.md#0x2_safe">safe</a>: <a href="object.md#0x2_object_id">object::id</a>(&<a href="safe.md#0x2_safe">safe</a>),
    };
    (<a href="safe.md#0x2_safe">safe</a>, cap)
}
</code></pre>



</details>

<a name="0x2_nft_safe_new_in_ecosystem"></a>

## Function `new_in_ecosystem`

We can ensure that the safe went through creation procedure in given
contract by assigning its typename to the safe's property
<code>ecosystem</code>.

This enables assertions for use cases where the owner cap should be
wrapped to amend certain actions.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_new_in_ecosystem">new_in_ecosystem</a>(publisher: &<a href="package.md#0x2_package_Publisher">package::Publisher</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, <a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_new_in_ecosystem">new_in_ecosystem</a>(
    publisher: &Publisher, ctx: &<b>mut</b> TxContext,
): (<a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>, <a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>) {
    <b>let</b> cap_uid = <a href="object.md#0x2_object_new">object::new</a>(ctx);
    <b>let</b> <a href="safe.md#0x2_safe">safe</a> = <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        refs: <a href="vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        profits: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
        ecosystem: <a href="_some">option::some</a>(*<a href="package.md#0x2_package_published_package">package::published_package</a>(publisher)),
        owner_cap_id: <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&cap_uid),
    };
    <b>let</b> cap = <a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a> {
        id: cap_uid,
        <a href="safe.md#0x2_safe">safe</a>: <a href="object.md#0x2_object_id">object::id</a>(&<a href="safe.md#0x2_safe">safe</a>),
    };
    (<a href="safe.md#0x2_safe">safe</a>, cap)
}
</code></pre>



</details>

<a name="0x2_nft_safe_deposit_nft"></a>

## Function `deposit_nft`

Given object is added to the safe and can be listed from now on.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_deposit_nft">deposit_nft</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, _owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>, nft: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_deposit_nft">deposit_nft</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>, _owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>, nft: T,
) {
    <b>let</b> nft_id = <a href="object.md#0x2_object_id">object::id</a>(&nft);

    <a href="vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> self.refs, nft_id, <a href="nft_safe.md#0x2_nft_safe_NftRef">NftRef</a> {
        listed_with: <a href="vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        is_exclusively_listed: <b>false</b>,
        listed_for: <a href="_none">option::none</a>(),
    });

    dof::add(&<b>mut</b> self.id, nft_id, nft);
}
</code></pre>



</details>

<a name="0x2_nft_safe_list_nft"></a>

## Function `list_nft`

After this, anyone can buy the NFT from the safe for the given price.


<a name="@Aborts_3"></a>

### Aborts

* If the NFT has already given exclusive redeem rights.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_list_nft">list_nft</a>(self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>, nft_id: <a href="object.md#0x2_object_ID">object::ID</a>, price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_list_nft">list_nft</a>(
    self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>,
    owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>,
    nft_id: ID,
    price: u64,
) {
    <a href="nft_safe.md#0x2_nft_safe_assert_owner_cap">assert_owner_cap</a>(self, owner_cap);
    <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self, &nft_id);

    <b>let</b> ref = <a href="vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.refs, &nft_id);
    <a href="nft_safe.md#0x2_nft_safe_assert_ref_not_exclusively_listed">assert_ref_not_exclusively_listed</a>(ref);

    <a href="_fill">option::fill</a>(&<b>mut</b> ref.listed_for, price);

    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="nft_safe.md#0x2_nft_safe_NftPubliclyListedEvent">NftPubliclyListedEvent</a> {
        <a href="safe.md#0x2_safe">safe</a>: <a href="object.md#0x2_object_id">object::id</a>(self),
        nft: nft_id,
        price,
    });
}
</code></pre>



</details>

<a name="0x2_nft_safe_purchase"></a>

## Function `purchase`

Buy a publicly listed NFT.

This function returns a hot potato which must be passed around and
finally destroyed in <code>allow_transfer</code>.


<a name="@Aborts_4"></a>

### Aborts

* If the NFT is not publicly listed
* If the wallet doesn't have enough tokens


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_purchase">purchase</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, wallet: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, nft_id: <a href="object.md#0x2_object_ID">object::ID</a>): (T, <a href="nft_safe.md#0x2_nft_safe_TransferRequest">nft_safe::TransferRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_purchase">purchase</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>, wallet: &<b>mut</b> Coin&lt;SUI&gt;, nft_id: ID,
): (T, <a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt;) {
    <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self, &nft_id);

    // NFT is being transferred - destroy the ref
    <b>let</b> (_, ref) = <a href="vec_map.md#0x2_vec_map_remove">vec_map::remove</a>(&<b>mut</b> self.refs, &nft_id);
    <b>let</b> listed_for = *<a href="_borrow">option::borrow</a>(&ref.listed_for);

    <b>let</b> payment = <a href="balance.md#0x2_balance_split">balance::split</a>(<a href="coin.md#0x2_coin_balance_mut">coin::balance_mut</a>(wallet), listed_for);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.profits, payment);

    <b>let</b> nft = dof::remove&lt;ID, T&gt;(&<b>mut</b> self.id, nft_id);
    (nft, <a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt; {
        paid: listed_for,
        <a href="safe.md#0x2_safe">safe</a>: <a href="object.md#0x2_object_id">object::id</a>(self),
        entity: <a href="_none">option::none</a>(),
        signatures: <a href="vec_set.md#0x2_vec_set_empty">vec_set::empty</a>(),
    })
}
</code></pre>



</details>

<a name="0x2_nft_safe_auth_entity_for_nft_transfer"></a>

## Function `auth_entity_for_nft_transfer`

Multiples entities can have redeem rights for the same NFT.
Additionally, the owner can remove redeem rights for a specific entity
at any time.


<a name="@Aborts_5"></a>

### Aborts

* If the NFT has already given exclusive redeem rights.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_auth_entity_for_nft_transfer">auth_entity_for_nft_transfer</a>(self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>, entity_id: <a href="object.md#0x2_object_ID">object::ID</a>, nft_id: <a href="object.md#0x2_object_ID">object::ID</a>, min_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_auth_entity_for_nft_transfer">auth_entity_for_nft_transfer</a>(
    self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>,
    owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>,
    entity_id: ID,
    nft_id: ID,
    min_price: u64,
) {
    <a href="nft_safe.md#0x2_nft_safe_assert_owner_cap">assert_owner_cap</a>(self, owner_cap);
    <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self, &nft_id);

    <b>let</b> ref = <a href="vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.refs, &nft_id);
    <a href="nft_safe.md#0x2_nft_safe_assert_ref_not_exclusively_listed">assert_ref_not_exclusively_listed</a>(ref);

    <a href="vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> ref.listed_with, entity_id, min_price);
}
</code></pre>



</details>

<a name="0x2_nft_safe_auth_entity_for_exclusive_nft_transfer"></a>

## Function `auth_entity_for_exclusive_nft_transfer`

One only entity can have exclusive redeem rights for the same NFT.
Only the same entity can then give up their rights.
Use carefully, if the entity is malicious, they can lock the NFT.


<a name="@Note_6"></a>

### Note

Unlike with <code>auth_entity_for_nft_transfer</code>, we require that the entity
approves this action <code>&UID</code>.
This gives the owner some sort of warranty that the implementation of
the entity took into account the exclusive listing.


<a name="@Aborts_7"></a>

### Aborts

* If the NFT already has given up redeem rights (not necessarily exclusive)


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_auth_entity_for_exclusive_nft_transfer">auth_entity_for_exclusive_nft_transfer</a>(self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>, entity_id: &<a href="object.md#0x2_object_UID">object::UID</a>, nft_id: <a href="object.md#0x2_object_ID">object::ID</a>, min_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_auth_entity_for_exclusive_nft_transfer">auth_entity_for_exclusive_nft_transfer</a>(
    self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>,
    owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>,
    entity_id: &UID,
    nft_id: ID,
    min_price: u64,
) {
    <a href="nft_safe.md#0x2_nft_safe_assert_owner_cap">assert_owner_cap</a>(self, owner_cap);
    <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self, &nft_id);

    <b>let</b> ref = <a href="vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.refs, &nft_id);
    <a href="nft_safe.md#0x2_nft_safe_assert_not_listed">assert_not_listed</a>(ref);

    <a href="vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(
        &<b>mut</b> ref.listed_with, <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(entity_id), min_price,
    );
    ref.is_exclusively_listed = <b>true</b>;
}
</code></pre>



</details>

<a name="0x2_nft_safe_purchase_as_entity"></a>

## Function `purchase_as_entity`

An entity uses the <code>&UID</code> as a token which has been granted a permission
for transfer of the specific NFT.
With this token, a transfer can be performed.

This function returns a hot potato which must be passed around and
finally destroyed in <code>allow_transfer</code>.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_purchase_as_entity">purchase_as_entity</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, entity_id: &<a href="object.md#0x2_object_UID">object::UID</a>, nft_id: <a href="object.md#0x2_object_ID">object::ID</a>, payment: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;): (T, <a href="nft_safe.md#0x2_nft_safe_TransferRequest">nft_safe::TransferRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_purchase_as_entity">purchase_as_entity</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>,
    entity_id: &UID,
    nft_id: ID,
    payment: Coin&lt;SUI&gt;,
): (T, <a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt;) {
    <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self, &nft_id);

    // NFT is being transferred - destroy the ref
    <b>let</b> (_, ref) = <a href="vec_map.md#0x2_vec_map_remove">vec_map::remove</a>(&<b>mut</b> self.refs, &nft_id);
    <b>let</b> listed_for = *<a href="_borrow">option::borrow</a>(&ref.listed_for);
    <b>let</b> paid = <a href="coin.md#0x2_coin_value">coin::value</a>(&payment);
    <b>assert</b>!(paid &gt;= listed_for, <a href="nft_safe.md#0x2_nft_safe_ENotEnough">ENotEnough</a>);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.profits, <a href="coin.md#0x2_coin_into_balance">coin::into_balance</a>(payment));

    // aborts <b>if</b> entity is not included in the map
    <b>let</b> entity_auth = <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(entity_id);
    <a href="vec_map.md#0x2_vec_map_remove">vec_map::remove</a>(&<b>mut</b> ref.listed_with, &entity_auth);

    <b>let</b> nft = dof::remove&lt;ID, T&gt;(&<b>mut</b> self.id, nft_id);
    (nft, <a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt; {
        paid,
        <a href="safe.md#0x2_safe">safe</a>: <a href="object.md#0x2_object_id">object::id</a>(self),
        entity: <a href="_none">option::none</a>(),
        signatures: <a href="vec_set.md#0x2_vec_set_empty">vec_set::empty</a>(),
    })
}
</code></pre>



</details>

<a name="0x2_nft_safe_get_nft_as_owner"></a>

## Function `get_nft_as_owner`

Get an NFT out of the safe as the owner.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_get_nft_as_owner">get_nft_as_owner</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>, nft_id: <a href="object.md#0x2_object_ID">object::ID</a>): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_get_nft_as_owner">get_nft_as_owner</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>,
    owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>,
    nft_id: ID,
): T {
    <a href="nft_safe.md#0x2_nft_safe_assert_owner_cap">assert_owner_cap</a>(self, owner_cap);
    <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self, &nft_id);

    // NFT is being transferred - destroy the ref
    <b>let</b> (_, ref) = <a href="vec_map.md#0x2_vec_map_remove">vec_map::remove</a>(&<b>mut</b> self.refs, &nft_id);
    <a href="nft_safe.md#0x2_nft_safe_assert_ref_not_exclusively_listed">assert_ref_not_exclusively_listed</a>(&ref);

    dof::remove&lt;ID, T&gt;(&<b>mut</b> self.id, nft_id)
}
</code></pre>



</details>

<a name="0x2_nft_safe_remove_entity_from_nft_listing"></a>

## Function `remove_entity_from_nft_listing`

An entity can remove itself from accessing (ie. delist) an NFT.

This method is the only way an exclusive listing can be delisted.


<a name="@Aborts_8"></a>

### Aborts

* If the entity is not listed as an auth for this NFT.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_remove_entity_from_nft_listing">remove_entity_from_nft_listing</a>(self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, entity_id: &<a href="object.md#0x2_object_UID">object::UID</a>, nft_id: &<a href="object.md#0x2_object_ID">object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_remove_entity_from_nft_listing">remove_entity_from_nft_listing</a>(
    self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>,
    entity_id: &UID,
    nft_id: &ID,
) {
    <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self, nft_id);

    <b>let</b> ref = <a href="vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.refs, nft_id);
    // aborts <b>if</b> the entity is not in the map
    <b>let</b> entity_auth = <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(entity_id);
    <a href="vec_map.md#0x2_vec_map_remove">vec_map::remove</a>(&<b>mut</b> ref.listed_with, &entity_auth);
    ref.is_exclusively_listed = <b>false</b>; // no-op unless it was exclusive
}
</code></pre>



</details>

<a name="0x2_nft_safe_remove_entity_from_nft_listing_as_owner"></a>

## Function `remove_entity_from_nft_listing_as_owner`

The safe owner can remove an entity from accessing an NFT unless
it's listed exclusively.
An exclusive listing can be canceled only via
<code>remove_auth_from_nft_listing</code>.


<a name="@Aborts_9"></a>

### Aborts

* If the NFT is exclusively listed.
* If the entity is not listed as an auth for this NFT.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_remove_entity_from_nft_listing_as_owner">remove_entity_from_nft_listing_as_owner</a>(self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>, entity_id: &<a href="object.md#0x2_object_ID">object::ID</a>, nft_id: &<a href="object.md#0x2_object_ID">object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_remove_entity_from_nft_listing_as_owner">remove_entity_from_nft_listing_as_owner</a>(
    self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>,
    owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>,
    entity_id: &ID,
    nft_id: &ID,
) {
    <a href="nft_safe.md#0x2_nft_safe_assert_owner_cap">assert_owner_cap</a>(self, owner_cap);
    <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self, nft_id);

    <b>let</b> ref = <a href="vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.refs, nft_id);
    <a href="nft_safe.md#0x2_nft_safe_assert_ref_not_exclusively_listed">assert_ref_not_exclusively_listed</a>(ref);
    // aborts <b>if</b> the entity is not in the map
    <a href="vec_map.md#0x2_vec_map_remove">vec_map::remove</a>(&<b>mut</b> ref.listed_with, entity_id);
}
</code></pre>



</details>

<a name="0x2_nft_safe_delist_nft"></a>

## Function `delist_nft`

Removes all access to an NFT.
An exclusive listing can be canceled only via
<code>remove_auth_from_nft_listing</code>.


<a name="@Aborts_10"></a>

### Aborts

* If the NFT is exclusively listed.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_delist_nft">delist_nft</a>(self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>, nft_id: &<a href="object.md#0x2_object_ID">object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_delist_nft">delist_nft</a>(
     self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>,
     owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>,
     nft_id: &ID,
 ) {
     <a href="nft_safe.md#0x2_nft_safe_assert_owner_cap">assert_owner_cap</a>(self, owner_cap);
     <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self, nft_id);

     <b>let</b> ref = <a href="vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.refs, nft_id);
     <a href="nft_safe.md#0x2_nft_safe_assert_ref_not_exclusively_listed">assert_ref_not_exclusively_listed</a>(ref);
     ref.listed_with = <a href="vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();
 }
</code></pre>



</details>

<a name="0x2_nft_safe_destroy_empty"></a>

## Function `destroy_empty`

If there are no deposited NFTs in the safe, the safe is destroyed.
Only works for non-shared safes.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_destroy_empty">destroy_empty</a>(self: <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, owner_cap: <a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_destroy_empty">destroy_empty</a>(
    self: <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>, owner_cap: <a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>, ctx: &<b>mut</b> TxContext,
): Coin&lt;SUI&gt; {
    <a href="nft_safe.md#0x2_nft_safe_assert_owner_cap">assert_owner_cap</a>(&self, &owner_cap);
    <b>assert</b>!(<a href="vec_map.md#0x2_vec_map_is_empty">vec_map::is_empty</a>(&self.refs), <a href="nft_safe.md#0x2_nft_safe_EMustBeEmpty">EMustBeEmpty</a>);

    <b>let</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a> {
        id, refs, profits, ecosystem: _, owner_cap_id: _,
    } = self;
    <b>let</b> <a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a> { id: cap_id, <a href="safe.md#0x2_safe">safe</a>: _ } = owner_cap;
    <a href="vec_map.md#0x2_vec_map_destroy_empty">vec_map::destroy_empty</a>(refs);
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
    <a href="object.md#0x2_object_delete">object::delete</a>(cap_id);

    <a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(profits, ctx)
}
</code></pre>



</details>

<a name="0x2_nft_safe_withdraw_profits"></a>

## Function `withdraw_profits`

Withdraws profits from the safe.
If <code>amount</code> is <code>none</code>, withdraws all profits.
Otherwise attempts to withdraw the specified amount.
Fails if there are not enough token.


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_withdraw_profits">withdraw_profits</a>(self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>, amount: <a href="_Option">option::Option</a>&lt;u64&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_withdraw_profits">withdraw_profits</a>(
    self: &<b>mut</b> <a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>,
    owner_cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>,
    amount: Option&lt;u64&gt;,
    ctx: &<b>mut</b> TxContext,
): Coin&lt;SUI&gt; {
    <a href="nft_safe.md#0x2_nft_safe_assert_owner_cap">assert_owner_cap</a>(self, owner_cap);

    <b>let</b> amount = <b>if</b> (<a href="_is_some">option::is_some</a>(&amount)) {
        <b>let</b> amt = <a href="_destroy_some">option::destroy_some</a>(amount);
        <b>assert</b>!(amt &lt;= <a href="balance.md#0x2_balance_value">balance::value</a>(&self.profits), <a href="nft_safe.md#0x2_nft_safe_ENotEnough">ENotEnough</a>);
        amt
    } <b>else</b> {
        <a href="balance.md#0x2_balance_value">balance::value</a>(&self.profits)
    };

    <a href="coin.md#0x2_coin_take">coin::take</a>(&<b>mut</b> self.profits, amount, ctx)
}
</code></pre>



</details>

<a name="0x2_nft_safe_ecosystem"></a>

## Function `ecosystem`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_ecosystem">ecosystem</a>(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>): &<a href="_Option">option::Option</a>&lt;<a href="_String">ascii::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_ecosystem">ecosystem</a>(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>): &Option&lt;<a href="_String">ascii::String</a>&gt; { &self.ecosystem }
</code></pre>



</details>

<a name="0x2_nft_safe_nfts_count"></a>

## Function `nfts_count`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_nfts_count">nfts_count</a>(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_nfts_count">nfts_count</a>(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>): u64 { <a href="vec_map.md#0x2_vec_map_size">vec_map::size</a>(&self.refs) }
</code></pre>



</details>

<a name="0x2_nft_safe_borrow_nft"></a>

## Function `borrow_nft`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_borrow_nft">borrow_nft</a>&lt;T: store, key&gt;(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, nft_id: <a href="object.md#0x2_object_ID">object::ID</a>): &T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_borrow_nft">borrow_nft</a>&lt;T: key + store&gt;(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>, nft_id: ID): &T {
    <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self, &nft_id);
    dof::borrow&lt;ID, T&gt;(&self.id, nft_id)
}
</code></pre>



</details>

<a name="0x2_nft_safe_has_nft"></a>

## Function `has_nft`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_has_nft">has_nft</a>&lt;T: store, key&gt;(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, nft_id: <a href="object.md#0x2_object_ID">object::ID</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_has_nft">has_nft</a>&lt;T: key + store&gt;(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>, nft_id: ID): bool {
    dof::exists_with_type&lt;ID, T&gt;(&self.id, nft_id)
}
</code></pre>



</details>

<a name="0x2_nft_safe_owner_cap_safe"></a>

## Function `owner_cap_safe`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_owner_cap_safe">owner_cap_safe</a>(cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>): <a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_owner_cap_safe">owner_cap_safe</a>(cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>): ID { cap.<a href="safe.md#0x2_safe">safe</a> }
</code></pre>



</details>

<a name="0x2_nft_safe_transfer_request_paid"></a>

## Function `transfer_request_paid`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_transfer_request_paid">transfer_request_paid</a>&lt;T&gt;(req: &<a href="nft_safe.md#0x2_nft_safe_TransferRequest">nft_safe::TransferRequest</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_transfer_request_paid">transfer_request_paid</a>&lt;T&gt;(req: &<a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt;): u64 { req.paid }
</code></pre>



</details>

<a name="0x2_nft_safe_transfer_request_safe"></a>

## Function `transfer_request_safe`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_transfer_request_safe">transfer_request_safe</a>&lt;T&gt;(req: &<a href="nft_safe.md#0x2_nft_safe_TransferRequest">nft_safe::TransferRequest</a>&lt;T&gt;): <a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_transfer_request_safe">transfer_request_safe</a>&lt;T&gt;(req: &<a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt;): ID { req.<a href="safe.md#0x2_safe">safe</a> }
</code></pre>



</details>

<a name="0x2_nft_safe_transfer_request_entity"></a>

## Function `transfer_request_entity`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_transfer_request_entity">transfer_request_entity</a>&lt;T&gt;(req: &<a href="nft_safe.md#0x2_nft_safe_TransferRequest">nft_safe::TransferRequest</a>&lt;T&gt;): <a href="_Option">option::Option</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_transfer_request_entity">transfer_request_entity</a>&lt;T&gt;(req: &<a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt;): Option&lt;ID&gt; { req.entity }
</code></pre>



</details>

<a name="0x2_nft_safe_transfer_request_signatures"></a>

## Function `transfer_request_signatures`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_transfer_request_signatures">transfer_request_signatures</a>&lt;T&gt;(req: &<a href="nft_safe.md#0x2_nft_safe_TransferRequest">nft_safe::TransferRequest</a>&lt;T&gt;): <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_transfer_request_signatures">transfer_request_signatures</a>&lt;T&gt;(req: &<a href="nft_safe.md#0x2_nft_safe_TransferRequest">TransferRequest</a>&lt;T&gt;): VecSet&lt;ID&gt; {
    req.signatures
}
</code></pre>



</details>

<a name="0x2_nft_safe_assert_owner_cap"></a>

## Function `assert_owner_cap`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_assert_owner_cap">assert_owner_cap</a>(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">nft_safe::OwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_assert_owner_cap">assert_owner_cap</a>(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>, cap: &<a href="nft_safe.md#0x2_nft_safe_OwnerCap">OwnerCap</a>) {
    <b>assert</b>!(cap.<a href="safe.md#0x2_safe">safe</a> == <a href="object.md#0x2_object_id">object::id</a>(self), <a href="nft_safe.md#0x2_nft_safe_ESafeOwnerMismatch">ESafeOwnerMismatch</a>);
}
</code></pre>



</details>

<a name="0x2_nft_safe_assert_has_nft"></a>

## Function `assert_has_nft`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, nft: &<a href="object.md#0x2_object_ID">object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_assert_has_nft">assert_has_nft</a>(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>, nft: &ID) {
    <b>assert</b>!(<a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&self.refs, nft), <a href="nft_safe.md#0x2_nft_safe_ESafeDoesNotContainNft">ESafeDoesNotContainNft</a>);
}
</code></pre>



</details>

<a name="0x2_nft_safe_assert_not_exclusively_listed"></a>

## Function `assert_not_exclusively_listed`



<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_assert_not_exclusively_listed">assert_not_exclusively_listed</a>(self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">nft_safe::NftSafe</a>, nft: &<a href="object.md#0x2_object_ID">object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nft_safe.md#0x2_nft_safe_assert_not_exclusively_listed">assert_not_exclusively_listed</a>(
    self: &<a href="nft_safe.md#0x2_nft_safe_NftSafe">NftSafe</a>, nft: &ID
) {
    <b>let</b> ref = <a href="vec_map.md#0x2_vec_map_get">vec_map::get</a>(&self.refs, nft);
    <a href="nft_safe.md#0x2_nft_safe_assert_ref_not_exclusively_listed">assert_ref_not_exclusively_listed</a>(ref);
}
</code></pre>



</details>

<a name="0x2_nft_safe_assert_ref_not_exclusively_listed"></a>

## Function `assert_ref_not_exclusively_listed`



<pre><code><b>fun</b> <a href="nft_safe.md#0x2_nft_safe_assert_ref_not_exclusively_listed">assert_ref_not_exclusively_listed</a>(ref: &<a href="nft_safe.md#0x2_nft_safe_NftRef">nft_safe::NftRef</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="nft_safe.md#0x2_nft_safe_assert_ref_not_exclusively_listed">assert_ref_not_exclusively_listed</a>(ref: &<a href="nft_safe.md#0x2_nft_safe_NftRef">NftRef</a>) {
    <b>assert</b>!(!ref.is_exclusively_listed, <a href="nft_safe.md#0x2_nft_safe_ENftAlreadyExclusivelyListed">ENftAlreadyExclusivelyListed</a>);
}
</code></pre>



</details>

<a name="0x2_nft_safe_assert_not_listed"></a>

## Function `assert_not_listed`



<pre><code><b>fun</b> <a href="nft_safe.md#0x2_nft_safe_assert_not_listed">assert_not_listed</a>(ref: &<a href="nft_safe.md#0x2_nft_safe_NftRef">nft_safe::NftRef</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="nft_safe.md#0x2_nft_safe_assert_not_listed">assert_not_listed</a>(ref: &<a href="nft_safe.md#0x2_nft_safe_NftRef">NftRef</a>) {
    <b>assert</b>!(<a href="vec_map.md#0x2_vec_map_size">vec_map::size</a>(&ref.listed_with) == 0, <a href="nft_safe.md#0x2_nft_safe_ENftAlreadyListed">ENftAlreadyListed</a>);
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(&ref.listed_for), <a href="nft_safe.md#0x2_nft_safe_ENftAlreadyListed">ENftAlreadyListed</a>);
}
</code></pre>



</details>
