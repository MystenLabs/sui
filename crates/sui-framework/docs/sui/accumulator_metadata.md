---
title: Module `sui::accumulator_metadata`
---



-  [Struct `OwnerKey`](#sui_accumulator_metadata_OwnerKey)
-  [Struct `Owner`](#sui_accumulator_metadata_Owner)
-  [Struct `MetadataKey`](#sui_accumulator_metadata_MetadataKey)
-  [Struct `Metadata`](#sui_accumulator_metadata_Metadata)
-  [Struct `AccumulatorObjectCountKey`](#sui_accumulator_metadata_AccumulatorObjectCountKey)
-  [Constants](#@Constants_0)
-  [Function `record_accumulator_object_changes`](#sui_accumulator_metadata_record_accumulator_object_changes)
-  [Function `get_accumulator_object_count`](#sui_accumulator_metadata_get_accumulator_object_count)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/accumulator.md#sui_accumulator">sui::accumulator</a>;
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



<a name="sui_accumulator_metadata_OwnerKey"></a>

## Struct `OwnerKey`

=== Accumulator metadata ===

Metadata system has been removed, but structs must remain for backwards compatibility.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_OwnerKey">OwnerKey</a> <b>has</b> <b>copy</b>, drop, store
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

<a name="sui_accumulator_metadata_Owner"></a>

## Struct `Owner`

An owner field, to which all AccumulatorMetadata fields for the owner are
attached.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">Owner</a> <b>has</b> store
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

<a name="sui_accumulator_metadata_MetadataKey"></a>

## Struct `MetadataKey`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_MetadataKey">MetadataKey</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_accumulator_metadata_Metadata"></a>

## Struct `Metadata`

A metadata field for a balance field with type T.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Metadata">Metadata</a>&lt;<b>phantom</b> T&gt; <b>has</b> store
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

<a name="sui_accumulator_metadata_AccumulatorObjectCountKey"></a>

## Struct `AccumulatorObjectCountKey`

=== Accumulator object count storage ===
Key for storing the net count of accumulator objects as a dynamic field on the accumulator root.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_AccumulatorObjectCountKey">AccumulatorObjectCountKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_accumulator_metadata_EInvariantViolation"></a>



<pre><code><b>const</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_EInvariantViolation">EInvariantViolation</a>: u64 = 0;
</code></pre>



<a name="sui_accumulator_metadata_record_accumulator_object_changes"></a>

## Function `record_accumulator_object_changes`

Records changes in the net count of accumulator objects. Called by the barrier transaction
as part of accumulator settlement.

This value is copied to the Sui system state object at end-of-epoch by the
WriteAccumulatorStorageCost transaction, for use in storage fund accounting. Copying once
at end-of-epoch lets us avoid depending on the Sui system state object in the settlement
barrier transaction.


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_record_accumulator_object_changes">record_accumulator_object_changes</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, objects_created: u64, objects_destroyed: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_record_accumulator_object_changes">record_accumulator_object_changes</a>(
    accumulator_root: &<b>mut</b> AccumulatorRoot,
    objects_created: u64,
    objects_destroyed: u64,
) {
    <b>let</b> key = <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_AccumulatorObjectCountKey">AccumulatorObjectCountKey</a>();
    <b>if</b> (<a href="../sui/dynamic_field.md#sui_dynamic_field_exists_">dynamic_field::exists_</a>(accumulator_root.id_mut(), key)) {
        <b>let</b> current_count: &<b>mut</b> u64 = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>(accumulator_root.id_mut(), key);
        <b>assert</b>!(*current_count + objects_created &gt;= objects_destroyed, <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_EInvariantViolation">EInvariantViolation</a>);
        *current_count = *current_count + objects_created - objects_destroyed;
    } <b>else</b> {
        <b>assert</b>!(objects_created &gt;= objects_destroyed, <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_EInvariantViolation">EInvariantViolation</a>);
        <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(accumulator_root.id_mut(), key, objects_created - objects_destroyed);
    };
}
</code></pre>



</details>

<a name="sui_accumulator_metadata_get_accumulator_object_count"></a>

## Function `get_accumulator_object_count`

Returns the current count of accumulator objects stored as a dynamic field.


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_get_accumulator_object_count">get_accumulator_object_count</a>(accumulator_root: &<a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_get_accumulator_object_count">get_accumulator_object_count</a>(accumulator_root: &AccumulatorRoot): u64 {
    <b>let</b> key = <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_AccumulatorObjectCountKey">AccumulatorObjectCountKey</a>();
    <b>if</b> (<a href="../sui/dynamic_field.md#sui_dynamic_field_exists_">dynamic_field::exists_</a>(accumulator_root.id(), key)) {
        *<a href="../sui/dynamic_field.md#sui_dynamic_field_borrow">dynamic_field::borrow</a>(accumulator_root.id(), key)
    } <b>else</b> {
        0
    }
}
</code></pre>



</details>
