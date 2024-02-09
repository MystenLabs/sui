
<a name="0x2_object_bag"></a>

# Module `0x2::object_bag`



-  [Resource `ObjectBag`](#0x2_object_bag_ObjectBag)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_object_bag_new)
-  [Function `add`](#0x2_object_bag_add)
-  [Function `borrow`](#0x2_object_bag_borrow)
-  [Function `borrow_mut`](#0x2_object_bag_borrow_mut)
-  [Function `remove`](#0x2_object_bag_remove)
-  [Function `contains`](#0x2_object_bag_contains)
-  [Function `contains_with_type`](#0x2_object_bag_contains_with_type)
-  [Function `length`](#0x2_object_bag_length)
-  [Function `is_empty`](#0x2_object_bag_is_empty)
-  [Function `destroy_empty`](#0x2_object_bag_destroy_empty)
-  [Function `value_id`](#0x2_object_bag_value_id)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field">0x2::dynamic_object_field</a>;
<b>use</b> <a href="../../dependencies/sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_object_bag_ObjectBag"></a>

## Resource `ObjectBag`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>size: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_object_bag_EBagNotEmpty"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_EBagNotEmpty">EBagNotEmpty</a>: u64 = 0;
</code></pre>



<a name="0x2_object_bag_new"></a>

## Function `new`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_new">new</a>(ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_new">new</a>(ctx: &<b>mut</b> TxContext): <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a> {
    <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_new">object::new</a>(ctx),
        size: 0,
    }
}
</code></pre>



</details>

<a name="0x2_object_bag_add"></a>

## Function `add`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_add">add</a>&lt;K: <b>copy</b>, drop, store, V: store, key&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a>, k: K, v: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_add">add</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a>, k: K, v: V) {
    ofield::add(&<b>mut</b> <a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.id, k, v);
    <a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.size = <a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.size + 1;
}
</code></pre>



</details>

<a name="0x2_object_bag_borrow"></a>

## Function `borrow`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_borrow">borrow</a>&lt;K: <b>copy</b>, drop, store, V: store, key&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a>, k: K): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_borrow">borrow</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a>, k: K): &V {
    ofield::borrow(&<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="0x2_object_bag_borrow_mut"></a>

## Function `borrow_mut`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b>, drop, store, V: store, key&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a>, k: K): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a>, k: K): &<b>mut</b> V {
    ofield::borrow_mut(&<b>mut</b> <a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="0x2_object_bag_remove"></a>

## Function `remove`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_remove">remove</a>&lt;K: <b>copy</b>, drop, store, V: store, key&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a>, k: K): V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_remove">remove</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a>, k: K): V {
    <b>let</b> v = ofield::remove(&<b>mut</b> <a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.id, k);
    <a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.size = <a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.size - 1;
    v
}
</code></pre>



</details>

<a name="0x2_object_bag_contains"></a>

## Function `contains`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_contains">contains</a>&lt;K: <b>copy</b>, drop, store&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a>, k: K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_contains">contains</a>&lt;K: <b>copy</b> + drop + store&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a>, k: K): bool {
    ofield::exists_&lt;K&gt;(&<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="0x2_object_bag_contains_with_type"></a>

## Function `contains_with_type`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_contains_with_type">contains_with_type</a>&lt;K: <b>copy</b>, drop, store, V: store, key&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a>, k: K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_contains_with_type">contains_with_type</a>&lt;K: <b>copy</b> + drop + store, V: key + store&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a>, k: K): bool {
    ofield::exists_with_type&lt;K, V&gt;(&<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.id, k)
}
</code></pre>



</details>

<a name="0x2_object_bag_length"></a>

## Function `length`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_length">length</a>(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_length">length</a>(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a>): u64 {
    <a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.size
}
</code></pre>



</details>

<a name="0x2_object_bag_is_empty"></a>

## Function `is_empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_is_empty">is_empty</a>(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_is_empty">is_empty</a>(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a>): bool {
    <a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.size == 0
}
</code></pre>



</details>

<a name="0x2_object_bag_destroy_empty"></a>

## Function `destroy_empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_destroy_empty">destroy_empty</a>(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_destroy_empty">destroy_empty</a>(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a>) {
    <b>let</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a> { id, size } = <a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>;
    <b>assert</b>!(size == 0, <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_EBagNotEmpty">EBagNotEmpty</a>);
    <a href="../../dependencies/sui-framework/object.md#0x2_object_delete">object::delete</a>(id)
}
</code></pre>



</details>

<a name="0x2_object_bag_value_id"></a>

## Function `value_id`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_value_id">value_id</a>&lt;K: <b>copy</b>, drop, store&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a>, k: K): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_value_id">value_id</a>&lt;K: <b>copy</b> + drop + store&gt;(<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>: &<a href="../../dependencies/sui-framework/object_bag.md#0x2_object_bag_ObjectBag">ObjectBag</a>, k: K): Option&lt;ID&gt; {
    ofield::id(&<a href="../../dependencies/sui-framework/bag.md#0x2_bag">bag</a>.id, k)
}
</code></pre>



</details>
