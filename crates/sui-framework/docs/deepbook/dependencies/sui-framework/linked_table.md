
<a name="0x2_linked_table"></a>

# Module `0x2::linked_table`



-  [Resource `LinkedTable`](#0x2_linked_table_LinkedTable)
-  [Struct `Node`](#0x2_linked_table_Node)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_linked_table_new)
-  [Function `front`](#0x2_linked_table_front)
-  [Function `back`](#0x2_linked_table_back)
-  [Function `push_front`](#0x2_linked_table_push_front)
-  [Function `push_back`](#0x2_linked_table_push_back)
-  [Function `borrow`](#0x2_linked_table_borrow)
-  [Function `borrow_mut`](#0x2_linked_table_borrow_mut)
-  [Function `prev`](#0x2_linked_table_prev)
-  [Function `next`](#0x2_linked_table_next)
-  [Function `remove`](#0x2_linked_table_remove)
-  [Function `pop_front`](#0x2_linked_table_pop_front)
-  [Function `pop_back`](#0x2_linked_table_pop_back)
-  [Function `contains`](#0x2_linked_table_contains)
-  [Function `length`](#0x2_linked_table_length)
-  [Function `is_empty`](#0x2_linked_table_is_empty)
-  [Function `destroy_empty`](#0x2_linked_table_destroy_empty)
-  [Function `drop`](#0x2_linked_table_drop)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="../../dependencies/sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_linked_table_LinkedTable"></a>

## Resource `LinkedTable`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K: <b>copy</b>, drop, store, V: store&gt; <b>has</b> store, key
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
<dt>
<code>head: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;K&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>tail: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;K&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_linked_table_Node"></a>

## Struct `Node`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a>&lt;K: <b>copy</b>, drop, store, V: store&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>prev: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;K&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>next: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;K&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>value: V</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_linked_table_ETableNotEmpty"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_ETableNotEmpty">ETableNotEmpty</a>: u64 = 0;
</code></pre>



<a name="0x2_linked_table_ETableIsEmpty"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_ETableIsEmpty">ETableIsEmpty</a>: u64 = 1;
</code></pre>



<a name="0x2_linked_table_new"></a>

## Function `new`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_new">new</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_new">new</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(ctx: &<b>mut</b> TxContext): <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt; {
    <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_new">object::new</a>(ctx),
        size: 0,
        head: <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>(),
        tail: <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>(),
    }
}
</code></pre>



</details>

<a name="0x2_linked_table_front"></a>

## Function `front`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_front">front</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;): &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_front">front</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;): &Option&lt;K&gt; {
    &<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.head
}
</code></pre>



</details>

<a name="0x2_linked_table_back"></a>

## Function `back`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_back">back</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;): &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_back">back</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;): &Option&lt;K&gt; {
    &<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.tail
}
</code></pre>



</details>

<a name="0x2_linked_table_push_front"></a>

## Function `push_front`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_push_front">push_front</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;, k: K, value: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_push_front">push_front</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(
    <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;,
    k: K,
    value: V,
) {
    <b>let</b> old_head = <a href="../../dependencies/move-stdlib/option.md#0x1_option_swap_or_fill">option::swap_or_fill</a>(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.head, k);
    <b>if</b> (<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_none">option::is_none</a>(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.tail)) <a href="../../dependencies/move-stdlib/option.md#0x1_option_fill">option::fill</a>(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.tail, k);
    <b>let</b> prev = <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>();
    <b>let</b> next = <b>if</b> (<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&old_head)) {
        <b>let</b> old_head_k = <a href="../../dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(old_head);
        field::borrow_mut&lt;K, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a>&lt;K, V&gt;&gt;(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, old_head_k).prev = <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(k);
        <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(old_head_k)
    } <b>else</b> {
        <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>()
    };
    field::add(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, k, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a> { prev, next, value });
    <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.size = <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.size + 1;
}
</code></pre>



</details>

<a name="0x2_linked_table_push_back"></a>

## Function `push_back`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_push_back">push_back</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;, k: K, value: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_push_back">push_back</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(
    <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;,
    k: K,
    value: V,
) {
    <b>if</b> (<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_none">option::is_none</a>(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.head)) <a href="../../dependencies/move-stdlib/option.md#0x1_option_fill">option::fill</a>(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.head, k);
    <b>let</b> old_tail = <a href="../../dependencies/move-stdlib/option.md#0x1_option_swap_or_fill">option::swap_or_fill</a>(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.tail, k);
    <b>let</b> prev = <b>if</b> (<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&old_tail)) {
        <b>let</b> old_tail_k = <a href="../../dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(old_tail);
        field::borrow_mut&lt;K, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a>&lt;K, V&gt;&gt;(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, old_tail_k).next = <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(k);
        <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(old_tail_k)
    } <b>else</b> {
        <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>()
    };
    <b>let</b> next = <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>();
    field::add(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, k, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a> { prev, next, value });
    <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.size = <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.size + 1;
}
</code></pre>



</details>

<a name="0x2_linked_table_borrow"></a>

## Function `borrow`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_borrow">borrow</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;, k: K): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_borrow">borrow</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;, k: K): &V {
    &field::borrow&lt;K, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a>&lt;K, V&gt;&gt;(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, k).value
}
</code></pre>



</details>

<a name="0x2_linked_table_borrow_mut"></a>

## Function `borrow_mut`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;, k: K): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_borrow_mut">borrow_mut</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(
    <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;,
    k: K,
): &<b>mut</b> V {
    &<b>mut</b> field::borrow_mut&lt;K, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a>&lt;K, V&gt;&gt;(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, k).value
}
</code></pre>



</details>

<a name="0x2_linked_table_prev"></a>

## Function `prev`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_prev">prev</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;, k: K): &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_prev">prev</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;, k: K): &Option&lt;K&gt; {
    &field::borrow&lt;K, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a>&lt;K, V&gt;&gt;(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, k).prev
}
</code></pre>



</details>

<a name="0x2_linked_table_next"></a>

## Function `next`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_next">next</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;, k: K): &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_next">next</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;, k: K): &Option&lt;K&gt; {
    &field::borrow&lt;K, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a>&lt;K, V&gt;&gt;(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, k).next
}
</code></pre>



</details>

<a name="0x2_linked_table_remove"></a>

## Function `remove`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_remove">remove</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;, k: K): V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_remove">remove</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;, k: K): V {
    <b>let</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a>&lt;K, V&gt; { prev, next, value } = field::remove(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, k);
    <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.size = <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.size - 1;
    <b>if</b> (<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&prev)) {
        field::borrow_mut&lt;K, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a>&lt;K, V&gt;&gt;(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, *<a href="../../dependencies/move-stdlib/option.md#0x1_option_borrow">option::borrow</a>(&prev)).next = next
    };
    <b>if</b> (<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&next)) {
        field::borrow_mut&lt;K, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a>&lt;K, V&gt;&gt;(&<b>mut</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, *<a href="../../dependencies/move-stdlib/option.md#0x1_option_borrow">option::borrow</a>(&next)).prev = prev
    };
    <b>if</b> (<a href="../../dependencies/move-stdlib/option.md#0x1_option_borrow">option::borrow</a>(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.head) == &k) <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.head = next;
    <b>if</b> (<a href="../../dependencies/move-stdlib/option.md#0x1_option_borrow">option::borrow</a>(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.tail) == &k) <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.tail = prev;
    value
}
</code></pre>



</details>

<a name="0x2_linked_table_pop_front"></a>

## Function `pop_front`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_pop_front">pop_front</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;): (K, V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_pop_front">pop_front</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;): (K, V) {
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.head), <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_ETableIsEmpty">ETableIsEmpty</a>);
    <b>let</b> head = *<a href="../../dependencies/move-stdlib/option.md#0x1_option_borrow">option::borrow</a>(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.head);
    (head, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_remove">remove</a>(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>, head))
}
</code></pre>



</details>

<a name="0x2_linked_table_pop_back"></a>

## Function `pop_back`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_pop_back">pop_back</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;): (K, V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_pop_back">pop_back</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;): (K, V) {
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.tail), <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_ETableIsEmpty">ETableIsEmpty</a>);
    <b>let</b> tail = *<a href="../../dependencies/move-stdlib/option.md#0x1_option_borrow">option::borrow</a>(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.tail);
    (tail, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_remove">remove</a>(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>, tail))
}
</code></pre>



</details>

<a name="0x2_linked_table_contains"></a>

## Function `contains`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_contains">contains</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;, k: K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_contains">contains</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;, k: K): bool {
    field::exists_with_type&lt;K, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_Node">Node</a>&lt;K, V&gt;&gt;(&<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.id, k)
}
</code></pre>



</details>

<a name="0x2_linked_table_length"></a>

## Function `length`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_length">length</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_length">length</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;): u64 {
    <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.size
}
</code></pre>



</details>

<a name="0x2_linked_table_is_empty"></a>

## Function `is_empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_is_empty">is_empty</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_is_empty">is_empty</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: &<a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;): bool {
    <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>.size == 0
}
</code></pre>



</details>

<a name="0x2_linked_table_destroy_empty"></a>

## Function `destroy_empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b>, drop, store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b> + drop + store, V: store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;) {
    <b>let</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a> { id, size, head: _, tail: _ } = <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>;
    <b>assert</b>!(size == 0, <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_ETableNotEmpty">ETableNotEmpty</a>);
    <a href="../../dependencies/sui-framework/object.md#0x2_object_delete">object::delete</a>(id)
}
</code></pre>



</details>

<a name="0x2_linked_table_drop"></a>

## Function `drop`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_drop">drop</a>&lt;K: <b>copy</b>, drop, store, V: drop, store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">linked_table::LinkedTable</a>&lt;K, V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_drop">drop</a>&lt;K: <b>copy</b> + drop + store, V: drop + store&gt;(<a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>: <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a>&lt;K, V&gt;) {
    <b>let</b> <a href="../../dependencies/sui-framework/linked_table.md#0x2_linked_table_LinkedTable">LinkedTable</a> { id, size: _, head: _, tail: _ } = <a href="../../dependencies/sui-framework/table.md#0x2_table">table</a>;
    <a href="../../dependencies/sui-framework/object.md#0x2_object_delete">object::delete</a>(id)
}
</code></pre>



</details>
