---
title: Module `sui::accumulator`
---



-  [Struct `AccumulatorRoot`](#sui_accumulator_AccumulatorRoot)
-  [Struct `Key`](#sui_accumulator_Key)
-  [Struct `OwnerKey`](#sui_accumulator_OwnerKey)
-  [Struct `Owner`](#sui_accumulator_Owner)
-  [Struct `MetadataKey`](#sui_accumulator_MetadataKey)
-  [Struct `Metadata`](#sui_accumulator_Metadata)
-  [Struct `U128`](#sui_accumulator_U128)
-  [Constants](#@Constants_0)
-  [Function `create`](#sui_accumulator_create)
-  [Function `accumulator_address`](#sui_accumulator_accumulator_address)
-  [Function `root_has_accumulator`](#sui_accumulator_root_has_accumulator)
-  [Function `root_add_accumulator`](#sui_accumulator_root_add_accumulator)
-  [Function `root_borrow_accumulator_mut`](#sui_accumulator_root_borrow_accumulator_mut)
-  [Function `root_remove_accumulator`](#sui_accumulator_root_remove_accumulator)
-  [Function `accumulator_root_owner_exists`](#sui_accumulator_accumulator_root_owner_exists)
-  [Function `accumulator_root_borrow_owner_mut`](#sui_accumulator_accumulator_root_borrow_owner_mut)
-  [Function `accumulator_root_attach_owner`](#sui_accumulator_accumulator_root_attach_owner)
-  [Function `accumulator_root_detach_owner`](#sui_accumulator_accumulator_root_detach_owner)
-  [Function `create_accumulator_metadata`](#sui_accumulator_create_accumulator_metadata)
-  [Function `accumulator_metadata_remove`](#sui_accumulator_accumulator_metadata_remove)
-  [Function `accumulator_owner_attach_metadata`](#sui_accumulator_accumulator_owner_attach_metadata)
-  [Function `accumulator_owner_detach_metadata`](#sui_accumulator_accumulator_owner_detach_metadata)
-  [Function `accumulator_owner_destroy`](#sui_accumulator_accumulator_owner_destroy)
-  [Function `settlement_prologue`](#sui_accumulator_settlement_prologue)
-  [Function `settle_u128`](#sui_accumulator_settle_u128)
-  [Function `emit_deposit_event`](#sui_accumulator_emit_deposit_event)
-  [Function `emit_withdraw_event`](#sui_accumulator_emit_withdraw_event)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_accumulator_AccumulatorRoot"></a>

## Struct `AccumulatorRoot`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_accumulator_Key"></a>

## Struct `Key`

<code><a href="../sui/accumulator.md#sui_accumulator_Key">Key</a></code> is used only for computing the field id of accumulator objects.
<code>T</code> is the type of the accumulated value, e.g. <code>Balance&lt;SUI&gt;</code>


<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_Key">Key</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><b>address</b>: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_accumulator_OwnerKey"></a>

## Struct `OwnerKey`

=== Accumulator metadata ===

Accumulator metadata is organized as follows:
- Each address that holds at least one type of accumulator has an owner object attached
to the accumulator root.
- For each type of accumulator held by that address, there is an AccumulatorMetadata object
attached to the owner object.
- When the value of an accumulator drops to zero, the metadata object is removed.
- If the owner object has no more accumulator metadata objects attached to it, it is removed
as well.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_OwnerKey">OwnerKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>owner: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_accumulator_Owner"></a>

## Struct `Owner`

An owner object, to which all AccumulatorMetadata objects for the owner are
attached.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_Owner">Owner</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>balances: <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a></code>
</dt>
<dd>
 The individual balances owned by the owner.
</dd>
<dt>
<code>owner: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_accumulator_MetadataKey"></a>

## Struct `MetadataKey`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_MetadataKey">MetadataKey</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_accumulator_Metadata"></a>

## Struct `Metadata`

A metadata object for a balance object with type T.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_Metadata">Metadata</a>&lt;<b>phantom</b> T&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>fields: <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a></code>
</dt>
<dd>
 Any per-balance fields we wish to add in the future.
</dd>
</dl>


</details>

<a name="sui_accumulator_U128"></a>

## Struct `U128`

Storage for 128-bit accumulator values.

Currently only used to represent the sum of 64 bit values (such as <code>Balance&lt;T&gt;</code>).
The additional bits are necessary to prevent overflow, as it would take 2^64 deposits of U64_MAX
to cause an overflow.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>value: u128</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_accumulator_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../sui/accumulator.md#sui_accumulator_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="sui_accumulator_EInvalidSplitAmount"></a>



<pre><code><b>const</b> <a href="../sui/accumulator.md#sui_accumulator_EInvalidSplitAmount">EInvalidSplitAmount</a>: u64 = 1;
</code></pre>



<a name="sui_accumulator_EInvariantViolation"></a>



<pre><code><b>const</b> <a href="../sui/accumulator.md#sui_accumulator_EInvariantViolation">EInvariantViolation</a>: u64 = 2;
</code></pre>



<a name="sui_accumulator_create"></a>

## Function `create`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_create">create</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/accumulator.md#sui_accumulator_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a> {
        id: <a href="../sui/object.md#sui_object_sui_accumulator_root_object_id">object::sui_accumulator_root_object_id</a>(),
    })
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_address"></a>

## Function `accumulator_address`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_address">accumulator_address</a>&lt;T&gt;(<b>address</b>: <b>address</b>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_address">accumulator_address</a>&lt;T&gt;(<b>address</b>: <b>address</b>): <b>address</b> {
    <b>let</b> key = <a href="../sui/accumulator.md#sui_accumulator_Key">Key</a>&lt;T&gt; { <b>address</b> };
    <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">dynamic_field::hash_type_and_key</a>(sui_accumulator_root_address(), key)
}
</code></pre>



</details>

<a name="sui_accumulator_root_has_accumulator"></a>

## Function `root_has_accumulator`

Balance object methods


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_root_has_accumulator">root_has_accumulator</a>&lt;K, V: store&gt;(accumulator_root: &<a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, name: <a href="../sui/accumulator.md#sui_accumulator_Key">sui::accumulator::Key</a>&lt;K&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_root_has_accumulator">root_has_accumulator</a>&lt;K, V: store&gt;(accumulator_root: &<a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>, name: <a href="../sui/accumulator.md#sui_accumulator_Key">Key</a>&lt;K&gt;): bool {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_exists_with_type">dynamic_field::exists_with_type</a>&lt;<a href="../sui/accumulator.md#sui_accumulator_Key">Key</a>&lt;K&gt;, V&gt;(&accumulator_root.id, name)
}
</code></pre>



</details>

<a name="sui_accumulator_root_add_accumulator"></a>

## Function `root_add_accumulator`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_root_add_accumulator">root_add_accumulator</a>&lt;K, V: store&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, name: <a href="../sui/accumulator.md#sui_accumulator_Key">sui::accumulator::Key</a>&lt;K&gt;, value: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_root_add_accumulator">root_add_accumulator</a>&lt;K, V: store&gt;(
    accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>,
    name: <a href="../sui/accumulator.md#sui_accumulator_Key">Key</a>&lt;K&gt;,
    value: V,
) {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> accumulator_root.id, name, value);
}
</code></pre>



</details>

<a name="sui_accumulator_root_borrow_accumulator_mut"></a>

## Function `root_borrow_accumulator_mut`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_root_borrow_accumulator_mut">root_borrow_accumulator_mut</a>&lt;K, V: store&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, name: <a href="../sui/accumulator.md#sui_accumulator_Key">sui::accumulator::Key</a>&lt;K&gt;): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_root_borrow_accumulator_mut">root_borrow_accumulator_mut</a>&lt;K, V: store&gt;(
    accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>,
    name: <a href="../sui/accumulator.md#sui_accumulator_Key">Key</a>&lt;K&gt;,
): &<b>mut</b> V {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>&lt;<a href="../sui/accumulator.md#sui_accumulator_Key">Key</a>&lt;K&gt;, V&gt;(&<b>mut</b> accumulator_root.id, name)
}
</code></pre>



</details>

<a name="sui_accumulator_root_remove_accumulator"></a>

## Function `root_remove_accumulator`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_root_remove_accumulator">root_remove_accumulator</a>&lt;K, V: store&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, name: <a href="../sui/accumulator.md#sui_accumulator_Key">sui::accumulator::Key</a>&lt;K&gt;): V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_root_remove_accumulator">root_remove_accumulator</a>&lt;K, V: store&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>, name: <a href="../sui/accumulator.md#sui_accumulator_Key">Key</a>&lt;K&gt;): V {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_remove">dynamic_field::remove</a>&lt;<a href="../sui/accumulator.md#sui_accumulator_Key">Key</a>&lt;K&gt;, V&gt;(&<b>mut</b> accumulator_root.id, name)
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_root_owner_exists"></a>

## Function `accumulator_root_owner_exists`

=== Owner functions ===
Check if there is an owner object attached to the accumulator root.


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_owner_exists">accumulator_root_owner_exists</a>(accumulator_root: &<a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_owner_exists">accumulator_root_owner_exists</a>(accumulator_root: &<a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>, owner: <b>address</b>): bool {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_exists_with_type">dynamic_field::exists_with_type</a>&lt;<a href="../sui/accumulator.md#sui_accumulator_OwnerKey">OwnerKey</a>, <a href="../sui/accumulator.md#sui_accumulator_Owner">Owner</a>&gt;(&accumulator_root.id, <a href="../sui/accumulator.md#sui_accumulator_OwnerKey">OwnerKey</a> { owner })
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_root_borrow_owner_mut"></a>

## Function `accumulator_root_borrow_owner_mut`

Borrow an owner object mutably.


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_borrow_owner_mut">accumulator_root_borrow_owner_mut</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>): &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_Owner">sui::accumulator::Owner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_borrow_owner_mut">accumulator_root_borrow_owner_mut</a>(
    accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>,
    owner: <b>address</b>,
): &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_Owner">Owner</a> {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>(&<b>mut</b> accumulator_root.id, <a href="../sui/accumulator.md#sui_accumulator_OwnerKey">OwnerKey</a> { owner })
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_root_attach_owner"></a>

## Function `accumulator_root_attach_owner`

Attach an owner object to the accumulator root.


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_attach_owner">accumulator_root_attach_owner</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <a href="../sui/accumulator.md#sui_accumulator_Owner">sui::accumulator::Owner</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_attach_owner">accumulator_root_attach_owner</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>, owner: <a href="../sui/accumulator.md#sui_accumulator_Owner">Owner</a>) {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> accumulator_root.id, <a href="../sui/accumulator.md#sui_accumulator_OwnerKey">OwnerKey</a> { owner: owner.owner }, owner);
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_root_detach_owner"></a>

## Function `accumulator_root_detach_owner`

Detach an owner object from the accumulator root.


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_detach_owner">accumulator_root_detach_owner</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>): <a href="../sui/accumulator.md#sui_accumulator_Owner">sui::accumulator::Owner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_root_detach_owner">accumulator_root_detach_owner</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>, owner: <b>address</b>): <a href="../sui/accumulator.md#sui_accumulator_Owner">Owner</a> {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_remove">dynamic_field::remove</a>(&<b>mut</b> accumulator_root.id, <a href="../sui/accumulator.md#sui_accumulator_OwnerKey">OwnerKey</a> { owner })
}
</code></pre>



</details>

<a name="sui_accumulator_create_accumulator_metadata"></a>

## Function `create_accumulator_metadata`

=== Metadata functions ===
Create a metadata object for a new balance object with type T.
The metadata will be attached to the owner object <code>owner</code>.
If the owner object does not exist, it will be created.


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_create_accumulator_metadata">create_accumulator_metadata</a>&lt;T&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_create_accumulator_metadata">create_accumulator_metadata</a>&lt;T&gt;(
    accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>,
    owner: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> metadata = <a href="../sui/accumulator.md#sui_accumulator_Metadata">Metadata</a>&lt;T&gt; {
        fields: <a href="../sui/bag.md#sui_bag_new">bag::new</a>(ctx),
    };
    <b>if</b> (accumulator_root.owner_exists(owner)) {
        <b>let</b> accumulator_owner = accumulator_root.borrow_owner_mut(owner);
        <b>assert</b>!(accumulator_owner.owner == owner, <a href="../sui/accumulator.md#sui_accumulator_EInvariantViolation">EInvariantViolation</a>);
        accumulator_owner.attach_metadata(metadata);
    } <b>else</b> {
        <b>let</b> <b>mut</b> accumulator_owner = <a href="../sui/accumulator.md#sui_accumulator_Owner">Owner</a> {
            balances: <a href="../sui/bag.md#sui_bag_new">bag::new</a>(ctx),
            owner,
        };
        accumulator_owner.attach_metadata(metadata);
        accumulator_root.attach_owner(accumulator_owner);
    }
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_metadata_remove"></a>

## Function `accumulator_metadata_remove`

Remove the metadata object for a balance object with type T.
The metadata will be detached from the owner object <code>owner</code>.
If there are no more balance objects attached to the owner object,
the owner object will be destroyed.


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_metadata_remove">accumulator_metadata_remove</a>&lt;T&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_metadata_remove">accumulator_metadata_remove</a>&lt;T&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>, owner: <b>address</b>) {
    <b>let</b> is_empty = {
        <b>let</b> accumulator_owner = accumulator_root.borrow_owner_mut(owner);
        <b>let</b> <a href="../sui/accumulator.md#sui_accumulator_Metadata">Metadata</a> { fields } = accumulator_owner.detach_metadata&lt;T&gt;();
        fields.destroy_empty();
        accumulator_owner.balances.is_empty()
    };
    <b>if</b> (is_empty) {
        <b>let</b> owner = accumulator_root.detach_owner(owner);
        owner.destroy();
    }
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_owner_attach_metadata"></a>

## Function `accumulator_owner_attach_metadata`

Attach a metadata object for type T to the owner object.


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_owner_attach_metadata">accumulator_owner_attach_metadata</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_Owner">sui::accumulator::Owner</a>, metadata: <a href="../sui/accumulator.md#sui_accumulator_Metadata">sui::accumulator::Metadata</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_owner_attach_metadata">accumulator_owner_attach_metadata</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_Owner">Owner</a>, metadata: <a href="../sui/accumulator.md#sui_accumulator_Metadata">Metadata</a>&lt;T&gt;) {
    self.balances.add(<a href="../sui/accumulator.md#sui_accumulator_MetadataKey">MetadataKey</a>&lt;T&gt; {}, metadata);
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_owner_detach_metadata"></a>

## Function `accumulator_owner_detach_metadata`

Detach a metadata object for type T from the owner object.


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_owner_detach_metadata">accumulator_owner_detach_metadata</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_Owner">sui::accumulator::Owner</a>): <a href="../sui/accumulator.md#sui_accumulator_Metadata">sui::accumulator::Metadata</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_owner_detach_metadata">accumulator_owner_detach_metadata</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_Owner">Owner</a>): <a href="../sui/accumulator.md#sui_accumulator_Metadata">Metadata</a>&lt;T&gt; {
    self.balances.remove(<a href="../sui/accumulator.md#sui_accumulator_MetadataKey">MetadataKey</a>&lt;T&gt; {})
}
</code></pre>



</details>

<a name="sui_accumulator_accumulator_owner_destroy"></a>

## Function `accumulator_owner_destroy`

Destroy an owner object.


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_owner_destroy">accumulator_owner_destroy</a>(this: <a href="../sui/accumulator.md#sui_accumulator_Owner">sui::accumulator::Owner</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_accumulator_owner_destroy">accumulator_owner_destroy</a>(this: <a href="../sui/accumulator.md#sui_accumulator_Owner">Owner</a>) {
    <b>let</b> <a href="../sui/accumulator.md#sui_accumulator_Owner">Owner</a> { balances, .. } = this;
    balances.destroy_empty();
}
</code></pre>



</details>

<a name="sui_accumulator_settlement_prologue"></a>

## Function `settlement_prologue`

Called by settlement transactions to ensure that the settlement transaction has a unique
digest.


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_settlement_prologue">settlement_prologue</a>(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_settlement_prologue">settlement_prologue</a>(_epoch: u64, _checkpoint_height: u64, _idx: u64, ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/accumulator.md#sui_accumulator_ENotSystemAddress">ENotSystemAddress</a>);
}
</code></pre>



</details>

<a name="sui_accumulator_settle_u128"></a>

## Function `settle_u128`



<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_settle_u128">settle_u128</a>&lt;T&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>, merge: u128, split: u128, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_settle_u128">settle_u128</a>&lt;T&gt;(
    accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">AccumulatorRoot</a>,
    owner: <b>address</b>,
    merge: u128,
    split: u128,
    ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/accumulator.md#sui_accumulator_ENotSystemAddress">ENotSystemAddress</a>);
    // Merge and split should be netted out prior to calling this function.
    <b>assert</b>!((merge == 0 ) != (split == 0), <a href="../sui/accumulator.md#sui_accumulator_EInvalidSplitAmount">EInvalidSplitAmount</a>);
    <b>let</b> name = <a href="../sui/accumulator.md#sui_accumulator_Key">Key</a>&lt;T&gt; { <b>address</b>: owner };
    <b>if</b> (accumulator_root.has_accumulator&lt;T, <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a>&gt;(name)) {
        <b>let</b> is_zero = {
            <b>let</b> value: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a> = accumulator_root.borrow_accumulator_mut(name);
            value.value = value.value + merge - split;
            value.value == 0
        };
        <b>if</b> (is_zero) {
            <b>let</b> <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a> { value: _ } = accumulator_root.remove_accumulator&lt;T, <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a>&gt;(
                name,
            );
            accumulator_root.remove_metadata&lt;T&gt;(owner);
        }
    } <b>else</b> {
        // cannot split <b>if</b> the field does not yet exist
        <b>assert</b>!(split == 0, <a href="../sui/accumulator.md#sui_accumulator_EInvalidSplitAmount">EInvalidSplitAmount</a>);
        <b>let</b> value = <a href="../sui/accumulator.md#sui_accumulator_U128">U128</a> {
            value: merge,
        };
        accumulator_root.add_accumulator(name, value);
        accumulator_root.create_metadata&lt;T&gt;(owner, ctx);
    };
}
</code></pre>



</details>

<a name="sui_accumulator_emit_deposit_event"></a>

## Function `emit_deposit_event`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_emit_deposit_event">emit_deposit_event</a>&lt;T&gt;(<a href="../sui/accumulator.md#sui_accumulator">accumulator</a>: <b>address</b>, recipient: <b>address</b>, amount: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_emit_deposit_event">emit_deposit_event</a>&lt;T&gt;(
    <a href="../sui/accumulator.md#sui_accumulator">accumulator</a>: <b>address</b>,
    recipient: <b>address</b>,
    amount: u64,
);
</code></pre>



</details>

<a name="sui_accumulator_emit_withdraw_event"></a>

## Function `emit_withdraw_event`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_emit_withdraw_event">emit_withdraw_event</a>&lt;T&gt;(<a href="../sui/accumulator.md#sui_accumulator">accumulator</a>: <b>address</b>, owner: <b>address</b>, amount: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/accumulator.md#sui_accumulator_emit_withdraw_event">emit_withdraw_event</a>&lt;T&gt;(
    <a href="../sui/accumulator.md#sui_accumulator">accumulator</a>: <b>address</b>,
    owner: <b>address</b>,
    amount: u64,
);
</code></pre>



</details>
