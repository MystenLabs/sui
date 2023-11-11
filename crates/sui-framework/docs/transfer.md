
<a name="0x2_transfer"></a>

# Module `0x2::transfer`



-  [Struct `Receiving`](#0x2_transfer_Receiving)
-  [Constants](#@Constants_0)
-  [Function `transfer`](#0x2_transfer_transfer)
-  [Function `public_transfer`](#0x2_transfer_public_transfer)
-  [Function `freeze_object`](#0x2_transfer_freeze_object)
-  [Function `public_freeze_object`](#0x2_transfer_public_freeze_object)
-  [Function `share_object`](#0x2_transfer_share_object)
-  [Function `public_share_object`](#0x2_transfer_public_share_object)
-  [Function `receive`](#0x2_transfer_receive)
-  [Function `freeze_object_impl`](#0x2_transfer_freeze_object_impl)
-  [Function `share_object_impl`](#0x2_transfer_share_object_impl)
-  [Function `transfer_impl`](#0x2_transfer_transfer_impl)
-  [Function `receive_impl`](#0x2_transfer_receive_impl)


<pre><code><b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
</code></pre>



<a name="0x2_transfer_Receiving"></a>

## Struct `Receiving`

This represents the ability to <code>receive</code> an object of type <code>T</code>.
This type is ephemeral per-transaction and cannot be stored on-chain.
This does not represent the obligation to receive the object that it
references, but simply the ability to receive the object with object ID
<code>id</code> at version <code>version</code> if you can prove mutable access to the parent
object during the transaction.
Internals of this struct are opaque outside this module.


<pre><code><b>struct</b> <a href="transfer.md#0x2_transfer_Receiving">Receiving</a>&lt;T: key&gt; <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>version: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_transfer_ESharedNonNewObject"></a>

Shared an object that was previously created. Shared objects must currently
be constructed in the transaction they are created.


<pre><code><b>const</b> <a href="transfer.md#0x2_transfer_ESharedNonNewObject">ESharedNonNewObject</a>: u64 = 0;
</code></pre>



<a name="0x2_transfer_ESharedObjectOperationNotSupported"></a>

Shared object operations such as wrapping, freezing, and converting to owned are not allowed.


<pre><code><b>const</b> <a href="transfer.md#0x2_transfer_ESharedObjectOperationNotSupported">ESharedObjectOperationNotSupported</a>: u64 = 1;
</code></pre>



<a name="0x2_transfer_transfer"></a>

## Function `transfer`

Transfer ownership of <code>obj</code> to <code>recipient</code>. <code>obj</code> must have the <code>key</code> attribute,
which (in turn) ensures that <code>obj</code> has a globally unique ID. Note that if the recipient
address represents an object ID, the <code>obj</code> sent will be inaccessible after the transfer
(though they will be retrievable at a future date once new features are added).
This function has custom rules performed by the Sui Move bytecode verifier that ensures
that <code>T</code> is an object defined in the module where <code><a href="transfer.md#0x2_transfer">transfer</a></code> is invoked. Use
<code>public_transfer</code> to transfer an object with <code>store</code> outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>) {
    <a href="transfer.md#0x2_transfer_transfer_impl">transfer_impl</a>(obj, recipient)
}
</code></pre>



</details>

<a name="0x2_transfer_public_transfer"></a>

## Function `public_transfer`

Transfer ownership of <code>obj</code> to <code>recipient</code>. <code>obj</code> must have the <code>key</code> attribute,
which (in turn) ensures that <code>obj</code> has a globally unique ID. Note that if the recipient
address represents an object ID, the <code>obj</code> sent will be inaccessible after the transfer
(though they will be retrievable at a future date once new features are added).
The object must have <code>store</code> to be transferred outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_public_transfer">public_transfer</a>&lt;T: store, key&gt;(obj: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_public_transfer">public_transfer</a>&lt;T: key + store&gt;(obj: T, recipient: <b>address</b>) {
    <a href="transfer.md#0x2_transfer_transfer_impl">transfer_impl</a>(obj, recipient)
}
</code></pre>



</details>

<a name="0x2_transfer_freeze_object"></a>

## Function `freeze_object`

Freeze <code>obj</code>. After freezing <code>obj</code> becomes immutable and can no longer be transferred or
mutated.
This function has custom rules performed by the Sui Move bytecode verifier that ensures
that <code>T</code> is an object defined in the module where <code>freeze_object</code> is invoked. Use
<code>public_freeze_object</code> to freeze an object with <code>store</code> outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_freeze_object">freeze_object</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_freeze_object">freeze_object</a>&lt;T: key&gt;(obj: T) {
    <a href="transfer.md#0x2_transfer_freeze_object_impl">freeze_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="0x2_transfer_public_freeze_object"></a>

## Function `public_freeze_object`

Freeze <code>obj</code>. After freezing <code>obj</code> becomes immutable and can no longer be transferred or
mutated.
The object must have <code>store</code> to be frozen outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_public_freeze_object">public_freeze_object</a>&lt;T: store, key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_public_freeze_object">public_freeze_object</a>&lt;T: key + store&gt;(obj: T) {
    <a href="transfer.md#0x2_transfer_freeze_object_impl">freeze_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="0x2_transfer_share_object"></a>

## Function `share_object`

Turn the given object into a mutable shared object that everyone can access and mutate.
This is irreversible, i.e. once an object is shared, it will stay shared forever.
Aborts with <code><a href="transfer.md#0x2_transfer_ESharedNonNewObject">ESharedNonNewObject</a></code> of the object being shared was not created in this
transaction. This restriction may be relaxed in the future.
This function has custom rules performed by the Sui Move bytecode verifier that ensures
that <code>T</code> is an object defined in the module where <code>share_object</code> is invoked. Use
<code>public_share_object</code> to share an object with <code>store</code> outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_share_object">share_object</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_share_object">share_object</a>&lt;T: key&gt;(obj: T) {
    <a href="transfer.md#0x2_transfer_share_object_impl">share_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="0x2_transfer_public_share_object"></a>

## Function `public_share_object`

Turn the given object into a mutable shared object that everyone can access and mutate.
This is irreversible, i.e. once an object is shared, it will stay shared forever.
Aborts with <code><a href="transfer.md#0x2_transfer_ESharedNonNewObject">ESharedNonNewObject</a></code> of the object being shared was not created in this
transaction. This restriction may be relaxed in the future.
The object must have <code>store</code> to be shared outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_public_share_object">public_share_object</a>&lt;T: store, key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_public_share_object">public_share_object</a>&lt;T: key + store&gt;(obj: T) {
    <a href="transfer.md#0x2_transfer_share_object_impl">share_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="0x2_transfer_receive"></a>

## Function `receive`

Given mutable (i.e., locked) access to the <code>parent</code> and a <code><a href="transfer.md#0x2_transfer_Receiving">Receiving</a></code> argument
referencing an object of type <code>T</code> owned by <code>parent</code> use the <code>to_receive</code>
argument to receive and return the referenced owned object of type <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_receive">receive</a>&lt;T: key&gt;(parent: &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>, to_receive: <a href="transfer.md#0x2_transfer_Receiving">transfer::Receiving</a>&lt;T&gt;): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_receive">receive</a>&lt;T: key&gt;(parent: &<b>mut</b> UID, to_receive: <a href="transfer.md#0x2_transfer_Receiving">Receiving</a>&lt;T&gt;): T {
    <b>let</b> <a href="transfer.md#0x2_transfer_Receiving">Receiving</a> {
        id,
        version,
    } = to_receive;
    <a href="transfer.md#0x2_transfer_receive_impl">receive_impl</a>(<a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(parent), id, version)
}
</code></pre>



</details>

<a name="0x2_transfer_freeze_object_impl"></a>

## Function `freeze_object_impl`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="transfer.md#0x2_transfer_freeze_object_impl">freeze_object_impl</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="transfer.md#0x2_transfer_freeze_object_impl">freeze_object_impl</a>&lt;T: key&gt;(obj: T);
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] sui::prover::shared(obj);
<b>modifies</b> [abstract] <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(<a href="object.md#0x2_object_id">object::id</a>(obj).bytes);
<b>ensures</b> [abstract] <b>exists</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(<a href="object.md#0x2_object_id">object::id</a>(obj).bytes);
<b>ensures</b> [abstract] <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(<a href="object.md#0x2_object_id">object::id</a>(obj).bytes).status == <a href="prover.md#0x2_prover_IMMUTABLE">prover::IMMUTABLE</a>;
</code></pre>



</details>

<a name="0x2_transfer_share_object_impl"></a>

## Function `share_object_impl`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="transfer.md#0x2_transfer_share_object_impl">share_object_impl</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="transfer.md#0x2_transfer_share_object_impl">share_object_impl</a>&lt;T: key&gt;(obj: T);
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] sui::prover::owned(obj);
<b>modifies</b> [abstract] <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(<a href="object.md#0x2_object_id">object::id</a>(obj).bytes);
<b>ensures</b> [abstract] <b>exists</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(<a href="object.md#0x2_object_id">object::id</a>(obj).bytes);
<b>ensures</b> [abstract] <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(<a href="object.md#0x2_object_id">object::id</a>(obj).bytes).status == <a href="prover.md#0x2_prover_SHARED">prover::SHARED</a>;
</code></pre>



</details>

<a name="0x2_transfer_transfer_impl"></a>

## Function `transfer_impl`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="transfer.md#0x2_transfer_transfer_impl">transfer_impl</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="transfer.md#0x2_transfer_transfer_impl">transfer_impl</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>);
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] sui::prover::shared(obj);
<b>modifies</b> [abstract] <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(<a href="object.md#0x2_object_id">object::id</a>(obj).bytes);
<b>ensures</b> [abstract] <b>exists</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(<a href="object.md#0x2_object_id">object::id</a>(obj).bytes);
<b>ensures</b> [abstract] <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(<a href="object.md#0x2_object_id">object::id</a>(obj).bytes).owner == recipient;
<b>ensures</b> [abstract] <b>global</b>&lt;<a href="object.md#0x2_object_Ownership">object::Ownership</a>&gt;(<a href="object.md#0x2_object_id">object::id</a>(obj).bytes).status == <a href="prover.md#0x2_prover_OWNED">prover::OWNED</a>;
</code></pre>



</details>

<a name="0x2_transfer_receive_impl"></a>

## Function `receive_impl`



<pre><code><b>fun</b> <a href="transfer.md#0x2_transfer_receive_impl">receive_impl</a>&lt;T: key&gt;(parent: <b>address</b>, to_receive: <a href="object.md#0x2_object_ID">object::ID</a>, version: u64): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="transfer.md#0x2_transfer_receive_impl">receive_impl</a>&lt;T: key&gt;(parent: <b>address</b>, to_receive: <a href="object.md#0x2_object_ID">object::ID</a>, version: u64): T;
</code></pre>



</details>
