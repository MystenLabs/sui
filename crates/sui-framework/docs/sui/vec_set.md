---
title: Module `sui::vec_set`
---



-  [Struct `VecSet`](#sui_vec_set_VecSet)
-  [Constants](#@Constants_0)
-  [Function `empty`](#sui_vec_set_empty)
-  [Function `singleton`](#sui_vec_set_singleton)
-  [Function `insert`](#sui_vec_set_insert)
-  [Function `remove`](#sui_vec_set_remove)
-  [Function `contains`](#sui_vec_set_contains)
-  [Function `length`](#sui_vec_set_length)
-  [Function `is_empty`](#sui_vec_set_is_empty)
-  [Function `into_keys`](#sui_vec_set_into_keys)
-  [Function `from_keys`](#sui_vec_set_from_keys)
-  [Function `keys`](#sui_vec_set_keys)
-  [Function `size`](#sui_vec_set_size)


<pre><code><b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
</code></pre>



<a name="sui_vec_set_VecSet"></a>

## Struct `VecSet`

A set data structure backed by a vector. The set is guaranteed not to
contain duplicate keys. All operations are O(N) in the size of the set
- the intention of this data structure is only to provide the convenience
of programming against a set API. Sets that need sorted iteration rather
than insertion order iteration should be handwritten.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K: <b>copy</b>, drop&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>contents: vector&lt;K&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_vec_set_EKeyAlreadyExists"></a>

This key already exists in the map


<pre><code><b>const</b> <a href="../sui/vec_set.md#sui_vec_set_EKeyAlreadyExists">EKeyAlreadyExists</a>: u64 = 0;
</code></pre>



<a name="sui_vec_set_EKeyDoesNotExist"></a>

This key does not exist in the map


<pre><code><b>const</b> <a href="../sui/vec_set.md#sui_vec_set_EKeyDoesNotExist">EKeyDoesNotExist</a>: u64 = 1;
</code></pre>



<a name="sui_vec_set_empty"></a>

## Function `empty`

Create an empty <code><a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_empty">empty</a>&lt;K: <b>copy</b>, drop&gt;(): <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_empty">empty</a>&lt;K: <b>copy</b> + drop&gt;(): <a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K&gt; {
    <a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a> { contents: vector[] }
}
</code></pre>



</details>

<a name="sui_vec_set_singleton"></a>

## Function `singleton`

Create a singleton <code><a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a></code> that only contains one element.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_singleton">singleton</a>&lt;K: <b>copy</b>, drop&gt;(key: K): <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_singleton">singleton</a>&lt;K: <b>copy</b> + drop&gt;(key: K): <a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K&gt; {
    <a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a> { contents: vector[key] }
}
</code></pre>



</details>

<a name="sui_vec_set_insert"></a>

## Function `insert`

Insert a <code>key</code> into self.
Aborts if <code>key</code> is already present in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_insert">insert</a>&lt;K: <b>copy</b>, drop&gt;(self: &<b>mut</b> <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;K&gt;, key: K)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_insert">insert</a>&lt;K: <b>copy</b> + drop&gt;(self: &<b>mut</b> <a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: K) {
    <b>assert</b>!(!self.<a href="../sui/vec_set.md#sui_vec_set_contains">contains</a>(&key), <a href="../sui/vec_set.md#sui_vec_set_EKeyAlreadyExists">EKeyAlreadyExists</a>);
    self.contents.push_back(key)
}
</code></pre>



</details>

<a name="sui_vec_set_remove"></a>

## Function `remove`

Remove the entry <code>key</code> from self. Aborts if <code>key</code> is not present in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_remove">remove</a>&lt;K: <b>copy</b>, drop&gt;(self: &<b>mut</b> <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;K&gt;, key: &K)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_remove">remove</a>&lt;K: <b>copy</b> + drop&gt;(self: &<b>mut</b> <a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: &K) {
    <b>let</b> idx = self.contents.find_index!(|k| k == key).destroy_or!(<b>abort</b> <a href="../sui/vec_set.md#sui_vec_set_EKeyDoesNotExist">EKeyDoesNotExist</a>);
    self.contents.<a href="../sui/vec_set.md#sui_vec_set_remove">remove</a>(idx);
}
</code></pre>



</details>

<a name="sui_vec_set_contains"></a>

## Function `contains`

Return true if <code>self</code> contains an entry for <code>key</code>, false otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_contains">contains</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;K&gt;, key: &K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_contains">contains</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: &K): bool {
    'search: {
        self.contents.do_ref!(|k| <b>if</b> (k == key) <b>return</b> 'search <b>true</b>);
        <b>false</b>
    }
}
</code></pre>



</details>

<a name="sui_vec_set_length"></a>

## Function `length`

Return the number of entries in <code>self</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_length">length</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;K&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_length">length</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K&gt;): u64 {
    self.contents.<a href="../sui/vec_set.md#sui_vec_set_length">length</a>()
}
</code></pre>



</details>

<a name="sui_vec_set_is_empty"></a>

## Function `is_empty`

Return true if <code>self</code> has 0 elements, false otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_is_empty">is_empty</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;K&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_is_empty">is_empty</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K&gt;): bool {
    self.<a href="../sui/vec_set.md#sui_vec_set_length">length</a>() == 0
}
</code></pre>



</details>

<a name="sui_vec_set_into_keys"></a>

## Function `into_keys`

Unpack <code>self</code> into vectors of keys.
The output keys are stored in insertion order, *not* sorted.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_into_keys">into_keys</a>&lt;K: <b>copy</b>, drop&gt;(self: <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;K&gt;): vector&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_into_keys">into_keys</a>&lt;K: <b>copy</b> + drop&gt;(self: <a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K&gt;): vector&lt;K&gt; {
    <b>let</b> <a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a> { contents } = self;
    contents
}
</code></pre>



</details>

<a name="sui_vec_set_from_keys"></a>

## Function `from_keys`

Construct a new <code><a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a></code> from a vector of keys.
The keys are stored in insertion order (the original <code><a href="../sui/vec_set.md#sui_vec_set_keys">keys</a></code> ordering)
and are *not* sorted.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_from_keys">from_keys</a>&lt;K: <b>copy</b>, drop&gt;(<a href="../sui/vec_set.md#sui_vec_set_keys">keys</a>: vector&lt;K&gt;): <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_from_keys">from_keys</a>&lt;K: <b>copy</b> + drop&gt;(<b>mut</b> <a href="../sui/vec_set.md#sui_vec_set_keys">keys</a>: vector&lt;K&gt;): <a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K&gt; {
    <a href="../sui/vec_set.md#sui_vec_set_keys">keys</a>.reverse();
    <b>let</b> <b>mut</b> set = <a href="../sui/vec_set.md#sui_vec_set_empty">empty</a>();
    <b>while</b> (<a href="../sui/vec_set.md#sui_vec_set_keys">keys</a>.<a href="../sui/vec_set.md#sui_vec_set_length">length</a>() != 0) set.<a href="../sui/vec_set.md#sui_vec_set_insert">insert</a>(<a href="../sui/vec_set.md#sui_vec_set_keys">keys</a>.pop_back());
    set
}
</code></pre>



</details>

<a name="sui_vec_set_keys"></a>

## Function `keys`

Borrow the <code>contents</code> of the <code><a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a></code> to access content by index
without unpacking. The contents are stored in insertion order,
*not* sorted.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_keys">keys</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;K&gt;): &vector&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_keys">keys</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K&gt;): &vector&lt;K&gt; {
    &self.contents
}
</code></pre>



</details>

<a name="sui_vec_set_size"></a>

## Function `size`

Return the number of entries in <code>self</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_size">size</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;K&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_set.md#sui_vec_set_size">size</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../sui/vec_set.md#sui_vec_set_VecSet">VecSet</a>&lt;K&gt;): u64 {
    self.contents.<a href="../sui/vec_set.md#sui_vec_set_length">length</a>()
}
</code></pre>



</details>
