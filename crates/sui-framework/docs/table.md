
<a name="0x2_table"></a>

# Module `0x2::table`

A table is a map-like collection. But unlike a traditional collection, it's keys and values are
not stored within the <code><a href="table.md#0x2_table_Table">Table</a></code> value, but instead are stored using Sui's object system. The
<code><a href="table.md#0x2_table_Table">Table</a></code> struct acts only as a handle into the object system to retrieve those keys and values.
Note that this means that <code><a href="table.md#0x2_table_Table">Table</a></code> values with exactly the same key-value mapping will not be
equal, with <code>==</code>, at runtime. For example
```
let table1 = table::new<u64, bool>();
let table2 = table::new<u64, bool>();
table::add(&mut table1, 0, false);
table::add(&mut table1, 1, true);
table::add(&mut table2, 0, false);
table::add(&mut table2, 1, true);
// table1 does not equal table2, despite having the same entries
assert!(&table1 != &table2, 0);
```


-  [Resource `Table`](#0x2_table_Table)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_table_new)
-  [Function `add`](#0x2_table_add)
-  [Function `borrow`](#0x2_table_borrow)
-  [Function `borrow_mut`](#0x2_table_borrow_mut)
-  [Function `remove`](#0x2_table_remove)
-  [Function `contains`](#0x2_table_contains)
-  [Function `length`](#0x2_table_length)
-  [Function `is_empty`](#0x2_table_is_empty)
-  [Function `destroy_empty`](#0x2_table_destroy_empty)
-  [Function `drop`](#0x2_table_drop)


<pre><code><b>use</b> <a href="dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_table_Table"></a>

## Resource `Table`



<pre><code><b>struct</b> <a href="table.md#0x2_table_Table">Table</a>&lt;K: <b>copy</b>, drop, store, V: store&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
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


<a name="0x2_table_ETableNotEmpty"></a>



<pre><code><b>const</b> <a href="table.md#0x2_table_ETableNotEmpty">ETableNotEmpty</a>: u64 = 0;
</code></pre>



<a name="0x2_table_new"></a>

## Function `new`

Creates a new, empty table


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_new">new</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="table.md#0x2_table_Table">table::Table</a>&lt;K, V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_new">new</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(ctx: &<b>mut</b> TxContext): <a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt; {
    <a href="table.md#0x2_table_Table">Table</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        size: 0,
    }
}
</code></pre>



</details>

<a name="0x2_table_add"></a>

## Function `add`

Adds a key-value pair to the table <code><a href="table.md#0x2_table">table</a>: &<b>mut</b> <a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;</code>
Aborts with <code>sui::dynamic_field::EFieldAlreadyExists</code> if the table already has an entry with
that key <code>k: K</code>.


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_add">add</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<b>mut</b> <a href="table.md#0x2_table_Table">table::Table</a>&lt;K, V&gt;, k: K, v: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_add">add</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<b>mut</b> <a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;, k: K, v: V) {
    field::add(&<b>mut</b> <a href="table.md#0x2_table">table</a>.id, k, v);
    <a href="table.md#0x2_table">table</a>.size = <a href="table.md#0x2_table">table</a>.size + 1;
}
</code></pre>



</details>

<a name="0x2_table_borrow"></a>

## Function `borrow`

Immutable borrows the value associated with the key in the table <code><a href="table.md#0x2_table">table</a>: &<a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;</code>.
Aborts with <code>sui::dynamic_field::EFieldDoesNotExist</code> if the table does not have an entry with
that key <code>k: K</code>.


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_borrow">borrow</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<a href="table.md#0x2_table_Table">table::Table</a>&lt;K, V&gt;, k: K): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_borrow">borrow</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;, k: K): &V {
    field::borrow(&<a href="table.md#0x2_table">table</a>.id, k)
}
</code></pre>



</details>

<a name="0x2_table_borrow_mut"></a>

## Function `borrow_mut`

Mutably borrows the value associated with the key in the table <code><a href="table.md#0x2_table">table</a>: &<b>mut</b> <a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;</code>.
Aborts with <code>sui::dynamic_field::EFieldDoesNotExist</code> if the table does not have an entry with
that key <code>k: K</code>.


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<b>mut</b> <a href="table.md#0x2_table_Table">table::Table</a>&lt;K, V&gt;, k: K): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<b>mut</b> <a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;, k: K): &<b>mut</b> V {
    field::borrow_mut(&<b>mut</b> <a href="table.md#0x2_table">table</a>.id, k)
}
</code></pre>



</details>

<a name="0x2_table_remove"></a>

## Function `remove`

Removes the key-value pair in the table <code><a href="table.md#0x2_table">table</a>: &<b>mut</b> <a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;</code> and returns the value.
Aborts with <code>sui::dynamic_field::EFieldDoesNotExist</code> if the table does not have an entry with
that key <code>k: K</code>.


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_remove">remove</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<b>mut</b> <a href="table.md#0x2_table_Table">table::Table</a>&lt;K, V&gt;, k: K): V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_remove">remove</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<b>mut</b> <a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;, k: K): V {
    <b>let</b> v = field::remove(&<b>mut</b> <a href="table.md#0x2_table">table</a>.id, k);
    <a href="table.md#0x2_table">table</a>.size = <a href="table.md#0x2_table">table</a>.size - 1;
    v
}
</code></pre>



</details>

<a name="0x2_table_contains"></a>

## Function `contains`

Returns true iff there is a value associated with the key <code>k: K</code> in table <code><a href="table.md#0x2_table">table</a>: &<a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;</code>


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_contains">contains</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<a href="table.md#0x2_table_Table">table::Table</a>&lt;K, V&gt;, k: K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_contains">contains</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;, k: K): bool {
    field::exists_with_type&lt;K, V&gt;(&<a href="table.md#0x2_table">table</a>.id, k)
}
</code></pre>



</details>

<a name="0x2_table_length"></a>

## Function `length`

Returns the size of the table, the number of key-value pairs


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_length">length</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<a href="table.md#0x2_table_Table">table::Table</a>&lt;K, V&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_length">length</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;): u64 {
    <a href="table.md#0x2_table">table</a>.size
}
</code></pre>



</details>

<a name="0x2_table_is_empty"></a>

## Function `is_empty`

Returns true iff the table is empty (if <code>length</code> returns <code>0</code>)


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_is_empty">is_empty</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<a href="table.md#0x2_table_Table">table::Table</a>&lt;K, V&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_is_empty">is_empty</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="table.md#0x2_table">table</a>: &<a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;): bool {
    <a href="table.md#0x2_table">table</a>.size == 0
}
</code></pre>



</details>

<a name="0x2_table_destroy_empty"></a>

## Function `destroy_empty`

Destroys an empty table
Aborts with <code><a href="table.md#0x2_table_ETableNotEmpty">ETableNotEmpty</a></code> if the table still contains values


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="table.md#0x2_table">table</a>: <a href="table.md#0x2_table_Table">table::Table</a>&lt;K, V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="table.md#0x2_table">table</a>: <a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;) {
    <b>let</b> <a href="table.md#0x2_table_Table">Table</a> { id, size } = <a href="table.md#0x2_table">table</a>;
    <b>assert</b>!(size == 0, <a href="table.md#0x2_table_ETableNotEmpty">ETableNotEmpty</a>);
    <a href="object.md#0x2_object_delete">object::delete</a>(id)
}
</code></pre>



</details>

<a name="0x2_table_drop"></a>

## Function `drop`

Drop a possibly non-empty table.
Usable only if the value type <code>V</code> has the <code>drop</code> ability


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_drop">drop</a>&lt;K: <b>copy</b>, drop, store, V: drop, store&gt;(<a href="table.md#0x2_table">table</a>: <a href="table.md#0x2_table_Table">table::Table</a>&lt;K, V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table.md#0x2_table_drop">drop</a>&lt;K: <b>copy</b> + drop + store, V: drop + store&gt;(<a href="table.md#0x2_table">table</a>: <a href="table.md#0x2_table_Table">Table</a>&lt;K, V&gt;) {
    <b>let</b> <a href="table.md#0x2_table_Table">Table</a> { id, size: _ } = <a href="table.md#0x2_table">table</a>;
    <a href="object.md#0x2_object_delete">object::delete</a>(id)
}
</code></pre>



</details>
