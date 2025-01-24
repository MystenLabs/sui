---
title: Module `deepbook::critbit`
---



-  [Struct `Leaf`](#deepbook_critbit_Leaf)
-  [Struct `InternalNode`](#deepbook_critbit_InternalNode)
-  [Struct `CritbitTree`](#deepbook_critbit_CritbitTree)
-  [Constants](#@Constants_0)
-  [Function `new`](#deepbook_critbit_new)
-  [Function `size`](#deepbook_critbit_size)
-  [Function `is_empty`](#deepbook_critbit_is_empty)
-  [Function `min_leaf`](#deepbook_critbit_min_leaf)
-  [Function `max_leaf`](#deepbook_critbit_max_leaf)
-  [Function `previous_leaf`](#deepbook_critbit_previous_leaf)
-  [Function `next_leaf`](#deepbook_critbit_next_leaf)
-  [Function `left_most_leaf`](#deepbook_critbit_left_most_leaf)
-  [Function `right_most_leaf`](#deepbook_critbit_right_most_leaf)
-  [Function `insert_leaf`](#deepbook_critbit_insert_leaf)
-  [Function `find_leaf`](#deepbook_critbit_find_leaf)
-  [Function `find_closest_key`](#deepbook_critbit_find_closest_key)
-  [Function `remove_leaf_by_index`](#deepbook_critbit_remove_leaf_by_index)
-  [Function `borrow_mut_leaf_by_index`](#deepbook_critbit_borrow_mut_leaf_by_index)
-  [Function `borrow_leaf_by_index`](#deepbook_critbit_borrow_leaf_by_index)
-  [Function `borrow_leaf_by_key`](#deepbook_critbit_borrow_leaf_by_key)
-  [Function `drop`](#deepbook_critbit_drop)
-  [Function `destroy_empty`](#deepbook_critbit_destroy_empty)
-  [Function `get_closest_leaf_index_by_key`](#deepbook_critbit_get_closest_leaf_index_by_key)
-  [Function `update_child`](#deepbook_critbit_update_child)
-  [Function `is_left_child`](#deepbook_critbit_is_left_child)


<pre><code><b>use</b> <a href="../deepbook/math.md#deepbook_math">deepbook::math</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="deepbook_critbit_Leaf"></a>

## Struct `Leaf`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/critbit.md#deepbook_critbit_Leaf">Leaf</a>&lt;V&gt; <b>has</b> <a href="../deepbook/critbit.md#deepbook_critbit_drop">drop</a>, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>key: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>value: V</code>
</dt>
<dd>
</dd>
<dt>
<code>parent: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="deepbook_critbit_InternalNode"></a>

## Struct `InternalNode`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/critbit.md#deepbook_critbit_InternalNode">InternalNode</a> <b>has</b> <a href="../deepbook/critbit.md#deepbook_critbit_drop">drop</a>, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>mask: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>left_child: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>right_child: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>parent: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="deepbook_critbit_CritbitTree"></a>

## Struct `CritbitTree`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V: store&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>root: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>internal_nodes: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;u64, <a href="../deepbook/critbit.md#deepbook_critbit_InternalNode">deepbook::critbit::InternalNode</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>leaves: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;u64, <a href="../deepbook/critbit.md#deepbook_critbit_Leaf">deepbook::critbit::Leaf</a>&lt;V&gt;&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>next_internal_node_index: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>next_leaf_index: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="deepbook_critbit_EExceedCapacity"></a>



<pre><code><b>const</b> <a href="../deepbook/critbit.md#deepbook_critbit_EExceedCapacity">EExceedCapacity</a>: u64 = 2;
</code></pre>



<a name="deepbook_critbit_EIndexOutOfRange"></a>



<pre><code><b>const</b> <a href="../deepbook/critbit.md#deepbook_critbit_EIndexOutOfRange">EIndexOutOfRange</a>: u64 = 7;
</code></pre>



<a name="deepbook_critbit_EKeyAlreadyExist"></a>



<pre><code><b>const</b> <a href="../deepbook/critbit.md#deepbook_critbit_EKeyAlreadyExist">EKeyAlreadyExist</a>: u64 = 4;
</code></pre>



<a name="deepbook_critbit_ELeafNotExist"></a>



<pre><code><b>const</b> <a href="../deepbook/critbit.md#deepbook_critbit_ELeafNotExist">ELeafNotExist</a>: u64 = 5;
</code></pre>



<a name="deepbook_critbit_ENullParent"></a>



<pre><code><b>const</b> <a href="../deepbook/critbit.md#deepbook_critbit_ENullParent">ENullParent</a>: u64 = 8;
</code></pre>



<a name="deepbook_critbit_ETreeNotEmpty"></a>



<pre><code><b>const</b> <a href="../deepbook/critbit.md#deepbook_critbit_ETreeNotEmpty">ETreeNotEmpty</a>: u64 = 3;
</code></pre>



<a name="deepbook_critbit_MAX_CAPACITY"></a>



<pre><code><b>const</b> <a href="../deepbook/critbit.md#deepbook_critbit_MAX_CAPACITY">MAX_CAPACITY</a>: u64 = 9223372036854775807;
</code></pre>



<a name="deepbook_critbit_MAX_U64"></a>



<pre><code><b>const</b> <a href="../deepbook/critbit.md#deepbook_critbit_MAX_U64">MAX_U64</a>: u64 = 18446744073709551615;
</code></pre>



<a name="deepbook_critbit_PARTITION_INDEX"></a>



<pre><code><b>const</b> <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>: u64 = 9223372036854775808;
</code></pre>



<a name="deepbook_critbit_new"></a>

## Function `new`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_new">new</a>&lt;V: store&gt;(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_new">new</a>&lt;V: store&gt;(ctx: &<b>mut</b> TxContext): <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt; {
    <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt; {
        root: <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
        internal_nodes: table::new(ctx),
        leaves: table::new(ctx),
        <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a>: <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
        <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a>: <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
        next_internal_node_index: 0,
        next_leaf_index: 0
    }
}
</code></pre>



</details>

<a name="deepbook_critbit_size"></a>

## Function `size`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_size">size</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_size">size</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;): u64 {
    table::length(&tree.leaves)
}
</code></pre>



</details>

<a name="deepbook_critbit_is_empty"></a>

## Function `is_empty`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_is_empty">is_empty</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_is_empty">is_empty</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;): bool {
    table::is_empty(&tree.leaves)
}
</code></pre>



</details>

<a name="deepbook_critbit_min_leaf"></a>

## Function `min_leaf`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;): (u64, u64) {
    <b>assert</b>!(!<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">is_empty</a>(tree), <a href="../deepbook/critbit.md#deepbook_critbit_ELeafNotExist">ELeafNotExist</a>);
    <b>let</b> <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a> = table::borrow(&tree.leaves, tree.<a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a>);
    <b>return</b> (<a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a>.key, tree.<a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a>)
}
</code></pre>



</details>

<a name="deepbook_critbit_max_leaf"></a>

## Function `max_leaf`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;): (u64, u64) {
    <b>assert</b>!(!<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">is_empty</a>(tree), <a href="../deepbook/critbit.md#deepbook_critbit_ELeafNotExist">ELeafNotExist</a>);
    <b>let</b> <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a> = table::borrow(&tree.leaves, tree.<a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a>);
    <b>return</b> (<a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a>.key, tree.<a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a>)
}
</code></pre>



</details>

<a name="deepbook_critbit_previous_leaf"></a>

## Function `previous_leaf`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_previous_leaf">previous_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, key: u64): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_previous_leaf">previous_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, key: u64): (u64, u64) {
    <b>let</b> (_, <b>mut</b> index) = <a href="../deepbook/critbit.md#deepbook_critbit_find_leaf">find_leaf</a>(tree, key);
    <b>assert</b>!(index != <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>, <a href="../deepbook/critbit.md#deepbook_critbit_ELeafNotExist">ELeafNotExist</a>);
    <b>let</b> <b>mut</b> ptr = <a href="../deepbook/critbit.md#deepbook_critbit_MAX_U64">MAX_U64</a> - index;
    <b>let</b> <b>mut</b> parent = table::borrow(&tree.leaves, index).parent;
    <b>while</b> (parent != <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a> && <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>(tree, parent, ptr)){
        ptr = parent;
        parent = table::borrow(&tree.internal_nodes, ptr).parent;
    };
    <b>if</b>(parent == <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
        <b>return</b> (0, <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>)
    };
    index = <a href="../deepbook/critbit.md#deepbook_critbit_MAX_U64">MAX_U64</a> - <a href="../deepbook/critbit.md#deepbook_critbit_right_most_leaf">right_most_leaf</a>(tree, table::borrow(&tree.internal_nodes, parent).left_child);
    <b>let</b> key = table::borrow(&tree.leaves, index).key;
    <b>return</b> (key, index)
}
</code></pre>



</details>

<a name="deepbook_critbit_next_leaf"></a>

## Function `next_leaf`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_next_leaf">next_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, key: u64): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_next_leaf">next_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, key: u64): (u64, u64) {
    <b>let</b> (_, <b>mut</b> index) = <a href="../deepbook/critbit.md#deepbook_critbit_find_leaf">find_leaf</a>(tree, key);
    <b>assert</b>!(index != <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>, <a href="../deepbook/critbit.md#deepbook_critbit_ELeafNotExist">ELeafNotExist</a>);
    <b>let</b> <b>mut</b> ptr = <a href="../deepbook/critbit.md#deepbook_critbit_MAX_U64">MAX_U64</a> - index;
    <b>let</b> <b>mut</b> parent = table::borrow(&tree.leaves, index).parent;
    <b>while</b> (parent != <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a> && !<a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>(tree, parent, ptr)){
        ptr = parent;
        parent = table::borrow(&tree.internal_nodes, ptr).parent;
    };
    <b>if</b>(parent == <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
        <b>return</b> (0, <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>)
    };
    index = <a href="../deepbook/critbit.md#deepbook_critbit_MAX_U64">MAX_U64</a> - <a href="../deepbook/critbit.md#deepbook_critbit_left_most_leaf">left_most_leaf</a>(tree, table::borrow(&tree.internal_nodes, parent).right_child);
    <b>let</b> key = table::borrow(&tree.leaves, index).key;
    <b>return</b> (key, index)
}
</code></pre>



</details>

<a name="deepbook_critbit_left_most_leaf"></a>

## Function `left_most_leaf`



<pre><code><b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_left_most_leaf">left_most_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, root: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_left_most_leaf">left_most_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, root: u64): u64 {
    <b>let</b> <b>mut</b> ptr = root;
    <b>while</b> (ptr &lt; <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>){
        ptr = table::borrow(& tree.internal_nodes, ptr).left_child;
    };
    ptr
}
</code></pre>



</details>

<a name="deepbook_critbit_right_most_leaf"></a>

## Function `right_most_leaf`



<pre><code><b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_right_most_leaf">right_most_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, root: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_right_most_leaf">right_most_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, root: u64): u64 {
    <b>let</b> <b>mut</b> ptr = root;
    <b>while</b> (ptr &lt; <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>){
        ptr = table::borrow(& tree.internal_nodes, ptr).right_child;
    };
    ptr
}
</code></pre>



</details>

<a name="deepbook_critbit_insert_leaf"></a>

## Function `insert_leaf`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_insert_leaf">insert_leaf</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, key: u64, value: V): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_insert_leaf">insert_leaf</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, key: u64, value: V): u64 {
    <b>let</b> new_leaf = <a href="../deepbook/critbit.md#deepbook_critbit_Leaf">Leaf</a>&lt;V&gt;{
        key,
        value,
        parent: <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
    };
    <b>let</b> new_leaf_index = tree.next_leaf_index;
    tree.next_leaf_index = tree.next_leaf_index + 1;
    <b>assert</b>!(new_leaf_index &lt; <a href="../deepbook/critbit.md#deepbook_critbit_MAX_CAPACITY">MAX_CAPACITY</a> - 1, <a href="../deepbook/critbit.md#deepbook_critbit_EExceedCapacity">EExceedCapacity</a>);
    table::add(&<b>mut</b> tree.leaves, new_leaf_index, new_leaf);
    <b>let</b> closest_leaf_index = <a href="../deepbook/critbit.md#deepbook_critbit_get_closest_leaf_index_by_key">get_closest_leaf_index_by_key</a>(tree, key);
    // Handle the first insertion
    <b>if</b> (closest_leaf_index == <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
        <b>assert</b>!(new_leaf_index == 0, <a href="../deepbook/critbit.md#deepbook_critbit_ETreeNotEmpty">ETreeNotEmpty</a>);
        tree.root = <a href="../deepbook/critbit.md#deepbook_critbit_MAX_U64">MAX_U64</a> - new_leaf_index;
        tree.<a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a> = new_leaf_index;
        tree.<a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a> = new_leaf_index;
        <b>return</b> 0
    };
    <b>let</b> closest_key = table::borrow(&tree.leaves, closest_leaf_index).key;
    <b>assert</b>!(closest_key != key, <a href="../deepbook/critbit.md#deepbook_critbit_EKeyAlreadyExist">EKeyAlreadyExist</a>);
    // Note that we reserve count_leading_zeros of form u128 <b>for</b> future <b>use</b>
    <b>let</b> <a href="../deepbook/critbit.md#deepbook_critbit">critbit</a> = 64 - (count_leading_zeros((closest_key ^ key) <b>as</b> u128) - 64);
    <b>let</b> new_mask = 1u64 &lt;&lt; (<a href="../deepbook/critbit.md#deepbook_critbit">critbit</a> - 1);
    <b>let</b> new_internal_node= <a href="../deepbook/critbit.md#deepbook_critbit_InternalNode">InternalNode</a> {
        mask: new_mask,
        left_child: <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
        right_child: <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
        parent: <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
    };
    <b>let</b> new_internal_node_index = tree.next_internal_node_index;
    tree.next_internal_node_index = tree.next_internal_node_index + 1;
    table::add(&<b>mut</b> tree.internal_nodes, new_internal_node_index, new_internal_node);
    <b>let</b> <b>mut</b> ptr = tree.root;
    <b>let</b> <b>mut</b> new_internal_node_parent_index = <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
    // Search position of the <a href="../deepbook/critbit.md#deepbook_critbit_new">new</a> internal node
    <b>while</b> (ptr &lt; <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
        <b>let</b> internal_node = table::borrow(&tree.internal_nodes, ptr);
        <b>if</b> (new_mask &gt; internal_node.mask) {
            <b>break</b>
        };
        new_internal_node_parent_index = ptr;
        <b>if</b> (key & internal_node.mask == 0) {
            ptr = internal_node.left_child;
        } <b>else</b> {
            ptr = internal_node.right_child;
        };
    };
    // Update the child info of <a href="../deepbook/critbit.md#deepbook_critbit_new">new</a> internal node's parent
    <b>if</b> (new_internal_node_parent_index == <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>){
        // <b>if</b> the <a href="../deepbook/critbit.md#deepbook_critbit_new">new</a> internal node is root
        tree.root = new_internal_node_index;
    } <b>else</b>{
        // In another case, we update the child field of the <a href="../deepbook/critbit.md#deepbook_critbit_new">new</a> internal node's parent
        // And the parent field of the <a href="../deepbook/critbit.md#deepbook_critbit_new">new</a> internal node
        <b>let</b> <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a> = <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>(tree, new_internal_node_parent_index, ptr);
        <a href="../deepbook/critbit.md#deepbook_critbit_update_child">update_child</a>(tree, new_internal_node_parent_index, new_internal_node_index, <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>);
    };
    // Finally, update the child field of the <a href="../deepbook/critbit.md#deepbook_critbit_new">new</a> internal node
    <b>let</b> <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a> = new_mask & key == 0;
    <a href="../deepbook/critbit.md#deepbook_critbit_update_child">update_child</a>(tree, new_internal_node_index, <a href="../deepbook/critbit.md#deepbook_critbit_MAX_U64">MAX_U64</a> - new_leaf_index, <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>);
    <a href="../deepbook/critbit.md#deepbook_critbit_update_child">update_child</a>(tree, new_internal_node_index, ptr, !<a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>);
    <b>if</b> (table::borrow(&tree.leaves, tree.<a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a>).key &gt; key) {
        tree.<a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a> = new_leaf_index;
    };
    <b>if</b> (table::borrow(&tree.leaves, tree.<a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a>).key &lt; key) {
        tree.<a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a> = new_leaf_index;
    };
    new_leaf_index
}
</code></pre>



</details>

<a name="deepbook_critbit_find_leaf"></a>

## Function `find_leaf`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_find_leaf">find_leaf</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, key: u64): (bool, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_find_leaf">find_leaf</a>&lt;V: store&gt;(tree: & <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, key: u64): (bool, u64) {
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">is_empty</a>(tree)) {
        <b>return</b> (<b>false</b>, <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>)
    };
    <b>let</b> closest_leaf_index = <a href="../deepbook/critbit.md#deepbook_critbit_get_closest_leaf_index_by_key">get_closest_leaf_index_by_key</a>(tree, key);
    <b>let</b> closeset_leaf = table::borrow(&tree.leaves, closest_leaf_index);
    <b>if</b> (closeset_leaf.key != key){
        <b>return</b> (<b>false</b>, <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>)
    } <b>else</b>{
        <b>return</b> (<b>true</b>, closest_leaf_index)
    }
}
</code></pre>



</details>

<a name="deepbook_critbit_find_closest_key"></a>

## Function `find_closest_key`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_find_closest_key">find_closest_key</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, key: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_find_closest_key">find_closest_key</a>&lt;V: store&gt;(tree: & <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, key: u64): u64 {
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_is_empty">is_empty</a>(tree)) {
        <b>return</b> 0
    };
    <b>let</b> closest_leaf_index = <a href="../deepbook/critbit.md#deepbook_critbit_get_closest_leaf_index_by_key">get_closest_leaf_index_by_key</a>(tree, key);
    <b>let</b> closeset_leaf = table::borrow(&tree.leaves, closest_leaf_index);
    closeset_leaf.key
}
</code></pre>



</details>

<a name="deepbook_critbit_remove_leaf_by_index"></a>

## Function `remove_leaf_by_index`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_remove_leaf_by_index">remove_leaf_by_index</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, index: u64): V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_remove_leaf_by_index">remove_leaf_by_index</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, index: u64): V {
    <b>let</b> key = table::borrow(& tree.leaves, index).key;
    <b>if</b> (tree.<a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a> == index) {
        <b>let</b> (_, index) = <a href="../deepbook/critbit.md#deepbook_critbit_next_leaf">next_leaf</a>(tree, key);
        tree.<a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a> = index;
    };
    <b>if</b> (tree.<a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a> == index) {
        <b>let</b> (_, index) = <a href="../deepbook/critbit.md#deepbook_critbit_previous_leaf">previous_leaf</a>(tree, key);
        tree.<a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a> = index;
    };
    <b>let</b> <b>mut</b> is_left_child_;
    <b>let</b> <a href="../deepbook/critbit.md#deepbook_critbit_Leaf">Leaf</a>&lt;V&gt; {key: _, value, parent: removed_leaf_parent_index} = table::remove(&<b>mut</b> tree.leaves, index);
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_size">size</a>(tree) == 0) {
        tree.root = <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
        tree.<a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a> = <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
        tree.<a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a> = <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
        tree.next_internal_node_index = 0;
        tree.next_leaf_index = 0;
    } <b>else</b> {
        <b>assert</b>!(removed_leaf_parent_index != <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>, <a href="../deepbook/critbit.md#deepbook_critbit_EIndexOutOfRange">EIndexOutOfRange</a>);
        <b>let</b> removed_leaf_parent = table::borrow(&tree.internal_nodes, removed_leaf_parent_index);
        <b>let</b> removed_leaf_grand_parent_index = removed_leaf_parent.parent;
        // Note that sibling of the removed leaf can be a leaf or an internal node
        is_left_child_ = <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>(tree, removed_leaf_parent_index, <a href="../deepbook/critbit.md#deepbook_critbit_MAX_U64">MAX_U64</a> - index);
        <b>let</b> sibling_index = <b>if</b> (is_left_child_) { removed_leaf_parent.right_child }
        <b>else</b> { removed_leaf_parent.left_child };
        <b>if</b> (removed_leaf_grand_parent_index == <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
            // Parent of the removed leaf is the tree root
            // Update the parent of the sibling node and set sibling <b>as</b> the tree root
            <b>if</b> (sibling_index &lt; <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
                // sibling is an internal node
                table::borrow_mut(&<b>mut</b> tree.internal_nodes, sibling_index).parent = <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
            } <b>else</b>{
                // sibling is a leaf
                table::borrow_mut(&<b>mut</b> tree.leaves, <a href="../deepbook/critbit.md#deepbook_critbit_MAX_U64">MAX_U64</a> - sibling_index).parent = <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
            };
            tree.root = sibling_index;
        } <b>else</b> {
            // grand parent of the removed leaf is a internal node
            // set sibling <b>as</b> the child of the grand parent of the removed leaf
            is_left_child_ = <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>(tree, removed_leaf_grand_parent_index, removed_leaf_parent_index);
            <a href="../deepbook/critbit.md#deepbook_critbit_update_child">update_child</a>(tree, removed_leaf_grand_parent_index, sibling_index, is_left_child_);
        };
        table::remove(&<b>mut</b> tree.internal_nodes, removed_leaf_parent_index);
    };
    value
}
</code></pre>



</details>

<a name="deepbook_critbit_borrow_mut_leaf_by_index"></a>

## Function `borrow_mut_leaf_by_index`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_borrow_mut_leaf_by_index">borrow_mut_leaf_by_index</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, index: u64): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_borrow_mut_leaf_by_index">borrow_mut_leaf_by_index</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, index: u64): &<b>mut</b> V {
    <b>let</b> <b>entry</b> = table::borrow_mut(&<b>mut</b> tree.leaves, index);
    &<b>mut</b> <b>entry</b>.value
}
</code></pre>



</details>

<a name="deepbook_critbit_borrow_leaf_by_index"></a>

## Function `borrow_leaf_by_index`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_borrow_leaf_by_index">borrow_leaf_by_index</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, index: u64): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_borrow_leaf_by_index">borrow_leaf_by_index</a>&lt;V: store&gt;(tree: & <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, index: u64): &V {
    <b>let</b> <b>entry</b> = table::borrow(&tree.leaves, index);
    &<b>entry</b>.value
}
</code></pre>



</details>

<a name="deepbook_critbit_borrow_leaf_by_key"></a>

## Function `borrow_leaf_by_key`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_borrow_leaf_by_key">borrow_leaf_by_key</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, key: u64): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_borrow_leaf_by_key">borrow_leaf_by_key</a>&lt;V: store&gt;(tree: & <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, key: u64): &V {
    <b>let</b> (is_exist, index) = <a href="../deepbook/critbit.md#deepbook_critbit_find_leaf">find_leaf</a>(tree, key);
    <b>assert</b>!(is_exist, <a href="../deepbook/critbit.md#deepbook_critbit_ELeafNotExist">ELeafNotExist</a>);
    <a href="../deepbook/critbit.md#deepbook_critbit_borrow_leaf_by_index">borrow_leaf_by_index</a>(tree, index)
}
</code></pre>



</details>

<a name="deepbook_critbit_drop"></a>

## Function `drop`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_drop">drop</a>&lt;V: <a href="../deepbook/critbit.md#deepbook_critbit_drop">drop</a>, store&gt;(tree: <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_drop">drop</a>&lt;V: store + <a href="../deepbook/critbit.md#deepbook_critbit_drop">drop</a>&gt;(tree: <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;) {
    <b>let</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt; {
        root: _,
        internal_nodes,
        leaves,
        <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a>: _,
        <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a>: _,
        next_internal_node_index: _,
        next_leaf_index: _,
    } = tree;
    table::drop(internal_nodes);
    table::drop(leaves);
}
</code></pre>



</details>

<a name="deepbook_critbit_destroy_empty"></a>

## Function `destroy_empty`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_destroy_empty">destroy_empty</a>&lt;V: store&gt;(tree: <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_destroy_empty">destroy_empty</a>&lt;V: store&gt;(tree: <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;) {
    <b>assert</b>!(table::length(&tree.leaves) == 0, 0);
    <b>let</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt; {
        root: _,
        leaves,
        internal_nodes,
        <a href="../deepbook/critbit.md#deepbook_critbit_min_leaf">min_leaf</a>: _,
        <a href="../deepbook/critbit.md#deepbook_critbit_max_leaf">max_leaf</a>: _,
        next_internal_node_index: _,
        next_leaf_index: _
    } = tree;
    table::destroy_empty(leaves);
    table::destroy_empty(internal_nodes);
}
</code></pre>



</details>

<a name="deepbook_critbit_get_closest_leaf_index_by_key"></a>

## Function `get_closest_leaf_index_by_key`



<pre><code><b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_get_closest_leaf_index_by_key">get_closest_leaf_index_by_key</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, key: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_get_closest_leaf_index_by_key">get_closest_leaf_index_by_key</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, key: u64): u64 {
    <b>let</b> <b>mut</b> ptr = tree.root;
    // <b>if</b> tree is empty, <b>return</b> the patrition index
    <b>if</b>(ptr == <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) <b>return</b> <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
    <b>while</b> (ptr &lt; <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>){
        <b>let</b> node = table::borrow(&tree.internal_nodes, ptr);
        <b>if</b> (key & node.mask == 0){
            ptr = node.left_child;
        } <b>else</b> {
            ptr = node.right_child;
        }
    };
    <b>return</b> (<a href="../deepbook/critbit.md#deepbook_critbit_MAX_U64">MAX_U64</a> - ptr)
}
</code></pre>



</details>

<a name="deepbook_critbit_update_child"></a>

## Function `update_child`



<pre><code><b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_update_child">update_child</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, parent_index: u64, new_child: u64, <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>: bool)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_update_child">update_child</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, parent_index: u64, new_child: u64, <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>: bool) {
    <b>assert</b>!(parent_index != <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>, <a href="../deepbook/critbit.md#deepbook_critbit_ENullParent">ENullParent</a>);
    <b>if</b> (<a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>) {
        table::borrow_mut(&<b>mut</b> tree.internal_nodes, parent_index).left_child = new_child;
    } <b>else</b>{
        table::borrow_mut(&<b>mut</b> tree.internal_nodes, parent_index).right_child = new_child;
    };
    <b>if</b> (new_child &gt; <a href="../deepbook/critbit.md#deepbook_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
        table::borrow_mut(&<b>mut</b> tree.leaves, <a href="../deepbook/critbit.md#deepbook_critbit_MAX_U64">MAX_U64</a> - new_child).parent = parent_index;
    } <b>else</b> {
        table::borrow_mut(&<b>mut</b> tree.internal_nodes, new_child).parent = parent_index;
    }
}
</code></pre>



</details>

<a name="deepbook_critbit_is_left_child"></a>

## Function `is_left_child`



<pre><code><b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">deepbook::critbit::CritbitTree</a>&lt;V&gt;, parent_index: u64, index: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/critbit.md#deepbook_critbit_is_left_child">is_left_child</a>&lt;V: store&gt;(tree: &<a href="../deepbook/critbit.md#deepbook_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, parent_index: u64, index: u64): bool {
    table::borrow(&tree.internal_nodes, parent_index).left_child == index
}
</code></pre>



</details>
