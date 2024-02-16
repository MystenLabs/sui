
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
-  [Function `public_receive`](#0x2_transfer_public_receive)
-  [Function `receiving_object_id`](#0x2_transfer_receiving_object_id)
-  [Function `freeze_object_impl`](#0x2_transfer_freeze_object_impl)
-  [Function `share_object_impl`](#0x2_transfer_share_object_impl)
-  [Function `transfer_impl`](#0x2_transfer_transfer_impl)
-  [Function `receive_impl`](#0x2_transfer_receive_impl)


<pre><code><b>use</b> <a href="../../dependencies/sui-framework/object.md#0x2_object">0x2::object</a>;
</code></pre>



<a name="0x2_transfer_Receiving"></a>

## Struct `Receiving`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_Receiving">Receiving</a>&lt;T: key&gt; <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a></code>
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


<a name="0x2_transfer_EBCSSerializationFailure"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_EBCSSerializationFailure">EBCSSerializationFailure</a>: u64 = 1;
</code></pre>



<a name="0x2_transfer_EReceivingObjectTypeMismatch"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_EReceivingObjectTypeMismatch">EReceivingObjectTypeMismatch</a>: u64 = 2;
</code></pre>



<a name="0x2_transfer_ESharedNonNewObject"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_ESharedNonNewObject">ESharedNonNewObject</a>: u64 = 0;
</code></pre>



<a name="0x2_transfer_ESharedObjectOperationNotSupported"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_ESharedObjectOperationNotSupported">ESharedObjectOperationNotSupported</a>: u64 = 4;
</code></pre>



<a name="0x2_transfer_EUnableToReceiveObject"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_EUnableToReceiveObject">EUnableToReceiveObject</a>: u64 = 3;
</code></pre>



<a name="0x2_transfer_transfer"></a>

## Function `transfer`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer">transfer</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer">transfer</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>) {
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_transfer_impl">transfer_impl</a>(obj, recipient)
}
</code></pre>



</details>

<a name="0x2_transfer_public_transfer"></a>

## Function `public_transfer`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">public_transfer</a>&lt;T: store, key&gt;(obj: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">public_transfer</a>&lt;T: key + store&gt;(obj: T, recipient: <b>address</b>) {
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_transfer_impl">transfer_impl</a>(obj, recipient)
}
</code></pre>



</details>

<a name="0x2_transfer_freeze_object"></a>

## Function `freeze_object`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_freeze_object">freeze_object</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_freeze_object">freeze_object</a>&lt;T: key&gt;(obj: T) {
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_freeze_object_impl">freeze_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="0x2_transfer_public_freeze_object"></a>

## Function `public_freeze_object`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_freeze_object">public_freeze_object</a>&lt;T: store, key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_freeze_object">public_freeze_object</a>&lt;T: key + store&gt;(obj: T) {
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_freeze_object_impl">freeze_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="0x2_transfer_share_object"></a>

## Function `share_object`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_share_object">share_object</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_share_object">share_object</a>&lt;T: key&gt;(obj: T) {
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_share_object_impl">share_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="0x2_transfer_public_share_object"></a>

## Function `public_share_object`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_share_object">public_share_object</a>&lt;T: store, key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_share_object">public_share_object</a>&lt;T: key + store&gt;(obj: T) {
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_share_object_impl">share_object_impl</a>(obj)
}
</code></pre>



</details>

<a name="0x2_transfer_receive"></a>

## Function `receive`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_receive">receive</a>&lt;T: key&gt;(parent: &<b>mut</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>, to_receive: <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_Receiving">transfer::Receiving</a>&lt;T&gt;): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_receive">receive</a>&lt;T: key&gt;(parent: &<b>mut</b> UID, to_receive: <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_Receiving">Receiving</a>&lt;T&gt;): T {
    <b>let</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_Receiving">Receiving</a> {
        id,
        version,
    } = to_receive;
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_receive_impl">receive_impl</a>(<a href="../../dependencies/sui-framework/object.md#0x2_object_uid_to_address">object::uid_to_address</a>(parent), id, version)
}
</code></pre>



</details>

<a name="0x2_transfer_public_receive"></a>

## Function `public_receive`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_receive">public_receive</a>&lt;T: store, key&gt;(parent: &<b>mut</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>, to_receive: <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_Receiving">transfer::Receiving</a>&lt;T&gt;): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_receive">public_receive</a>&lt;T: key + store&gt;(parent: &<b>mut</b> UID, to_receive: <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_Receiving">Receiving</a>&lt;T&gt;): T {
    <b>let</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_Receiving">Receiving</a> {
        id,
        version,
    } = to_receive;
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_receive_impl">receive_impl</a>(<a href="../../dependencies/sui-framework/object.md#0x2_object_uid_to_address">object::uid_to_address</a>(parent), id, version)
}
</code></pre>



</details>

<a name="0x2_transfer_receiving_object_id"></a>

## Function `receiving_object_id`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_receiving_object_id">receiving_object_id</a>&lt;T: key&gt;(receiving: &<a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_Receiving">transfer::Receiving</a>&lt;T&gt;): <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_receiving_object_id">receiving_object_id</a>&lt;T: key&gt;(receiving: &<a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_Receiving">Receiving</a>&lt;T&gt;): ID {
    receiving.id
}
</code></pre>



</details>

<a name="0x2_transfer_freeze_object_impl"></a>

## Function `freeze_object_impl`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_freeze_object_impl">freeze_object_impl</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_freeze_object_impl">freeze_object_impl</a>&lt;T: key&gt;(obj: T);
</code></pre>



</details>

<a name="0x2_transfer_share_object_impl"></a>

## Function `share_object_impl`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_share_object_impl">share_object_impl</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_share_object_impl">share_object_impl</a>&lt;T: key&gt;(obj: T);
</code></pre>



</details>

<a name="0x2_transfer_transfer_impl"></a>

## Function `transfer_impl`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_transfer_impl">transfer_impl</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_transfer_impl">transfer_impl</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>);
</code></pre>



</details>

<a name="0x2_transfer_receive_impl"></a>

## Function `receive_impl`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_receive_impl">receive_impl</a>&lt;T: key&gt;(parent: <b>address</b>, to_receive: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>, version: u64): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_receive_impl">receive_impl</a>&lt;T: key&gt;(parent: <b>address</b>, to_receive: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>, version: u64): T;
</code></pre>



</details>
