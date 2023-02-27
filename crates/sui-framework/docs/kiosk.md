
<a name="0x2_kiosk"></a>

# Module `0x2::kiosk`

Ownership modes:
- either the <code><a href="kiosk.md#0x2_kiosk">kiosk</a>.owner</code> is set - address owner;
- or a Cap is issued;
- mode can be changed at any point by its owner / capability bearer.



-  [Struct `TransferRequest`](#0x2_kiosk_TransferRequest)
-  [Struct `ApprovedTransfer`](#0x2_kiosk_ApprovedTransfer)
-  [Resource `AllowTransferCap`](#0x2_kiosk_AllowTransferCap)
-  [Resource `Kiosk`](#0x2_kiosk_Kiosk)
-  [Resource `KioskOwnerCap`](#0x2_kiosk_KioskOwnerCap)
-  [Struct `Key`](#0x2_kiosk_Key)
-  [Struct `Offer`](#0x2_kiosk_Offer)
-  [Constants](#@Constants_0)
-  [Function `new_for_sender`](#0x2_kiosk_new_for_sender)
-  [Function `new_with_capability`](#0x2_kiosk_new_with_capability)
-  [Function `switch_mode`](#0x2_kiosk_switch_mode)
-  [Function `create_allow_transfer_cap`](#0x2_kiosk_create_allow_transfer_cap)
-  [Function `place`](#0x2_kiosk_place)
-  [Function `take`](#0x2_kiosk_take)
-  [Function `make_offer`](#0x2_kiosk_make_offer)
-  [Function `purchase`](#0x2_kiosk_purchase)
-  [Function `allow`](#0x2_kiosk_allow)
-  [Function `store_purchased`](#0x2_kiosk_store_purchased)
-  [Function `withdraw`](#0x2_kiosk_withdraw)
-  [Function `try_access`](#0x2_kiosk_try_access)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field">0x2::dynamic_object_field</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="publisher.md#0x2_publisher">0x2::publisher</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_kiosk_TransferRequest"></a>

## Struct `TransferRequest`

A Hot Potato making sure the buyer gets an authorization
from the owner of the T to perform a transfer after a purchase.

Contains the amount paid for the <code>T</code> so the commission could be
calculated; <code>from</code> field contains the seller of the <code>T</code>.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a>&lt;T: store, key&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>inner: T</code>
</dt>
<dd>

</dd>
<dt>
<code>paid: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>from: <a href="_Option">option::Option</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_kiosk_ApprovedTransfer"></a>

## Struct `ApprovedTransfer`

Another Hot Potato returned by the <code>allow</code> method. Makes sure
that the object goes into a Safe.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_ApprovedTransfer">ApprovedTransfer</a>&lt;T: store, key&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>inner: T</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_kiosk_AllowTransferCap"></a>

## Resource `AllowTransferCap`

A unique capability that allows owner of the <code>T</code> to authorize
transfers. Can only be created with the <code>Publisher</code> object.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_AllowTransferCap">AllowTransferCap</a>&lt;T: store, key&gt; <b>has</b> store, key
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

</dd>
<dt>
<code>owner: <a href="_Option">option::Option</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>

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

<a name="@Constants_0"></a>

## Constants


<a name="0x2_kiosk_ENotOwner"></a>

For when trying to withdraw profits and sender is not owner.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>: u64 = 1;
</code></pre>



<a name="0x2_kiosk_EIncorrectAmount"></a>

For when Coin paid does not match the offer price.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EIncorrectAmount">EIncorrectAmount</a>: u64 = 2;
</code></pre>



<a name="0x2_kiosk_EIncorrectArgument"></a>

For when incorrect arguments passed into <code>switch_mode</code> function.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EIncorrectArgument">EIncorrectArgument</a>: u64 = 3;
</code></pre>



<a name="0x2_kiosk_EOwnerNotSet"></a>

For when trying to withdraw profits as owner and owner is not set.


<pre><code><b>const</b> <a href="kiosk.md#0x2_kiosk_EOwnerNotSet">EOwnerNotSet</a>: u64 = 0;
</code></pre>



<a name="0x2_kiosk_new_for_sender"></a>

## Function `new_for_sender`

Creates a new Kiosk with the owner set.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_new_for_sender">new_for_sender</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_new_for_sender">new_for_sender</a>(ctx: &<b>mut</b> TxContext): <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a> {
    <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        profits: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
        owner: <a href="_some">option::some</a>(sender(ctx))
    }
}
</code></pre>



</details>

<a name="0x2_kiosk_new_with_capability"></a>

## Function `new_with_capability`

Creates a new Kiosk without owner but with a Capability.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_new_with_capability">new_with_capability</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_new_with_capability">new_with_capability</a>(ctx: &<b>mut</b> TxContext): (<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>) {
    <b>let</b> <a href="kiosk.md#0x2_kiosk">kiosk</a> = <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        profits: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
        owner: <a href="_none">option::none</a>()
    };

    <b>let</b> cap = <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        for: <a href="object.md#0x2_object_id">object::id</a>(&<a href="kiosk.md#0x2_kiosk">kiosk</a>)
    };

    (<a href="kiosk.md#0x2_kiosk">kiosk</a>, cap)
}
</code></pre>



</details>

<a name="0x2_kiosk_switch_mode"></a>

## Function `switch_mode`

Switch the ownership mode.
1. If <code><a href="kiosk.md#0x2_kiosk">kiosk</a>.owner</code> is set, unset it and issue a <code><a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a></code>
2. If <code><a href="kiosk.md#0x2_kiosk">kiosk</a>.owner</code> is not set, exchange <code><a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a></code> for this setting.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_switch_mode">switch_mode</a>(<a href="kiosk.md#0x2_kiosk">kiosk</a>: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: <a href="_Option">option::Option</a>&lt;<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="_Option">option::Option</a>&lt;<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_switch_mode">switch_mode</a>(<a href="kiosk.md#0x2_kiosk">kiosk</a>: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: Option&lt;<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>&gt;, ctx: &<b>mut</b> TxContext): Option&lt;<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>&gt; {
    <b>assert</b>!(
        (<a href="_is_some">option::is_some</a>(&cap) && <a href="_is_none">option::is_none</a>(&<a href="kiosk.md#0x2_kiosk">kiosk</a>.owner)) ||
        (<a href="_is_none">option::is_none</a>(&cap) && <a href="_is_some">option::is_some</a>(&<a href="kiosk.md#0x2_kiosk">kiosk</a>.owner))
    , <a href="kiosk.md#0x2_kiosk_EIncorrectArgument">EIncorrectArgument</a>);

    <b>if</b> (<a href="_is_some">option::is_some</a>(&cap)) {
        <b>let</b> <a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a> { id, for } = <a href="_destroy_some">option::destroy_some</a>(cap);
        <b>assert</b>!(for == <a href="object.md#0x2_object_id">object::id</a>(<a href="kiosk.md#0x2_kiosk">kiosk</a>), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
        <a href="kiosk.md#0x2_kiosk">kiosk</a>.owner = <a href="_some">option::some</a>(sender(ctx));
        <a href="object.md#0x2_object_delete">object::delete</a>(id);
        <a href="_none">option::none</a>()
    } <b>else</b> {
        <b>assert</b>!(sender(ctx) == *<a href="_borrow">option::borrow</a>(&<a href="kiosk.md#0x2_kiosk">kiosk</a>.owner), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
        <a href="kiosk.md#0x2_kiosk">kiosk</a>.owner = <a href="_none">option::none</a>();
        <a href="_destroy_none">option::destroy_none</a>(cap);
        <a href="_some">option::some</a>(<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a> {
            id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
            for: <a href="object.md#0x2_object_id">object::id</a>(<a href="kiosk.md#0x2_kiosk">kiosk</a>)
        })
    }
}
</code></pre>



</details>

<a name="0x2_kiosk_create_allow_transfer_cap"></a>

## Function `create_allow_transfer_cap`

TODO: better naming


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_create_allow_transfer_cap">create_allow_transfer_cap</a>&lt;T: store, key&gt;(pub: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="kiosk.md#0x2_kiosk_AllowTransferCap">kiosk::AllowTransferCap</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_create_allow_transfer_cap">create_allow_transfer_cap</a>&lt;T: key + store&gt;(
    pub: &Publisher, ctx: &<b>mut</b> TxContext
): <a href="kiosk.md#0x2_kiosk_AllowTransferCap">AllowTransferCap</a>&lt;T&gt; {
    // TODO: consider "is_module"
    <b>assert</b>!(<a href="publisher.md#0x2_publisher_is_package">publisher::is_package</a>&lt;T&gt;(pub), 0);
    <a href="kiosk.md#0x2_kiosk_AllowTransferCap">AllowTransferCap</a> { id: <a href="object.md#0x2_object_new">object::new</a>(ctx) }
}
</code></pre>



</details>

<a name="0x2_kiosk_place"></a>

## Function `place`



<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_place">place</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, item: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_place">place</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, item: T) {
    dof::add(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Key">Key</a> { id: <a href="object.md#0x2_object_id">object::id</a>(&item) }, item)
}
</code></pre>



</details>

<a name="0x2_kiosk_take"></a>

## Function `take`



<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_take">take</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_take">take</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, id: ID): T {
    dof::remove(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Key">Key</a> { id })
}
</code></pre>



</details>

<a name="0x2_kiosk_make_offer"></a>

## Function `make_offer`



<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_make_offer">make_offer</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>, price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_make_offer">make_offer</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, id: ID, price: u64) {
    df::add(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Offer">Offer</a> { id }, price)
}
</code></pre>



</details>

<a name="0x2_kiosk_purchase"></a>

## Function `purchase`

Make a trade - pay the owner of the item


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase">purchase</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>, payment: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;): <a href="kiosk.md#0x2_kiosk_TransferRequest">kiosk::TransferRequest</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase">purchase</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, id: ID, payment: Coin&lt;SUI&gt;
): <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a>&lt;T&gt; {
    <b>let</b> price = df::remove&lt;<a href="kiosk.md#0x2_kiosk_Offer">Offer</a>, u64&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Offer">Offer</a> { id });
    <b>let</b> inner = dof::remove&lt;<a href="kiosk.md#0x2_kiosk_Key">Key</a>, T&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Key">Key</a> { id });

    <b>assert</b>!(price == <a href="coin.md#0x2_coin_value">coin::value</a>(&payment), <a href="kiosk.md#0x2_kiosk_EIncorrectAmount">EIncorrectAmount</a>);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.profits, <a href="coin.md#0x2_coin_into_balance">coin::into_balance</a>(payment));

    <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a> {
        inner,
        paid: price,
        from: self.owner
    }
}
</code></pre>



</details>

<a name="0x2_kiosk_allow"></a>

## Function `allow`

Allow a <code><a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a></code> to happen.

Can only be performed by an <code>AcceptTransferCap</code> making the approval logic a resposibility
of the owner of the T.
It also means that unless there's a policy for T to allow transfers, Kiosk trades will not be possible.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_allow">allow</a>&lt;T: store, key&gt;(_cap: &<a href="kiosk.md#0x2_kiosk_AllowTransferCap">kiosk::AllowTransferCap</a>&lt;T&gt;, req: <a href="kiosk.md#0x2_kiosk_TransferRequest">kiosk::TransferRequest</a>&lt;T&gt;): (<a href="kiosk.md#0x2_kiosk_ApprovedTransfer">kiosk::ApprovedTransfer</a>&lt;T&gt;, u64, <a href="_Option">option::Option</a>&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_allow">allow</a>&lt;T: key + store&gt;(
    _cap: &<a href="kiosk.md#0x2_kiosk_AllowTransferCap">AllowTransferCap</a>&lt;T&gt;, req: <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a>&lt;T&gt;
): (<a href="kiosk.md#0x2_kiosk_ApprovedTransfer">ApprovedTransfer</a>&lt;T&gt;, u64, Option&lt;<b>address</b>&gt;) {
    <b>let</b> <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a> { inner, paid, from } = req;
    (<a href="kiosk.md#0x2_kiosk_ApprovedTransfer">ApprovedTransfer</a> { inner }, paid, from)
}
</code></pre>



</details>

<a name="0x2_kiosk_store_purchased"></a>

## Function `store_purchased`

Finalize the trade by placing the purchased item into the destination Kiosk.
The operation can only be performed by the owner of the Kiosk be it an explicit
setting or an issued Capability.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_store_purchased">store_purchased</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, <a href="transfer.md#0x2_transfer">transfer</a>: <a href="kiosk.md#0x2_kiosk_ApprovedTransfer">kiosk::ApprovedTransfer</a>&lt;T&gt;, cap: &<a href="_Option">option::Option</a>&lt;<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_store_purchased">store_purchased</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>,
    <a href="transfer.md#0x2_transfer">transfer</a>: <a href="kiosk.md#0x2_kiosk_ApprovedTransfer">ApprovedTransfer</a>&lt;T&gt;,
    cap: &Option&lt;<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>&gt;,
    ctx: &<b>mut</b> TxContext
) {
    <a href="kiosk.md#0x2_kiosk_try_access">try_access</a>(self, cap, ctx);
    <b>let</b> <a href="kiosk.md#0x2_kiosk_ApprovedTransfer">ApprovedTransfer</a> { inner: item } = <a href="transfer.md#0x2_transfer">transfer</a>;
    <a href="kiosk.md#0x2_kiosk_place">place</a>(self, item)
}
</code></pre>



</details>

<a name="0x2_kiosk_withdraw"></a>

## Function `withdraw`

Withdraw profits from the Kiosk.
If <code><a href="kiosk.md#0x2_kiosk">kiosk</a>.owner</code> is set -> check for transaction sender.
If <code><a href="kiosk.md#0x2_kiosk">kiosk</a>.owner</code> is none -> require capability.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_withdraw">withdraw</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="_Option">option::Option</a>&lt;<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_withdraw">withdraw</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &Option&lt;<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>&gt;, ctx: &<b>mut</b> TxContext
): Coin&lt;SUI&gt; {
    <a href="kiosk.md#0x2_kiosk_try_access">try_access</a>(self, cap, ctx);

    <b>let</b> amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&self.profits);
    <a href="coin.md#0x2_coin_take">coin::take</a>(&<b>mut</b> self.profits, amount, ctx)
}
</code></pre>



</details>

<a name="0x2_kiosk_try_access"></a>

## Function `try_access`

Abort if credentials are incorrect and the party attempts to access the Kiosk.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_try_access">try_access</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, cap: &<a href="_Option">option::Option</a>&lt;<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">kiosk::KioskOwnerCap</a>&gt;, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_try_access">try_access</a>(self: &<a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, cap: &Option&lt;<a href="kiosk.md#0x2_kiosk_KioskOwnerCap">KioskOwnerCap</a>&gt;, ctx: &TxContext) {
    <b>assert</b>!(
        (<a href="_is_some">option::is_some</a>(cap) && <a href="_is_none">option::is_none</a>(&self.owner)) ||
        (<a href="_is_none">option::is_none</a>(cap) && <a href="_is_some">option::is_some</a>(&self.owner))
    , <a href="kiosk.md#0x2_kiosk_EIncorrectArgument">EIncorrectArgument</a>);

    <b>if</b> (<a href="_is_some">option::is_some</a>(&self.owner)) {
        <b>assert</b>!(*<a href="_borrow">option::borrow</a>(&self.owner) == sender(ctx), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    } <b>else</b> {
        <b>assert</b>!(<a href="_borrow">option::borrow</a>(cap).for == <a href="object.md#0x2_object_id">object::id</a>(self), <a href="kiosk.md#0x2_kiosk_ENotOwner">ENotOwner</a>);
    };
}
</code></pre>



</details>
