
<a name="0x2_table_vec"></a>

# Module `0x2::table_vec`

A basic scalable vector library implemented using <code>Table</code>.


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


<pre><code><b>use</b> <a href="table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_table_vec_TableVec"></a>

## Struct `TableVec`



<pre><code><b>struct</b> <a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element: store&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>contents: <a href="table.md#0x2_table_Table">table::Table</a>&lt;u64, Element&gt;</code>
</dt>
<dd>
 The contents of the table vector.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_table_vec_EIndexOutOfBound"></a>



<pre><code><b>const</b> <a href="table_vec.md#0x2_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>: u64 = 0;
</code></pre>



<a name="0x2_table_vec_ETableNonEmpty"></a>



<pre><code><b>const</b> <a href="table_vec.md#0x2_table_vec_ETableNonEmpty">ETableNonEmpty</a>: u64 = 1;
</code></pre>



<a name="0x2_table_vec_empty"></a>

## Function `empty`

Create an empty TableVec.


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_empty">empty</a>&lt;Element: store&gt;(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_empty">empty</a>&lt;Element: store&gt;(ctx: &<b>mut</b> TxContext): <a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt; {
    <a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a> {
        contents: <a href="table.md#0x2_table_new">table::new</a>(ctx)
    }
}
</code></pre>



</details>

<a name="0x2_table_vec_singleton"></a>

## Function `singleton`

Return a TableVec of size one containing element <code>e</code>.


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_singleton">singleton</a>&lt;Element: store&gt;(e: Element, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_singleton">singleton</a>&lt;Element: store&gt;(e: Element, ctx: &<b>mut</b> TxContext): <a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt; {
    <b>let</b> t = <a href="table_vec.md#0x2_table_vec_empty">empty</a>(ctx);
    <a href="table_vec.md#0x2_table_vec_push_back">push_back</a>(&<b>mut</b> t, e);
    t
}
</code></pre>



</details>

<a name="0x2_table_vec_length"></a>

## Function `length`

Return the length of the TableVec.


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_length">length</a>&lt;Element: store&gt;(t: &<a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_length">length</a>&lt;Element: store&gt;(t: &<a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;): u64 {
    <a href="table.md#0x2_table_length">table::length</a>(&t.contents)
}
</code></pre>



</details>

<a name="0x2_table_vec_is_empty"></a>

## Function `is_empty`

Return if the TableVec is empty or not.


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_is_empty">is_empty</a>&lt;Element: store&gt;(t: &<a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_is_empty">is_empty</a>&lt;Element: store&gt;(t: &<a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;): bool {
    <a href="table_vec.md#0x2_table_vec_length">length</a>(t) == 0
}
</code></pre>



</details>

<a name="0x2_table_vec_borrow"></a>

## Function `borrow`

Acquire an immutable reference to the <code>i</code>th element of the TableVec <code>t</code>.
Aborts if <code>i</code> is out of bounds.


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow">borrow</a>&lt;Element: store&gt;(t: &<a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;, i: u64): &Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow">borrow</a>&lt;Element: store&gt;(t: &<a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;, i: u64): &Element {
    <b>assert</b>!(<a href="table_vec.md#0x2_table_vec_length">length</a>(t) &gt; i, <a href="table_vec.md#0x2_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <a href="table.md#0x2_table_borrow">table::borrow</a>(&t.contents, i)
}
</code></pre>



</details>

<a name="0x2_table_vec_push_back"></a>

## Function `push_back`

Add element <code>e</code> to the end of the TableVec <code>t</code>.


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_push_back">push_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;, e: Element)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_push_back">push_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;, e: Element) {
    <b>let</b> key = <a href="table_vec.md#0x2_table_vec_length">length</a>(t);
    <a href="table.md#0x2_table_add">table::add</a>(&<b>mut</b> t.contents, key, e);
}
</code></pre>



</details>

<a name="0x2_table_vec_borrow_mut"></a>

## Function `borrow_mut`

Return a mutable reference to the <code>i</code>th element in the TableVec <code>t</code>.
Aborts if <code>i</code> is out of bounds.


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_borrow_mut">borrow_mut</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;, i: u64): &<b>mut</b> Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_borrow_mut">borrow_mut</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;, i: u64): &<b>mut</b> Element {
    <b>assert</b>!(<a href="table_vec.md#0x2_table_vec_length">length</a>(t) &gt; i, <a href="table_vec.md#0x2_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <a href="table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> t.contents, i)
}
</code></pre>



</details>

<a name="0x2_table_vec_pop_back"></a>

## Function `pop_back`

Pop an element from the end of TableVec <code>t</code>.
Aborts if <code>t</code> is empty.


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_pop_back">pop_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_pop_back">pop_back</a>&lt;Element: store&gt;(t: &<b>mut</b> <a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;): Element {
    <b>let</b> length = <a href="table_vec.md#0x2_table_vec_length">length</a>(t);
    <b>assert</b>!(length &gt; 0, <a href="table_vec.md#0x2_table_vec_EIndexOutOfBound">EIndexOutOfBound</a>);
    <a href="table.md#0x2_table_remove">table::remove</a>(&<b>mut</b> t.contents, length - 1)
}
</code></pre>



</details>

<a name="0x2_table_vec_destroy_empty"></a>

## Function `destroy_empty`

Destroy the TableVec <code>t</code>.
Aborts if <code>t</code> is not empty.


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_destroy_empty">destroy_empty</a>&lt;Element: store&gt;(t: <a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_destroy_empty">destroy_empty</a>&lt;Element: store&gt;(t: <a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;) {
    <b>assert</b>!(<a href="table_vec.md#0x2_table_vec_length">length</a>(&t) == 0, <a href="table_vec.md#0x2_table_vec_ETableNonEmpty">ETableNonEmpty</a>);
    <b>let</b> <a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a> { contents } = t;
    <a href="table.md#0x2_table_destroy_empty">table::destroy_empty</a>(contents);
}
</code></pre>



</details>

<a name="0x2_table_vec_drop"></a>

## Function `drop`

Drop a possibly non-empty TableVec <code>t</code>.
Usable only if the value type <code>Element</code> has the <code>drop</code> ability


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_drop">drop</a>&lt;Element: drop, store&gt;(t: <a href="table_vec.md#0x2_table_vec_TableVec">table_vec::TableVec</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="table_vec.md#0x2_table_vec_drop">drop</a>&lt;Element: drop + store&gt;(t: <a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a>&lt;Element&gt;) {
    <b>let</b> <a href="table_vec.md#0x2_table_vec_TableVec">TableVec</a> { contents } = t;
    <a href="table.md#0x2_table_drop">table::drop</a>(contents)
}
</code></pre>



</details>
