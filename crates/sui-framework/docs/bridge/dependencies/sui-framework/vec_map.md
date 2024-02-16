
<a name="0x2_vec_map"></a>

# Module `0x2::vec_map`



-  [Struct `VecMap`](#0x2_vec_map_VecMap)
-  [Struct `Entry`](#0x2_vec_map_Entry)
-  [Constants](#@Constants_0)
-  [Function `empty`](#0x2_vec_map_empty)
-  [Function `insert`](#0x2_vec_map_insert)
-  [Function `remove`](#0x2_vec_map_remove)
-  [Function `pop`](#0x2_vec_map_pop)
-  [Function `get_mut`](#0x2_vec_map_get_mut)
-  [Function `get`](#0x2_vec_map_get)
-  [Function `try_get`](#0x2_vec_map_try_get)
-  [Function `contains`](#0x2_vec_map_contains)
-  [Function `size`](#0x2_vec_map_size)
-  [Function `is_empty`](#0x2_vec_map_is_empty)
-  [Function `destroy_empty`](#0x2_vec_map_destroy_empty)
-  [Function `into_keys_values`](#0x2_vec_map_into_keys_values)
-  [Function `keys`](#0x2_vec_map_keys)
-  [Function `get_idx_opt`](#0x2_vec_map_get_idx_opt)
-  [Function `get_idx`](#0x2_vec_map_get_idx)
-  [Function `get_entry_by_idx`](#0x2_vec_map_get_entry_by_idx)
-  [Function `get_entry_by_idx_mut`](#0x2_vec_map_get_entry_by_idx_mut)
-  [Function `remove_entry_by_idx`](#0x2_vec_map_remove_entry_by_idx)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="0x2_vec_map_VecMap"></a>

## Struct `VecMap`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K: <b>copy</b>, V&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>contents: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_Entry">vec_map::Entry</a>&lt;K, V&gt;&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_vec_map_Entry"></a>

## Struct `Entry`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a>&lt;K: <b>copy</b>, V&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>key: K</code>
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


<a name="0x2_vec_map_EKeyAlreadyExists"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EKeyAlreadyExists">EKeyAlreadyExists</a>: u64 = 0;
</code></pre>



<a name="0x2_vec_map_EKeyDoesNotExist"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EKeyDoesNotExist">EKeyDoesNotExist</a>: u64 = 1;
</code></pre>



<a name="0x2_vec_map_EIndexOutOfBounds"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>: u64 = 3;
</code></pre>



<a name="0x2_vec_map_EMapEmpty"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EMapEmpty">EMapEmpty</a>: u64 = 4;
</code></pre>



<a name="0x2_vec_map_EMapNotEmpty"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EMapNotEmpty">EMapNotEmpty</a>: u64 = 2;
</code></pre>



<a name="0x2_vec_map_empty"></a>

## Function `empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_empty">empty</a>&lt;K: <b>copy</b>, V&gt;(): <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_empty">empty</a>&lt;K: <b>copy</b>, V&gt;(): <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K,V&gt; {
    <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a> { contents: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_empty">vector::empty</a>() }
}
</code></pre>



</details>

<a name="0x2_vec_map_insert"></a>

## Function `insert`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">insert</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: K, value: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_insert">insert</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K,V&gt;, key: K, value: V) {
    <b>assert</b>!(!<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_contains">contains</a>(self, &key), <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EKeyAlreadyExists">EKeyAlreadyExists</a>);
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> self.contents, <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a> { key, value })
}
</code></pre>



</details>

<a name="0x2_vec_map_remove"></a>

## Function `remove`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_remove">remove</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): (K, V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_remove">remove</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K,V&gt;, key: &K): (K, V) {
    <b>let</b> idx = <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_idx">get_idx</a>(self, key);
    <b>let</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a> { key, value } = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_remove">vector::remove</a>(&<b>mut</b> self.contents, idx);
    (key, value)
}
</code></pre>



</details>

<a name="0x2_vec_map_pop"></a>

## Function `pop`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_pop">pop</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;): (K, V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_pop">pop</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K,V&gt;): (K, V) {
    <b>assert</b>!(!<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&self.contents), <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EMapEmpty">EMapEmpty</a>);
    <b>let</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a> { key, value } = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> self.contents);
    (key, value)
}
</code></pre>



</details>

<a name="0x2_vec_map_get_mut"></a>

## Function `get_mut`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_mut">get_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_mut">get_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K,V&gt;, key: &K): &<b>mut</b> V {
    <b>let</b> idx = <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_idx">get_idx</a>(self, key);
    <b>let</b> entry = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow_mut">vector::borrow_mut</a>(&<b>mut</b> self.contents, idx);
    &<b>mut</b> entry.value
}
</code></pre>



</details>

<a name="0x2_vec_map_get"></a>

## Function `get`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get">get</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get">get</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K,V&gt;, key: &K): &V {
    <b>let</b> idx = <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_idx">get_idx</a>(self, key);
    <b>let</b> entry = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&self.contents, idx);
    &entry.value
}
</code></pre>



</details>

<a name="0x2_vec_map_try_get"></a>

## Function `try_get`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_try_get">try_get</a>&lt;K: <b>copy</b>, V: <b>copy</b>&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_try_get">try_get</a>&lt;K: <b>copy</b>, V: <b>copy</b>&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K,V&gt;, key: &K): Option&lt;V&gt; {
    <b>if</b> (<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_contains">contains</a>(self, key)) {
        <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(*<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get">get</a>(self, key))
    } <b>else</b> {
        <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>()
    }
}
</code></pre>



</details>

<a name="0x2_vec_map_contains"></a>

## Function `contains`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_contains">contains</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_contains">contains</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): bool {
    <a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_idx_opt">get_idx_opt</a>(self, key))
}
</code></pre>



</details>

<a name="0x2_vec_map_size"></a>

## Function `size`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_size">size</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_size">size</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K,V&gt;): u64 {
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&self.contents)
}
</code></pre>



</details>

<a name="0x2_vec_map_is_empty"></a>

## Function `is_empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_is_empty">is_empty</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_is_empty">is_empty</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K,V&gt;): bool {
    <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self) == 0
}
</code></pre>



</details>

<a name="0x2_vec_map_destroy_empty"></a>

## Function `destroy_empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;) {
    <b>let</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a> { contents } = self;
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&contents), <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EMapNotEmpty">EMapNotEmpty</a>);
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_destroy_empty">vector::destroy_empty</a>(contents)
}
</code></pre>



</details>

<a name="0x2_vec_map_into_keys_values"></a>

## Function `into_keys_values`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_into_keys_values">into_keys_values</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;): (<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;, <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_into_keys_values">into_keys_values</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): (<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;, <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;V&gt;) {
    <b>let</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a> { contents } = self;
    // reverse the <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a> so the output keys and values will appear in insertion order
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_reverse">vector::reverse</a>(&<b>mut</b> contents);
    <b>let</b> i = 0;
    <b>let</b> n = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&contents);
    <b>let</b> keys = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_empty">vector::empty</a>();
    <b>let</b> values = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_empty">vector::empty</a>();
    <b>while</b> (i &lt; n) {
        <b>let</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a> { key, value } = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> contents);
        <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> keys, key);
        <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> values, value);
        i = i + 1;
    };
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_destroy_empty">vector::destroy_empty</a>(contents);
    (keys, values)
}
</code></pre>



</details>

<a name="0x2_vec_map_keys"></a>

## Function `keys`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_keys">keys</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_keys">keys</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt; {
    <b>let</b> i = 0;
    <b>let</b> n = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&self.contents);
    <b>let</b> keys = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_empty">vector::empty</a>();
    <b>while</b> (i &lt; n) {
        <b>let</b> entry = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&self.contents, i);
        <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> keys, entry.key);
        i = i + 1;
    };
    keys
}
</code></pre>



</details>

<a name="0x2_vec_map_get_idx_opt"></a>

## Function `get_idx_opt`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_idx_opt">get_idx_opt</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_idx_opt">get_idx_opt</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K,V&gt;, key: &K): Option&lt;u64&gt; {
    <b>let</b> i = 0;
    <b>let</b> n = <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self);
    <b>while</b> (i &lt; n) {
        <b>if</b> (&<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&self.contents, i).key == key) {
            <b>return</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(i)
        };
        i = i + 1;
    };
    <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>()
}
</code></pre>



</details>

<a name="0x2_vec_map_get_idx"></a>

## Function `get_idx`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_idx">get_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_idx">get_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K,V&gt;, key: &K): u64 {
    <b>let</b> idx_opt = <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_idx_opt">get_idx_opt</a>(self, key);
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&idx_opt), <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EKeyDoesNotExist">EKeyDoesNotExist</a>);
    <a href="../../dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(idx_opt)
}
</code></pre>



</details>

<a name="0x2_vec_map_get_entry_by_idx"></a>

## Function `get_entry_by_idx`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_entry_by_idx">get_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, idx: u64): (&K, &V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_entry_by_idx">get_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, idx: u64): (&K, &V) {
    <b>assert</b>!(idx &lt; <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self), <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>);
    <b>let</b> entry = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&self.contents, idx);
    (&entry.key, &entry.value)
}
</code></pre>



</details>

<a name="0x2_vec_map_get_entry_by_idx_mut"></a>

## Function `get_entry_by_idx_mut`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_entry_by_idx_mut">get_entry_by_idx_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, idx: u64): (&K, &<b>mut</b> V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_get_entry_by_idx_mut">get_entry_by_idx_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, idx: u64): (&K, &<b>mut</b> V) {
    <b>assert</b>!(idx &lt; <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self), <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>);
    <b>let</b> entry = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow_mut">vector::borrow_mut</a>(&<b>mut</b> self.contents, idx);
    (&entry.key, &<b>mut</b> entry.value)
}
</code></pre>



</details>

<a name="0x2_vec_map_remove_entry_by_idx"></a>

## Function `remove_entry_by_idx`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_remove_entry_by_idx">remove_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, idx: u64): (K, V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_remove_entry_by_idx">remove_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, idx: u64): (K, V) {
    <b>assert</b>!(idx &lt; <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self), <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>);
    <b>let</b> <a href="../../dependencies/sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a> { key, value } = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_remove">vector::remove</a>(&<b>mut</b> self.contents, idx);
    (key, value)
}
</code></pre>



</details>
