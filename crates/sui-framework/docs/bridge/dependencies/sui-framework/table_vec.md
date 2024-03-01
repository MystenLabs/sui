
<a name="0x2_table_vec"></a>

# Module `0x2::table_vec`



-  [Struct `TableVec`](#0x2_table_vec_TableVec)
-  [Constants](#@Constants_0)
-  [Function `empty`](#0x2_table_vec_empty)
-  [Function `singleton`](#0x2_table_vec_singleton)
-  [Function `length`](#0x2_table_vec_length)
-  [Function `is_empty`](#0x2_table_vec_is_empty)
-  [Function `borrow`](#0x2_table_vec_borrow)
-  [Function `push_back`](#0x2_table_vec_push_back)
-  [Function `borrow_mut`](#0x2_table_vec_borrow_mut)
-  [Function `pop_back`](#0x2_table_vec_pop_back)
-  [Function `destroy_empty`](#0x2_table_vec_destroy_empty)
-  [Function `drop`](#0x2_table_vec_drop)
-  [Function `swap`](#0x2_table_vec_swap)
-  [Function `swap_remove`](#0x2_table_vec_swap_remove)


<pre><code><b>use</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_table_vec_TableVec"></a>

## Struct `TableVec`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element: store&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>contents: <a href="../../dependencies/sui-framework/table.md#0x2_table_Table">table::Table</a>&lt;u64, Element&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_table_vec_EIndexOutOfBound"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>: u64 = 0;
</code></pre>



<a name="0x2_table_vec_ETableNonEmpty"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_ETableNonEmpty">ETableNonEmpty</a>: u64 = 1;
</code></pre>



<a name="0x2_table_vec_empty"></a>

## Function `empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_empty">empty</a>&lt;Element: store&gt;(ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_empty">empty</a>&lt;Element: store&gt;(ctx: &<b>mut</b> TxContext): <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt; {
    <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a> {
        contents: <a href="../../dependencies/sui-framework/table.md#0x2_table_new">table::new</a>(ctx)
    }
}
</code></pre>



</details>

<a name="0x2_table_vec_singleton"></a>

## Function `singleton`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_singleton">singleton</a>&lt;Element: store&gt;(e: Element, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_singleton">singleton</a>&lt;Element: store&gt;(e: Element, ctx: &<b>mut</b> TxContext): <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt; {
    <b>let</b> t = <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_empty">empty</a>(ctx);
    <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_push_back">push_back</a>(&<b>mut</b> t, e);
    t
}
</code></pre>



</details>

<a name="0x2_table_vec_length"></a>

## Function `length`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>&lt;Element: store&gt;(t: &<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>&lt;Element: store&gt;(t: &<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;): u64 {
    <a href="../../dependencies/sui-framework/table.md#0x2_table_length">table::length</a>(&t.contents)
}
</code></pre>



</details>

<a name="0x2_table_vec_is_empty"></a>

## Function `is_empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_is_empty">is_empty</a>&lt;Element: store&gt;(t: &<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_is_empty">is_empty</a>&lt;Element: store&gt;(t: &<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;): bool {
    <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>(t) == 0
}
</code></pre>



</details>

<a name="0x2_table_vec_borrow"></a>

## Function `borrow`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_borrow">borrow</a>&lt;Element: store&gt;(t: &<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;, i: u64): &Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_borrow">borrow</a>&lt;Element: store&gt;(t: &<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;, i: u64): &Element {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>(t) &gt; i, <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <a href="../../dependencies/sui-framework/table.md#0x2_table_borrow">table::borrow</a>(&t.contents, i)
}
</code></pre>



</details>

<a name="0x2_table_vec_push_back"></a>

## Function `push_back`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_push_back">push_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;, e: Element)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_push_back">push_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;, e: Element) {
    <b>let</b> key = <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>(t);
    <a href="../../dependencies/sui-framework/table.md#0x2_table_add">table::add</a>(&<b>mut</b> t.contents, key, e);
}
</code></pre>



</details>

<a name="0x2_table_vec_borrow_mut"></a>

## Function `borrow_mut`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_borrow_mut">borrow_mut</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;, i: u64): &<b>mut</b> Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_borrow_mut">borrow_mut</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;, i: u64): &<b>mut</b> Element {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>(t) &gt; i, <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <a href="../../dependencies/sui-framework/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> t.contents, i)
}
</code></pre>



</details>

<a name="0x2_table_vec_pop_back"></a>

## Function `pop_back`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_pop_back">pop_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_pop_back">pop_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;): Element {
    <b>let</b> length = <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>(t);
    <b>assert</b>!(length &gt; 0, <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <a href="../../dependencies/sui-framework/table.md#0x2_table_remove">table::remove</a>(&<b>mut</b> t.contents, length - 1)
}
</code></pre>



</details>

<a name="0x2_table_vec_destroy_empty"></a>

## Function `destroy_empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_destroy_empty">destroy_empty</a>&lt;Element: store&gt;(t: <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_destroy_empty">destroy_empty</a>&lt;Element: store&gt;(t: <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;) {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>(&t) == 0, <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_ETableNonEmpty">ETableNonEmpty</a>);
    <b>let</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a> { contents } = t;
    <a href="../../dependencies/sui-framework/table.md#0x2_table_destroy_empty">table::destroy_empty</a>(contents);
}
</code></pre>



</details>

<a name="0x2_table_vec_drop"></a>

## Function `drop`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_drop">drop</a>&lt;Element: drop, store&gt;(t: <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_drop">drop</a>&lt;Element: drop + store&gt;(t: <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;) {
    <b>let</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a> { contents } = t;
    <a href="../../dependencies/sui-framework/table.md#0x2_table_drop">table::drop</a>(contents)
}
</code></pre>



</details>

<a name="0x2_table_vec_swap"></a>

## Function `swap`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_swap">swap</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;, i: u64, j: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_swap">swap</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;, i: u64, j: u64) {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>(t) &gt; i, <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <b>assert</b>!(<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>(t) &gt; j, <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <b>if</b> (i == j) { <b>return</b> };
    <b>let</b> element_i = <a href="../../dependencies/sui-framework/table.md#0x2_table_remove">table::remove</a>(&<b>mut</b> t.contents, i);
    <b>let</b> element_j = <a href="../../dependencies/sui-framework/table.md#0x2_table_remove">table::remove</a>(&<b>mut</b> t.contents, j);
    <a href="../../dependencies/sui-framework/table.md#0x2_table_add">table::add</a>(&<b>mut</b> t.contents, j, element_i);
    <a href="../../dependencies/sui-framework/table.md#0x2_table_add">table::add</a>(&<b>mut</b> t.contents, i, element_j);
}
</code></pre>



</details>

<a name="0x2_table_vec_swap_remove"></a>

## Function `swap_remove`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_swap_remove">swap_remove</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;, i: u64): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_swap_remove">swap_remove</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;, i: u64): Element {
    <b>assert</b>!(<a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>(t) &gt; i, <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <b>let</b> last_idx = <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_length">length</a>(t) - 1;
    <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_swap">swap</a>(t, i, last_idx);
    <a href="../../dependencies/sui-framework/table_vec.md#0x2_table_vec_pop_back">pop_back</a>(t)
}
</code></pre>



</details>
