
<a name="0x2_randomness"></a>

# Module `0x2::randomness`

Randomness objects can only be created, set or consumed. They cannot be created and consumed
in the *same* transaction since it might allow validators decide whether to create and use those
objects *after* seeing the randomness they depend on.

- On creation, the object contains the epoch in which it was created and a unique object id.

- After the object creation transaction is committed, anyone can retrieve the BLS signature on
message "randomness":epoch:id from validators (signed using the Threshold-BLS key of that
epoch).

- Anyone that can mutate the object can set the randomness of the object by supplying the BLS
signature. This operation verifies the signature and sets the value of the randomness object
to be the hash of the signature.

Note that there is a single signature that could pass the verification for a specific object,
thus, the only options the owner of the object has after retrieving the signature (and learning
the randomness) is to either set the randomness or leave it unset. Applications that use
Randomness objects must make sure they handle both options (e.g., debit the user on object
creation so even if the user aborts, depending on the randomness it received, the application
is not harmed).

- Once set, the random value can be read/consumed.


This object can be used as a shared-/owned-object.


-  [Resource `Randomness`](#0x2_randomness_Randomness)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_randomness_new)
-  [Function `transfer`](#0x2_randomness_transfer)
-  [Function `share_object`](#0x2_randomness_share_object)
-  [Function `set`](#0x2_randomness_set)
-  [Function `destroy`](#0x2_randomness_destroy)
-  [Function `epoch`](#0x2_randomness_epoch)
-  [Function `value`](#0x2_randomness_value)
-  [Function `to_bytes`](#0x2_randomness_to_bytes)
-  [Function `native_tbls_verify_signature`](#0x2_randomness_native_tbls_verify_signature)
-  [Function `native_tbls_sign`](#0x2_randomness_native_tbls_sign)
-  [Function `safe_selection`](#0x2_randomness_safe_selection)


<pre><code><b>use</b> <a href="">0x1::hash</a>;
<b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::vector</a>;
<b>use</b> <a href="bcs.md#0x2_bcs">0x2::bcs</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_randomness_Randomness"></a>

## Resource `Randomness`



<pre><code><b>struct</b> <a href="randomness.md#0x2_randomness_Randomness">Randomness</a>&lt;T&gt; <b>has</b> key
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
<code>epoch: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>value: <a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_randomness_EInvalidSignature"></a>

Set is called with an invalid signature.


<pre><code><b>const</b> <a href="randomness.md#0x2_randomness_EInvalidSignature">EInvalidSignature</a>: u64 = 0;
</code></pre>



<a name="0x2_randomness_Domain"></a>

All signatures are prefixed with Domain.


<pre><code><b>const</b> <a href="randomness.md#0x2_randomness_Domain">Domain</a>: <a href="">vector</a>&lt;u8&gt; = [114, 97, 110, 100, 111, 109, 110, 101, 115, 115];
</code></pre>



<a name="0x2_randomness_EAlreadySet"></a>

Already set object cannot be set again.


<pre><code><b>const</b> <a href="randomness.md#0x2_randomness_EAlreadySet">EAlreadySet</a>: u64 = 1;
</code></pre>



<a name="0x2_randomness_EInvalidRndLength"></a>

Supplied randomness is not of the right length.


<pre><code><b>const</b> <a href="randomness.md#0x2_randomness_EInvalidRndLength">EInvalidRndLength</a>: u64 = 2;
</code></pre>



<a name="0x2_randomness_new"></a>

## Function `new`



<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_new">new</a>&lt;T: drop&gt;(_w: T, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="randomness.md#0x2_randomness_Randomness">randomness::Randomness</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_new">new</a>&lt;T: drop&gt;(_w: T, ctx: &<b>mut</b> TxContext): <a href="randomness.md#0x2_randomness_Randomness">Randomness</a>&lt;T&gt; {
    <a href="randomness.md#0x2_randomness_Randomness">Randomness</a>&lt;T&gt; {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        epoch: <a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx),
        value: <a href="_none">option::none</a>(),
    }
    // TODO: Front load the fee.
}
</code></pre>



</details>

<a name="0x2_randomness_transfer"></a>

## Function `transfer`



<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>&lt;T&gt;(self: <a href="randomness.md#0x2_randomness_Randomness">randomness::Randomness</a>&lt;T&gt;, <b>to</b>: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>&lt;T&gt;(self: <a href="randomness.md#0x2_randomness_Randomness">Randomness</a>&lt;T&gt;, <b>to</b>: <b>address</b>) {
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(self, <b>to</b>);
}
</code></pre>



</details>

<a name="0x2_randomness_share_object"></a>

## Function `share_object`



<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_share_object">share_object</a>&lt;T&gt;(self: <a href="randomness.md#0x2_randomness_Randomness">randomness::Randomness</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_share_object">share_object</a>&lt;T&gt;(self: <a href="randomness.md#0x2_randomness_Randomness">Randomness</a>&lt;T&gt;) {
    <a href="transfer.md#0x2_transfer_share_object">transfer::share_object</a>(self);
}
</code></pre>



</details>

<a name="0x2_randomness_set"></a>

## Function `set`

Owner(s) can use this function for setting the randomness.


<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_set">set</a>&lt;T&gt;(self: &<b>mut</b> <a href="randomness.md#0x2_randomness_Randomness">randomness::Randomness</a>&lt;T&gt;, sig: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_set">set</a>&lt;T&gt;(self: &<b>mut</b> <a href="randomness.md#0x2_randomness_Randomness">Randomness</a>&lt;T&gt;, sig: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(&self.value), <a href="randomness.md#0x2_randomness_EAlreadySet">EAlreadySet</a>);
    <b>let</b> msg = <a href="randomness.md#0x2_randomness_to_bytes">to_bytes</a>(&<a href="randomness.md#0x2_randomness_Domain">Domain</a>, self.epoch, &<a href="object.md#0x2_object_id">object::id</a>(self));
    <b>assert</b>!(<a href="randomness.md#0x2_randomness_native_tbls_verify_signature">native_tbls_verify_signature</a>(self.epoch, &msg, &sig), <a href="randomness.md#0x2_randomness_EInvalidSignature">EInvalidSignature</a>);
    <b>let</b> hashed = sha3_256(sig);
    self.value = <a href="_some">option::some</a>(hashed);
}
</code></pre>



</details>

<a name="0x2_randomness_destroy"></a>

## Function `destroy`

Delete the object.


<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_destroy">destroy</a>&lt;T&gt;(r: <a href="randomness.md#0x2_randomness_Randomness">randomness::Randomness</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_destroy">destroy</a>&lt;T&gt;(r: <a href="randomness.md#0x2_randomness_Randomness">Randomness</a>&lt;T&gt;) {
    <b>let</b> <a href="randomness.md#0x2_randomness_Randomness">Randomness</a> { id, epoch: _, value: _ } = r;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
}
</code></pre>



</details>

<a name="0x2_randomness_epoch"></a>

## Function `epoch`

Read the epoch of the object.


<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_epoch">epoch</a>&lt;T&gt;(self: &<a href="randomness.md#0x2_randomness_Randomness">randomness::Randomness</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_epoch">epoch</a>&lt;T&gt;(self: &<a href="randomness.md#0x2_randomness_Randomness">Randomness</a>&lt;T&gt;): u64 {
    self.epoch
}
</code></pre>



</details>

<a name="0x2_randomness_value"></a>

## Function `value`

Read the current value of the object.


<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_value">value</a>&lt;T&gt;(self: &<a href="randomness.md#0x2_randomness_Randomness">randomness::Randomness</a>&lt;T&gt;): &<a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_value">value</a>&lt;T&gt;(self: &<a href="randomness.md#0x2_randomness_Randomness">Randomness</a>&lt;T&gt;): &<a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt; {
    &self.value
}
</code></pre>



</details>

<a name="0x2_randomness_to_bytes"></a>

## Function `to_bytes`



<pre><code><b>fun</b> <a href="randomness.md#0x2_randomness_to_bytes">to_bytes</a>(domain: &<a href="">vector</a>&lt;u8&gt;, epoch: u64, id: &<a href="object.md#0x2_object_ID">object::ID</a>): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="randomness.md#0x2_randomness_to_bytes">to_bytes</a>(domain: &<a href="">vector</a>&lt;u8&gt;, epoch: u64, id: &ID): <a href="">vector</a>&lt;u8&gt; {
    <b>let</b> buffer: <a href="">vector</a>&lt;u8&gt; = <a href="_empty">vector::empty</a>();
    // All elements below are of fixed sizes.
    <a href="_append">vector::append</a>(&<b>mut</b> buffer, *domain);
    <a href="_append">vector::append</a>(&<b>mut</b> buffer, <a href="_to_bytes">bcs::to_bytes</a>(&epoch));
    <a href="_append">vector::append</a>(&<b>mut</b> buffer, <a href="object.md#0x2_object_id_to_bytes">object::id_to_bytes</a>(id));
    buffer
}
</code></pre>



</details>

<a name="0x2_randomness_native_tbls_verify_signature"></a>

## Function `native_tbls_verify_signature`

Verify signature sig on message msg using the epoch's BLS key.


<pre><code><b>fun</b> <a href="randomness.md#0x2_randomness_native_tbls_verify_signature">native_tbls_verify_signature</a>(epoch: u64, msg: &<a href="">vector</a>&lt;u8&gt;, sig: &<a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="randomness.md#0x2_randomness_native_tbls_verify_signature">native_tbls_verify_signature</a>(epoch: u64, msg: &<a href="">vector</a>&lt;u8&gt;, sig: &<a href="">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
</code></pre>



</details>

<a name="0x2_randomness_native_tbls_sign"></a>

## Function `native_tbls_sign`

Helper functions to sign on messages in tests.


<pre><code><b>fun</b> <a href="randomness.md#0x2_randomness_native_tbls_sign">native_tbls_sign</a>(epoch: u64, msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="randomness.md#0x2_randomness_native_tbls_sign">native_tbls_sign</a>(epoch: u64, msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
</code></pre>



</details>

<a name="0x2_randomness_safe_selection"></a>

## Function `safe_selection`



<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_safe_selection">safe_selection</a>(n: u64, rnd: &<a href="">vector</a>&lt;u8&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="randomness.md#0x2_randomness_safe_selection">safe_selection</a>(n: u64, rnd: &<a href="">vector</a>&lt;u8&gt;): u64 {
    <b>assert</b>!(<a href="_length">vector::length</a>(rnd) &gt;= 16, <a href="randomness.md#0x2_randomness_EInvalidRndLength">EInvalidRndLength</a>);
    <b>let</b> m: u128 = 0;
    <b>let</b> i = 0;
    <b>while</b> (i &lt; 16) {
        m = m &lt;&lt; 8;
        <b>let</b> curr_byte = *<a href="_borrow">vector::borrow</a>(rnd, i);
        m = m + (curr_byte <b>as</b> u128);
        i = i + 1;
    };
    <b>let</b> n_128 = (n <b>as</b> u128);
    <b>let</b> module_128  = m % n_128;
    <b>let</b> res = (module_128 <b>as</b> u64);
    res
}
</code></pre>



</details>
