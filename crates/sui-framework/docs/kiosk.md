
<a name="0x2_kiosk"></a>

# Module `0x2::kiosk`



-  [Struct `TransferRequest`](#0x2_kiosk_TransferRequest)
-  [Resource `AcceptTransferCap`](#0x2_kiosk_AcceptTransferCap)
-  [Resource `Kiosk`](#0x2_kiosk_Kiosk)
-  [Struct `Key`](#0x2_kiosk_Key)
-  [Struct `Offer`](#0x2_kiosk_Offer)
-  [Function `place`](#0x2_kiosk_place)
-  [Function `make_offer`](#0x2_kiosk_make_offer)
-  [Function `purchase`](#0x2_kiosk_purchase)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field">0x2::dynamic_object_field</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
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

<a name="0x2_kiosk_AcceptTransferCap"></a>

## Resource `AcceptTransferCap`

A unique capability that allows owner of the <code>T</code> to authorize
transfers. Can only be created with the <code>Publisher</code> object.


<pre><code><b>struct</b> <a href="kiosk.md#0x2_kiosk_AcceptTransferCap">AcceptTransferCap</a>&lt;T: store, key&gt; <b>has</b> store, key
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

<a name="0x2_kiosk_Key"></a>

## Struct `Key`

Custom key for the items placed into the kiosk.


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

An active offer to purchase the T.


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

<a name="0x2_kiosk_make_offer"></a>

## Function `make_offer`



<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_make_offer">make_offer</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, item_id: <a href="object.md#0x2_object_ID">object::ID</a>, price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_make_offer">make_offer</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, item_id: ID, price: u64) {
    df::add(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Offer">Offer</a> { id: item_id }, price)
}
</code></pre>



</details>

<a name="0x2_kiosk_purchase"></a>

## Function `purchase`



<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase">purchase</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>, id: <a href="object.md#0x2_object_ID">object::ID</a>, payment: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;): <a href="kiosk.md#0x2_kiosk_TransferRequest">kiosk::TransferRequest</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk.md#0x2_kiosk_purchase">purchase</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="kiosk.md#0x2_kiosk_Kiosk">Kiosk</a>, id: ID, payment: Coin&lt;SUI&gt;): <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a>&lt;T&gt; {
    <b>let</b> price = df::remove&lt;<a href="kiosk.md#0x2_kiosk_Offer">Offer</a>, u64&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Offer">Offer</a> { id });
    <b>let</b> inner = dof::remove&lt;<a href="kiosk.md#0x2_kiosk_Key">Key</a>, T&gt;(&<b>mut</b> self.id, <a href="kiosk.md#0x2_kiosk_Key">Key</a> { id });

    <b>assert</b>!(price == <a href="coin.md#0x2_coin_value">coin::value</a>(&payment), 0);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.profits, <a href="coin.md#0x2_coin_into_balance">coin::into_balance</a>(payment));

    <a href="kiosk.md#0x2_kiosk_TransferRequest">TransferRequest</a> {
        inner,
        paid: price,
        from: self.owner
    }
}
</code></pre>



</details>
