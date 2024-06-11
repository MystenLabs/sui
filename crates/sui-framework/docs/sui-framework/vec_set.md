---
title: Module `0x2::vec_set`
---



-  [Struct `VecSet`](#0x2_vec_set_VecSet)
-  [Constants](#@Constants_0)
-  [Function `empty`](#0x2_vec_set_empty)
-  [Function `singleton`](#0x2_vec_set_singleton)
-  [Function `insert`](#0x2_vec_set_insert)
-  [Function `remove`](#0x2_vec_set_remove)
-  [Function `contains`](#0x2_vec_set_contains)
-  [Function `size`](#0x2_vec_set_size)
-  [Function `is_empty`](#0x2_vec_set_is_empty)
-  [Function `into_keys`](#0x2_vec_set_into_keys)
-  [Function `from_keys`](#0x2_vec_set_from_keys)
-  [Function `keys`](#0x2_vec_set_keys)
-  [Function `get_idx_opt`](#0x2_vec_set_get_idx_opt)
-  [Function `get_idx`](#0x2_vec_set_get_idx)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="0x2_vec_set_VecSet"></a>

## Struct `VecSet`

A set data structure backed by a vector. The set is guaranteed not to
contain duplicate keys. All operations are O(N) in the size of the set
- the intention of this data structure is only to provide the convenience
of programming against a set API. Sets that need sorted iteration rather
than insertion order iteration should be handwritten.


<pre><code><b>struct</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K: <b>copy</b>, drop&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>contents: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_vec_set_EKeyAlreadyExists"></a>

This key already exists in the map


<pre><code><b>const</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_EKeyAlreadyExists">EKeyAlreadyExists</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_vec_set_EKeyDoesNotExist"></a>

This key does not exist in the map


<pre><code><b>const</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_EKeyDoesNotExist">EKeyDoesNotExist</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x2_vec_set_empty"></a>

## Function `empty`

Create an empty <code><a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_empty">empty</a>&lt;K: <b>copy</b>, drop&gt;(): <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_empty">empty</a>&lt;K: <b>copy</b> + drop&gt;(): <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt; {
    <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a> { contents: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[] }
}
</code></pre>



</details>

<a name="0x2_vec_set_singleton"></a>

## Function `singleton`

Create a singleton <code><a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a></code> that only contains one element.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_singleton">singleton</a>&lt;K: <b>copy</b>, drop&gt;(key: K): <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_singleton">singleton</a>&lt;K: <b>copy</b> + drop&gt;(key: K): <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt; {
    <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a> { contents: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[key] }
}
</code></pre>



</details>

<a name="0x2_vec_set_insert"></a>

## Function `insert`

Insert a <code>key</code> into self.
Aborts if <code>key</code> is already present in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_insert">insert</a>&lt;K: <b>copy</b>, drop&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;, key: K)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_insert">insert</a>&lt;K: <b>copy</b> + drop&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: K) {
    <b>assert</b>!(!self.<a href="../sui-framework/vec_set.md#0x2_vec_set_contains">contains</a>(&key), <a href="../sui-framework/vec_set.md#0x2_vec_set_EKeyAlreadyExists">EKeyAlreadyExists</a>);
    self.contents.push_back(key)
}
</code></pre>



</details>

<a name="0x2_vec_set_remove"></a>

## Function `remove`

Remove the entry <code>key</code> from self. Aborts if <code>key</code> is not present in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_remove">remove</a>&lt;K: <b>copy</b>, drop&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;, key: &K)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_remove">remove</a>&lt;K: <b>copy</b> + drop&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: &K) {
    <b>let</b> idx = <a href="../sui-framework/vec_set.md#0x2_vec_set_get_idx">get_idx</a>(self, key);
    self.contents.<a href="../sui-framework/vec_set.md#0x2_vec_set_remove">remove</a>(idx);
}
</code></pre>



</details>

<a name="0x2_vec_set_contains"></a>

## Function `contains`

Return true if <code>self</code> contains an entry for <code>key</code>, false otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_contains">contains</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;, key: &K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_contains">contains</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: &K): bool {
    <a href="../sui-framework/vec_set.md#0x2_vec_set_get_idx_opt">get_idx_opt</a>(self, key).is_some()
}
</code></pre>



</details>

<a name="0x2_vec_set_size"></a>

## Function `size`

Return the number of entries in <code>self</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_size">size</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_size">size</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.contents.length()
}
</code></pre>



</details>

<a name="0x2_vec_set_is_empty"></a>

## Function `is_empty`

Return true if <code>self</code> has 0 elements, false otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_is_empty">is_empty</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_is_empty">is_empty</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;): bool {
    <a href="../sui-framework/vec_set.md#0x2_vec_set_size">size</a>(self) == 0
}
</code></pre>



</details>

<a name="0x2_vec_set_into_keys"></a>

## Function `into_keys`

Unpack <code>self</code> into vectors of keys.
The output keys are stored in insertion order, *not* sorted.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_into_keys">into_keys</a>&lt;K: <b>copy</b>, drop&gt;(self: <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_into_keys">into_keys</a>&lt;K: <b>copy</b> + drop&gt;(self: <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt; {
    <b>let</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a> { contents } = self;
    contents
}
</code></pre>



</details>

<a name="0x2_vec_set_from_keys"></a>

## Function `from_keys`

Construct a new <code><a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a></code> from a vector of keys.
The keys are stored in insertion order (the original <code>keys</code> ordering)
and are *not* sorted.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_from_keys">from_keys</a>&lt;K: <b>copy</b>, drop&gt;(keys: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;): <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_from_keys">from_keys</a>&lt;K: <b>copy</b> + drop&gt;(<b>mut</b> keys: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;): <a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt; {
    keys.reverse();
    <b>let</b> <b>mut</b> set = <a href="../sui-framework/vec_set.md#0x2_vec_set_empty">empty</a>();
    <b>while</b> (!keys.<a href="../sui-framework/vec_set.md#0x2_vec_set_is_empty">is_empty</a>()) set.<a href="../sui-framework/vec_set.md#0x2_vec_set_insert">insert</a>(keys.pop_back());
    set
}
</code></pre>



</details>

<a name="0x2_vec_set_keys"></a>

## Function `keys`

Borrow the <code>contents</code> of the <code><a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a></code> to access content by index
without unpacking. The contents are stored in insertion order,
*not* sorted.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_keys">keys</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_keys">keys</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt; {
    &self.contents
}
</code></pre>



</details>

<a name="0x2_vec_set_get_idx_opt"></a>

## Function `get_idx_opt`

Find the index of <code>key</code> in <code>self</code>. Return <code>None</code> if <code>key</code> is not in <code>self</code>.
Note that keys are stored in insertion order, *not* sorted.


<pre><code><b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_get_idx_opt">get_idx_opt</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;, key: &K): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_get_idx_opt">get_idx_opt</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: &K): Option&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt; {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> n = <a href="../sui-framework/vec_set.md#0x2_vec_set_size">size</a>(self);
    <b>while</b> (i &lt; n) {
        <b>if</b> (&self.contents[i] == key) {
            <b>return</b> <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(i)
        };
        i = i + 1;
    };
    <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>()
}
</code></pre>



</details>

<a name="0x2_vec_set_get_idx"></a>

## Function `get_idx`

Find the index of <code>key</code> in <code>self</code>. Aborts if <code>key</code> is not in <code>self</code>.
Note that map entries are stored in insertion order, *not* sorted.


<pre><code><b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_get_idx">get_idx</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;, key: &K): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/vec_set.md#0x2_vec_set_get_idx">get_idx</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: &K): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <b>let</b> idx_opt = <a href="../sui-framework/vec_set.md#0x2_vec_set_get_idx_opt">get_idx_opt</a>(self, key);
    <b>assert</b>!(idx_opt.is_some(), <a href="../sui-framework/vec_set.md#0x2_vec_set_EKeyDoesNotExist">EKeyDoesNotExist</a>);
    idx_opt.destroy_some()
}
</code></pre>



</details>
