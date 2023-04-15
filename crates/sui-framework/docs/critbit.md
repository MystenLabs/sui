
<a name="0xdee9_critbit"></a>

# Module `0xdee9::critbit`



-  [Struct `Leaf`](#0xdee9_critbit_Leaf)
-  [Struct `InternalNode`](#0xdee9_critbit_InternalNode)
-  [Struct `CritbitTree`](#0xdee9_critbit_CritbitTree)
-  [Constants](#@Constants_0)
-  [Function `new`](#0xdee9_critbit_new)
-  [Function `size`](#0xdee9_critbit_size)
-  [Function `is_empty`](#0xdee9_critbit_is_empty)
-  [Function `min_leaf`](#0xdee9_critbit_min_leaf)
-  [Function `max_leaf`](#0xdee9_critbit_max_leaf)
-  [Function `previous_leaf`](#0xdee9_critbit_previous_leaf)
-  [Function `next_leaf`](#0xdee9_critbit_next_leaf)
-  [Function `left_most_leaf`](#0xdee9_critbit_left_most_leaf)
-  [Function `right_most_leaf`](#0xdee9_critbit_right_most_leaf)
-  [Function `insert_leaf`](#0xdee9_critbit_insert_leaf)
-  [Function `find_leaf`](#0xdee9_critbit_find_leaf)
-  [Function `find_closest_key`](#0xdee9_critbit_find_closest_key)
-  [Function `remove_leaf_by_index`](#0xdee9_critbit_remove_leaf_by_index)
-  [Function `borrow_mut_leaf_by_index`](#0xdee9_critbit_borrow_mut_leaf_by_index)
-  [Function `borrow_leaf_by_index`](#0xdee9_critbit_borrow_leaf_by_index)
-  [Function `borrow_leaf_by_key`](#0xdee9_critbit_borrow_leaf_by_key)
-  [Function `drop`](#0xdee9_critbit_drop)
-  [Function `destroy_empty`](#0xdee9_critbit_destroy_empty)
-  [Function `get_closest_leaf_index_by_key`](#0xdee9_critbit_get_closest_leaf_index_by_key)
-  [Function `update_child`](#0xdee9_critbit_update_child)
-  [Function `is_left_child`](#0xdee9_critbit_is_left_child)
-  [Function `count_leading_zeros`](#0xdee9_critbit_count_leading_zeros)


<pre><code><b>use</b> <a href="../../../.././build/Sui/docs/table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0xdee9_critbit_Leaf"></a>

## Struct `Leaf`



<pre><code><b>struct</b> <a href="critbit.md#0xdee9_critbit_Leaf">Leaf</a>&lt;V&gt; <b>has</b> drop, store
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

<a name="0xdee9_critbit_InternalNode"></a>

## Struct `InternalNode`



<pre><code><b>struct</b> <a href="critbit.md#0xdee9_critbit_InternalNode">InternalNode</a> <b>has</b> drop, store
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

<a name="0xdee9_critbit_CritbitTree"></a>

## Struct `CritbitTree`



<pre><code><b>struct</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V: store&gt; <b>has</b> store
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
<code>internal_nodes: <a href="../../../.././build/Sui/docs/table.md#0x2_table_Table">table::Table</a>&lt;u64, <a href="critbit.md#0xdee9_critbit_InternalNode">critbit::InternalNode</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>leaves: <a href="../../../.././build/Sui/docs/table.md#0x2_table_Table">table::Table</a>&lt;u64, <a href="critbit.md#0xdee9_critbit_Leaf">critbit::Leaf</a>&lt;V&gt;&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>min_leaf: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>max_leaf: u64</code>
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


<a name="0xdee9_critbit_MAX_U64"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_MAX_U64">MAX_U64</a>: u64 = 18446744073709551615;
</code></pre>



<a name="0xdee9_critbit_EAssertFalse"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_EAssertFalse">EAssertFalse</a>: u64 = 6;
</code></pre>



<a name="0xdee9_critbit_EExceedCapacity"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_EExceedCapacity">EExceedCapacity</a>: u64 = 2;
</code></pre>



<a name="0xdee9_critbit_EIndexOutOfRange"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_EIndexOutOfRange">EIndexOutOfRange</a>: u64 = 7;
</code></pre>



<a name="0xdee9_critbit_EKeyAlreadyExist"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_EKeyAlreadyExist">EKeyAlreadyExist</a>: u64 = 4;
</code></pre>



<a name="0xdee9_critbit_ELeafNotExist"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_ELeafNotExist">ELeafNotExist</a>: u64 = 5;
</code></pre>



<a name="0xdee9_critbit_ENotImplemented"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_ENotImplemented">ENotImplemented</a>: u64 = 1;
</code></pre>



<a name="0xdee9_critbit_ENullChild"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_ENullChild">ENullChild</a>: u64 = 9;
</code></pre>



<a name="0xdee9_critbit_ENullParent"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_ENullParent">ENullParent</a>: u64 = 8;
</code></pre>



<a name="0xdee9_critbit_ETreeNotEmpty"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_ETreeNotEmpty">ETreeNotEmpty</a>: u64 = 3;
</code></pre>



<a name="0xdee9_critbit_MAX_CAPACITY"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_MAX_CAPACITY">MAX_CAPACITY</a>: u64 = 9223372036854775807;
</code></pre>



<a name="0xdee9_critbit_PARTITION_INDEX"></a>



<pre><code><b>const</b> <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>: u64 = 9223372036854775808;
</code></pre>



<a name="0xdee9_critbit_new"></a>

## Function `new`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_new">new</a>&lt;V: store&gt;(ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_new">new</a>&lt;V: store&gt;(ctx: &<b>mut</b> TxContext): <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt; {
    <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;{
        root: <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
        internal_nodes: <a href="../../../.././build/Sui/docs/table.md#0x2_table_new">table::new</a>(ctx),
        leaves: <a href="../../../.././build/Sui/docs/table.md#0x2_table_new">table::new</a>(ctx),
        min_leaf: <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
        max_leaf: <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
        next_internal_node_index: 0,
        next_leaf_index: 0
    }
}
</code></pre>



</details>

<a name="0xdee9_critbit_size"></a>

## Function `size`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_size">size</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_size">size</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;): u64 {
    <a href="../../../.././build/Sui/docs/table.md#0x2_table_length">table::length</a>(&tree.leaves)
}
</code></pre>



</details>

<a name="0xdee9_critbit_is_empty"></a>

## Function `is_empty`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_is_empty">is_empty</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_is_empty">is_empty</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;): bool {
    <a href="../../../.././build/Sui/docs/table.md#0x2_table_is_empty">table::is_empty</a>(&tree.leaves)
}
</code></pre>



</details>

<a name="0xdee9_critbit_min_leaf"></a>

## Function `min_leaf`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_min_leaf">min_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_min_leaf">min_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;): (u64, u64) {
    <b>assert</b>!(!<a href="critbit.md#0xdee9_critbit_is_empty">is_empty</a>(tree), <a href="critbit.md#0xdee9_critbit_ELeafNotExist">ELeafNotExist</a>);
    <b>let</b> min_leaf = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, tree.min_leaf);
    <b>return</b> (min_leaf.key, tree.min_leaf)
}
</code></pre>



</details>

<a name="0xdee9_critbit_max_leaf"></a>

## Function `max_leaf`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_max_leaf">max_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_max_leaf">max_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;): (u64, u64) {
    <b>assert</b>!(!<a href="critbit.md#0xdee9_critbit_is_empty">is_empty</a>(tree), <a href="critbit.md#0xdee9_critbit_ELeafNotExist">ELeafNotExist</a>);
    <b>let</b> max_leaf = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, tree.max_leaf);
    <b>return</b> (max_leaf.key, tree.max_leaf)
}
</code></pre>



</details>

<a name="0xdee9_critbit_previous_leaf"></a>

## Function `previous_leaf`



<pre><code><b>public</b> <b>fun</b> <a href="critbit.md#0xdee9_critbit_previous_leaf">previous_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, _key: u64): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="critbit.md#0xdee9_critbit_previous_leaf">previous_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, _key: u64): (u64, u64) {
    <b>let</b> (_, index) = <a href="critbit.md#0xdee9_critbit_find_leaf">find_leaf</a>(tree, _key);
    <b>assert</b>!(index != <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>, <a href="critbit.md#0xdee9_critbit_ELeafNotExist">ELeafNotExist</a>);
    <b>let</b> ptr = <a href="critbit.md#0xdee9_critbit_MAX_U64">MAX_U64</a> - index;
    <b>let</b> parent = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, index).parent;
    <b>while</b> (parent != <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a> && <a href="critbit.md#0xdee9_critbit_is_left_child">is_left_child</a>(tree, parent, ptr)){
        ptr = parent;
        parent = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.internal_nodes, ptr).parent;
    };
    <b>if</b>(parent == <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
        <b>return</b> (0, <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>)
    };
    index = <a href="critbit.md#0xdee9_critbit_MAX_U64">MAX_U64</a> - <a href="critbit.md#0xdee9_critbit_right_most_leaf">right_most_leaf</a>(tree, <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.internal_nodes, parent).left_child);
    <b>let</b> key = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, index).key;
    <b>return</b> (key, index)
}
</code></pre>



</details>

<a name="0xdee9_critbit_next_leaf"></a>

## Function `next_leaf`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_next_leaf">next_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, _key: u64): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_next_leaf">next_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, _key: u64): (u64, u64) {
    <b>let</b> (_, index) = <a href="critbit.md#0xdee9_critbit_find_leaf">find_leaf</a>(tree, _key);
    <b>assert</b>!(index != <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>, <a href="critbit.md#0xdee9_critbit_ELeafNotExist">ELeafNotExist</a>);
    <b>let</b> ptr = <a href="critbit.md#0xdee9_critbit_MAX_U64">MAX_U64</a> - index;
    <b>let</b> parent = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, index).parent;
    <b>while</b> (parent != <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a> && !<a href="critbit.md#0xdee9_critbit_is_left_child">is_left_child</a>(tree, parent, ptr)){
        ptr = parent;
        parent = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.internal_nodes, ptr).parent;
    };
    <b>if</b>(parent == <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
        <b>return</b> (0, <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>)
    };
    index = <a href="critbit.md#0xdee9_critbit_MAX_U64">MAX_U64</a> - <a href="critbit.md#0xdee9_critbit_left_most_leaf">left_most_leaf</a>(tree, <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.internal_nodes, parent).right_child);
    <b>let</b> key = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, index).key;
    <b>return</b> (key, index)
}
</code></pre>



</details>

<a name="0xdee9_critbit_left_most_leaf"></a>

## Function `left_most_leaf`



<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_left_most_leaf">left_most_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, root: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_left_most_leaf">left_most_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, root: u64): u64 {
    <b>let</b> ptr = root;
    <b>while</b> (ptr &lt; <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>){
        ptr = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(& tree.internal_nodes, ptr).left_child;
    };
    ptr
}
</code></pre>



</details>

<a name="0xdee9_critbit_right_most_leaf"></a>

## Function `right_most_leaf`



<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_right_most_leaf">right_most_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, root: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_right_most_leaf">right_most_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, root: u64): u64 {
    <b>let</b> ptr = root;
    <b>while</b> (ptr &lt; <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>){
        ptr = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(& tree.internal_nodes, ptr).right_child;
    };
    ptr
}
</code></pre>



</details>

<a name="0xdee9_critbit_insert_leaf"></a>

## Function `insert_leaf`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_insert_leaf">insert_leaf</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, _key: u64, _value: V): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_insert_leaf">insert_leaf</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, _key: u64, _value: V): u64 {
    <b>let</b> new_leaf = <a href="critbit.md#0xdee9_critbit_Leaf">Leaf</a>&lt;V&gt;{
        key: _key,
        value: _value,
        parent: <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
    };
    <b>let</b> new_leaf_index = tree.next_leaf_index;
    tree.next_leaf_index = tree.next_leaf_index + 1;
    <b>assert</b>!(new_leaf_index &lt; <a href="critbit.md#0xdee9_critbit_MAX_CAPACITY">MAX_CAPACITY</a> - 1, <a href="critbit.md#0xdee9_critbit_EExceedCapacity">EExceedCapacity</a>);
    <a href="../../../.././build/Sui/docs/table.md#0x2_table_add">table::add</a>(&<b>mut</b> tree.leaves, new_leaf_index, new_leaf);

    <b>let</b> closest_leaf_index = <a href="critbit.md#0xdee9_critbit_get_closest_leaf_index_by_key">get_closest_leaf_index_by_key</a>(tree, _key);

    // handle the first insertion
    <b>if</b>(closest_leaf_index == <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>){
        <b>assert</b>!(new_leaf_index == 0, <a href="critbit.md#0xdee9_critbit_ETreeNotEmpty">ETreeNotEmpty</a>);
        tree.root = <a href="critbit.md#0xdee9_critbit_MAX_U64">MAX_U64</a> - new_leaf_index;
        tree.min_leaf = new_leaf_index;
        tree.max_leaf = new_leaf_index;
        <b>return</b> 0
    };

    <b>let</b> closest_key = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, closest_leaf_index).key;
    <b>assert</b>!(closest_key != _key, <a href="critbit.md#0xdee9_critbit_EKeyAlreadyExist">EKeyAlreadyExist</a>);

    // note that we reserve count_leading_zeros of form u128 for future usage
    <b>let</b> <a href="critbit.md#0xdee9_critbit">critbit</a> = 64 - (<a href="critbit.md#0xdee9_critbit_count_leading_zeros">count_leading_zeros</a>(((closest_key ^ _key) <b>as</b> u128) ) -64);
    <b>let</b> new_mask = 1u64 &lt;&lt; (<a href="critbit.md#0xdee9_critbit">critbit</a> - 1);

    <b>let</b> new_internal_node = <a href="critbit.md#0xdee9_critbit_InternalNode">InternalNode</a>{
        mask: new_mask,
        left_child: <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
        right_child: <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
        parent: <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>,
    };
    <b>let</b> new_internal_node_index = tree.next_internal_node_index;
    tree.next_internal_node_index = tree.next_internal_node_index + 1;
    <a href="../../../.././build/Sui/docs/table.md#0x2_table_add">table::add</a>(&<b>mut</b> tree.internal_nodes, new_internal_node_index, new_internal_node);

    <b>let</b> ptr = tree.root;
    <b>let</b> new_internal_node_parent_index = <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
    // search position of the new <b>internal</b> node
    <b>while</b> (ptr &lt; <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
        <b>let</b> internal_node = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.internal_nodes, ptr);
        <b>if</b> (new_mask &gt; internal_node.mask) {
            <b>break</b>
        };
        new_internal_node_parent_index = ptr;
        <b>if</b> (_key & internal_node.mask == 0){
            ptr = internal_node.left_child;
        }<b>else</b> {
            ptr = internal_node.right_child;
        };
    };

    // we <b>update</b> the child info of new <b>internal</b> node's parent
    <b>if</b> (new_internal_node_parent_index == <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>){
        // <b>if</b> the new <b>internal</b> node is root
        tree.root = new_internal_node_index;
    } <b>else</b>{
        // In another case, we <b>update</b> the child field of the new <b>internal</b> node's parent
        // and the parent field of the new <b>internal</b> node
        <b>let</b> is_left_child = <a href="critbit.md#0xdee9_critbit_is_left_child">is_left_child</a>(tree, new_internal_node_parent_index, ptr);
        <a href="critbit.md#0xdee9_critbit_update_child">update_child</a>(tree, new_internal_node_parent_index, new_internal_node_index, is_left_child);
    };

    // finally, we <b>update</b> the child filed of the new <b>internal</b> node
    <b>let</b> is_left_child = new_mask & _key == 0;
    <a href="critbit.md#0xdee9_critbit_update_child">update_child</a>(tree, new_internal_node_index, <a href="critbit.md#0xdee9_critbit_MAX_U64">MAX_U64</a> - new_leaf_index, is_left_child);
    <a href="critbit.md#0xdee9_critbit_update_child">update_child</a>(tree, new_internal_node_index, ptr, !is_left_child);

    <b>if</b> (<a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, tree.min_leaf).key &gt; _key) {
        tree.min_leaf = new_leaf_index;
    };
    <b>if</b> (<a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, tree.max_leaf).key &lt; _key) {
        tree.max_leaf = new_leaf_index;
    };
    new_leaf_index
}
</code></pre>



</details>

<a name="0xdee9_critbit_find_leaf"></a>

## Function `find_leaf`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_find_leaf">find_leaf</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, _key: u64): (bool, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_find_leaf">find_leaf</a>&lt;V: store&gt;(tree: & <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, _key: u64): (bool, u64) {
    <b>if</b> (<a href="critbit.md#0xdee9_critbit_is_empty">is_empty</a>(tree)) {
        <b>return</b> (<b>false</b>, <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>)
    };
    <b>let</b> closest_leaf_index = <a href="critbit.md#0xdee9_critbit_get_closest_leaf_index_by_key">get_closest_leaf_index_by_key</a>(tree, _key);
    <b>let</b> closeset_leaf = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, closest_leaf_index);
    <b>if</b> (closeset_leaf.key != _key){
        <b>return</b> (<b>false</b>, <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>)
    } <b>else</b>{
        <b>return</b> (<b>true</b>, closest_leaf_index)
    }
}
</code></pre>



</details>

<a name="0xdee9_critbit_find_closest_key"></a>

## Function `find_closest_key`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_find_closest_key">find_closest_key</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, _key: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_find_closest_key">find_closest_key</a>&lt;V: store&gt;(tree: & <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, _key: u64): u64 {
    <b>if</b> (<a href="critbit.md#0xdee9_critbit_is_empty">is_empty</a>(tree)) {
        <b>return</b> 0
    };
    <b>let</b> closest_leaf_index = <a href="critbit.md#0xdee9_critbit_get_closest_leaf_index_by_key">get_closest_leaf_index_by_key</a>(tree, _key);
    <b>let</b> closeset_leaf = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, closest_leaf_index);
    closeset_leaf.key
}
</code></pre>



</details>

<a name="0xdee9_critbit_remove_leaf_by_index"></a>

## Function `remove_leaf_by_index`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_remove_leaf_by_index">remove_leaf_by_index</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, _index: u64): V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_remove_leaf_by_index">remove_leaf_by_index</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, _index: u64): V {
    <b>let</b> key = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(& tree.leaves, _index).key;
    <b>if</b>(tree.min_leaf == _index) {
        <b>let</b> (_, index) = <a href="critbit.md#0xdee9_critbit_next_leaf">next_leaf</a>(tree, key);
        tree.min_leaf = index;
    };
    <b>if</b>(tree.max_leaf == _index) {
        <b>let</b> (_, index) = <a href="critbit.md#0xdee9_critbit_previous_leaf">previous_leaf</a>(tree, key);
        tree.max_leaf = index;
    };

    <b>let</b> is_left_child_;
    <b>let</b> <a href="critbit.md#0xdee9_critbit_Leaf">Leaf</a>&lt;V&gt; {key: _, value, parent: removed_leaf_parent_index} = <a href="../../../.././build/Sui/docs/table.md#0x2_table_remove">table::remove</a>(&<b>mut</b> tree.leaves, _index);
    <b>if</b> (<a href="critbit.md#0xdee9_critbit_size">size</a>(tree) == 0) {
        tree.root = <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
        tree.min_leaf = <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
        tree.max_leaf = <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
        tree.next_internal_node_index = 0;
        tree.next_leaf_index = 0;
    } <b>else</b>{
        <b>assert</b>!(removed_leaf_parent_index != <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>, <a href="critbit.md#0xdee9_critbit_EIndexOutOfRange">EIndexOutOfRange</a>);
        <b>let</b> removed_leaf_parent = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.internal_nodes, removed_leaf_parent_index);
        <b>let</b> removed_leaf_grand_parent_index = removed_leaf_parent.parent;

        // note that sibling of the removed leaf can be a leaf or a <b>internal</b> node
        is_left_child_ = <a href="critbit.md#0xdee9_critbit_is_left_child">is_left_child</a>(tree, removed_leaf_parent_index, <a href="critbit.md#0xdee9_critbit_MAX_U64">MAX_U64</a> - _index);
        <b>let</b> sibling_index = <b>if</b> (is_left_child_) { removed_leaf_parent.right_child }
        <b>else</b> { removed_leaf_parent.left_child };

        <b>if</b> (removed_leaf_grand_parent_index == <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
            // parent of the removed leaf is the tree root
            // <b>update</b> the parent of the sibling node and and set sibling <b>as</b> the tree root
            <b>if</b> (sibling_index &lt; <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
                // sibling is a <b>internal</b> node
                <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> tree.internal_nodes, sibling_index).parent = <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
            } <b>else</b>{
                // sibling is a leaf
                <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> tree.leaves, <a href="critbit.md#0xdee9_critbit_MAX_U64">MAX_U64</a> - sibling_index).parent = <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
            };
            tree.root = sibling_index;
        } <b>else</b> {
            // grand parent of the removed leaf is a <b>internal</b> node
            // set sibling <b>as</b> the child of the grand parent of the removed leaf
            is_left_child_ = <a href="critbit.md#0xdee9_critbit_is_left_child">is_left_child</a>(tree, removed_leaf_grand_parent_index, removed_leaf_parent_index);
            <a href="critbit.md#0xdee9_critbit_update_child">update_child</a>(tree, removed_leaf_grand_parent_index, sibling_index, is_left_child_);
        };
        <a href="../../../.././build/Sui/docs/table.md#0x2_table_remove">table::remove</a>(&<b>mut</b> tree.internal_nodes, removed_leaf_parent_index);
    };
    value
}
</code></pre>



</details>

<a name="0xdee9_critbit_borrow_mut_leaf_by_index"></a>

## Function `borrow_mut_leaf_by_index`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_borrow_mut_leaf_by_index">borrow_mut_leaf_by_index</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, index: u64): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_borrow_mut_leaf_by_index">borrow_mut_leaf_by_index</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, index: u64): &<b>mut</b> V {
    <b>let</b> entry = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> tree.leaves, index);
    &<b>mut</b> entry.value
}
</code></pre>



</details>

<a name="0xdee9_critbit_borrow_leaf_by_index"></a>

## Function `borrow_leaf_by_index`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_borrow_leaf_by_index">borrow_leaf_by_index</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, index: u64): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_borrow_leaf_by_index">borrow_leaf_by_index</a>&lt;V: store&gt;(tree: & <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, index: u64): &V {
    <b>let</b> entry = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.leaves, index);
    &entry.value
}
</code></pre>



</details>

<a name="0xdee9_critbit_borrow_leaf_by_key"></a>

## Function `borrow_leaf_by_key`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_borrow_leaf_by_key">borrow_leaf_by_key</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, key: u64): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_borrow_leaf_by_key">borrow_leaf_by_key</a>&lt;V: store&gt;(tree: & <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, key: u64): &V {
    <b>let</b> (is_exist, index) = <a href="critbit.md#0xdee9_critbit_find_leaf">find_leaf</a>(tree, key);
    <b>assert</b>!(is_exist, <a href="critbit.md#0xdee9_critbit_ELeafNotExist">ELeafNotExist</a>);
    <a href="critbit.md#0xdee9_critbit_borrow_leaf_by_index">borrow_leaf_by_index</a>(tree, index)
}
</code></pre>



</details>

<a name="0xdee9_critbit_drop"></a>

## Function `drop`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_drop">drop</a>&lt;V: drop, store&gt;(tree: <a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_drop">drop</a>&lt;V: store + drop&gt;(tree: <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;) {
    <b>let</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt; {
        root: _,
        internal_nodes,
        leaves,
        min_leaf: _,
        max_leaf: _,
        next_internal_node_index: _,
        next_leaf_index: _,

    } = tree;
    <a href="../../../.././build/Sui/docs/table.md#0x2_table_drop">table::drop</a>(internal_nodes);
    <a href="../../../.././build/Sui/docs/table.md#0x2_table_drop">table::drop</a>(leaves);
}
</code></pre>



</details>

<a name="0xdee9_critbit_destroy_empty"></a>

## Function `destroy_empty`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_destroy_empty">destroy_empty</a>&lt;V: store&gt;(tree: <a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="critbit.md#0xdee9_critbit_destroy_empty">destroy_empty</a>&lt;V: store&gt;(tree: <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;) {
    <b>assert</b>!(<a href="../../../.././build/Sui/docs/table.md#0x2_table_length">table::length</a>(&tree.leaves) == 0, 0);

    <b>let</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt; {
        root: _,
        leaves,
        internal_nodes,
        min_leaf: _,
        max_leaf: _,
        next_internal_node_index: _,
        next_leaf_index: _
    } = tree;

    <a href="../../../.././build/Sui/docs/table.md#0x2_table_destroy_empty">table::destroy_empty</a>(leaves);
    <a href="../../../.././build/Sui/docs/table.md#0x2_table_destroy_empty">table::destroy_empty</a>(internal_nodes);
}
</code></pre>



</details>

<a name="0xdee9_critbit_get_closest_leaf_index_by_key"></a>

## Function `get_closest_leaf_index_by_key`



<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_get_closest_leaf_index_by_key">get_closest_leaf_index_by_key</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, _key: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_get_closest_leaf_index_by_key">get_closest_leaf_index_by_key</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, _key: u64): u64 {
    <b>let</b> ptr = tree.root;
    // <b>if</b> tree is empty, <b>return</b> the patrition index
    <b>if</b>(ptr == <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) <b>return</b> <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>;
    <b>while</b> (ptr &lt; <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>){
        <b>let</b> node = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.internal_nodes, ptr);
        <b>if</b> (_key & node.mask == 0){
            ptr = node.left_child;
        } <b>else</b> {
            ptr = node.right_child;
        }
    };
    <b>return</b> (<a href="critbit.md#0xdee9_critbit_MAX_U64">MAX_U64</a> - ptr)
}
</code></pre>



</details>

<a name="0xdee9_critbit_update_child"></a>

## Function `update_child`



<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_update_child">update_child</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, parent_index: u64, new_child: u64, is_left_child: bool)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_update_child">update_child</a>&lt;V: store&gt;(tree: &<b>mut</b> <a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, parent_index: u64, new_child: u64, is_left_child: bool) {
    <b>assert</b>!(parent_index != <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>, <a href="critbit.md#0xdee9_critbit_ENullParent">ENullParent</a>);
    <b>if</b> (is_left_child) {
        <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> tree.internal_nodes, parent_index).left_child = new_child;
    } <b>else</b>{
        <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> tree.internal_nodes, parent_index).right_child = new_child;
    };
    <b>if</b> (new_child != <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>) {
        <b>if</b> (new_child &gt; <a href="critbit.md#0xdee9_critbit_PARTITION_INDEX">PARTITION_INDEX</a>){
            <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> tree.leaves, <a href="critbit.md#0xdee9_critbit_MAX_U64">MAX_U64</a> - new_child).parent = parent_index;
        }<b>else</b>{
            <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> tree.internal_nodes, new_child).parent = parent_index;
        }
    };
}
</code></pre>



</details>

<a name="0xdee9_critbit_is_left_child"></a>

## Function `is_left_child`



<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_is_left_child">is_left_child</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">critbit::CritbitTree</a>&lt;V&gt;, parent_index: u64, index: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_is_left_child">is_left_child</a>&lt;V: store&gt;(tree: &<a href="critbit.md#0xdee9_critbit_CritbitTree">CritbitTree</a>&lt;V&gt;, parent_index: u64, index: u64): bool {
    <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&tree.internal_nodes, parent_index).left_child == index
}
</code></pre>



</details>

<a name="0xdee9_critbit_count_leading_zeros"></a>

## Function `count_leading_zeros`



<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_count_leading_zeros">count_leading_zeros</a>(x: u128): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="critbit.md#0xdee9_critbit_count_leading_zeros">count_leading_zeros</a>(x: u128): u8 {
    <b>if</b> (x == 0) {
        128
    } <b>else</b> {
        <b>let</b> n: u8 = 0;
        <b>if</b> (x & 0xFFFFFFFFFFFFFFFF0000000000000000 == 0) {
            // x's higher 64 is all zero, shift the lower part over
            x = x &lt;&lt; 64;
            n = n + 64;
        };
        <b>if</b> (x & 0xFFFFFFFF000000000000000000000000 == 0) {
            // x's higher 32 is all zero, shift the lower part over
            x = x &lt;&lt; 32;
            n = n + 32;
        };
        <b>if</b> (x & 0xFFFF0000000000000000000000000000 == 0) {
            // x's higher 16 is all zero, shift the lower part over
            x = x &lt;&lt; 16;
            n = n + 16;
        };
        <b>if</b> (x & 0xFF000000000000000000000000000000 == 0) {
            // x's higher 8 is all zero, shift the lower part over
            x = x &lt;&lt; 8;
            n = n + 8;
        };
        <b>if</b> (x & 0xF0000000000000000000000000000000 == 0) {
            // x's higher 4 is all zero, shift the lower part over
            x = x &lt;&lt; 4;
            n = n + 4;
        };
        <b>if</b> (x & 0xC0000000000000000000000000000000 == 0) {
            // x's higher 2 is all zero, shift the lower part over
            x = x &lt;&lt; 2;
            n = n + 2;
        };
        <b>if</b> (x & 0x80000000000000000000000000000000 == 0) {
            n = n + 1;
        };

        n
    }
}
</code></pre>



</details>
