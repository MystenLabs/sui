---
title: Module `sui::vec_map`
---



-  [Struct `VecMap`](#sui_vec_map_VecMap)
-  [Struct `Entry`](#sui_vec_map_Entry)
-  [Constants](#@Constants_0)
-  [Function `empty`](#sui_vec_map_empty)
-  [Function `insert`](#sui_vec_map_insert)
-  [Function `remove`](#sui_vec_map_remove)
-  [Function `pop`](#sui_vec_map_pop)
-  [Function `get_mut`](#sui_vec_map_get_mut)
-  [Function `get`](#sui_vec_map_get)
-  [Function `try_get`](#sui_vec_map_try_get)
-  [Function `contains`](#sui_vec_map_contains)
-  [Function `size`](#sui_vec_map_size)
-  [Function `is_empty`](#sui_vec_map_is_empty)
-  [Function `destroy_empty`](#sui_vec_map_destroy_empty)
-  [Function `into_keys_values`](#sui_vec_map_into_keys_values)
-  [Function `from_keys_values`](#sui_vec_map_from_keys_values)
-  [Function `keys`](#sui_vec_map_keys)
-  [Function `get_idx_opt`](#sui_vec_map_get_idx_opt)
-  [Function `get_idx`](#sui_vec_map_get_idx)
-  [Function `get_entry_by_idx`](#sui_vec_map_get_entry_by_idx)
-  [Function `get_entry_by_idx_mut`](#sui_vec_map_get_entry_by_idx_mut)
-  [Function `remove_entry_by_idx`](#sui_vec_map_remove_entry_by_idx)


<pre><code><b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
</code></pre>



<a name="sui_vec_map_VecMap"></a>

## Struct `VecMap`

A map data structure backed by a vector. The map is guaranteed not to contain duplicate keys, but entries
are *not* sorted by key--entries are included in insertion order.
All operations are O(N) in the size of the map--the intention of this data structure is only to provide
the convenience of programming against a map API.
Large maps should use handwritten parent/child relationships instead.
Maps that need sorted iteration rather than insertion order iteration should also be handwritten.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K: <b>copy</b>, V&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>contents: vector&lt;<a href="../sui/vec_map.md#sui_vec_map_Entry">sui::vec_map::Entry</a>&lt;K, V&gt;&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_vec_map_Entry"></a>

## Struct `Entry`

An entry in the map


<pre><code><b>public</b> <b>struct</b> <a href="../sui/vec_map.md#sui_vec_map_Entry">Entry</a>&lt;K: <b>copy</b>, V&gt; <b>has</b> <b>copy</b>, drop, store
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


<a name="sui_vec_map_EIndexOutOfBounds"></a>

Trying to access an element of the map at an invalid index


<pre><code><b>const</b> <a href="../sui/vec_map.md#sui_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>: u64 = 3;
</code></pre>



<a name="sui_vec_map_EKeyAlreadyExists"></a>

This key already exists in the map


<pre><code><b>const</b> <a href="../sui/vec_map.md#sui_vec_map_EKeyAlreadyExists">EKeyAlreadyExists</a>: u64 = 0;
</code></pre>



<a name="sui_vec_map_EKeyDoesNotExist"></a>

This key does not exist in the map


<pre><code><b>const</b> <a href="../sui/vec_map.md#sui_vec_map_EKeyDoesNotExist">EKeyDoesNotExist</a>: u64 = 1;
</code></pre>



<a name="sui_vec_map_EMapEmpty"></a>

Trying to pop from a map that is empty


<pre><code><b>const</b> <a href="../sui/vec_map.md#sui_vec_map_EMapEmpty">EMapEmpty</a>: u64 = 4;
</code></pre>



<a name="sui_vec_map_EMapNotEmpty"></a>

Trying to destroy a map that is not empty


<pre><code><b>const</b> <a href="../sui/vec_map.md#sui_vec_map_EMapNotEmpty">EMapNotEmpty</a>: u64 = 2;
</code></pre>



<a name="sui_vec_map_EUnequalLengths"></a>

Trying to construct a map from keys and values of different lengths


<pre><code><b>const</b> <a href="../sui/vec_map.md#sui_vec_map_EUnequalLengths">EUnequalLengths</a>: u64 = 5;
</code></pre>



<a name="sui_vec_map_empty"></a>

## Function `empty`

Create an empty <code><a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_empty">empty</a>&lt;K: <b>copy</b>, V&gt;(): <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_empty">empty</a>&lt;K: <b>copy</b>, V&gt;(): <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt; {
    <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a> { contents: vector[] }
}
</code></pre>



</details>

<a name="sui_vec_map_insert"></a>

## Function `insert`

Insert the entry <code>key</code> |-> <code>value</code> into <code>self</code>.
Aborts if <code>key</code> is already bound in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_insert">insert</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;, key: K, value: V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_insert">insert</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: K, value: V) {
    <b>assert</b>!(!self.<a href="../sui/vec_map.md#sui_vec_map_contains">contains</a>(&key), <a href="../sui/vec_map.md#sui_vec_map_EKeyAlreadyExists">EKeyAlreadyExists</a>);
    self.contents.push_back(<a href="../sui/vec_map.md#sui_vec_map_Entry">Entry</a> { key, value })
}
</code></pre>



</details>

<a name="sui_vec_map_remove"></a>

## Function `remove`

Remove the entry <code>key</code> |-> <code>value</code> from self. Aborts if <code>key</code> is not bound in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_remove">remove</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;, key: &K): (K, V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_remove">remove</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): (K, V) {
    <b>let</b> idx = self.<a href="../sui/vec_map.md#sui_vec_map_get_idx">get_idx</a>(key);
    <b>let</b> <a href="../sui/vec_map.md#sui_vec_map_Entry">Entry</a> { key, value } = self.contents.<a href="../sui/vec_map.md#sui_vec_map_remove">remove</a>(idx);
    (key, value)
}
</code></pre>



</details>

<a name="sui_vec_map_pop"></a>

## Function `pop`

Pop the most recently inserted entry from the map. Aborts if the map is empty.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_pop">pop</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;): (K, V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_pop">pop</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): (K, V) {
    <b>assert</b>!(self.contents.length() != 0, <a href="../sui/vec_map.md#sui_vec_map_EMapEmpty">EMapEmpty</a>);
    <b>let</b> <a href="../sui/vec_map.md#sui_vec_map_Entry">Entry</a> { key, value } = self.contents.pop_back();
    (key, value)
}
</code></pre>



</details>

<a name="sui_vec_map_get_mut"></a>

## Function `get_mut`

Get a mutable reference to the value bound to <code>key</code> in <code>self</code>.
Aborts if <code>key</code> is not bound in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get_mut">get_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;, key: &K): &<b>mut</b> V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get_mut">get_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): &<b>mut</b> V {
    <b>let</b> idx = self.<a href="../sui/vec_map.md#sui_vec_map_get_idx">get_idx</a>(key);
    <b>let</b> <b>entry</b> = &<b>mut</b> self.contents[idx];
    &<b>mut</b> <b>entry</b>.value
}
</code></pre>



</details>

<a name="sui_vec_map_get"></a>

## Function `get`

Get a reference to the value bound to <code>key</code> in <code>self</code>.
Aborts if <code>key</code> is not bound in <code>self</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get">get</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;, key: &K): &V
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get">get</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): &V {
    <b>let</b> idx = self.<a href="../sui/vec_map.md#sui_vec_map_get_idx">get_idx</a>(key);
    <b>let</b> <b>entry</b> = &self.contents[idx];
    &<b>entry</b>.value
}
</code></pre>



</details>

<a name="sui_vec_map_try_get"></a>

## Function `try_get`

Safely try borrow a value bound to <code>key</code> in <code>self</code>.
Return Some(V) if the value exists, None otherwise.
Only works for a "copyable" value as references cannot be stored in <code>vector</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_try_get">try_get</a>&lt;K: <b>copy</b>, V: <b>copy</b>&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;, key: &K): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_try_get">try_get</a>&lt;K: <b>copy</b>, V: <b>copy</b>&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): Option&lt;V&gt; {
    <b>if</b> (self.<a href="../sui/vec_map.md#sui_vec_map_contains">contains</a>(key)) {
        option::some(*<a href="../sui/vec_map.md#sui_vec_map_get">get</a>(self, key))
    } <b>else</b> {
        option::none()
    }
}
</code></pre>



</details>

<a name="sui_vec_map_contains"></a>

## Function `contains`

Return true if <code>self</code> contains an entry for <code>key</code>, false otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_contains">contains</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;, key: &K): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_contains">contains</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): bool {
    <a href="../sui/vec_map.md#sui_vec_map_get_idx_opt">get_idx_opt</a>(self, key).is_some()
}
</code></pre>



</details>

<a name="sui_vec_map_size"></a>

## Function `size`

Return the number of entries in <code>self</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_size">size</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_size">size</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): u64 {
    self.contents.length()
}
</code></pre>



</details>

<a name="sui_vec_map_is_empty"></a>

## Function `is_empty`

Return true if <code>self</code> has 0 elements, false otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_is_empty">is_empty</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_is_empty">is_empty</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): bool {
    self.<a href="../sui/vec_map.md#sui_vec_map_size">size</a>() == 0
}
</code></pre>



</details>

<a name="sui_vec_map_destroy_empty"></a>

## Function `destroy_empty`

Destroy an empty map. Aborts if <code>self</code> is not empty


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_destroy_empty">destroy_empty</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;) {
    <b>let</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a> { contents } = self;
    <b>assert</b>!(contents.<a href="../sui/vec_map.md#sui_vec_map_is_empty">is_empty</a>(), <a href="../sui/vec_map.md#sui_vec_map_EMapNotEmpty">EMapNotEmpty</a>);
    contents.<a href="../sui/vec_map.md#sui_vec_map_destroy_empty">destroy_empty</a>()
}
</code></pre>



</details>

<a name="sui_vec_map_into_keys_values"></a>

## Function `into_keys_values`

Unpack <code>self</code> into vectors of its keys and values.
The output keys and values are stored in insertion order, *not* sorted by key.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_into_keys_values">into_keys_values</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;): (vector&lt;K&gt;, vector&lt;V&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_into_keys_values">into_keys_values</a>&lt;K: <b>copy</b>, V&gt;(self: <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): (vector&lt;K&gt;, vector&lt;V&gt;) {
    <b>let</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a> { <b>mut</b> contents } = self;
    // reverse the vector so the output <a href="../sui/vec_map.md#sui_vec_map_keys">keys</a> and values will appear in insertion order
    contents.reverse();
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> n = contents.length();
    <b>let</b> <b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_keys">keys</a> = vector[];
    <b>let</b> <b>mut</b> values = vector[];
    <b>while</b> (i &lt; n) {
        <b>let</b> <a href="../sui/vec_map.md#sui_vec_map_Entry">Entry</a> { key, value } = contents.pop_back();
        <a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>.push_back(key);
        values.push_back(value);
        i = i + 1;
    };
    contents.<a href="../sui/vec_map.md#sui_vec_map_destroy_empty">destroy_empty</a>();
    (<a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>, values)
}
</code></pre>



</details>

<a name="sui_vec_map_from_keys_values"></a>

## Function `from_keys_values`

Construct a new <code><a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a></code> from two vectors, one for keys and one for values.
The key value pairs are associated via their indices in the vectors, e.g. the key at index i
in <code><a href="../sui/vec_map.md#sui_vec_map_keys">keys</a></code> is associated with the value at index i in <code>values</code>.
The key value pairs are stored in insertion order (the original vectors ordering)
and are *not* sorted.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_from_keys_values">from_keys_values</a>&lt;K: <b>copy</b>, V&gt;(<a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>: vector&lt;K&gt;, values: vector&lt;V&gt;): <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_from_keys_values">from_keys_values</a>&lt;K: <b>copy</b>, V&gt;(<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>: vector&lt;K&gt;, <b>mut</b> values: vector&lt;V&gt;): <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt; {
    <b>assert</b>!(<a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>.length() == values.length(), <a href="../sui/vec_map.md#sui_vec_map_EUnequalLengths">EUnequalLengths</a>);
    <a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>.reverse();
    values.reverse();
    <b>let</b> <b>mut</b> map = <a href="../sui/vec_map.md#sui_vec_map_empty">empty</a>();
    <b>while</b> (<a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>.length() != 0) map.<a href="../sui/vec_map.md#sui_vec_map_insert">insert</a>(<a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>.pop_back(), values.pop_back());
    <a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>.<a href="../sui/vec_map.md#sui_vec_map_destroy_empty">destroy_empty</a>();
    values.<a href="../sui/vec_map.md#sui_vec_map_destroy_empty">destroy_empty</a>();
    map
}
</code></pre>



</details>

<a name="sui_vec_map_keys"></a>

## Function `keys`

Returns a list of keys in the map.
Do not assume any particular ordering.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;): vector&lt;K&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;): vector&lt;K&gt; {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> n = self.contents.length();
    <b>let</b> <b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_keys">keys</a> = vector[];
    <b>while</b> (i &lt; n) {
        <b>let</b> <b>entry</b> = self.contents.<a href="../sui/borrow.md#sui_borrow">borrow</a>(i);
        <a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>.push_back(<b>entry</b>.key);
        i = i + 1;
    };
    <a href="../sui/vec_map.md#sui_vec_map_keys">keys</a>
}
</code></pre>



</details>

<a name="sui_vec_map_get_idx_opt"></a>

## Function `get_idx_opt`

Find the index of <code>key</code> in <code>self</code>. Return <code>None</code> if <code>key</code> is not in <code>self</code>.
Note that map entries are stored in insertion order, *not* sorted by key.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get_idx_opt">get_idx_opt</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;, key: &K): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get_idx_opt">get_idx_opt</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): Option&lt;u64&gt; {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> n = <a href="../sui/vec_map.md#sui_vec_map_size">size</a>(self);
    <b>while</b> (i &lt; n) {
        <b>if</b> (&self.contents[i].key == key) {
            <b>return</b> option::some(i)
        };
        i = i + 1;
    };
    option::none()
}
</code></pre>



</details>

<a name="sui_vec_map_get_idx"></a>

## Function `get_idx`

Find the index of <code>key</code> in <code>self</code>. Aborts if <code>key</code> is not in <code>self</code>.
Note that map entries are stored in insertion order, *not* sorted by key.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get_idx">get_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;, key: &K): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get_idx">get_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, key: &K): u64 {
    <b>let</b> idx_opt = self.<a href="../sui/vec_map.md#sui_vec_map_get_idx_opt">get_idx_opt</a>(key);
    <b>assert</b>!(idx_opt.is_some(), <a href="../sui/vec_map.md#sui_vec_map_EKeyDoesNotExist">EKeyDoesNotExist</a>);
    idx_opt.destroy_some()
}
</code></pre>



</details>

<a name="sui_vec_map_get_entry_by_idx"></a>

## Function `get_entry_by_idx`

Return a reference to the <code>idx</code>th entry of <code>self</code>. This gives direct access into the backing array of the map--use with caution.
Note that map entries are stored in insertion order, *not* sorted by key.
Aborts if <code>idx</code> is greater than or equal to <code><a href="../sui/vec_map.md#sui_vec_map_size">size</a>(self)</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get_entry_by_idx">get_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;, idx: u64): (&K, &V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get_entry_by_idx">get_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, idx: u64): (&K, &V) {
    <b>assert</b>!(idx &lt; <a href="../sui/vec_map.md#sui_vec_map_size">size</a>(self), <a href="../sui/vec_map.md#sui_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>);
    <b>let</b> <b>entry</b> = &self.contents[idx];
    (&<b>entry</b>.key, &<b>entry</b>.value)
}
</code></pre>



</details>

<a name="sui_vec_map_get_entry_by_idx_mut"></a>

## Function `get_entry_by_idx_mut`

Return a mutable reference to the <code>idx</code>th entry of <code>self</code>. This gives direct access into the backing array of the map--use with caution.
Note that map entries are stored in insertion order, *not* sorted by key.
Aborts if <code>idx</code> is greater than or equal to <code><a href="../sui/vec_map.md#sui_vec_map_size">size</a>(self)</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get_entry_by_idx_mut">get_entry_by_idx_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;, idx: u64): (&K, &<b>mut</b> V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_get_entry_by_idx_mut">get_entry_by_idx_mut</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, idx: u64): (&K, &<b>mut</b> V) {
    <b>assert</b>!(idx &lt; <a href="../sui/vec_map.md#sui_vec_map_size">size</a>(self), <a href="../sui/vec_map.md#sui_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>);
    <b>let</b> <b>entry</b> = &<b>mut</b> self.contents[idx];
    (&<b>entry</b>.key, &<b>mut</b> <b>entry</b>.value)
}
</code></pre>



</details>

<a name="sui_vec_map_remove_entry_by_idx"></a>

## Function `remove_entry_by_idx`

Remove the entry at index <code>idx</code> from self.
Aborts if <code>idx</code> is greater than or equal to <code><a href="../sui/vec_map.md#sui_vec_map_size">size</a>(self)</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_remove_entry_by_idx">remove_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;K, V&gt;, idx: u64): (K, V)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/vec_map.md#sui_vec_map_remove_entry_by_idx">remove_entry_by_idx</a>&lt;K: <b>copy</b>, V&gt;(self: &<b>mut</b> <a href="../sui/vec_map.md#sui_vec_map_VecMap">VecMap</a>&lt;K, V&gt;, idx: u64): (K, V) {
    <b>assert</b>!(idx &lt; <a href="../sui/vec_map.md#sui_vec_map_size">size</a>(self), <a href="../sui/vec_map.md#sui_vec_map_EIndexOutOfBounds">EIndexOutOfBounds</a>);
    <b>let</b> <a href="../sui/vec_map.md#sui_vec_map_Entry">Entry</a> { key, value } = self.contents.<a href="../sui/vec_map.md#sui_vec_map_remove">remove</a>(idx);
    (key, value)
}
</code></pre>



</details>
