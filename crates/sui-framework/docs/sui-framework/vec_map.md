---
title: Module `0x2::vec_map`
---



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
-  [Function `from_keys_values`](#0x2_vec_map_from_keys_values)
-  [Function `keys`](#0x2_vec_map_keys)
-  [Function `get_idx_opt`](#0x2_vec_map_get_idx_opt)
-  [Function `get_idx`](#0x2_vec_map_get_idx)
-  [Function `get_entry_by_idx`](#0x2_vec_map_get_entry_by_idx)
-  [Function `get_entry_by_idx_mut`](#0x2_vec_map_get_entry_by_idx_mut)
-  [Function `remove_entry_by_idx`](#0x2_vec_map_remove_entry_by_idx)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="0x2_vec_map_VecMap"></a>

## Struct `VecMap`

A map data structure backed by a vector. The map is guaranteed not to contain duplicate keys, but entries
are *not* sorted by key--entries are included in insertion order.
All operations are O(N) in the size of the map--the intention of this data structure is only to provide
the convenience of programming against a map API.
Large maps should use handwritten parent/child relationships instead.
Maps that need sorted iteration rather than insertion order iteration should also be handwritten.


<pre><code><b>struct</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K: <b>copy</b>, V&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>contents: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../sui-framework/vec_map.md#0x2_vec_map_Entry">vec_map::Entry</a>&lt;K, V&gt;&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_vec_map_Entry"></a>

## Struct `Entry`

An entry in the map


<pre><code><b>struct</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a>&lt;K: <b>copy</b>, V&gt; <b>has</b> <b>copy</b>, drop, store
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

This key already exists in the map


<pre><code><b>const</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_EKeyAlreadyExists">EKeyAlreadyExists</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_vec_map_EKeyDoesNotExist"></a>

This key does not exist in the map


<pre><code><b>const</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_EKeyDoesNotExist">EKeyDoesNotExist</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x2_vec_map_EIndexOutOfBounds"></a>

Trying to access an element of the map at an invalid index


<pre><code><b>const</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 3;
</code></pre>



<a name="0x2_vec_map_EMapEmpty"></a>

Trying to pop from a map that is empty


<pre><code><b>const</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_EMapEmpty">EMapEmpty</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 4;
</code></pre>



<a name="0x2_vec_map_EMapNotEmpty"></a>

Trying to destroy a map that is not empty


<pre><code><b>const</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_EMapNotEmpty">EMapNotEmpty</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0x2_vec_map_EUnequalLengths"></a>

Trying to construct a map from keys and values of different lengths


<pre><code><b>const</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_EUnequalLengths">EUnequalLengths</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 5;
</code></pre>



<a name="0x2_vec_map_empty"></a>

## Function `empty`

Create an empty <code><a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">empty</a>&lt;K: <b>copy</b>, V&gt;(): <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">empty</a>&lt;K: <b>copy</b>, V&gt;(): <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt; {
    <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a> { contents: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[] }
}
</code></pre>



</details>

<a name="0x2_vec_map_insert"></a>

## Function `insert`

Insert the entry <code>key</code> |-> <code>value</code> into <code>self</code>.
Aborts if <code>key</code> is already bound in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_insert">insert</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: K, value: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_insert">insert</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: K, value: V) {
    <b>assert</b>!(!self.<a href="../sui-framework/vec_map.md#0x2_vec_map_contains">contains</a>(&key), <a href="../sui-framework/vec_map.md#0x2_vec_map_EKeyAlreadyExists">EKeyAlreadyExists</a>);
    self.contents.push_back(<a href="../sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a> { key, value })
}
</code></pre>



</details>

<a name="0x2_vec_map_remove"></a>

## Function `remove`

Remove the entry <code>key</code> |-> <code>value</code> from self. Aborts if <code>key</code> is not bound in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_remove">remove</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): (K, V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_remove">remove</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): (K, V) {
    <b>let</b> idx = self.<a href="../sui-framework/vec_map.md#0x2_vec_map_get_idx">get_idx</a>(key);
    <b>let</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a> { key, value } = self.contents.<a href="../sui-framework/vec_map.md#0x2_vec_map_remove">remove</a>(idx);
    (key, value)
}
</code></pre>



</details>

<a name="0x2_vec_map_pop"></a>

## Function `pop`

Pop the most recently inserted entry from the map. Aborts if the map is empty.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_pop">pop</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;): (K, V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_pop">pop</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): (K, V) {
    <b>assert</b>!(!self.contents.<a href="../sui-framework/vec_map.md#0x2_vec_map_is_empty">is_empty</a>(), <a href="../sui-framework/vec_map.md#0x2_vec_map_EMapEmpty">EMapEmpty</a>);
    <b>let</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a> { key, value } = self.contents.pop_back();
    (key, value)
}
</code></pre>



</details>

<a name="0x2_vec_map_get_mut"></a>

## Function `get_mut`

Get a mutable reference to the value bound to <code>key</code> in <code>self</code>.
Aborts if <code>key</code> is not bound in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get_mut">get_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get_mut">get_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): &<b>mut</b> V {
    <b>let</b> idx = self.<a href="../sui-framework/vec_map.md#0x2_vec_map_get_idx">get_idx</a>(key);
    <b>let</b> entry = &<b>mut</b> self.contents[idx];
    &<b>mut</b> entry.value
}
</code></pre>



</details>

<a name="0x2_vec_map_get"></a>

## Function `get`

Get a reference to the value bound to <code>key</code> in <code>self</code>.
Aborts if <code>key</code> is not bound in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get">get</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get">get</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): &V {
    <b>let</b> idx = self.<a href="../sui-framework/vec_map.md#0x2_vec_map_get_idx">get_idx</a>(key);
    <b>let</b> entry = &self.contents[idx];
    &entry.value
}
</code></pre>



</details>

<a name="0x2_vec_map_try_get"></a>

## Function `try_get`

Safely try borrow a value bound to <code>key</code> in <code>self</code>.
Return Some(V) if the value exists, None otherwise.
Only works for a "copyable" value as references cannot be stored in <code><a href="../move-stdlib/vector.md#0x1_vector">vector</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_try_get">try_get</a>&lt;K: <b>copy</b>, V: <b>copy</b>&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_try_get">try_get</a>&lt;K: <b>copy</b>, V: <b>copy</b>&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): Option&lt;V&gt; {
    <b>if</b> (self.<a href="../sui-framework/vec_map.md#0x2_vec_map_contains">contains</a>(key)) {
        <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(*<a href="../sui-framework/vec_map.md#0x2_vec_map_get">get</a>(self, key))
    } <b>else</b> {
        <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>()
    }
}
</code></pre>



</details>

<a name="0x2_vec_map_contains"></a>

## Function `contains`

Return true if <code>self</code> contains an entry for <code>key</code>, false otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_contains">contains</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_contains">contains</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): bool {
    <a href="../sui-framework/vec_map.md#0x2_vec_map_get_idx_opt">get_idx_opt</a>(self, key).is_some()
}
</code></pre>



</details>

<a name="0x2_vec_map_size"></a>

## Function `size`

Return the number of entries in <code>self</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_size">size</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_size">size</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.contents.length()
}
</code></pre>



</details>

<a name="0x2_vec_map_is_empty"></a>

## Function `is_empty`

Return true if <code>self</code> has 0 elements, false otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_is_empty">is_empty</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_is_empty">is_empty</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): bool {
    self.<a href="../sui-framework/vec_map.md#0x2_vec_map_size">size</a>() == 0
}
</code></pre>



</details>

<a name="0x2_vec_map_destroy_empty"></a>

## Function `destroy_empty`

Destroy an empty map. Aborts if <code>self</code> is not empty


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;) {
    <b>let</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a> { contents } = self;
    <b>assert</b>!(contents.<a href="../sui-framework/vec_map.md#0x2_vec_map_is_empty">is_empty</a>(), <a href="../sui-framework/vec_map.md#0x2_vec_map_EMapNotEmpty">EMapNotEmpty</a>);
    contents.<a href="../sui-framework/vec_map.md#0x2_vec_map_destroy_empty">destroy_empty</a>()
}
</code></pre>



</details>

<a name="0x2_vec_map_into_keys_values"></a>

## Function `into_keys_values`

Unpack <code>self</code> into vectors of its keys and values.
The output keys and values are stored in insertion order, *not* sorted by key.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_into_keys_values">into_keys_values</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;): (<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;, <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_into_keys_values">into_keys_values</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): (<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;, <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;V&gt;) {
    <b>let</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a> { <b>mut</b> contents } = self;
    // reverse the <a href="../move-stdlib/vector.md#0x1_vector">vector</a> so the output keys and values will appear in insertion order
    contents.reverse();
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> n = contents.length();
    <b>let</b> <b>mut</b> keys = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>let</b> <b>mut</b> values = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>while</b> (i &lt; n) {
        <b>let</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a> { key, value } = contents.pop_back();
        keys.push_back(key);
        values.push_back(value);
        i = i + 1;
    };
    contents.<a href="../sui-framework/vec_map.md#0x2_vec_map_destroy_empty">destroy_empty</a>();
    (keys, values)
}
</code></pre>



</details>

<a name="0x2_vec_map_from_keys_values"></a>

## Function `from_keys_values`

Construct a new <code><a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a></code> from two vectors, one for keys and one for values.
The key value pairs are associated via their indices in the vectors, e.g. the key at index i
in <code>keys</code> is associated with the value at index i in <code>values</code>.
The key value pairs are stored in insertion order (the original vectors ordering)
and are *not* sorted.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_from_keys_values">from_keys_values</a>&lt;K: <b>copy</b>, V&gt;(keys: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;, values: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;V&gt;): <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_from_keys_values">from_keys_values</a>&lt;K: <b>copy</b>, V&gt;(<b>mut</b> keys: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;, <b>mut</b> values: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;V&gt;): <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt; {
    <b>assert</b>!(keys.length() == values.length(), <a href="../sui-framework/vec_map.md#0x2_vec_map_EUnequalLengths">EUnequalLengths</a>);
    keys.reverse();
    values.reverse();
    <b>let</b> <b>mut</b> map = <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">empty</a>();
    <b>while</b> (!keys.<a href="../sui-framework/vec_map.md#0x2_vec_map_is_empty">is_empty</a>()) map.<a href="../sui-framework/vec_map.md#0x2_vec_map_insert">insert</a>(keys.pop_back(), values.pop_back());
    keys.<a href="../sui-framework/vec_map.md#0x2_vec_map_destroy_empty">destroy_empty</a>();
    values.<a href="../sui-framework/vec_map.md#0x2_vec_map_destroy_empty">destroy_empty</a>();
    map
}
</code></pre>



</details>

<a name="0x2_vec_map_keys"></a>

## Function `keys`

Returns a list of keys in the map.
Do not assume any particular ordering.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_keys">keys</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_keys">keys</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;K&gt; {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> n = self.contents.length();
    <b>let</b> <b>mut</b> keys = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>while</b> (i &lt; n) {
        <b>let</b> entry = self.contents.borrow(i);
        keys.push_back(entry.key);
        i = i + 1;
    };
    keys
}
</code></pre>



</details>

<a name="0x2_vec_map_get_idx_opt"></a>

## Function `get_idx_opt`

Find the index of <code>key</code> in <code>self</code>. Return <code>None</code> if <code>key</code> is not in <code>self</code>.
Note that map entries are stored in insertion order, *not* sorted by key.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get_idx_opt">get_idx_opt</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get_idx_opt">get_idx_opt</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): Option&lt;<a href="../move-stdlib/u64.md#0x1_u64">u64</a>&gt; {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> n = <a href="../sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self);
    <b>while</b> (i &lt; n) {
        <b>if</b> (&self.contents[i].key == key) {
            <b>return</b> <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(i)
        };
        i = i + 1;
    };
    <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>()
}
</code></pre>



</details>

<a name="0x2_vec_map_get_idx"></a>

## Function `get_idx`

Find the index of <code>key</code> in <code>self</code>. Aborts if <code>key</code> is not in <code>self</code>.
Note that map entries are stored in insertion order, *not* sorted by key.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get_idx">get_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, key: &K): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get_idx">get_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <b>let</b> idx_opt = self.<a href="../sui-framework/vec_map.md#0x2_vec_map_get_idx_opt">get_idx_opt</a>(key);
    <b>assert</b>!(idx_opt.is_some(), <a href="../sui-framework/vec_map.md#0x2_vec_map_EKeyDoesNotExist">EKeyDoesNotExist</a>);
    idx_opt.destroy_some()
}
</code></pre>



</details>

<a name="0x2_vec_map_get_entry_by_idx"></a>

## Function `get_entry_by_idx`

Return a reference to the <code>idx</code>th entry of <code>self</code>. This gives direct access into the backing array of the map--use with caution.
Note that map entries are stored in insertion order, *not* sorted by key.
Aborts if <code>idx</code> is greater than or equal to <code><a href="../sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self)</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get_entry_by_idx">get_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, idx: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): (&K, &V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get_entry_by_idx">get_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, idx: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): (&K, &V) {
    <b>assert</b>!(idx &lt; <a href="../sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self), <a href="../sui-framework/vec_map.md#0x2_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>);
    <b>let</b> entry = &self.contents[idx];
    (&entry.key, &entry.value)
}
</code></pre>



</details>

<a name="0x2_vec_map_get_entry_by_idx_mut"></a>

## Function `get_entry_by_idx_mut`

Return a mutable reference to the <code>idx</code>th entry of <code>self</code>. This gives direct access into the backing array of the map--use with caution.
Note that map entries are stored in insertion order, *not* sorted by key.
Aborts if <code>idx</code> is greater than or equal to <code><a href="../sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self)</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get_entry_by_idx_mut">get_entry_by_idx_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, idx: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): (&K, &<b>mut</b> V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_get_entry_by_idx_mut">get_entry_by_idx_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, idx: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): (&K, &<b>mut</b> V) {
    <b>assert</b>!(idx &lt; <a href="../sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self), <a href="../sui-framework/vec_map.md#0x2_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>);
    <b>let</b> entry = &<b>mut</b> self.contents[idx];
    (&entry.key, &<b>mut</b> entry.value)
}
</code></pre>



</details>

<a name="0x2_vec_map_remove_entry_by_idx"></a>

## Function `remove_entry_by_idx`

Remove the entry at index <code>idx</code> from self.
Aborts if <code>idx</code> is greater than or equal to <code><a href="../sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self)</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_remove_entry_by_idx">remove_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;K, V&gt;, idx: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): (K, V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_remove_entry_by_idx">remove_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, idx: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): (K, V) {
    <b>assert</b>!(idx &lt; <a href="../sui-framework/vec_map.md#0x2_vec_map_size">size</a>(self), <a href="../sui-framework/vec_map.md#0x2_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>);
    <b>let</b> <a href="../sui-framework/vec_map.md#0x2_vec_map_Entry">Entry</a> { key, value } = self.contents.<a href="../sui-framework/vec_map.md#0x2_vec_map_remove">remove</a>(idx);
    (key, value)
}
</code></pre>



</details>
