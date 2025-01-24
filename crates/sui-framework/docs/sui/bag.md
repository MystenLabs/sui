---
title: Module `sui::bag`
---

A bag is a heterogeneous map-like collection. The collection is similar to <code><a href="../sui/table.md#sui_table">sui::table</a></code> in that
its keys and values are not stored within the <code><a href="../sui/bag.md#sui_bag_Bag">Bag</a></code> value, but instead are stored using Sui's
object system. The <code><a href="../sui/bag.md#sui_bag_Bag">Bag</a></code> struct acts only as a handle into the object system to retrieve those
keys and values.
Note that this means that <code><a href="../sui/bag.md#sui_bag_Bag">Bag</a></code> values with exactly the same key-value mapping will not be
equal, with <code>==</code>, at runtime. For example
```
let bag1 = bag::new();
let bag2 = bag::new();
bag::add(&mut bag1, 0, false);
bag::add(&mut bag1, 1, true);
bag::add(&mut bag2, 0, false);
bag::add(&mut bag2, 1, true);
// bag1 does not equal bag2, despite having the same entries
assert!(&bag1 != &bag2);
```
At it's core, <code><a href="../sui/bag.md#sui_bag">sui::bag</a></code> is a wrapper around <code>UID</code> that allows for access to
<code><a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a></code> while preventing accidentally stranding field values. A <code>UID</code> can be
deleted, even if it has dynamic fields associated with it, but a bag, on the other hand, must be
empty to be destroyed.


-  [Struct `Bag`](#sui_bag_Bag)
-  [Constants](#@Constants_0)
-  [Function `new`](#sui_bag_new)
-  [Function `add`](#sui_bag_add)
-  [Function `borrow`](#sui_bag_borrow)
-  [Function `borrow_mut`](#sui_bag_borrow_mut)
-  [Function `remove`](#sui_bag_remove)
-  [Function `contains`](#sui_bag_contains)
-  [Function `contains_with_type`](#sui_bag_contains_with_type)
-  [Function `length`](#sui_bag_length)
-  [Function `is_empty`](#sui_bag_is_empty)
-  [Function `destroy_empty`](#sui_bag_destroy_empty)


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



<a name="sui_bag_Bag"></a>

## Struct `Bag`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/bag.md#sui_bag_Bag">Bag</a> <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
 the ID of this bag
</dd>
<dt>
<code>size: u64</code>
</dt>
<dd>
 the number of key-value pairs in the bag
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_bag_EBagNotEmpty"></a>



<pre><code><b>const</b> <a href="../sui/bag.md#sui_bag_EBagNotEmpty">EBagNotEmpty</a>: u64 = 0;
</code></pre>



<a name="sui_bag_new"></a>

## Function `new`

Creates a new, empty bag


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_new">new</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_new">new</a>(ctx: &<b>mut</b> TxContext): <a href="../sui/bag.md#sui_bag_Bag">Bag</a> {
    <a href="../sui/bag.md#sui_bag_Bag">Bag</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        size: 0,
    }
}
</code></pre>



</details>

<a name="sui_bag_add"></a>

## Function `add`

Adds a key-value pair to the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/bag.md#sui_bag_Bag">Bag</a></code>
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldAlreadyExists">sui::dynamic_field::EFieldAlreadyExists</a></code> if the bag already has an entry with
that key <code>k: K</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_add">add</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>, k: K, v: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_add">add</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/bag.md#sui_bag_Bag">Bag</a>, k: K, v: V) {
    field::add(&<b>mut</b> <a href="../sui/bag.md#sui_bag">bag</a>.id, k, v);
    <a href="../sui/bag.md#sui_bag">bag</a>.size = <a href="../sui/bag.md#sui_bag">bag</a>.size + 1;
}
</code></pre>



</details>

<a name="sui_bag_borrow"></a>

## Function `borrow`

Immutable borrows the value associated with the key in the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">Bag</a></code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">sui::dynamic_field::EFieldDoesNotExist</a></code> if the bag does not have an entry with
that key <code>k: K</code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">sui::dynamic_field::EFieldTypeMismatch</a></code> if the bag has an entry for the key, but
the value does not have the specified type.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/borrow.md#sui_borrow">borrow</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>, k: K): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/borrow.md#sui_borrow">borrow</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">Bag</a>, k: K): &V {
    field::borrow(&<a href="../sui/bag.md#sui_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="sui_bag_borrow_mut"></a>

## Function `borrow_mut`

Mutably borrows the value associated with the key in the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/bag.md#sui_bag_Bag">Bag</a></code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">sui::dynamic_field::EFieldDoesNotExist</a></code> if the bag does not have an entry with
that key <code>k: K</code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">sui::dynamic_field::EFieldTypeMismatch</a></code> if the bag has an entry for the key, but
the value does not have the specified type.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>, k: K): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/bag.md#sui_bag_Bag">Bag</a>, k: K): &<b>mut</b> V {
    field::borrow_mut(&<b>mut</b> <a href="../sui/bag.md#sui_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="sui_bag_remove"></a>

## Function `remove`

Mutably borrows the key-value pair in the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/bag.md#sui_bag_Bag">Bag</a></code> and returns the value.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">sui::dynamic_field::EFieldDoesNotExist</a></code> if the bag does not have an entry with
that key <code>k: K</code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">sui::dynamic_field::EFieldTypeMismatch</a></code> if the bag has an entry for the key, but
the value does not have the specified type.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_remove">remove</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>, k: K): V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_remove">remove</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/bag.md#sui_bag_Bag">Bag</a>, k: K): V {
    <b>let</b> v = field::remove(&<b>mut</b> <a href="../sui/bag.md#sui_bag">bag</a>.id, k);
    <a href="../sui/bag.md#sui_bag">bag</a>.size = <a href="../sui/bag.md#sui_bag">bag</a>.size - 1;
    v
}
</code></pre>



</details>

<a name="sui_bag_contains"></a>

## Function `contains`

Returns true iff there is an value associated with the key <code>k: K</code> in the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">Bag</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_contains">contains</a>&lt;K: <b>copy</b>, drop, store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>, k: K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_contains">contains</a>&lt;K: <b>copy</b> + drop + store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">Bag</a>, k: K): bool {
    field::exists_&lt;K&gt;(&<a href="../sui/bag.md#sui_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="sui_bag_contains_with_type"></a>

## Function `contains_with_type`

Returns true iff there is an value associated with the key <code>k: K</code> in the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">Bag</a></code>
with an assigned value of type <code>V</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_contains_with_type">contains_with_type</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>, k: K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_contains_with_type">contains_with_type</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">Bag</a>, k: K): bool {
    field::exists_with_type&lt;K, V&gt;(&<a href="../sui/bag.md#sui_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="sui_bag_length"></a>

## Function `length`

Returns the size of the bag, the number of key-value pairs


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_length">length</a>(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_length">length</a>(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">Bag</a>): u64 {
    <a href="../sui/bag.md#sui_bag">bag</a>.size
}
</code></pre>



</details>

<a name="sui_bag_is_empty"></a>

## Function `is_empty`

Returns true iff the bag is empty (if <code><a href="../sui/bag.md#sui_bag_length">length</a></code> returns <code>0</code>)


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_is_empty">is_empty</a>(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_is_empty">is_empty</a>(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/bag.md#sui_bag_Bag">Bag</a>): bool {
    <a href="../sui/bag.md#sui_bag">bag</a>.size == 0
}
</code></pre>



</details>

<a name="sui_bag_destroy_empty"></a>

## Function `destroy_empty`

Destroys an empty bag
Aborts with <code><a href="../sui/bag.md#sui_bag_EBagNotEmpty">EBagNotEmpty</a></code> if the bag still contains values


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_destroy_empty">destroy_empty</a>(<a href="../sui/bag.md#sui_bag">bag</a>: <a href="../sui/bag.md#sui_bag_Bag">sui::bag::Bag</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bag.md#sui_bag_destroy_empty">destroy_empty</a>(<a href="../sui/bag.md#sui_bag">bag</a>: <a href="../sui/bag.md#sui_bag_Bag">Bag</a>) {
    <b>let</b> <a href="../sui/bag.md#sui_bag_Bag">Bag</a> { id, size } = <a href="../sui/bag.md#sui_bag">bag</a>;
    <b>assert</b>!(size == 0, <a href="../sui/bag.md#sui_bag_EBagNotEmpty">EBagNotEmpty</a>);
    id.delete()
}
</code></pre>



</details>
