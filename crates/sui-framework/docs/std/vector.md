---
title: Module `std::vector`
---

A variable-sized container that can hold any type. Indexing is 0-based, and
vectors are growable. This module has many native functions.


-  [Constants](#@Constants_0)
-  [Function `empty`](#std_vector_empty)
-  [Function `length`](#std_vector_length)
-  [Function `borrow`](#std_vector_borrow)
-  [Function `push_back`](#std_vector_push_back)
-  [Function `borrow_mut`](#std_vector_borrow_mut)
-  [Function `pop_back`](#std_vector_pop_back)
-  [Function `destroy_empty`](#std_vector_destroy_empty)
-  [Function `swap`](#std_vector_swap)
-  [Function `singleton`](#std_vector_singleton)
-  [Function `reverse`](#std_vector_reverse)
-  [Function `append`](#std_vector_append)
-  [Function `is_empty`](#std_vector_is_empty)
-  [Function `contains`](#std_vector_contains)
-  [Function `index_of`](#std_vector_index_of)
-  [Function `remove`](#std_vector_remove)
-  [Function `insert`](#std_vector_insert)
-  [Function `swap_remove`](#std_vector_swap_remove)
-  [Macro function `tabulate`](#std_vector_tabulate)
-  [Macro function `destroy`](#std_vector_destroy)
-  [Macro function `do`](#std_vector_do)
-  [Macro function `do_ref`](#std_vector_do_ref)
-  [Macro function `do_mut`](#std_vector_do_mut)
-  [Macro function `map`](#std_vector_map)
-  [Macro function `map_ref`](#std_vector_map_ref)
-  [Macro function `filter`](#std_vector_filter)
-  [Macro function `partition`](#std_vector_partition)
-  [Macro function `find_index`](#std_vector_find_index)
-  [Macro function `count`](#std_vector_count)
-  [Macro function `fold`](#std_vector_fold)
-  [Function `flatten`](#std_vector_flatten)
-  [Macro function `any`](#std_vector_any)
-  [Macro function `all`](#std_vector_all)
-  [Macro function `zip_do`](#std_vector_zip_do)
-  [Macro function `zip_do_reverse`](#std_vector_zip_do_reverse)
-  [Macro function `zip_do_ref`](#std_vector_zip_do_ref)
-  [Macro function `zip_do_mut`](#std_vector_zip_do_mut)
-  [Macro function `zip_map`](#std_vector_zip_map)
-  [Macro function `zip_map_ref`](#std_vector_zip_map_ref)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="std_vector_EINDEX_OUT_OF_BOUNDS"></a>

The index into the vector is out of bounds


<pre><code><b>const</b> <a href="../std/vector.md#std_vector_EINDEX_OUT_OF_BOUNDS">EINDEX_OUT_OF_BOUNDS</a>: <a href="../std/u64.md#std_u64">u64</a> = 131072;
</code></pre>



<a name="std_vector_empty"></a>

## Function `empty`

Create an empty vector.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_empty">empty</a>&lt;Element&gt;(): <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../std/vector.md#std_vector_empty">empty</a>&lt;Element&gt;(): <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;;
</code></pre>



</details>

<a name="std_vector_length"></a>

## Function `length`

Return the length of the vector.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_length">length</a>&lt;Element&gt;(v: &<a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;): <a href="../std/u64.md#std_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../std/vector.md#std_vector_length">length</a>&lt;Element&gt;(v: &<a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;): <a href="../std/u64.md#std_u64">u64</a>;
</code></pre>



</details>

<a name="std_vector_borrow"></a>

## Function `borrow`

Acquire an immutable reference to the <code>i</code>th element of the vector <code>v</code>.
Aborts if <code>i</code> is out of bounds.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_borrow">borrow</a>&lt;Element&gt;(v: &<a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, i: <a href="../std/u64.md#std_u64">u64</a>): &Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../std/vector.md#std_vector_borrow">borrow</a>&lt;Element&gt;(v: &<a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, i: <a href="../std/u64.md#std_u64">u64</a>): &Element;
</code></pre>



</details>

<a name="std_vector_push_back"></a>

## Function `push_back`

Add element <code>e</code> to the end of the vector <code>v</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_push_back">push_back</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, e: Element)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../std/vector.md#std_vector_push_back">push_back</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, e: Element);
</code></pre>



</details>

<a name="std_vector_borrow_mut"></a>

## Function `borrow_mut`

Return a mutable reference to the <code>i</code>th element in the vector <code>v</code>.
Aborts if <code>i</code> is out of bounds.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_borrow_mut">borrow_mut</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, i: <a href="../std/u64.md#std_u64">u64</a>): &<b>mut</b> Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../std/vector.md#std_vector_borrow_mut">borrow_mut</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, i: <a href="../std/u64.md#std_u64">u64</a>): &<b>mut</b> Element;
</code></pre>



</details>

<a name="std_vector_pop_back"></a>

## Function `pop_back`

Pop an element from the end of vector <code>v</code>.
Aborts if <code>v</code> is empty.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_pop_back">pop_back</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../std/vector.md#std_vector_pop_back">pop_back</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;): Element;
</code></pre>



</details>

<a name="std_vector_destroy_empty"></a>

## Function `destroy_empty`

Destroy the vector <code>v</code>.
Aborts if <code>v</code> is not empty.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_destroy_empty">destroy_empty</a>&lt;Element&gt;(v: <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../std/vector.md#std_vector_destroy_empty">destroy_empty</a>&lt;Element&gt;(v: <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;);
</code></pre>



</details>

<a name="std_vector_swap"></a>

## Function `swap`

Swaps the elements at the <code>i</code>th and <code>j</code>th indices in the vector <code>v</code>.
Aborts if <code>i</code> or <code>j</code> is out of bounds.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_swap">swap</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, i: <a href="../std/u64.md#std_u64">u64</a>, j: <a href="../std/u64.md#std_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../std/vector.md#std_vector_swap">swap</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, i: <a href="../std/u64.md#std_u64">u64</a>, j: <a href="../std/u64.md#std_u64">u64</a>);
</code></pre>



</details>

<a name="std_vector_singleton"></a>

## Function `singleton`

Return an vector of size one containing element <code>e</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_singleton">singleton</a>&lt;Element&gt;(e: Element): <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_singleton">singleton</a>&lt;Element&gt;(e: Element): <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt; {
    <b>let</b> <b>mut</b> v = <a href="../std/vector.md#std_vector_empty">empty</a>();
    v.<a href="../std/vector.md#std_vector_push_back">push_back</a>(e);
    v
}
</code></pre>



</details>

<a name="std_vector_reverse"></a>

## Function `reverse`

Reverses the order of the elements in the vector <code>v</code> in place.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_reverse">reverse</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_reverse">reverse</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;) {
    <b>let</b> len = v.<a href="../std/vector.md#std_vector_length">length</a>();
    <b>if</b> (len == 0) <b>return</b> ();
    <b>let</b> <b>mut</b> front_index = 0;
    <b>let</b> <b>mut</b> back_index = len - 1;
    <b>while</b> (front_index &lt; back_index) {
        v.<a href="../std/vector.md#std_vector_swap">swap</a>(front_index, back_index);
        front_index = front_index + 1;
        back_index = back_index - 1;
    }
}
</code></pre>



</details>

<a name="std_vector_append"></a>

## Function `append`

Pushes all of the elements of the <code>other</code> vector into the <code>lhs</code> vector.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_append">append</a>&lt;Element&gt;(lhs: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, other: <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_append">append</a>&lt;Element&gt;(lhs: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, other: <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;) {
    other.<a href="../std/vector.md#std_vector_do">do</a>!(|e| lhs.<a href="../std/vector.md#std_vector_push_back">push_back</a>(e));
}
</code></pre>



</details>

<a name="std_vector_is_empty"></a>

## Function `is_empty`

Return <code><b>true</b></code> if the vector <code>v</code> has no elements and <code><b>false</b></code> otherwise.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_is_empty">is_empty</a>&lt;Element&gt;(v: &<a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_is_empty">is_empty</a>&lt;Element&gt;(v: &<a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;): bool {
    v.<a href="../std/vector.md#std_vector_length">length</a>() == 0
}
</code></pre>



</details>

<a name="std_vector_contains"></a>

## Function `contains`

Return true if <code>e</code> is in the vector <code>v</code>.
Otherwise, returns false.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_contains">contains</a>&lt;Element&gt;(v: &<a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, e: &Element): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_contains">contains</a>&lt;Element&gt;(v: &<a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, e: &Element): bool {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = v.<a href="../std/vector.md#std_vector_length">length</a>();
    <b>while</b> (i &lt; len) {
        <b>if</b> (&v[i] == e) <b>return</b> <b>true</b>;
        i = i + 1;
    };
    <b>false</b>
}
</code></pre>



</details>

<a name="std_vector_index_of"></a>

## Function `index_of`

Return <code>(<b>true</b>, i)</code> if <code>e</code> is in the vector <code>v</code> at index <code>i</code>.
Otherwise, returns <code>(<b>false</b>, 0)</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_index_of">index_of</a>&lt;Element&gt;(v: &<a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, e: &Element): (bool, <a href="../std/u64.md#std_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_index_of">index_of</a>&lt;Element&gt;(v: &<a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, e: &Element): (bool, <a href="../std/u64.md#std_u64">u64</a>) {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = v.<a href="../std/vector.md#std_vector_length">length</a>();
    <b>while</b> (i &lt; len) {
        <b>if</b> (&v[i] == e) <b>return</b> (<b>true</b>, i);
        i = i + 1;
    };
    (<b>false</b>, 0)
}
</code></pre>



</details>

<a name="std_vector_remove"></a>

## Function `remove`

Remove the <code>i</code>th element of the vector <code>v</code>, shifting all subsequent elements.
This is O(n) and preserves ordering of elements in the vector.
Aborts if <code>i</code> is out of bounds.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_remove">remove</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, i: <a href="../std/u64.md#std_u64">u64</a>): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_remove">remove</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, <b>mut</b> i: <a href="../std/u64.md#std_u64">u64</a>): Element {
    <b>let</b> <b>mut</b> len = v.<a href="../std/vector.md#std_vector_length">length</a>();
    // i out of bounds; <b>abort</b>
    <b>if</b> (i &gt;= len) <b>abort</b> <a href="../std/vector.md#std_vector_EINDEX_OUT_OF_BOUNDS">EINDEX_OUT_OF_BOUNDS</a>;
    len = len - 1;
    <b>while</b> (i &lt; len) {
        v.<a href="../std/vector.md#std_vector_swap">swap</a>(i, { i = i + 1; i });
    };
    v.<a href="../std/vector.md#std_vector_pop_back">pop_back</a>()
}
</code></pre>



</details>

<a name="std_vector_insert"></a>

## Function `insert`

Insert <code>e</code> at position <code>i</code> in the vector <code>v</code>.
If <code>i</code> is in bounds, this shifts the old <code>v[i]</code> and all subsequent elements to the right.
If <code>i == v.<a href="../std/vector.md#std_vector_length">length</a>()</code>, this adds <code>e</code> to the end of the vector.
This is O(n) and preserves ordering of elements in the vector.
Aborts if <code>i &gt; v.<a href="../std/vector.md#std_vector_length">length</a>()</code>


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_insert">insert</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, e: Element, i: <a href="../std/u64.md#std_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_insert">insert</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, e: Element, <b>mut</b> i: <a href="../std/u64.md#std_u64">u64</a>) {
    <b>let</b> len = v.<a href="../std/vector.md#std_vector_length">length</a>();
    // i too big <b>abort</b>
    <b>if</b> (i &gt; len) <b>abort</b> <a href="../std/vector.md#std_vector_EINDEX_OUT_OF_BOUNDS">EINDEX_OUT_OF_BOUNDS</a>;
    v.<a href="../std/vector.md#std_vector_push_back">push_back</a>(e);
    <b>while</b> (i &lt; len) {
        v.<a href="../std/vector.md#std_vector_swap">swap</a>(i, len);
        i = i + 1
    }
}
</code></pre>



</details>

<a name="std_vector_swap_remove"></a>

## Function `swap_remove`

Swap the <code>i</code>th element of the vector <code>v</code> with the last element and then pop the vector.
This is O(1), but does not preserve ordering of elements in the vector.
Aborts if <code>i</code> is out of bounds.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_swap_remove">swap_remove</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, i: <a href="../std/u64.md#std_u64">u64</a>): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_swap_remove">swap_remove</a>&lt;Element&gt;(v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;, i: <a href="../std/u64.md#std_u64">u64</a>): Element {
    <b>assert</b>!(v.<a href="../std/vector.md#std_vector_length">length</a>() != 0, <a href="../std/vector.md#std_vector_EINDEX_OUT_OF_BOUNDS">EINDEX_OUT_OF_BOUNDS</a>);
    <b>let</b> last_idx = v.<a href="../std/vector.md#std_vector_length">length</a>() - 1;
    v.<a href="../std/vector.md#std_vector_swap">swap</a>(i, last_idx);
    v.<a href="../std/vector.md#std_vector_pop_back">pop_back</a>()
}
</code></pre>



</details>

<a name="std_vector_tabulate"></a>

## Macro function `tabulate`

Create a vector of length <code>n</code> by calling the function <code>f</code> on each index.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_tabulate">tabulate</a>&lt;$T&gt;($n: <a href="../std/u64.md#std_u64">u64</a>, $f: |<a href="../std/u64.md#std_u64">u64</a>| -&gt; $T): <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_tabulate">tabulate</a>&lt;$T&gt;($n: <a href="../std/u64.md#std_u64">u64</a>, $f: |<a href="../std/u64.md#std_u64">u64</a>| -&gt; $T): <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt; {
    <b>let</b> <b>mut</b> v = <a href="../std/vector.md#std_vector">vector</a>[];
    <b>let</b> n = $n;
    n.<a href="../std/vector.md#std_vector_do">do</a>!(|i| v.<a href="../std/vector.md#std_vector_push_back">push_back</a>($f(i)));
    v
}
</code></pre>



</details>

<a name="std_vector_destroy"></a>

## Macro function `destroy`

Destroy the vector <code>v</code> by calling <code>f</code> on each element and then destroying the vector.
Does not preserve the order of elements in the vector (starts from the end of the vector).


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_destroy">destroy</a>&lt;$T, $R: drop&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |$T| -&gt; $R)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_destroy">destroy</a>&lt;$T, $R: drop&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |$T| -&gt; $R) {
    <b>let</b> <b>mut</b> v = $v;
    v.<a href="../std/vector.md#std_vector_length">length</a>().<a href="../std/vector.md#std_vector_do">do</a>!(|_| $f(v.<a href="../std/vector.md#std_vector_pop_back">pop_back</a>()));
    v.<a href="../std/vector.md#std_vector_destroy_empty">destroy_empty</a>();
}
</code></pre>



</details>

<a name="std_vector_do"></a>

## Macro function `do`

Destroy the vector <code>v</code> by calling <code>f</code> on each element and then destroying the vector.
Preserves the order of elements in the vector.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_do">do</a>&lt;$T, $R: drop&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |$T| -&gt; $R)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_do">do</a>&lt;$T, $R: drop&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |$T| -&gt; $R) {
    <b>let</b> <b>mut</b> v = $v;
    v.<a href="../std/vector.md#std_vector_reverse">reverse</a>();
    v.<a href="../std/vector.md#std_vector_length">length</a>().<a href="../std/vector.md#std_vector_do">do</a>!(|_| $f(v.<a href="../std/vector.md#std_vector_pop_back">pop_back</a>()));
    v.<a href="../std/vector.md#std_vector_destroy_empty">destroy_empty</a>();
}
</code></pre>



</details>

<a name="std_vector_do_ref"></a>

## Macro function `do_ref`

Perform an action <code>f</code> on each element of the vector <code>v</code>. The vector is not modified.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_do_ref">do_ref</a>&lt;$T, $R: drop&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; $R)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_do_ref">do_ref</a>&lt;$T, $R: drop&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; $R) {
    <b>let</b> v = $v;
    v.<a href="../std/vector.md#std_vector_length">length</a>().<a href="../std/vector.md#std_vector_do">do</a>!(|i| $f(&v[i]))
}
</code></pre>



</details>

<a name="std_vector_do_mut"></a>

## Macro function `do_mut`

Perform an action <code>f</code> on each element of the vector <code>v</code>.
The function <code>f</code> takes a mutable reference to the element.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_do_mut">do_mut</a>&lt;$T, $R: drop&gt;($v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&<b>mut</b> $T| -&gt; $R)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_do_mut">do_mut</a>&lt;$T, $R: drop&gt;($v: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&<b>mut</b> $T| -&gt; $R) {
    <b>let</b> v = $v;
    v.<a href="../std/vector.md#std_vector_length">length</a>().<a href="../std/vector.md#std_vector_do">do</a>!(|i| $f(&<b>mut</b> v[i]))
}
</code></pre>



</details>

<a name="std_vector_map"></a>

## Macro function `map`

Map the vector <code>v</code> to a new vector by applying the function <code>f</code> to each element.
Preserves the order of elements in the vector, first is called first.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_map">map</a>&lt;$T, $U&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |$T| -&gt; $U): <a href="../std/vector.md#std_vector">vector</a>&lt;$U&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_map">map</a>&lt;$T, $U&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |$T| -&gt; $U): <a href="../std/vector.md#std_vector">vector</a>&lt;$U&gt; {
    <b>let</b> v = $v;
    <b>let</b> <b>mut</b> r = <a href="../std/vector.md#std_vector">vector</a>[];
    v.<a href="../std/vector.md#std_vector_do">do</a>!(|e| r.<a href="../std/vector.md#std_vector_push_back">push_back</a>($f(e)));
    r
}
</code></pre>



</details>

<a name="std_vector_map_ref"></a>

## Macro function `map_ref`

Map the vector <code>v</code> to a new vector by applying the function <code>f</code> to each element.
Preserves the order of elements in the vector, first is called first.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_map_ref">map_ref</a>&lt;$T, $U&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; $U): <a href="../std/vector.md#std_vector">vector</a>&lt;$U&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_map_ref">map_ref</a>&lt;$T, $U&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; $U): <a href="../std/vector.md#std_vector">vector</a>&lt;$U&gt; {
    <b>let</b> v = $v;
    <b>let</b> <b>mut</b> r = <a href="../std/vector.md#std_vector">vector</a>[];
    v.<a href="../std/vector.md#std_vector_do_ref">do_ref</a>!(|e| r.<a href="../std/vector.md#std_vector_push_back">push_back</a>($f(e)));
    r
}
</code></pre>



</details>

<a name="std_vector_filter"></a>

## Macro function `filter`

Filter the vector <code>v</code> by applying the function <code>f</code> to each element.
Return a new vector containing only the elements for which <code>f</code> returns <code><b>true</b></code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_filter">filter</a>&lt;$T: drop&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_filter">filter</a>&lt;$T: drop&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt; {
    <b>let</b> v = $v;
    <b>let</b> <b>mut</b> r = <a href="../std/vector.md#std_vector">vector</a>[];
    v.<a href="../std/vector.md#std_vector_do">do</a>!(|e| <b>if</b> ($f(&e)) r.<a href="../std/vector.md#std_vector_push_back">push_back</a>(e));
    r
}
</code></pre>



</details>

<a name="std_vector_partition"></a>

## Macro function `partition`

Split the vector <code>v</code> into two vectors by applying the function <code>f</code> to each element.
Return a tuple containing two vectors: the first containing the elements for which <code>f</code> returns <code><b>true</b></code>,
and the second containing the elements for which <code>f</code> returns <code><b>false</b></code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_partition">partition</a>&lt;$T&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): (<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_partition">partition</a>&lt;$T&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): (<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;) {
    <b>let</b> v = $v;
    <b>let</b> <b>mut</b> r1 = <a href="../std/vector.md#std_vector">vector</a>[];
    <b>let</b> <b>mut</b> r2 = <a href="../std/vector.md#std_vector">vector</a>[];
    v.<a href="../std/vector.md#std_vector_do">do</a>!(|e| <b>if</b> ($f(&e)) r1.<a href="../std/vector.md#std_vector_push_back">push_back</a>(e) <b>else</b> r2.<a href="../std/vector.md#std_vector_push_back">push_back</a>(e));
    (r1, r2)
}
</code></pre>



</details>

<a name="std_vector_find_index"></a>

## Macro function `find_index`

Finds the index of first element in the vector <code>v</code> that satisfies the predicate <code>f</code>.
Returns <code>some(index)</code> if such an element is found, otherwise <code>none()</code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_find_index">find_index</a>&lt;$T&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../std/u64.md#std_u64">u64</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_find_index">find_index</a>&lt;$T&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): Option&lt;<a href="../std/u64.md#std_u64">u64</a>&gt; {
    <b>let</b> v = $v;
    '<a href="../std/vector.md#std_vector_find_index">find_index</a>: {
        v.<a href="../std/vector.md#std_vector_length">length</a>().<a href="../std/vector.md#std_vector_do">do</a>!(|i| <b>if</b> ($f(&v[i])) <b>return</b> '<a href="../std/vector.md#std_vector_find_index">find_index</a> <a href="../std/option.md#std_option_some">option::some</a>(i));
        <a href="../std/option.md#std_option_none">option::none</a>()
    }
}
</code></pre>



</details>

<a name="std_vector_count"></a>

## Macro function `count`

Count how many elements in the vector <code>v</code> satisfy the predicate <code>f</code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_count">count</a>&lt;$T&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): <a href="../std/u64.md#std_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_count">count</a>&lt;$T&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): <a href="../std/u64.md#std_u64">u64</a> {
    <b>let</b> v = $v;
    <b>let</b> <b>mut</b> <a href="../std/vector.md#std_vector_count">count</a> = 0;
    v.<a href="../std/vector.md#std_vector_do_ref">do_ref</a>!(|e| <b>if</b> ($f(e)) <a href="../std/vector.md#std_vector_count">count</a> = <a href="../std/vector.md#std_vector_count">count</a> + 1);
    <a href="../std/vector.md#std_vector_count">count</a>
}
</code></pre>



</details>

<a name="std_vector_fold"></a>

## Macro function `fold`

Reduce the vector <code>v</code> to a single value by applying the function <code>f</code> to each element.
Similar to <code>fold_left</code> in Rust and <code>reduce</code> in Python and JavaScript.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_fold">fold</a>&lt;$T, $Acc&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $init: $Acc, $f: |$Acc, $T| -&gt; $Acc): $Acc
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_fold">fold</a>&lt;$T, $Acc&gt;($v: <a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $init: $Acc, $f: |$Acc, $T| -&gt; $Acc): $Acc {
    <b>let</b> v = $v;
    <b>let</b> <b>mut</b> acc = $init;
    v.<a href="../std/vector.md#std_vector_do">do</a>!(|e| acc = $f(acc, e));
    acc
}
</code></pre>



</details>

<a name="std_vector_flatten"></a>

## Function `flatten`

Concatenate the vectors of <code>v</code> into a single vector, keeping the order of the elements.


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_flatten">flatten</a>&lt;T&gt;(v: <a href="../std/vector.md#std_vector">vector</a>&lt;<a href="../std/vector.md#std_vector">vector</a>&lt;T&gt;&gt;): <a href="../std/vector.md#std_vector">vector</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/vector.md#std_vector_flatten">flatten</a>&lt;T&gt;(v: <a href="../std/vector.md#std_vector">vector</a>&lt;<a href="../std/vector.md#std_vector">vector</a>&lt;T&gt;&gt;): <a href="../std/vector.md#std_vector">vector</a>&lt;T&gt; {
    <b>let</b> <b>mut</b> r = <a href="../std/vector.md#std_vector">vector</a>[];
    v.<a href="../std/vector.md#std_vector_do">do</a>!(|u| r.<a href="../std/vector.md#std_vector_append">append</a>(u));
    r
}
</code></pre>



</details>

<a name="std_vector_any"></a>

## Macro function `any`

Whether any element in the vector <code>v</code> satisfies the predicate <code>f</code>.
If the vector is empty, returns <code><b>false</b></code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_any">any</a>&lt;$T&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_any">any</a>&lt;$T&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): bool {
    <b>let</b> v = $v;
    '<a href="../std/vector.md#std_vector_any">any</a>: {
        v.<a href="../std/vector.md#std_vector_do_ref">do_ref</a>!(|e| <b>if</b> ($f(e)) <b>return</b> '<a href="../std/vector.md#std_vector_any">any</a> <b>true</b>);
        <b>false</b>
    }
}
</code></pre>



</details>

<a name="std_vector_all"></a>

## Macro function `all`

Whether all elements in the vector <code>v</code> satisfy the predicate <code>f</code>.
If the vector is empty, returns <code><b>true</b></code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_all">all</a>&lt;$T&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_all">all</a>&lt;$T&gt;($v: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): bool {
    <b>let</b> v = $v;
    '<a href="../std/vector.md#std_vector_all">all</a>: {
        v.<a href="../std/vector.md#std_vector_do_ref">do_ref</a>!(|e| <b>if</b> (!$f(e)) <b>return</b> '<a href="../std/vector.md#std_vector_all">all</a> <b>false</b>);
        <b>true</b>
    }
}
</code></pre>



</details>

<a name="std_vector_zip_do"></a>

## Macro function `zip_do`

Destroys two vectors <code>v1</code> and <code>v2</code> by calling <code>f</code> to each pair of elements.
Aborts if the vectors are not of the same length.
The order of elements in the vectors is preserved.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_do">zip_do</a>&lt;$T1, $T2, $R: drop&gt;($v1: <a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;, $v2: <a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;, $f: |$T1, $T2| -&gt; $R)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_do">zip_do</a>&lt;$T1, $T2, $R: drop&gt;(
    $v1: <a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;,
    $v2: <a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;,
    $f: |$T1, $T2| -&gt; $R,
) {
    <b>let</b> v1 = $v1;
    <b>let</b> <b>mut</b> v2 = $v2;
    v2.<a href="../std/vector.md#std_vector_reverse">reverse</a>();
    <b>let</b> len = v1.<a href="../std/vector.md#std_vector_length">length</a>();
    <b>assert</b>!(len == v2.<a href="../std/vector.md#std_vector_length">length</a>());
    v1.<a href="../std/vector.md#std_vector_do">do</a>!(|el1| $f(el1, v2.<a href="../std/vector.md#std_vector_pop_back">pop_back</a>()));
    v2.<a href="../std/vector.md#std_vector_destroy_empty">destroy_empty</a>();
}
</code></pre>



</details>

<a name="std_vector_zip_do_reverse"></a>

## Macro function `zip_do_reverse`

Destroys two vectors <code>v1</code> and <code>v2</code> by calling <code>f</code> to each pair of elements.
Aborts if the vectors are not of the same length.
Starts from the end of the vectors.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_do_reverse">zip_do_reverse</a>&lt;$T1, $T2, $R: drop&gt;($v1: <a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;, $v2: <a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;, $f: |$T1, $T2| -&gt; $R)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_do_reverse">zip_do_reverse</a>&lt;$T1, $T2, $R: drop&gt;(
    $v1: <a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;,
    $v2: <a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;,
    $f: |$T1, $T2| -&gt; $R,
) {
    <b>let</b> v1 = $v1;
    <b>let</b> <b>mut</b> v2 = $v2;
    <b>let</b> len = v1.<a href="../std/vector.md#std_vector_length">length</a>();
    <b>assert</b>!(len == v2.<a href="../std/vector.md#std_vector_length">length</a>());
    v1.<a href="../std/vector.md#std_vector_destroy">destroy</a>!(|el1| $f(el1, v2.<a href="../std/vector.md#std_vector_pop_back">pop_back</a>()));
}
</code></pre>



</details>

<a name="std_vector_zip_do_ref"></a>

## Macro function `zip_do_ref`

Iterate through <code>v1</code> and <code>v2</code> and apply the function <code>f</code> to references of each pair of
elements. The vectors are not modified.
Aborts if the vectors are not of the same length.
The order of elements in the vectors is preserved.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_do_ref">zip_do_ref</a>&lt;$T1, $T2, $R: drop&gt;($v1: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;, $v2: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;, $f: |&$T1, &$T2| -&gt; $R)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_do_ref">zip_do_ref</a>&lt;$T1, $T2, $R: drop&gt;(
    $v1: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;,
    $v2: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;,
    $f: |&$T1, &$T2| -&gt; $R,
) {
    <b>let</b> v1 = $v1;
    <b>let</b> v2 = $v2;
    <b>let</b> len = v1.<a href="../std/vector.md#std_vector_length">length</a>();
    <b>assert</b>!(len == v2.<a href="../std/vector.md#std_vector_length">length</a>());
    len.<a href="../std/vector.md#std_vector_do">do</a>!(|i| $f(&v1[i], &v2[i]));
}
</code></pre>



</details>

<a name="std_vector_zip_do_mut"></a>

## Macro function `zip_do_mut`

Iterate through <code>v1</code> and <code>v2</code> and apply the function <code>f</code> to mutable references of each pair
of elements. The vectors may be modified.
Aborts if the vectors are not of the same length.
The order of elements in the vectors is preserved.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_do_mut">zip_do_mut</a>&lt;$T1, $T2, $R: drop&gt;($v1: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;, $v2: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;, $f: |&<b>mut</b> $T1, &<b>mut</b> $T2| -&gt; $R)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_do_mut">zip_do_mut</a>&lt;$T1, $T2, $R: drop&gt;(
    $v1: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;,
    $v2: &<b>mut</b> <a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;,
    $f: |&<b>mut</b> $T1, &<b>mut</b> $T2| -&gt; $R,
) {
    <b>let</b> v1 = $v1;
    <b>let</b> v2 = $v2;
    <b>let</b> len = v1.<a href="../std/vector.md#std_vector_length">length</a>();
    <b>assert</b>!(len == v2.<a href="../std/vector.md#std_vector_length">length</a>());
    len.<a href="../std/vector.md#std_vector_do">do</a>!(|i| $f(&<b>mut</b> v1[i], &<b>mut</b> v2[i]));
}
</code></pre>



</details>

<a name="std_vector_zip_map"></a>

## Macro function `zip_map`

Destroys two vectors <code>v1</code> and <code>v2</code> by applying the function <code>f</code> to each pair of elements.
The returned values are collected into a new vector.
Aborts if the vectors are not of the same length.
The order of elements in the vectors is preserved.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_map">zip_map</a>&lt;$T1, $T2, $U&gt;($v1: <a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;, $v2: <a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;, $f: |$T1, $T2| -&gt; $U): <a href="../std/vector.md#std_vector">vector</a>&lt;$U&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_map">zip_map</a>&lt;$T1, $T2, $U&gt;(
    $v1: <a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;,
    $v2: <a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;,
    $f: |$T1, $T2| -&gt; $U,
): <a href="../std/vector.md#std_vector">vector</a>&lt;$U&gt; {
    <b>let</b> <b>mut</b> r = <a href="../std/vector.md#std_vector">vector</a>[];
    <a href="../std/vector.md#std_vector_zip_do">zip_do</a>!($v1, $v2, |el1, el2| r.<a href="../std/vector.md#std_vector_push_back">push_back</a>($f(el1, el2)));
    r
}
</code></pre>



</details>

<a name="std_vector_zip_map_ref"></a>

## Macro function `zip_map_ref`

Iterate through <code>v1</code> and <code>v2</code> and apply the function <code>f</code> to references of each pair of
elements. The returned values are collected into a new vector.
Aborts if the vectors are not of the same length.
The order of elements in the vectors is preserved.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_map_ref">zip_map_ref</a>&lt;$T1, $T2, $U&gt;($v1: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;, $v2: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;, $f: |&$T1, &$T2| -&gt; $U): <a href="../std/vector.md#std_vector">vector</a>&lt;$U&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/vector.md#std_vector_zip_map_ref">zip_map_ref</a>&lt;$T1, $T2, $U&gt;(
    $v1: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T1&gt;,
    $v2: &<a href="../std/vector.md#std_vector">vector</a>&lt;$T2&gt;,
    $f: |&$T1, &$T2| -&gt; $U,
): <a href="../std/vector.md#std_vector">vector</a>&lt;$U&gt; {
    <b>let</b> <b>mut</b> r = <a href="../std/vector.md#std_vector">vector</a>[];
    <a href="../std/vector.md#std_vector_zip_do_ref">zip_do_ref</a>!($v1, $v2, |el1, el2| r.<a href="../std/vector.md#std_vector_push_back">push_back</a>($f(el1, el2)));
    r
}
</code></pre>



</details>
