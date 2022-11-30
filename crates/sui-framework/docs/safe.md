
<a name="0x2_safe"></a>

# Module `0x2::safe`

Safe for collectibles.

- listing functionality
- paying royalties to creators
- no restrictions on transfers or taking / pulling an asset


-  [Resource `Safe`](#0x2_safe_Safe)
-  [Struct `Listing`](#0x2_safe_Listing)
-  [Struct `Promise`](#0x2_safe_Promise)
-  [Constants](#@Constants_0)
-  [Function `create_safe`](#0x2_safe_create_safe)
-  [Function `put`](#0x2_safe_put)
-  [Function `list`](#0x2_safe_list)
-  [Function `purchase`](#0x2_safe_purchase)
-  [Function `take`](#0x2_safe_take)
-  [Function `borrow`](#0x2_safe_borrow)
-  [Function `borrow_mut`](#0x2_safe_borrow_mut)
-  [Function `take_with_promise`](#0x2_safe_take_with_promise)
-  [Function `return_promise`](#0x2_safe_return_promise)
-  [Function `prove_destruction`](#0x2_safe_prove_destruction)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field">0x2::dynamic_object_field</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_safe_Safe"></a>

## Resource `Safe`

Safe is abstraction layer which separates and protects
collectibles or other types of assets which require royalties
or overall transfer safety.


<pre><code><b>struct</b> <a href="safe.md#0x2_safe_Safe">Safe</a> <b>has</b> store, key
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
<code>owner: <a href="_Option">option::Option</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>whitelist: <a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;<b>address</b>&gt;&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_safe_Listing"></a>

## Struct `Listing`



<pre><code><b>struct</b> <a href="safe.md#0x2_safe_Listing">Listing</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>price: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>item_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_safe_Promise"></a>

## Struct `Promise`



<pre><code><b>struct</b> <a href="safe.md#0x2_safe_Promise">Promise</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>expects: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_safe_FEE"></a>

Fee in basis points to pay to the creator.


<pre><code><b>const</b> <a href="safe.md#0x2_safe_FEE">FEE</a>: u64 = 100;
</code></pre>



<a name="0x2_safe_create_safe"></a>

## Function `create_safe`

Free action of creating a new Safe. Owner and whitelisted (up to X)
safes can be specified to allow free transfers between them.


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_create_safe">create_safe</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="safe.md#0x2_safe_Safe">safe::Safe</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_create_safe">create_safe</a>(ctx: &<b>mut</b> TxContext): <a href="safe.md#0x2_safe_Safe">Safe</a> {
    <a href="safe.md#0x2_safe_Safe">Safe</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        owner: <a href="_some">option::some</a>(<a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx)),
        whitelist: <a href="_none">option::none</a>()
    }
}
</code></pre>



</details>

<a name="0x2_safe_put"></a>

## Function `put`

Type linker. Which witness type matches royalty type.
Add an Object to the safe effectively locking it from the outer world.

When an Object is first added to the Safe, a witness parameter <code>S</code> is locked,
and will travel along the <code>T</code>, marking the proper <code>RoyaltyReceipt&lt;S&gt;</code> for <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_put">put</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>, item: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_put">put</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>, item: T) {
    // df::add(&<b>mut</b> self.id, TypeLink&lt;T&gt; {}, TypeLink&lt;R&gt; {});
    dof::add(&<b>mut</b> self.id, <a href="object.md#0x2_object_id">object::id</a>(&item), item)
    // <b>abort</b> 0
}
</code></pre>



</details>

<a name="0x2_safe_list"></a>

## Function `list`

List an item for sale in a safe.


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_list">list</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>, item_id: <a href="object.md#0x2_object_ID">object::ID</a>, price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_list">list</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>, item_id: ID, price: u64) {
    <b>let</b> item = dof::remove&lt;ID, T&gt;(&<b>mut</b> self.id, item_id);
    dof::add(&<b>mut</b> self.id, <a href="safe.md#0x2_safe_Listing">Listing</a> { price, item_id }, item)
}
</code></pre>



</details>

<a name="0x2_safe_purchase"></a>

## Function `purchase`

Purchase a listed item from a Safe by an item ID.


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_purchase">purchase</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>, target: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>, item_id: <a href="object.md#0x2_object_ID">object::ID</a>, payment: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_purchase">purchase</a>&lt;T: key + store&gt;(
    self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>, target: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>, item_id: ID, payment: Coin&lt;SUI&gt;, _ctx: &<b>mut</b> TxContext
) {
    <b>let</b> price = <a href="coin.md#0x2_coin_value">coin::value</a>(&payment);
    <b>let</b> item = dof::remove&lt;<a href="safe.md#0x2_safe_Listing">Listing</a>, T&gt;(&<b>mut</b> self.id, <a href="safe.md#0x2_safe_Listing">Listing</a> { price, item_id });

    <a href="safe.md#0x2_safe_put">put</a>(target, item);

    // we need <b>to</b> do something <b>with</b> the payment
    sui::transfer::transfer(payment, sui::tx_context::sender(_ctx))
}
</code></pre>



</details>

<a name="0x2_safe_take"></a>

## Function `take`

Take an item from the Safe freeing it from the safe.


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_take">take</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>, item_id: <a href="object.md#0x2_object_ID">object::ID</a>): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_take">take</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>, item_id: ID): T {
    dof::remove(&<b>mut</b> self.id, item_id)
}
</code></pre>



</details>

<a name="0x2_safe_borrow"></a>

## Function `borrow`

Borrow an Object from the safe allowing read access. If additional constraints
are needed, Safe can be wrapped into an access-control wrapper.


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_borrow">borrow</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>, item_id: <a href="object.md#0x2_object_ID">object::ID</a>): &T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_borrow">borrow</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>, item_id: ID): &T {
    dof::borrow(&self.id, item_id)
}
</code></pre>



</details>

<a name="0x2_safe_borrow_mut"></a>

## Function `borrow_mut`

Mutably borrow an Object from the safe allowing modifications. Access control can
be enforced on the higher level if needed.


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_borrow_mut">borrow_mut</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>, item_id: <a href="object.md#0x2_object_ID">object::ID</a>): &<b>mut</b> T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_borrow_mut">borrow_mut</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>, item_id: ID): &<b>mut</b> T {
    dof::borrow_mut(&<b>mut</b> self.id, item_id)
}
</code></pre>



</details>

<a name="0x2_safe_take_with_promise"></a>

## Function `take_with_promise`



<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_take_with_promise">take_with_promise</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>, item_id: <a href="object.md#0x2_object_ID">object::ID</a>): (T, <a href="safe.md#0x2_safe_Promise">safe::Promise</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_take_with_promise">take_with_promise</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>, item_id: ID): (T, <a href="safe.md#0x2_safe_Promise">Promise</a>) {
    (dof::remove(&<b>mut</b> self.id, *&item_id), <a href="safe.md#0x2_safe_Promise">Promise</a> { expects: item_id })
}
</code></pre>



</details>

<a name="0x2_safe_return_promise"></a>

## Function `return_promise`



<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_return_promise">return_promise</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>, item: T, promise: <a href="safe.md#0x2_safe_Promise">safe::Promise</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_return_promise">return_promise</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>, item: T, promise: <a href="safe.md#0x2_safe_Promise">Promise</a>) {
    <b>let</b> <a href="safe.md#0x2_safe_Promise">Promise</a> { expects } = promise;
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(&item) == expects, 0);
    dof::add(&<b>mut</b> self.id, <a href="object.md#0x2_object_id">object::id</a>(&item), item)
}
</code></pre>



</details>

<a name="0x2_safe_prove_destruction"></a>

## Function `prove_destruction`

A very fancy way to prove that object was destroyed within the current transaction.
This way we ensure that the Object was unpacked. Yay!

We can consider taking the responsibility of deleting the UID; most of the cleanups
and dynamic objects can be managed prior to this call (eg nothing is stopping us from it)


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_prove_destruction">prove_destruction</a>(id: <a href="object.md#0x2_object_UID">object::UID</a>, promise: <a href="safe.md#0x2_safe_Promise">safe::Promise</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_prove_destruction">prove_destruction</a>(id: UID, promise: <a href="safe.md#0x2_safe_Promise">Promise</a>) {
    <b>let</b> <a href="safe.md#0x2_safe_Promise">Promise</a> { expects } = promise;
    <b>assert</b>!(<a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&id) == expects, 0);
    <a href="object.md#0x2_object_delete">object::delete</a>(id)
}
</code></pre>



</details>
