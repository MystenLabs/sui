
<a name="0x2_kiosk"></a>

# Module `0x2::kiosk`

Kiosk is a primitive and an open, zero-fee trading platform
for assets with high degree of customization over transfer
policies.

The system has 3 main audiences:

1. Creators: for a type to be tradable in the Kiosk ecosystem,
creator (publisher) of the type needs to issue a <code><a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a></code>
which gives them a power to enforce any constraint on trades by
either using one of the pre-built primitives (see <code>sui::royalty</code>)
or by implementing a custom policy. The latter requires additional
support for discoverability in the ecosystem and should be performed
within the scope of an Application or some platform.

- A type can not be traded in the Kiosk unless there's a policy for it.
- 0-royalty policy is just as easy as "freezing" the <code>AllowTransferCap</code>
making it available for everyone to authorize deals "for free"

2. Traders: anyone can create a Kiosk and depending on whether it's
a shared object or some shared-wrapper the owner can trade any type
that has issued <code><a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a></code> in a Kiosk. To do so, they need
to make an offer, and any party can purchase the item for the amount of
SUI set in the offer. The responsibility to follow the transfer policy
set by the creator of the <code>T</code> is on the buyer.

3. Marketplaces: marketplaces can either watch for the offers made in
personal Kiosks or even integrate the Kiosk primitive and build on top
of it. In the custom logic scenario, the <code><a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a></code> can also
be used to implement application-specific transfer rules.


-  [Resource `Kiosk`](#0x2_kiosk_Kiosk)
-  [Resource `KioskOwnerCap`](#0x2_kiosk_KioskOwnerCap)
-  [Struct `TransferRequest`](#0x2_kiosk_TransferRequest)
-  [Resource `TransferPolicyCap`](#0x2_kiosk_TransferPolicyCap)
-  [Struct `Key`](#0x2_kiosk_Key)
-  [Struct `Offer`](#0x2_kiosk_Offer)
-  [Struct `NewOfferEvent`](#0x2_kiosk_NewOfferEvent)
-  [Struct `TransferPolicyCapIssued`](#0x2_kiosk_TransferPolicyCapIssued)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_kiosk_new)
-  [Function `close_and_withdraw`](#0x2_kiosk_close_and_withdraw)
-  [Function `set_owner`](#0x2_kiosk_set_owner)
-  [Function `new_transfer_policy_cap`](#0x2_kiosk_new_transfer_policy_cap)
-  [Function `new_transfer_policy_cap_protected`](#0x2_kiosk_new_transfer_policy_cap_protected)
-  [Function `destroy_transfer_policy_cap`](#0x2_kiosk_destroy_transfer_policy_cap)
-  [Function `place`](#0x2_kiosk_place)
-  [Function `take`](#0x2_kiosk_take)
-  [Function `make_offer`](#0x2_kiosk_make_offer)
-  [Function `place_and_offer`](#0x2_kiosk_place_and_offer)
-  [Function `purchase`](#0x2_kiosk_purchase)
-  [Function `allow_transfer`](#0x2_kiosk_allow_transfer)
-  [Function `withdraw`](#0x2_kiosk_withdraw)
-  [Function `owner`](#0x2_kiosk_owner)
-  [Function `item_count`](#0x2_kiosk_item_count)
-  [Function `profits_amount`](#0x2_kiosk_profits_amount)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field">0x2::dynamic_object_field</a>;
<b>use</b> <a href="event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="package.md#0x2_package">0x2::package</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_kiosk_Kiosk"></a>

## Resource `Kiosk`

An object that stores collectibles of all sorts.
For sale, for collecting reasons, for fun.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a> <b>has</b> store, key
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
<code>profits: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>
 Balance of the Kiosk - all profits from sales go here.
</dd>
<dt>
<code>owner: <b>address</b></code>
</dt>
<dd>
 Always point to <code>sender</code> of the transaction.
 Can be changed by calling <code>set_owner</code> with Cap.
</dd>
<dt>
<code>item_count: u32</code>
</dt>
<dd>
 Number of items stored in a Kiosk. Used to allow unpacking
 an empty Kiosk if it was wrapped or has a single owner.
</dd>
</dl>


</details>

<a name="0x2_kiosk_KioskOwnerCap"></a>

## Resource `KioskOwnerCap`

A capability that is issued for Kiosks that don't have owner
specified.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a> <b>has</b> store, key
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
<code>for: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_kiosk_TransferRequest"></a>

## Struct `TransferRequest`

A "Hot Potato" forcing the buyer to get a transfer permission
from the item type (<code>T</code>) owner on purchase attempt.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a>&lt;T: store, key&gt;
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
<code>from: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>
 The ID of the Kiosk the object is being sold from.
 Can be used by the TransferPolicy implementors to
 create an allowlist of Kiosks which can trade the type.
</dd>
</dl>


</details>

<a name="0x2_kiosk_TransferPolicyCap"></a>

## Resource `TransferPolicyCap`

A unique capability that allows owner of the <code>T</code> to authorize
transfers. Can only be created with the <code>Publisher</code> object.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a>&lt;T: store, key&gt; <b>has</b> store, key
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

<a name="0x2_kiosk_Key"></a>

## Struct `Key`

Dynamic field key for an item placed into the kiosk.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_Key">Key</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_kiosk_Offer"></a>

## Struct `Offer`

Dynamic field key for an active offer to purchase the T.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_Offer">Offer</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_kiosk_NewOfferEvent"></a>

## Struct `NewOfferEvent`

Emitted when an item was listed by the safe owner. Can be used
to track available offers anywhere on the network; the event is
type-indexed which allows for searching for offers of a specific <code>T</code>


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_NewOfferEvent">NewOfferEvent</a>&lt;T: store, key&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="kiosk.md#0x2_kiosk">kiosk</a>: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
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

<a name="0x2_kiosk_TransferPolicyCapIssued"></a>

## Struct `TransferPolicyCapIssued`

Emitted when a publisher creates a new <code><a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a></code> making
the discoverability and tracking the supported types easier.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_TransferPolicyCapIssued">TransferPolicyCapIssued</a>&lt;T: store, key&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_kiosk_ENotEnough"></a>

For when trying to withdraw higher amount than stored.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_ENotEnough">ENotEnough</a>: u64 = 5;
</code></pre>



<a name="0x2_kiosk_EIncorrectAmount"></a>

For when Coin paid does not match the offer price.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EIncorrectAmount">EIncorrectAmount</a>: u64 = 2;
</code></pre>



<a name="0x2_kiosk_EIncorrectArgument"></a>

For when incorrect arguments passed into <code>switch_mode</code> function.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EIncorrectArgument">EIncorrectArgument</a>: u64 = 3;
</code></pre>



<a name="0x2_kiosk_ENotEmpty"></a>

For when trying to close a Kiosk and it has items in it.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_ENotEmpty">ENotEmpty</a>: u64 = 6;
</code></pre>



<a name="0x2_kiosk_ENotOwner"></a>

For when trying to withdraw profits and sender is not owner.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>: u64 = 1;
</code></pre>



<a name="0x2_kiosk_EOwnerNotSet"></a>

For when trying to withdraw profits as owner and owner is not set.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EOwnerNotSet">EOwnerNotSet</a>: u64 = 0;
</code></pre>



<a name="0x2_kiosk_EWrongTarget"></a>

For when Transfer is accepted by a wrong Kiosk.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EWrongTarget">EWrongTarget</a>: u64 = 4;
</code></pre>



<a name="0x2_kiosk_new"></a>

## Function `new`

Creates a new Kiosk without owner but with a Capability.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_new">new</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_new">new</a>(ctx: &<b>mut</b> TxContext): (<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>) {
    <b>let</b> <a href="kiosk.md#0x2_kiosk">kiosk</a> = <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        profits: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
        owner: sender(ctx),
        item_count: 0
    };

    <b>let</b> cap = <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        for: <a href="object.md#0x2_object_id">object::id</a>(&<a href="kiosk.md#0x2_kiosk">kiosk</a>)
    };

    (<a href="kiosk.md#0x2_kiosk">kiosk</a>, cap)
}
</code></pre>



</details>

<a name="0x2_kiosk_close_and_withdraw"></a>

## Function `close_and_withdraw`

Unpacks and destroys a Kiosk returning the profits (even if "0").
Can only be performed by the bearer of the <code><a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a></code> in the
case where there's no items inside and a <code><a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a></code> is not shared.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_close_and_withdraw">close_and_withdraw</a>(self: <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_close_and_withdraw">close_and_withdraw</a>(
    self: <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, ctx: &<b>mut</b> TxContext
): Coin&lt;SUI&gt; {
    <b>let</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a> { id, profits, owner: _, item_count } = self;
    <b>let</b> <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a> { id: cap_id, for } = cap;

    <b>assert</b>!(<a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&id) == for, <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(item_count == 0, <a href="kiosk.md#0x2_kiosk_ENotEmpty">ENotEmpty</a>);

    <a href="object.md#0x2_object_delete">object::delete</a>(cap_id);
    <a href="object.md#0x2_object_delete">object::delete</a>(id);

    <a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(profits, ctx)
}
</code></pre>



</details>

<a name="0x2_kiosk_set_owner"></a>

## Function `set_owner`

Change the owner to the transaction sender.
The change is purely cosmetical and does not affect any of the
basic kiosk functions unless some logic for this is implemented
in a third party module.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_set_owner">set_owner</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_set_owner">set_owner</a>(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, ctx: &TxContext
) {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    self.owner = sender(ctx);
}
</code></pre>



</details>

<a name="0x2_kiosk_new_transfer_policy_cap"></a>

## Function `new_transfer_policy_cap`

Register a type in the Kiosk system and receive an <code><a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a></code>
which is required to confirm kiosk deals for the <code>T</code>. If there's no
<code><a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a></code> available for use, the type can not be traded in
kiosks.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_new_transfer_policy_cap">new_transfer_policy_cap</a>&lt;T: store, key&gt;(pub: &<a href="package.md#0x2_package_Publisher">package::Publisher</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">kiosk::TransferPolicyCap</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_new_transfer_policy_cap">new_transfer_policy_cap</a>&lt;T: key + store&gt;(
    pub: &Publisher, ctx: &<b>mut</b> TxContext
): <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a>&lt;T&gt; {
    <b>assert</b>!(<a href="package.md#0x2_package_from_package">package::from_package</a>&lt;T&gt;(pub), 0);
    <b>let</b> id = <a href="object.md#0x2_object_new">object::new</a>(ctx);
    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="kiosk.md#0x2_kiosk_TransferPolicyCapIssued">TransferPolicyCapIssued</a>&lt;T&gt; { id: <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&id) });
    <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a> { id }
}
</code></pre>



</details>

<a name="0x2_kiosk_new_transfer_policy_cap_protected"></a>

## Function `new_transfer_policy_cap_protected`

Special case for the <code>sui::collectible</code> module to be able to register
type without a <code>Publisher</code> object. Is not magical and a similar logic
can be implemented for the regular <code>register_type</code> call for wrapped types.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="kiosk.md#0x2_kiosk_new_transfer_policy_cap_protected">new_transfer_policy_cap_protected</a>&lt;T: store, key&gt;(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">kiosk::TransferPolicyCap</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="kiosk.md#0x2_kiosk_new_transfer_policy_cap_protected">new_transfer_policy_cap_protected</a>&lt;T: key + store&gt;(
    ctx: &<b>mut</b> TxContext
): <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a>&lt;T&gt; {
    <b>let</b> id = <a href="object.md#0x2_object_new">object::new</a>(ctx);
    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="kiosk.md#0x2_kiosk_TransferPolicyCapIssued">TransferPolicyCapIssued</a>&lt;T&gt; { id: <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&id) });
    <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a> { id }
}
</code></pre>



</details>

<a name="0x2_kiosk_destroy_transfer_policy_cap"></a>

## Function `destroy_transfer_policy_cap`

Destroy a TransferPolicyCap.
Can be performed by any party as long as they own it.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_destroy_transfer_policy_cap">destroy_transfer_policy_cap</a>&lt;T: store, key&gt;(cap: <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">kiosk::TransferPolicyCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_destroy_transfer_policy_cap">destroy_transfer_policy_cap</a>&lt;T: key + store&gt;(
    cap: <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a>&lt;T&gt;
) {
    <b>let</b> <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a> { id } = cap;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
}
</code></pre>



</details>

<a name="0x2_kiosk_place"></a>

## Function `place`

Place any object into a Kiosk.
Performs an authorization check to make sure only owner can do that.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_place">place</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, item: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_place">place</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, item: T
) {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    self.item_count = self.item_count + 1;
    dof::add(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Key">Key</a> { id: <a href="object.md#0x2_object_id">object::id</a>(&item) }, item)
}
</code></pre>



</details>

<a name="0x2_kiosk_take"></a>

## Function `take`

Take any object from the Kiosk.
Performs an authorization check to make sure only owner can do that.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_take">take</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_take">take</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, id: ID
): T {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    self.item_count = self.item_count - 1;
    df::remove_if_exists&lt;<a href="kiosk.md#0x2_kiosk_Offer">Offer</a>, u64&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Offer">Offer</a> { id });
    dof::remove(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Key">Key</a> { id })
}
</code></pre>



</details>

<a name="0x2_kiosk_make_offer"></a>

## Function `make_offer`

Make an offer by setting a price for the item and making it publicly
purchasable by anyone on the network.

Performs an authorization check to make sure only owner can sell.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_make_offer">make_offer</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>, price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_make_offer">make_offer</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, id: ID, price: u64
) {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    df::add(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Offer">Offer</a> { id }, price);
    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="kiosk.md#0x2_kiosk_NewOfferEvent">NewOfferEvent</a>&lt;T&gt; {
        <a href="kiosk.md#0x2_kiosk">kiosk</a>: <a href="object.md#0x2_object_id">object::id</a>(self), id, price
    })
}
</code></pre>



</details>

<a name="0x2_kiosk_place_and_offer"></a>

## Function `place_and_offer`

Place an item into the Kiosk and make an offer - simplifies the flow.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_place_and_offer">place_and_offer</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, item: T, price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_place_and_offer">place_and_offer</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, item: T, price: u64
) {
    <b>let</b> id = <a href="object.md#0x2_object_id">object::id</a>(&item);
    <a href="kiosk.md#0x2_kiosk_place">place</a>(self, cap, item);
    <a href="kiosk.md#0x2_kiosk_make_offer">make_offer</a>&lt;T&gt;(self, cap, id, price)
}
</code></pre>



</details>

<a name="0x2_kiosk_purchase"></a>

## Function `purchase`

Make a trade: pay the owner of the item and request a Transfer to the <code>target</code>
kiosk (to prevent item being taken by the approving party).

Received <code><a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a></code> needs to be handled by the publisher of the T,
if they have a method implemented that allows a trade, it is possible to
request their approval (by calling some function) so that the trade can be
finalized.

After a confirmation is received from the creator, an item can be placed to
a destination safe.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase">purchase</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>, payment: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;): (T, <a href="kiosk.md#0x2_kiosk_TransferRequest">kiosk::TransferRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase">purchase</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, id: ID, payment: Coin&lt;SUI&gt;
): (T, <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a>&lt;T&gt;) {
    <b>let</b> price = df::remove&lt;<a href="kiosk.md#0x2_kiosk_Offer">Offer</a>, u64&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Offer">Offer</a> { id });
    <b>let</b> inner = dof::remove&lt;<a href="kiosk.md#0x2_kiosk_Key">Key</a>, T&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Key">Key</a> { id });

    self.item_count = self.item_count - 1;
    <b>assert</b>!(price == <a href="coin.md#0x2_coin_value">coin::value</a>(&payment), <a href="kiosk.md#0x2_kiosk_EIncorrectAmount">EIncorrectAmount</a>);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.profits, <a href="coin.md#0x2_coin_into_balance">coin::into_balance</a>(payment));

    (inner, <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a>&lt;T&gt; {
        paid: price,
        from: <a href="object.md#0x2_object_id">object::id</a>(self),
    })
}
</code></pre>



</details>

<a name="0x2_kiosk_allow_transfer"></a>

## Function `allow_transfer`

Allow a <code><a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a></code> for the type <code>T</code>. The call is protected
by the type constraint, as only the publisher of the <code>T</code> can get
<code><a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a>&lt;T&gt;</code>.

Note: unless there's a policy for <code>T</code> to allow transfers,
Kiosk trades will not be possible.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_allow_transfer">allow_transfer</a>&lt;T: store, key&gt;(_cap: &<a href="kiosk.md#0x2_kiosk_TransferPolicyCap">kiosk::TransferPolicyCap</a>&lt;T&gt;, req: <a href="kiosk.md#0x2_kiosk_TransferRequest">kiosk::TransferRequest</a>&lt;T&gt;): (u64, <a href="object.md#0x2_object_ID">object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_allow_transfer">allow_transfer</a>&lt;T: key + store&gt;(
    _cap: &<a href="kiosk.md#0x2_kiosk_TransferPolicyCap">TransferPolicyCap</a>&lt;T&gt;, req: <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a>&lt;T&gt;
): (u64, ID) {
    <b>let</b> <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a> { paid, from } = req;
    (paid, from)
}
</code></pre>



</details>

<a name="0x2_kiosk_withdraw"></a>

## Function `withdraw`

Withdraw profits from the Kiosk.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_withdraw">withdraw</a>(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>, amount: <a href="_Option">option::Option</a>&lt;u64&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_withdraw">withdraw</a>(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>, amount: Option&lt;u64&gt;, ctx: &<b>mut</b> TxContext
): Coin&lt;SUI&gt; {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);

    <b>let</b> amount = <b>if</b> (<a href="_is_some">option::is_some</a>(&amount)) {
        <b>let</b> amt = <a href="_destroy_some">option::destroy_some</a>(amount);
        <b>assert</b>!(amt &lt;= <a href="balance.md#0x2_balance_value">balance::value</a>(&self.profits), <a href="kiosk.md#0x2_kiosk_ENotEnough">ENotEnough</a>);
        amt
    } <b>else</b> {
        <a href="balance.md#0x2_balance_value">balance::value</a>(&self.profits)
    };

    <a href="coin.md#0x2_coin_take">coin::take</a>(&<b>mut</b> self.profits, amount, ctx)
}
</code></pre>



</details>

<a name="0x2_kiosk_owner"></a>

## Function `owner`

Get the owner of the Kiosk.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_owner">owner</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_owner">owner</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>): <b>address</b> {
    self.owner
}
</code></pre>



</details>

<a name="0x2_kiosk_item_count"></a>

## Function `item_count`

Get the number of items stored in a Kiosk.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_item_count">item_count</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>): u32
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_item_count">item_count</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>): u32 {
    self.item_count
}
</code></pre>



</details>

<a name="0x2_kiosk_profits_amount"></a>

## Function `profits_amount`

Get the amount of profits collected by selling items.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_profits_amount">profits_amount</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_profits_amount">profits_amount</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>): u64 {
    <a href="balance.md#0x2_balance_value">balance::value</a>(&self.profits)
}
</code></pre>



</details>
