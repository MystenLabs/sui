---
title: Module `sui::object_table`
---

Similar to <code><a href="../sui/table.md#sui_table">sui::table</a></code>, an <code><a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;</code> is a map-like collection. But unlike
<code><a href="../sui/table.md#sui_table">sui::table</a></code>, the values bound to these dynamic fields _must_ be objects themselves. This allows
for the objects to still exist within in storage, which may be important for external tools.
The difference is otherwise not observable from within Move.


-  [Struct `ObjectTable`](#sui_object_table_ObjectTable)
-  [Constants](#@Constants_0)
-  [Function `new`](#sui_object_table_new)
-  [Function `add`](#sui_object_table_add)
-  [Function `borrow`](#sui_object_table_borrow)
-  [Function `borrow_mut`](#sui_object_table_borrow_mut)
-  [Function `remove`](#sui_object_table_remove)
-  [Function `contains`](#sui_object_table_contains)
-  [Function `length`](#sui_object_table_length)
-  [Function `is_empty`](#sui_object_table_is_empty)
-  [Function `destroy_empty`](#sui_object_table_destroy_empty)
-  [Function `value_id`](#sui_object_table_value_id)


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



<a name="sui_object_table_ObjectTable"></a>

## Struct `ObjectTable`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;<b>phantom</b> K: <b>copy</b>, drop, store, <b>phantom</b> V: key, store&gt; <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
 the ID of this table
</dd>
<dt>
<code>size: u64</code>
</dt>
<dd>
 the number of key-value pairs in the table
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_object_table_ETableNotEmpty"></a>



<pre><code><b>const</b> <a href="../sui/object_table.md#sui_object_table_ETableNotEmpty">ETableNotEmpty</a>: u64 = 0;
</code></pre>



<a name="sui_object_table_new"></a>

## Function `new`

Creates a new, empty table


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_new">new</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/object_table.md#sui_object_table_ObjectTable">sui::object_table::ObjectTable</a>&lt;K, V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_new">new</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(ctx: &<b>mut</b> TxContext): <a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt; {
    <a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        size: 0,
    }
}
</code></pre>



</details>

<a name="sui_object_table_add"></a>

## Function `add`

Adds a key-value pair to the table <code><a href="../sui/table.md#sui_table">table</a>: &<b>mut</b> <a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;</code>
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldAlreadyExists">sui::dynamic_field::EFieldAlreadyExists</a></code> if the table already has an entry with
that key <code>k: K</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_add">add</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<b>mut</b> <a href="../sui/object_table.md#sui_object_table_ObjectTable">sui::object_table::ObjectTable</a>&lt;K, V&gt;, k: K, v: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_add">add</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<b>mut</b> <a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;, k: K, v: V) {
    ofield::add(&<b>mut</b> <a href="../sui/table.md#sui_table">table</a>.id, k, v);
    <a href="../sui/table.md#sui_table">table</a>.size = <a href="../sui/table.md#sui_table">table</a>.size + 1;
}
</code></pre>



</details>

<a name="sui_object_table_borrow"></a>

## Function `borrow`

Immutable borrows the value associated with the key in the table <code><a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;</code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">sui::dynamic_field::EFieldDoesNotExist</a></code> if the table does not have an entry with
that key <code>k: K</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/borrow.md#sui_borrow">borrow</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">sui::object_table::ObjectTable</a>&lt;K, V&gt;, k: K): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/borrow.md#sui_borrow">borrow</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;, k: K): &V {
    ofield::borrow(&<a href="../sui/table.md#sui_table">table</a>.id, k)
}
</code></pre>



</details>

<a name="sui_object_table_borrow_mut"></a>

## Function `borrow_mut`

Mutably borrows the value associated with the key in the table <code><a href="../sui/table.md#sui_table">table</a>: &<b>mut</b> <a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;</code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">sui::dynamic_field::EFieldDoesNotExist</a></code> if the table does not have an entry with
that key <code>k: K</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<b>mut</b> <a href="../sui/object_table.md#sui_object_table_ObjectTable">sui::object_table::ObjectTable</a>&lt;K, V&gt;, k: K): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(
    <a href="../sui/table.md#sui_table">table</a>: &<b>mut</b> <a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;,
    k: K,
): &<b>mut</b> V {
    ofield::borrow_mut(&<b>mut</b> <a href="../sui/table.md#sui_table">table</a>.id, k)
}
</code></pre>



</details>

<a name="sui_object_table_remove"></a>

## Function `remove`

Removes the key-value pair in the table <code><a href="../sui/table.md#sui_table">table</a>: &<b>mut</b> <a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;</code> and returns the value.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">sui::dynamic_field::EFieldDoesNotExist</a></code> if the table does not have an entry with
that key <code>k: K</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_remove">remove</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<b>mut</b> <a href="../sui/object_table.md#sui_object_table_ObjectTable">sui::object_table::ObjectTable</a>&lt;K, V&gt;, k: K): V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_remove">remove</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<b>mut</b> <a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;, k: K): V {
    <b>let</b> v = ofield::remove(&<b>mut</b> <a href="../sui/table.md#sui_table">table</a>.id, k);
    <a href="../sui/table.md#sui_table">table</a>.size = <a href="../sui/table.md#sui_table">table</a>.size - 1;
    v
}
</code></pre>



</details>

<a name="sui_object_table_contains"></a>

## Function `contains`

Returns true if there is a value associated with the key <code>k: K</code> in table
<code><a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_contains">contains</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">sui::object_table::ObjectTable</a>&lt;K, V&gt;, k: K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_contains">contains</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;, k: K): bool {
    ofield::exists_&lt;K&gt;(&<a href="../sui/table.md#sui_table">table</a>.id, k)
}
</code></pre>



</details>

<a name="sui_object_table_length"></a>

## Function `length`

Returns the size of the table, the number of key-value pairs


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_length">length</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">sui::object_table::ObjectTable</a>&lt;K, V&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_length">length</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;): u64 {
    <a href="../sui/table.md#sui_table">table</a>.size
}
</code></pre>



</details>

<a name="sui_object_table_is_empty"></a>

## Function `is_empty`

Returns true if the table is empty (if <code><a href="../sui/object_table.md#sui_object_table_length">length</a></code> returns <code>0</code>)


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_is_empty">is_empty</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">sui::object_table::ObjectTable</a>&lt;K, V&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_is_empty">is_empty</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;): bool {
    <a href="../sui/table.md#sui_table">table</a>.size == 0
}
</code></pre>



</details>

<a name="sui_object_table_destroy_empty"></a>

## Function `destroy_empty`

Destroys an empty table
Aborts with <code><a href="../sui/object_table.md#sui_object_table_ETableNotEmpty">ETableNotEmpty</a></code> if the table still contains values


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/table.md#sui_table">table</a>: <a href="../sui/object_table.md#sui_object_table_ObjectTable">sui::object_table::ObjectTable</a>&lt;K, V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../sui/table.md#sui_table">table</a>: <a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;) {
    <b>let</b> <a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a> { id, size } = <a href="../sui/table.md#sui_table">table</a>;
    <b>assert</b>!(size == 0, <a href="../sui/object_table.md#sui_object_table_ETableNotEmpty">ETableNotEmpty</a>);
    id.delete()
}
</code></pre>



</details>

<a name="sui_object_table_value_id"></a>

## Function `value_id`

Returns the ID of the object associated with the key if the table has an entry with key <code>k: K</code>
Returns none otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_value_id">value_id</a>&lt;K: <b>copy</b>, drop, store, V: key, store&gt;(<a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">sui::object_table::ObjectTable</a>&lt;K, V&gt;, k: K): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/object_table.md#sui_object_table_value_id">value_id</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(
    <a href="../sui/table.md#sui_table">table</a>: &<a href="../sui/object_table.md#sui_object_table_ObjectTable">ObjectTable</a>&lt;K, V&gt;,
    k: K,
): Option&lt;ID&gt; {
    ofield::id(&<a href="../sui/table.md#sui_table">table</a>.id, k)
}
</code></pre>



</details>
