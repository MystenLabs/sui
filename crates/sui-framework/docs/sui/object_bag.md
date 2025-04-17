---
title: Module `sui::object_bag`
---

Similar to <code><a href="../sui/bag.md#sui_bag">sui::bag</a></code>, an <code><a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a></code> is a heterogeneous map-like collection. But unlike
<code><a href="../sui/bag.md#sui_bag">sui::bag</a></code>, the values bound to these dynamic fields _must_ be objects themselves. This allows
for the objects to still exist in storage, which may be important for external tools.
The difference is otherwise not observable from within Move.


-  [Struct `ObjectBag`](#sui_object_bag_ObjectBag)
-  [Constants](#@Constants_0)
-  [Function `new`](#sui_object_bag_new)
-  [Function `add`](#sui_object_bag_add)
-  [Function `borrow`](#sui_object_bag_borrow)
-  [Function `borrow_mut`](#sui_object_bag_borrow_mut)
-  [Function `remove`](#sui_object_bag_remove)
-  [Function `contains`](#sui_object_bag_contains)
-  [Function `contains_with_type`](#sui_object_bag_contains_with_type)
-  [Function `length`](#sui_object_bag_length)
-  [Function `is_empty`](#sui_object_bag_is_empty)
-  [Function `destroy_empty`](#sui_object_bag_destroy_empty)
-  [Function `value_id`](#sui_object_bag_value_id)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="sui_object_bag_ObjectBag"></a>

## Struct `ObjectBag`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a> <b>has</b> key, store
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


<a name="sui_object_bag_EBagNotEmpty"></a>



<pre><code><b>const</b> <a href="../sui/object_bag.md#sui_object_bag_EBagNotEmpty">EBagNotEmpty</a>: u64 = 0;
</code></pre>



<a name="sui_object_bag_new"></a>

## Function `new`

Creates a new, empty bag


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_new">new</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">sui::object_bag::ObjectBag</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_new">new</a>(ctx: &<b>mut</b> TxContext): <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a> {
    <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        size: 0,
    }
}
</code></pre>



</details>

<a name="sui_object_bag_add"></a>

## Function `add`

Adds a key-value pair to the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a></code>
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldAlreadyExists">sui::dynamic_field::EFieldAlreadyExists</a></code> if the bag already has an entry with
that key <code>k: K</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_add">add</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">sui::object_bag::ObjectBag</a>, k: K, v: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_add">add</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a>, k: K, v: V) {
    ofield::add(&<b>mut</b> <a href="../sui/bag.md#sui_bag">bag</a>.id, k, v);
    <a href="../sui/bag.md#sui_bag">bag</a>.size = <a href="../sui/bag.md#sui_bag">bag</a>.size + 1;
}
</code></pre>



</details>

<a name="sui_object_bag_borrow"></a>

## Function `borrow`

Immutably borrows the value associated with the key in the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a></code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">sui::dynamic_field::EFieldDoesNotExist</a></code> if the bag does not have an entry with
that key <code>k: K</code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">sui::dynamic_field::EFieldTypeMismatch</a></code> if the bag has an entry for the key, but
the value does not have the specified type.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/borrow.md#sui_borrow">borrow</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">sui::object_bag::ObjectBag</a>, k: K): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/borrow.md#sui_borrow">borrow</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a>, k: K): &V {
    ofield::borrow(&<a href="../sui/bag.md#sui_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="sui_object_bag_borrow_mut"></a>

## Function `borrow_mut`

Mutably borrows the value associated with the key in the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a></code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">sui::dynamic_field::EFieldDoesNotExist</a></code> if the bag does not have an entry with
that key <code>k: K</code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">sui::dynamic_field::EFieldTypeMismatch</a></code> if the bag has an entry for the key, but
the value does not have the specified type.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">sui::object_bag::ObjectBag</a>, k: K): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a>, k: K): &<b>mut</b> V {
    ofield::borrow_mut(&<b>mut</b> <a href="../sui/bag.md#sui_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="sui_object_bag_remove"></a>

## Function `remove`

Mutably borrows the key-value pair in the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a></code> and returns the value.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">sui::dynamic_field::EFieldDoesNotExist</a></code> if the bag does not have an entry with
that key <code>k: K</code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">sui::dynamic_field::EFieldTypeMismatch</a></code> if the bag has an entry for the key, but
the value does not have the specified type.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_remove">remove</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">sui::object_bag::ObjectBag</a>, k: K): V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_remove">remove</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<b>mut</b> <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a>, k: K): V {
    <b>let</b> v = ofield::remove(&<b>mut</b> <a href="../sui/bag.md#sui_bag">bag</a>.id, k);
    <a href="../sui/bag.md#sui_bag">bag</a>.size = <a href="../sui/bag.md#sui_bag">bag</a>.size - 1;
    v
}
</code></pre>



</details>

<a name="sui_object_bag_contains"></a>

## Function `contains`

Returns true iff there is an value associated with the key <code>k: K</code> in the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_contains">contains</a>&lt;K: <b>copy</b>, drop, store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">sui::object_bag::ObjectBag</a>, k: K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_contains">contains</a>&lt;K: <b>copy</b> + drop + store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a>, k: K): bool {
    ofield::exists_&lt;K&gt;(&<a href="../sui/bag.md#sui_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="sui_object_bag_contains_with_type"></a>

## Function `contains_with_type`

Returns true iff there is an value associated with the key <code>k: K</code> in the bag <code><a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a></code>
with an assigned value of type <code>V</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_contains_with_type">contains_with_type</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">sui::object_bag::ObjectBag</a>, k: K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_contains_with_type">contains_with_type</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a>, k: K): bool {
    ofield::exists_with_type&lt;K, V&gt;(&<a href="../sui/bag.md#sui_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="sui_object_bag_length"></a>

## Function `length`

Returns the size of the bag, the number of key-value pairs


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_length">length</a>(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">sui::object_bag::ObjectBag</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_length">length</a>(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a>): u64 {
    <a href="../sui/bag.md#sui_bag">bag</a>.size
}
</code></pre>



</details>

<a name="sui_object_bag_is_empty"></a>

## Function `is_empty`

Returns true iff the bag is empty (if <code><a href="../sui/object_bag.md#sui_object_bag_length">length</a></code> returns <code>0</code>)


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_is_empty">is_empty</a>(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">sui::object_bag::ObjectBag</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_is_empty">is_empty</a>(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a>): bool {
    <a href="../sui/bag.md#sui_bag">bag</a>.size == 0
}
</code></pre>



</details>

<a name="sui_object_bag_destroy_empty"></a>

## Function `destroy_empty`

Destroys an empty bag
Aborts with <code><a href="../sui/object_bag.md#sui_object_bag_EBagNotEmpty">EBagNotEmpty</a></code> if the bag still contains values


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_destroy_empty">destroy_empty</a>(<a href="../sui/bag.md#sui_bag">bag</a>: <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">sui::object_bag::ObjectBag</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_destroy_empty">destroy_empty</a>(<a href="../sui/bag.md#sui_bag">bag</a>: <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a>) {
    <b>let</b> <a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a> { id, size } = <a href="../sui/bag.md#sui_bag">bag</a>;
    <b>assert</b>!(size == 0, <a href="../sui/object_bag.md#sui_object_bag_EBagNotEmpty">EBagNotEmpty</a>);
    id.delete()
}
</code></pre>



</details>

<a name="sui_object_bag_value_id"></a>

## Function `value_id`

Returns the ID of the object associated with the key if the bag has an entry with key <code>k: K</code>
Returns none otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_value_id">value_id</a>&lt;K: <b>copy</b>, drop, store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">sui::object_bag::ObjectBag</a>, k: K): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_bag.md#sui_object_bag_value_id">value_id</a>&lt;K: <b>copy</b> + drop + store&gt;(<a href="../sui/bag.md#sui_bag">bag</a>: &<a href="../sui/object_bag.md#sui_object_bag_ObjectBag">ObjectBag</a>, k: K): Option&lt;ID&gt; {
    ofield::id(&<a href="../sui/bag.md#sui_bag">bag</a>.id, k)
}
</code></pre>



</details>
