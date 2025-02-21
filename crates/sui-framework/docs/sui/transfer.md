---
title: Module `sui::transfer`
---



-  [Struct `Receiving`](#sui_transfer_Receiving)
-  [Constants](#@Constants_0)
-  [Function `transfer`](#sui_transfer_transfer)
-  [Function `public_transfer`](#sui_transfer_public_transfer)
-  [Function `freeze_object`](#sui_transfer_freeze_object)
-  [Function `public_freeze_object`](#sui_transfer_public_freeze_object)
-  [Function `share_object`](#sui_transfer_share_object)
-  [Function `public_share_object`](#sui_transfer_public_share_object)
-  [Function `receive`](#sui_transfer_receive)
-  [Function `public_receive`](#sui_transfer_public_receive)
-  [Function `receiving_object_id`](#sui_transfer_receiving_object_id)
-  [Function `freeze_object_impl`](#sui_transfer_freeze_object_impl)
-  [Function `share_object_impl`](#sui_transfer_share_object_impl)
-  [Function `transfer_impl`](#sui_transfer_transfer_impl)
-  [Function `receive_impl`](#sui_transfer_receive_impl)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="sui_transfer_Receiving"></a>

## Struct `Receiving`

This represents the ability to <code><a href="../sui/transfer.md#sui_transfer_receive">receive</a></code> an object of type <code>T</code>.
This type is ephemeral per-transaction and cannot be stored on-chain.
This does not represent the obligation to receive the object that it
references, but simply the ability to receive the object with object ID
<code>id</code> at version <code>version</code> if you can prove mutable access to the parent
object during the transaction.
Internals of this struct are opaque outside this module.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/transfer.md#sui_transfer_Receiving">Receiving</a>&lt;<b>phantom</b> T: key&gt; <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
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


<a name="sui_transfer_EBCSSerializationFailure"></a>

Serialization of the object failed.


<pre><code><b>const</b> <a href="../sui/transfer.md#sui_transfer_EBCSSerializationFailure">EBCSSerializationFailure</a>: u64 = 1;
</code></pre>



<a name="sui_transfer_EReceivingObjectTypeMismatch"></a>

The object being received is not of the expected type.


<pre><code><b>const</b> <a href="../sui/transfer.md#sui_transfer_EReceivingObjectTypeMismatch">EReceivingObjectTypeMismatch</a>: u64 = 2;
</code></pre>



<a name="sui_transfer_ESharedNonNewObject"></a>

Shared an object that was previously created. Shared objects must currently
be constructed in the transaction they are created.


<pre><code><b>const</b> <a href="../sui/transfer.md#sui_transfer_ESharedNonNewObject">ESharedNonNewObject</a>: u64 = 0;
</code></pre>



<a name="sui_transfer_ESharedObjectOperationNotSupported"></a>

Shared object operations such as wrapping, freezing, and converting to owned are not allowed.


<pre><code><b>const</b> <a href="../sui/transfer.md#sui_transfer_ESharedObjectOperationNotSupported">ESharedObjectOperationNotSupported</a>: u64 = 4;
</code></pre>



<a name="sui_transfer_EUnableToReceiveObject"></a>

Represents both the case where the object does not exist and the case where the object is not
able to be accessed through the parent that is passed-in.


<pre><code><b>const</b> <a href="../sui/transfer.md#sui_transfer_EUnableToReceiveObject">EUnableToReceiveObject</a>: u64 = 3;
</code></pre>



<a name="sui_transfer_transfer"></a>

## Function `transfer`

Transfer ownership of <code>obj</code> to <code>recipient</code>. <code>obj</code> must have the <code>key</code> attribute,
which (in turn) ensures that <code>obj</code> has a globally unique ID. Note that if the recipient
address represents an object ID, the <code>obj</code> sent will be inaccessible after the transfer
(though they will be retrievable at a future date once new features are added).
This function has custom rules performed by the Sui Move bytecode verifier that ensures
that <code>T</code> is an object defined in the module where <code><a href="../sui/transfer.md#sui_transfer">transfer</a></code> is invoked. Use
<code><a href="../sui/transfer.md#sui_transfer_public_transfer">public_transfer</a></code> to transfer an object with <code>store</code> outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer">transfer</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer">transfer</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>) {
    <a href="../sui/transfer.md#sui_transfer_transfer_impl">transfer_impl</a>(obj, recipient)
}
</code></pre>



</details>

<a name="sui_transfer_public_transfer"></a>

## Function `public_transfer`

Transfer ownership of <code>obj</code> to <code>recipient</code>. <code>obj</code> must have the <code>key</code> attribute,
which (in turn) ensures that <code>obj</code> has a globally unique ID. Note that if the recipient
address represents an object ID, the <code>obj</code> sent will be inaccessible after the transfer
(though they will be retrievable at a future date once new features are added).
The object must have <code>store</code> to be transferred outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_public_transfer">public_transfer</a>&lt;T: key, store&gt;(obj: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_public_transfer">public_transfer</a>&lt;T: key + store&gt;(obj: T, recipient: <b>address</b>) {
    <a href="../sui/transfer.md#sui_transfer_transfer_impl">transfer_impl</a>(obj, recipient)
}
</code></pre>



</details>

<a name="sui_transfer_freeze_object"></a>

## Function `freeze_object`

Freeze <code>obj</code>. After freezing <code>obj</code> becomes immutable and can no longer be transferred or
mutated.
This function has custom rules performed by the Sui Move bytecode verifier that ensures
that <code>T</code> is an object defined in the module where <code><a href="../sui/transfer.md#sui_transfer_freeze_object">freeze_object</a></code> is invoked. Use
<code><a href="../sui/transfer.md#sui_transfer_public_freeze_object">public_freeze_object</a></code> to freeze an object with <code>store</code> outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_freeze_object">freeze_object</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_freeze_object">freeze_object</a>&lt;T: key&gt;(obj: T) {
    <a href="../sui/transfer.md#sui_transfer_freeze_object_impl">freeze_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="sui_transfer_public_freeze_object"></a>

## Function `public_freeze_object`

Freeze <code>obj</code>. After freezing <code>obj</code> becomes immutable and can no longer be transferred or
mutated.
The object must have <code>store</code> to be frozen outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_public_freeze_object">public_freeze_object</a>&lt;T: key, store&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_public_freeze_object">public_freeze_object</a>&lt;T: key + store&gt;(obj: T) {
    <a href="../sui/transfer.md#sui_transfer_freeze_object_impl">freeze_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="sui_transfer_share_object"></a>

## Function `share_object`

Turn the given object into a mutable shared object that everyone can access and mutate.
This is irreversible, i.e. once an object is shared, it will stay shared forever.
Aborts with <code><a href="../sui/transfer.md#sui_transfer_ESharedNonNewObject">ESharedNonNewObject</a></code> of the object being shared was not created in this
transaction. This restriction may be relaxed in the future.
This function has custom rules performed by the Sui Move bytecode verifier that ensures
that <code>T</code> is an object defined in the module where <code><a href="../sui/transfer.md#sui_transfer_share_object">share_object</a></code> is invoked. Use
<code><a href="../sui/transfer.md#sui_transfer_public_share_object">public_share_object</a></code> to share an object with <code>store</code> outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_share_object">share_object</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_share_object">share_object</a>&lt;T: key&gt;(obj: T) {
    <a href="../sui/transfer.md#sui_transfer_share_object_impl">share_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="sui_transfer_public_share_object"></a>

## Function `public_share_object`

Turn the given object into a mutable shared object that everyone can access and mutate.
This is irreversible, i.e. once an object is shared, it will stay shared forever.
Aborts with <code><a href="../sui/transfer.md#sui_transfer_ESharedNonNewObject">ESharedNonNewObject</a></code> of the object being shared was not created in this
transaction. This restriction may be relaxed in the future.
The object must have <code>store</code> to be shared outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_public_share_object">public_share_object</a>&lt;T: key, store&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_public_share_object">public_share_object</a>&lt;T: key + store&gt;(obj: T) {
    <a href="../sui/transfer.md#sui_transfer_share_object_impl">share_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="sui_transfer_receive"></a>

## Function `receive`

Given mutable (i.e., locked) access to the <code>parent</code> and a <code><a href="../sui/transfer.md#sui_transfer_Receiving">Receiving</a></code> argument
referencing an object of type <code>T</code> owned by <code>parent</code> use the <code>to_receive</code>
argument to receive and return the referenced owned object of type <code>T</code>.
This function has custom rules performed by the Sui Move bytecode verifier that ensures
that <code>T</code> is an object defined in the module where <code><a href="../sui/transfer.md#sui_transfer_receive">receive</a></code> is invoked. Use
<code><a href="../sui/transfer.md#sui_transfer_public_receive">public_receive</a></code> to receivne an object with <code>store</code> outside of its module.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_receive">receive</a>&lt;T: key&gt;(parent: &<b>mut</b> <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, to_receive: <a href="../sui/transfer.md#sui_transfer_Receiving">sui::transfer::Receiving</a>&lt;T&gt;): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_receive">receive</a>&lt;T: key&gt;(parent: &<b>mut</b> UID, to_receive: <a href="../sui/transfer.md#sui_transfer_Receiving">Receiving</a>&lt;T&gt;): T {
    <b>let</b> <a href="../sui/transfer.md#sui_transfer_Receiving">Receiving</a> { id, version } = to_receive;
    <a href="../sui/transfer.md#sui_transfer_receive_impl">receive_impl</a>(parent.to_address(), id, version)
}
</code></pre>



</details>

<a name="sui_transfer_public_receive"></a>

## Function `public_receive`

Given mutable (i.e., locked) access to the <code>parent</code> and a <code><a href="../sui/transfer.md#sui_transfer_Receiving">Receiving</a></code> argument
referencing an object of type <code>T</code> owned by <code>parent</code> use the <code>to_receive</code>
argument to receive and return the referenced owned object of type <code>T</code>.
The object must have <code>store</code> to be received outside of its defining module.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_public_receive">public_receive</a>&lt;T: key, store&gt;(parent: &<b>mut</b> <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, to_receive: <a href="../sui/transfer.md#sui_transfer_Receiving">sui::transfer::Receiving</a>&lt;T&gt;): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_public_receive">public_receive</a>&lt;T: key + store&gt;(parent: &<b>mut</b> UID, to_receive: <a href="../sui/transfer.md#sui_transfer_Receiving">Receiving</a>&lt;T&gt;): T {
    <b>let</b> <a href="../sui/transfer.md#sui_transfer_Receiving">Receiving</a> { id, version } = to_receive;
    <a href="../sui/transfer.md#sui_transfer_receive_impl">receive_impl</a>(parent.to_address(), id, version)
}
</code></pre>



</details>

<a name="sui_transfer_receiving_object_id"></a>

## Function `receiving_object_id`

Return the object ID that the given <code><a href="../sui/transfer.md#sui_transfer_Receiving">Receiving</a></code> argument references.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_receiving_object_id">receiving_object_id</a>&lt;T: key&gt;(receiving: &<a href="../sui/transfer.md#sui_transfer_Receiving">sui::transfer::Receiving</a>&lt;T&gt;): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_receiving_object_id">receiving_object_id</a>&lt;T: key&gt;(receiving: &<a href="../sui/transfer.md#sui_transfer_Receiving">Receiving</a>&lt;T&gt;): ID {
    receiving.id
}
</code></pre>



</details>

<a name="sui_transfer_freeze_object_impl"></a>

## Function `freeze_object_impl`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/transfer.md#sui_transfer_freeze_object_impl">freeze_object_impl</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_freeze_object_impl">freeze_object_impl</a>&lt;T: key&gt;(obj: T);
</code></pre>



</details>

<a name="sui_transfer_share_object_impl"></a>

## Function `share_object_impl`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/transfer.md#sui_transfer_share_object_impl">share_object_impl</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_share_object_impl">share_object_impl</a>&lt;T: key&gt;(obj: T);
</code></pre>



</details>

<a name="sui_transfer_transfer_impl"></a>

## Function `transfer_impl`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/transfer.md#sui_transfer_transfer_impl">transfer_impl</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_transfer_impl">transfer_impl</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>);
</code></pre>



</details>

<a name="sui_transfer_receive_impl"></a>

## Function `receive_impl`



<pre><code><b>fun</b> <a href="../sui/transfer.md#sui_transfer_receive_impl">receive_impl</a>&lt;T: key&gt;(parent: <b>address</b>, to_receive: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, version: u64): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer_receive_impl">receive_impl</a>&lt;T: key&gt;(parent: <b>address</b>, to_receive: ID, version: u64): T;
</code></pre>



</details>
