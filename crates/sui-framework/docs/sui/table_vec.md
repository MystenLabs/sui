---
title: Module `sui::table_vec`
---

A basic scalable vector library implemented using <code>Table</code>.


-  [Struct `TableVec`](#sui_table_vec_TableVec)
-  [Constants](#@Constants_0)
-  [Function `empty`](#sui_table_vec_empty)
-  [Function `singleton`](#sui_table_vec_singleton)
-  [Function `length`](#sui_table_vec_length)
-  [Function `is_empty`](#sui_table_vec_is_empty)
-  [Function `borrow`](#sui_table_vec_borrow)
-  [Function `push_back`](#sui_table_vec_push_back)
-  [Function `borrow_mut`](#sui_table_vec_borrow_mut)
-  [Function `pop_back`](#sui_table_vec_pop_back)
-  [Function `destroy_empty`](#sui_table_vec_destroy_empty)
-  [Function `drop`](#sui_table_vec_drop)
-  [Function `swap`](#sui_table_vec_swap)
-  [Function `swap_remove`](#sui_table_vec_swap_remove)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
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



<a name="sui_table_vec_TableVec"></a>

## Struct `TableVec`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;<b>phantom</b> Element: store&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>contents: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;u64, Element&gt;</code>
</dt>
<dd>
 The contents of the table vector.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_table_vec_EIndexOutOfBound"></a>



<pre><code><b>const</b> <a href="../sui/table_vec.md#sui_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>: u64 = 0;
</code></pre>



<a name="sui_table_vec_ETableNonEmpty"></a>



<pre><code><b>const</b> <a href="../sui/table_vec.md#sui_table_vec_ETableNonEmpty">ETableNonEmpty</a>: u64 = 1;
</code></pre>



<a name="sui_table_vec_empty"></a>

## Function `empty`

Create an empty TableVec.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_empty">empty</a>&lt;Element: store&gt;(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_empty">empty</a>&lt;Element: store&gt;(ctx: &<b>mut</b> TxContext): <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt; {
    <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a> {
        contents: <a href="../sui/table.md#sui_table_new">table::new</a>(ctx),
    }
}
</code></pre>



</details>

<a name="sui_table_vec_singleton"></a>

## Function `singleton`

Return a TableVec of size one containing element <code>e</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_singleton">singleton</a>&lt;Element: store&gt;(e: Element, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_singleton">singleton</a>&lt;Element: store&gt;(e: Element, ctx: &<b>mut</b> TxContext): <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt; {
    <b>let</b> <b>mut</b> t = <a href="../sui/table_vec.md#sui_table_vec_empty">empty</a>(ctx);
    t.<a href="../sui/table_vec.md#sui_table_vec_push_back">push_back</a>(e);
    t
}
</code></pre>



</details>

<a name="sui_table_vec_length"></a>

## Function `length`

Return the length of the TableVec.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_length">length</a>&lt;Element: store&gt;(t: &<a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_length">length</a>&lt;Element: store&gt;(t: &<a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt;): u64 {
    t.contents.<a href="../sui/table_vec.md#sui_table_vec_length">length</a>()
}
</code></pre>



</details>

<a name="sui_table_vec_is_empty"></a>

## Function `is_empty`

Return if the TableVec is empty or not.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_is_empty">is_empty</a>&lt;Element: store&gt;(t: &<a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_is_empty">is_empty</a>&lt;Element: store&gt;(t: &<a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt;): bool {
    t.<a href="../sui/table_vec.md#sui_table_vec_length">length</a>() == 0
}
</code></pre>



</details>

<a name="sui_table_vec_borrow"></a>

## Function `borrow`

Acquire an immutable reference to the <code>i</code>th element of the TableVec <code>t</code>.
Aborts if <code>i</code> is out of bounds.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/borrow.md#sui_borrow">borrow</a>&lt;Element: store&gt;(t: &<a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;, i: u64): &Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/borrow.md#sui_borrow">borrow</a>&lt;Element: store&gt;(t: &<a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt;, i: u64): &Element {
    <b>assert</b>!(t.<a href="../sui/table_vec.md#sui_table_vec_length">length</a>() &gt; i, <a href="../sui/table_vec.md#sui_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    &t.contents[i]
}
</code></pre>



</details>

<a name="sui_table_vec_push_back"></a>

## Function `push_back`

Add element <code>e</code> to the end of the TableVec <code>t</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_push_back">push_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;, e: Element)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_push_back">push_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt;, e: Element) {
    <b>let</b> key = t.<a href="../sui/table_vec.md#sui_table_vec_length">length</a>();
    t.contents.add(key, e);
}
</code></pre>



</details>

<a name="sui_table_vec_borrow_mut"></a>

## Function `borrow_mut`

Return a mutable reference to the <code>i</code>th element in the TableVec <code>t</code>.
Aborts if <code>i</code> is out of bounds.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_borrow_mut">borrow_mut</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;, i: u64): &<b>mut</b> Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_borrow_mut">borrow_mut</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt;, i: u64): &<b>mut</b> Element {
    <b>assert</b>!(t.<a href="../sui/table_vec.md#sui_table_vec_length">length</a>() &gt; i, <a href="../sui/table_vec.md#sui_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    &<b>mut</b> t.contents[i]
}
</code></pre>



</details>

<a name="sui_table_vec_pop_back"></a>

## Function `pop_back`

Pop an element from the end of TableVec <code>t</code>.
Aborts if <code>t</code> is empty.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_pop_back">pop_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_pop_back">pop_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt;): Element {
    <b>let</b> <a href="../sui/table_vec.md#sui_table_vec_length">length</a> = <a href="../sui/table_vec.md#sui_table_vec_length">length</a>(t);
    <b>assert</b>!(<a href="../sui/table_vec.md#sui_table_vec_length">length</a> &gt; 0, <a href="../sui/table_vec.md#sui_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    t.contents.remove(<a href="../sui/table_vec.md#sui_table_vec_length">length</a> - 1)
}
</code></pre>



</details>

<a name="sui_table_vec_destroy_empty"></a>

## Function `destroy_empty`

Destroy the TableVec <code>t</code>.
Aborts if <code>t</code> is not empty.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_destroy_empty">destroy_empty</a>&lt;Element: store&gt;(t: <a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_destroy_empty">destroy_empty</a>&lt;Element: store&gt;(t: <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt;) {
    <b>assert</b>!(<a href="../sui/table_vec.md#sui_table_vec_length">length</a>(&t) == 0, <a href="../sui/table_vec.md#sui_table_vec_ETableNonEmpty">ETableNonEmpty</a>);
    <b>let</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a> { contents } = t;
    contents.<a href="../sui/table_vec.md#sui_table_vec_destroy_empty">destroy_empty</a>();
}
</code></pre>



</details>

<a name="sui_table_vec_drop"></a>

## Function `drop`

Drop a possibly non-empty TableVec <code>t</code>.
Usable only if the value type <code>Element</code> has the <code><a href="../sui/table_vec.md#sui_table_vec_drop">drop</a></code> ability


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_drop">drop</a>&lt;Element: <a href="../sui/table_vec.md#sui_table_vec_drop">drop</a>, store&gt;(t: <a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_drop">drop</a>&lt;Element: <a href="../sui/table_vec.md#sui_table_vec_drop">drop</a> + store&gt;(t: <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt;) {
    <b>let</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a> { contents } = t;
    contents.<a href="../sui/table_vec.md#sui_table_vec_drop">drop</a>()
}
</code></pre>



</details>

<a name="sui_table_vec_swap"></a>

## Function `swap`

Swaps the elements at the <code>i</code>th and <code>j</code>th indices in the TableVec <code>t</code>.
Aborts if <code>i</code> or <code>j</code> is out of bounds.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_swap">swap</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;, i: u64, j: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_swap">swap</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt;, i: u64, j: u64) {
    <b>assert</b>!(t.<a href="../sui/table_vec.md#sui_table_vec_length">length</a>() &gt; i, <a href="../sui/table_vec.md#sui_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <b>assert</b>!(t.<a href="../sui/table_vec.md#sui_table_vec_length">length</a>() &gt; j, <a href="../sui/table_vec.md#sui_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <b>if</b> (i == j) {
        <b>return</b>
    };
    <b>let</b> element_i = t.contents.remove(i);
    <b>let</b> element_j = t.contents.remove(j);
    t.contents.add(j, element_i);
    t.contents.add(i, element_j);
}
</code></pre>



</details>

<a name="sui_table_vec_swap_remove"></a>

## Function `swap_remove`

Swap the <code>i</code>th element of the TableVec <code>t</code> with the last element and then pop the TableVec.
This is O(1), but does not preserve ordering of elements in the TableVec.
Aborts if <code>i</code> is out of bounds.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_swap_remove">swap_remove</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">sui::table_vec::TableVec</a>&lt;Element&gt;, i: u64): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/table_vec.md#sui_table_vec_swap_remove">swap_remove</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="../sui/table_vec.md#sui_table_vec_TableVec">TableVec</a>&lt;Element&gt;, i: u64): Element {
    <b>assert</b>!(t.<a href="../sui/table_vec.md#sui_table_vec_length">length</a>() &gt; i, <a href="../sui/table_vec.md#sui_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <b>let</b> last_idx = t.<a href="../sui/table_vec.md#sui_table_vec_length">length</a>() - 1;
    t.<a href="../sui/table_vec.md#sui_table_vec_swap">swap</a>(i, last_idx);
    t.<a href="../sui/table_vec.md#sui_table_vec_pop_back">pop_back</a>()
}
</code></pre>



</details>
