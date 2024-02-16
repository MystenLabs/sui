
<a name="0x2_vec_set"></a>

# Module `0x2::vec_set`



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
-  [Function `keys`](#0x2_vec_set_keys)
-  [Function `get_idx_opt`](#0x2_vec_set_get_idx_opt)
-  [Function `get_idx`](#0x2_vec_set_get_idx)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="0x2_vec_set_VecSet"></a>

## Struct `VecSet`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K: <b>copy</b>, drop&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>contents: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_vec_set_EKeyAlreadyExists"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_EKeyAlreadyExists">EKeyAlreadyExists</a>: u64 = 0;
</code></pre>



<a name="0x2_vec_set_EKeyDoesNotExist"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_EKeyDoesNotExist">EKeyDoesNotExist</a>: u64 = 1;
</code></pre>



<a name="0x2_vec_set_empty"></a>

## Function `empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_empty">empty</a>&lt;K: <b>copy</b>, drop&gt;(): <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_empty">empty</a>&lt;K: <b>copy</b> + drop&gt;(): <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt; {
    <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a> { contents: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_empty">vector::empty</a>() }
}
</code></pre>



</details>

<a name="0x2_vec_set_singleton"></a>

## Function `singleton`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_singleton">singleton</a>&lt;K: <b>copy</b>, drop&gt;(key: K): <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_singleton">singleton</a>&lt;K: <b>copy</b> + drop&gt;(key: K): <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt; {
    <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a> { contents: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_singleton">vector::singleton</a>(key) }
}
</code></pre>



</details>

<a name="0x2_vec_set_insert"></a>

## Function `insert`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_insert">insert</a>&lt;K: <b>copy</b>, drop&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;, key: K)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_insert">insert</a>&lt;K: <b>copy</b> + drop&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: K) {
    <b>assert</b>!(!<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_contains">contains</a>(self, &key), <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_EKeyAlreadyExists">EKeyAlreadyExists</a>);
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> self.contents, key)
}
</code></pre>



</details>

<a name="0x2_vec_set_remove"></a>

## Function `remove`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_remove">remove</a>&lt;K: <b>copy</b>, drop&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;, key: &K)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_remove">remove</a>&lt;K: <b>copy</b> + drop&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: &K) {
    <b>let</b> idx = <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_get_idx">get_idx</a>(self, key);
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_remove">vector::remove</a>(&<b>mut</b> self.contents, idx);
}
</code></pre>



</details>

<a name="0x2_vec_set_contains"></a>

## Function `contains`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_contains">contains</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;, key: &K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_contains">contains</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: &K): bool {
    <a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_get_idx_opt">get_idx_opt</a>(self, key))
}
</code></pre>



</details>

<a name="0x2_vec_set_size"></a>

## Function `size`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_size">size</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_size">size</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;): u64 {
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&self.contents)
}
</code></pre>



</details>

<a name="0x2_vec_set_is_empty"></a>

## Function `is_empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_is_empty">is_empty</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_is_empty">is_empty</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;): bool {
    <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_size">size</a>(self) == 0
}
</code></pre>



</details>

<a name="0x2_vec_set_into_keys"></a>

## Function `into_keys`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_into_keys">into_keys</a>&lt;K: <b>copy</b>, drop&gt;(self: <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_into_keys">into_keys</a>&lt;K: <b>copy</b> + drop&gt;(self: <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt; {
    <b>let</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a> { contents } = self;
    contents
}
</code></pre>



</details>

<a name="0x2_vec_set_keys"></a>

## Function `keys`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_keys">keys</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;): &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_keys">keys</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;): &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt; {
    &self.contents
}
</code></pre>



</details>

<a name="0x2_vec_set_get_idx_opt"></a>

## Function `get_idx_opt`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_get_idx_opt">get_idx_opt</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;, key: &K): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_get_idx_opt">get_idx_opt</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: &K): Option&lt;u64&gt; {
    <b>let</b> i = 0;
    <b>let</b> n = <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_size">size</a>(self);
    <b>while</b> (i &lt; n) {
        <b>if</b> (<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&self.contents, i) == key) {
            <b>return</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(i)
        };
        i = i + 1;
    };
    <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>()
}
</code></pre>



</details>

<a name="0x2_vec_set_get_idx"></a>

## Function `get_idx`



<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_get_idx">get_idx</a>&lt;K: <b>copy</b>, drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;K&gt;, key: &K): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_get_idx">get_idx</a>&lt;K: <b>copy</b> + drop&gt;(self: &<a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_VecSet">VecSet</a>&lt;K&gt;, key: &K): u64 {
    <b>let</b> idx_opt = <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_get_idx_opt">get_idx_opt</a>(self, key);
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&idx_opt), <a href="../../dependencies/sui-framework/vec_set.md#0x2_vec_set_EKeyDoesNotExist">EKeyDoesNotExist</a>);
    <a href="../../dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(idx_opt)
}
</code></pre>



</details>
