---
title: Module `sui::derived_object`
---

Enables the creation of objects with deterministic addresses derived from a parent object's UID.
This module provides a way to generate objects with predictable addresses based on a parent UID
and a key, creating a namespace that ensures uniqueness for each parent-key combination,
which is usually how registries are built.

Key features:
- Deterministic address generation based on parent object UID and key
- Derived objects can exist and operate independently of their parent

The derived UIDs, once created, are independent and do not require sequencing on the parent
object. They can be used without affecting the parent. The parent only maintains a record of
which derived addresses have been claimed to prevent duplicates.


-  [Struct `Claimed`](#sui_derived_object_Claimed)
-  [Struct `DerivedObjectKey`](#sui_derived_object_DerivedObjectKey)
-  [Enum `ClaimedStatus`](#sui_derived_object_ClaimedStatus)
-  [Constants](#@Constants_0)
-  [Function `claim`](#sui_derived_object_claim)
-  [Function `exists`](#sui_derived_object_exists)
-  [Function `derive_address`](#sui_derived_object_derive_address)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="sui_derived_object_Claimed"></a>

## Struct `Claimed`

Added as a DF to the parent's UID, to mark an ID as claimed.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/derived_object.md#sui_derived_object_Claimed">Claimed</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_derived_object_DerivedObjectKey"></a>

## Struct `DerivedObjectKey`

An internal key to protect from generating the same UID twice (e.g. collide with DFs)


<pre><code><b>public</b> <b>struct</b> <a href="../sui/derived_object.md#sui_derived_object_DerivedObjectKey">DerivedObjectKey</a>&lt;K: <b>copy</b>, drop, store&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: K</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_derived_object_ClaimedStatus"></a>

## Enum `ClaimedStatus`

The possible values of a claimed UID.
We make it an enum to make upgradeability easier in the future.


<pre><code><b>public</b> <b>enum</b> <a href="../sui/derived_object.md#sui_derived_object_ClaimedStatus">ClaimedStatus</a> <b>has</b> store
</code></pre>



<details>
<summary>Variants</summary>


<dl>
<dt>
Variant <code>Reserved</code>
</dt>
<dd>
 The UID has been claimed and cannot be re-claimed or used.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_derived_object_EObjectAlreadyExists"></a>

Tries to create an object twice with the same parent-key combination.


<pre><code>#[error]
<b>const</b> <a href="../sui/derived_object.md#sui_derived_object_EObjectAlreadyExists">EObjectAlreadyExists</a>: vector&lt;u8&gt; = b"Derived <a href="../sui/object.md#sui_object">object</a> is already claimed.";
</code></pre>



<a name="sui_derived_object_claim"></a>

## Function `claim`

Claim a deterministic UID, using the parent's UID & any key.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/derived_object.md#sui_derived_object_claim">claim</a>&lt;K: <b>copy</b>, drop, store&gt;(parent: &<b>mut</b> <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, key: K): <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/derived_object.md#sui_derived_object_claim">claim</a>&lt;K: <b>copy</b> + drop + store&gt;(parent: &<b>mut</b> UID, key: K): UID {
    <b>let</b> addr = <a href="../sui/derived_object.md#sui_derived_object_derive_address">derive_address</a>(parent.to_inner(), key);
    <b>let</b> id = addr.to_id();
    <b>assert</b>!(!df::exists_(parent, <a href="../sui/derived_object.md#sui_derived_object_Claimed">Claimed</a>(id)), <a href="../sui/derived_object.md#sui_derived_object_EObjectAlreadyExists">EObjectAlreadyExists</a>);
    df::add(parent, <a href="../sui/derived_object.md#sui_derived_object_Claimed">Claimed</a>(id), ClaimedStatus::Reserved);
    <a href="../sui/object.md#sui_object_new_uid_from_hash">object::new_uid_from_hash</a>(addr)
}
</code></pre>



</details>

<a name="sui_derived_object_exists"></a>

## Function `exists`

Checks if a provided <code>key</code> has been claimed for the given parent.
Note: If the UID has been deleted through <code><a href="../sui/object.md#sui_object_delete">object::delete</a></code>, this will always return true.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/derived_object.md#sui_derived_object_exists">exists</a>&lt;K: <b>copy</b>, drop, store&gt;(parent: &<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, key: K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/derived_object.md#sui_derived_object_exists">exists</a>&lt;K: <b>copy</b> + drop + store&gt;(parent: &UID, key: K): bool {
    <b>let</b> addr = <a href="../sui/derived_object.md#sui_derived_object_derive_address">derive_address</a>(parent.to_inner(), key);
    df::exists_(parent, <a href="../sui/derived_object.md#sui_derived_object_Claimed">Claimed</a>(addr.to_id()))
}
</code></pre>



</details>

<a name="sui_derived_object_derive_address"></a>

## Function `derive_address`

Given an ID and a Key, it calculates the derived address.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/derived_object.md#sui_derived_object_derive_address">derive_address</a>&lt;K: <b>copy</b>, drop, store&gt;(parent: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, key: K): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/derived_object.md#sui_derived_object_derive_address">derive_address</a>&lt;K: <b>copy</b> + drop + store&gt;(parent: ID, key: K): <b>address</b> {
    df::hash_type_and_key(parent.to_address(), <a href="../sui/derived_object.md#sui_derived_object_DerivedObjectKey">DerivedObjectKey</a>(key))
}
</code></pre>



</details>
