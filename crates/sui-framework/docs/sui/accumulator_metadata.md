---
title: Module `sui::accumulator_metadata`
---



-  [Struct `OwnerKey`](#sui_accumulator_metadata_OwnerKey)
-  [Struct `Owner`](#sui_accumulator_metadata_Owner)
-  [Struct `MetadataKey`](#sui_accumulator_metadata_MetadataKey)
-  [Struct `Metadata`](#sui_accumulator_metadata_Metadata)
-  [Constants](#@Constants_0)
-  [Function `accumulator_root_owner_exists`](#sui_accumulator_metadata_accumulator_root_owner_exists)
-  [Function `accumulator_root_borrow_owner_mut`](#sui_accumulator_metadata_accumulator_root_borrow_owner_mut)
-  [Function `accumulator_root_attach_owner`](#sui_accumulator_metadata_accumulator_root_attach_owner)
-  [Function `accumulator_root_detach_owner`](#sui_accumulator_metadata_accumulator_root_detach_owner)
-  [Function `create_accumulator_metadata`](#sui_accumulator_metadata_create_accumulator_metadata)
-  [Function `remove_accumulator_metadata`](#sui_accumulator_metadata_remove_accumulator_metadata)
-  [Function `accumulator_owner_attach_metadata`](#sui_accumulator_metadata_accumulator_owner_attach_metadata)
-  [Function `accumulator_owner_detach_metadata`](#sui_accumulator_metadata_accumulator_owner_detach_metadata)
-  [Function `accumulator_owner_destroy`](#sui_accumulator_metadata_accumulator_owner_destroy)


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

Accumulator metadata is organized as follows:
- Each address that holds at least one type of accumulator has an owner field attached
to the accumulator root.
- For each type of accumulator held by that address, there is an AccumulatorMetadata field
attached to the owner field.
- When the value of an accumulator drops to zero, the metadata field is removed.
- If the owner field has no more accumulator metadata field attached to it, it is removed
as well.


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

<a name="@Constants_0"></a>

## Constants


<a name="sui_accumulator_metadata_EInvariantViolation"></a>



<pre><code><b>const</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_EInvariantViolation">EInvariantViolation</a>: u64 = 0;
</code></pre>



<a name="sui_accumulator_metadata_accumulator_root_owner_exists"></a>

## Function `accumulator_root_owner_exists`

=== Owner functions ===
Check if there is an owner field attached to the accumulator root.


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_root_owner_exists">accumulator_root_owner_exists</a>(accumulator_root: &<a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_root_owner_exists">accumulator_root_owner_exists</a>(accumulator_root: &AccumulatorRoot, owner: <b>address</b>): bool {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_exists_with_type">dynamic_field::exists_with_type</a>&lt;<a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_OwnerKey">OwnerKey</a>, <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">Owner</a>&gt;(accumulator_root.id(), <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_OwnerKey">OwnerKey</a> { owner })
}
</code></pre>



</details>

<a name="sui_accumulator_metadata_accumulator_root_borrow_owner_mut"></a>

## Function `accumulator_root_borrow_owner_mut`

Borrow an owner field mutably.


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_root_borrow_owner_mut">accumulator_root_borrow_owner_mut</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>): &<b>mut</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">sui::accumulator_metadata::Owner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_root_borrow_owner_mut">accumulator_root_borrow_owner_mut</a>(
    accumulator_root: &<b>mut</b> AccumulatorRoot,
    owner: <b>address</b>,
): &<b>mut</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">Owner</a> {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>(accumulator_root.id_mut(), <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_OwnerKey">OwnerKey</a> { owner })
}
</code></pre>



</details>

<a name="sui_accumulator_metadata_accumulator_root_attach_owner"></a>

## Function `accumulator_root_attach_owner`

Attach an owner field to the accumulator root.


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_root_attach_owner">accumulator_root_attach_owner</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">sui::accumulator_metadata::Owner</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_root_attach_owner">accumulator_root_attach_owner</a>(accumulator_root: &<b>mut</b> AccumulatorRoot, owner: <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">Owner</a>) {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_add">dynamic_field::add</a>(accumulator_root.id_mut(), <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_OwnerKey">OwnerKey</a> { owner: owner.owner }, owner);
}
</code></pre>



</details>

<a name="sui_accumulator_metadata_accumulator_root_detach_owner"></a>

## Function `accumulator_root_detach_owner`

Detach an owner field from the accumulator root.


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_root_detach_owner">accumulator_root_detach_owner</a>(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>): <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">sui::accumulator_metadata::Owner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_root_detach_owner">accumulator_root_detach_owner</a>(accumulator_root: &<b>mut</b> AccumulatorRoot, owner: <b>address</b>): <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">Owner</a> {
    <a href="../sui/dynamic_field.md#sui_dynamic_field_remove">dynamic_field::remove</a>(accumulator_root.id_mut(), <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_OwnerKey">OwnerKey</a> { owner })
}
</code></pre>



</details>

<a name="sui_accumulator_metadata_create_accumulator_metadata"></a>

## Function `create_accumulator_metadata`

=== Metadata functions ===
Create a metadata field for a new balance field with type T.
The metadata will be attached to the owner field <code>owner</code>.
If the owner field does not exist, it will be created.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_create_accumulator_metadata">create_accumulator_metadata</a>&lt;T&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_create_accumulator_metadata">create_accumulator_metadata</a>&lt;T&gt;(
    accumulator_root: &<b>mut</b> AccumulatorRoot,
    owner: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> metadata = <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Metadata">Metadata</a>&lt;T&gt; {
        fields: <a href="../sui/bag.md#sui_bag_new">bag::new</a>(ctx),
    };
    <b>if</b> (accumulator_root.owner_exists(owner)) {
        <b>let</b> accumulator_owner = accumulator_root.borrow_owner_mut(owner);
        <b>assert</b>!(accumulator_owner.owner == owner, <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_EInvariantViolation">EInvariantViolation</a>);
        accumulator_owner.attach_metadata(metadata);
    } <b>else</b> {
        <b>let</b> <b>mut</b> accumulator_owner = <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">Owner</a> {
            balances: <a href="../sui/bag.md#sui_bag_new">bag::new</a>(ctx),
            owner,
        };
        accumulator_owner.attach_metadata(metadata);
        accumulator_root.attach_owner(accumulator_owner);
    }
}
</code></pre>



</details>

<a name="sui_accumulator_metadata_remove_accumulator_metadata"></a>

## Function `remove_accumulator_metadata`

Remove the metadata field for a balance field with type T.
The metadata will be detached from the owner field <code>owner</code>.
If there are no more balance fields attached to the owner field,
the owner field will be destroyed.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_remove_accumulator_metadata">remove_accumulator_metadata</a>&lt;T&gt;(accumulator_root: &<b>mut</b> <a href="../sui/accumulator.md#sui_accumulator_AccumulatorRoot">sui::accumulator::AccumulatorRoot</a>, owner: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_remove_accumulator_metadata">remove_accumulator_metadata</a>&lt;T&gt;(
    accumulator_root: &<b>mut</b> AccumulatorRoot,
    owner: <b>address</b>,
) {
    <b>let</b> is_empty = {
        <b>let</b> accumulator_owner = accumulator_root.borrow_owner_mut(owner);
        <b>let</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Metadata">Metadata</a> { fields } = accumulator_owner.detach_metadata&lt;T&gt;();
        fields.destroy_empty();
        accumulator_owner.balances.is_empty()
    };
    <b>if</b> (is_empty) {
        accumulator_root.detach_owner(owner).destroy();
    }
}
</code></pre>



</details>

<a name="sui_accumulator_metadata_accumulator_owner_attach_metadata"></a>

## Function `accumulator_owner_attach_metadata`

Attach a metadata field for type T to the owner field.


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_owner_attach_metadata">accumulator_owner_attach_metadata</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">sui::accumulator_metadata::Owner</a>, metadata: <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Metadata">sui::accumulator_metadata::Metadata</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_owner_attach_metadata">accumulator_owner_attach_metadata</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">Owner</a>, metadata: <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Metadata">Metadata</a>&lt;T&gt;) {
    self.balances.add(<a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_MetadataKey">MetadataKey</a>&lt;T&gt;(), metadata);
}
</code></pre>



</details>

<a name="sui_accumulator_metadata_accumulator_owner_detach_metadata"></a>

## Function `accumulator_owner_detach_metadata`

Detach a metadata field for type T from the owner field.


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_owner_detach_metadata">accumulator_owner_detach_metadata</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">sui::accumulator_metadata::Owner</a>): <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Metadata">sui::accumulator_metadata::Metadata</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_owner_detach_metadata">accumulator_owner_detach_metadata</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">Owner</a>): <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Metadata">Metadata</a>&lt;T&gt; {
    self.balances.remove(<a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_MetadataKey">MetadataKey</a>&lt;T&gt;())
}
</code></pre>



</details>

<a name="sui_accumulator_metadata_accumulator_owner_destroy"></a>

## Function `accumulator_owner_destroy`

Destroy an owner field.


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_owner_destroy">accumulator_owner_destroy</a>(this: <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">sui::accumulator_metadata::Owner</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_accumulator_owner_destroy">accumulator_owner_destroy</a>(this: <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">Owner</a>) {
    <b>let</b> <a href="../sui/accumulator_metadata.md#sui_accumulator_metadata_Owner">Owner</a> { balances, .. } = this;
    balances.destroy_empty();
}
</code></pre>



</details>
