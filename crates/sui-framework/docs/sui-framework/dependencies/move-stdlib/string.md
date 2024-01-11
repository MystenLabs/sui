
<a name="0x1_string"></a>

# Module `0x1::string`



-  [Struct `String`](#0x1_string_String)
-  [Constants](#@Constants_0)
-  [Function `utf8`](#0x1_string_utf8)
-  [Function `from_ascii`](#0x1_string_from_ascii)
-  [Function `to_ascii`](#0x1_string_to_ascii)
-  [Function `try_utf8`](#0x1_string_try_utf8)
-  [Function `bytes`](#0x1_string_bytes)
-  [Function `is_empty`](#0x1_string_is_empty)
-  [Function `length`](#0x1_string_length)
-  [Function `append`](#0x1_string_append)
-  [Function `append_utf8`](#0x1_string_append_utf8)
-  [Function `insert`](#0x1_string_insert)
-  [Function `sub_string`](#0x1_string_sub_string)
-  [Function `index_of`](#0x1_string_index_of)
-  [Function `internal_check_utf8`](#0x1_string_internal_check_utf8)
-  [Function `internal_is_char_boundary`](#0x1_string_internal_is_char_boundary)
-  [Function `internal_sub_string`](#0x1_string_internal_sub_string)
-  [Function `internal_index_of`](#0x1_string_internal_index_of)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii">0x1::ascii</a>;
<b>use</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="0x1_string_String"></a>

## Struct `String`



<pre><code><b>struct</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x1_string_EINVALID_INDEX"></a>



<pre><code><b>const</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_EINVALID_INDEX">EINVALID_INDEX</a>: u64 = 2;
</code></pre>



<a name="0x1_string_EINVALID_UTF8"></a>



<pre><code><b>const</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_EINVALID_UTF8">EINVALID_UTF8</a>: u64 = 1;
</code></pre>



<a name="0x1_string_utf8"></a>

## Function `utf8`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_utf8">utf8</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_utf8">utf8</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a> {
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_check_utf8">internal_check_utf8</a>(&bytes), <a href="../../dependencies/move-stdlib/string.md#0x1_string_EINVALID_UTF8">EINVALID_UTF8</a>);
    <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>{bytes}
}
</code></pre>



</details>

<a name="0x1_string_from_ascii"></a>

## Function `from_ascii`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_from_ascii">from_ascii</a>(s: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_from_ascii">from_ascii</a>(s: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a> {
    <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a> { bytes: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_into_bytes">ascii::into_bytes</a>(s) }
}
</code></pre>



</details>

<a name="0x1_string_to_ascii"></a>

## Function `to_ascii`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_to_ascii">to_ascii</a>(s: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_to_ascii">to_ascii</a>(s: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a> {
    <b>let</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a> { bytes } = s;
    <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_string">ascii::string</a>(bytes)
}
</code></pre>



</details>

<a name="0x1_string_try_utf8"></a>

## Function `try_utf8`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_try_utf8">try_utf8</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_try_utf8">try_utf8</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): Option&lt;<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>&gt; {
    <b>if</b> (<a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_check_utf8">internal_check_utf8</a>(&bytes)) {
        <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>{bytes})
    } <b>else</b> {
        <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>()
    }
}
</code></pre>



</details>

<a name="0x1_string_bytes"></a>

## Function `bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_bytes">bytes</a>(s: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>): &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_bytes">bytes</a>(s: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>): &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    &s.bytes
}
</code></pre>



</details>

<a name="0x1_string_is_empty"></a>

## Function `is_empty`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_is_empty">is_empty</a>(s: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_is_empty">is_empty</a>(s: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>): bool {
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&s.bytes)
}
</code></pre>



</details>

<a name="0x1_string_length"></a>

## Function `length`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_length">length</a>(s: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_length">length</a>(s: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>): u64 {
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&s.bytes)
}
</code></pre>



</details>

<a name="0x1_string_append"></a>

## Function `append`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_append">append</a>(s: &<b>mut</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, r: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_append">append</a>(s: &<b>mut</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>, r: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>) {
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_append">vector::append</a>(&<b>mut</b> s.bytes, r.bytes)
}
</code></pre>



</details>

<a name="0x1_string_append_utf8"></a>

## Function `append_utf8`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_append_utf8">append_utf8</a>(s: &<b>mut</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_append_utf8">append_utf8</a>(s: &<b>mut</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>, bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;) {
    <a href="../../dependencies/move-stdlib/string.md#0x1_string_append">append</a>(s, <a href="../../dependencies/move-stdlib/string.md#0x1_string_utf8">utf8</a>(bytes))
}
</code></pre>



</details>

<a name="0x1_string_insert"></a>

## Function `insert`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_insert">insert</a>(s: &<b>mut</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, at: u64, o: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_insert">insert</a>(s: &<b>mut</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>, at: u64, o: <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>) {
    <b>let</b> bytes = &s.bytes;
    <b>assert</b>!(at &lt;= <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(bytes) && <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_is_char_boundary">internal_is_char_boundary</a>(bytes, at), <a href="../../dependencies/move-stdlib/string.md#0x1_string_EINVALID_INDEX">EINVALID_INDEX</a>);
    <b>let</b> l = <a href="../../dependencies/move-stdlib/string.md#0x1_string_length">length</a>(s);
    <b>let</b> front = <a href="../../dependencies/move-stdlib/string.md#0x1_string_sub_string">sub_string</a>(s, 0, at);
    <b>let</b> end = <a href="../../dependencies/move-stdlib/string.md#0x1_string_sub_string">sub_string</a>(s, at, l);
    <a href="../../dependencies/move-stdlib/string.md#0x1_string_append">append</a>(&<b>mut</b> front, o);
    <a href="../../dependencies/move-stdlib/string.md#0x1_string_append">append</a>(&<b>mut</b> front, end);
    *s = front;
}
</code></pre>



</details>

<a name="0x1_string_sub_string"></a>

## Function `sub_string`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_sub_string">sub_string</a>(s: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, i: u64, j: u64): <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_sub_string">sub_string</a>(s: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>, i: u64, j: u64): <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a> {
    <b>let</b> bytes = &s.bytes;
    <b>let</b> l = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(bytes);
    <b>assert</b>!(
        j &lt;= l && i &lt;= j && <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_is_char_boundary">internal_is_char_boundary</a>(bytes, i) && <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_is_char_boundary">internal_is_char_boundary</a>(bytes, j),
        <a href="../../dependencies/move-stdlib/string.md#0x1_string_EINVALID_INDEX">EINVALID_INDEX</a>
    );
    <a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>{bytes: <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_sub_string">internal_sub_string</a>(bytes, i, j)}
}
</code></pre>



</details>

<a name="0x1_string_index_of"></a>

## Function `index_of`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_index_of">index_of</a>(s: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, r: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_index_of">index_of</a>(s: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>, r: &<a href="../../dependencies/move-stdlib/string.md#0x1_string_String">String</a>): u64 {
    <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_index_of">internal_index_of</a>(&s.bytes, &r.bytes)
}
</code></pre>



</details>

<a name="0x1_string_internal_check_utf8"></a>

## Function `internal_check_utf8`



<pre><code><b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_check_utf8">internal_check_utf8</a>(v: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_check_utf8">internal_check_utf8</a>(v: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>

<a name="0x1_string_internal_is_char_boundary"></a>

## Function `internal_is_char_boundary`



<pre><code><b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_is_char_boundary">internal_is_char_boundary</a>(v: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, i: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_is_char_boundary">internal_is_char_boundary</a>(v: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, i: u64): bool;
</code></pre>



</details>

<a name="0x1_string_internal_sub_string"></a>

## Function `internal_sub_string`



<pre><code><b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_sub_string">internal_sub_string</a>(v: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, i: u64, j: u64): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_sub_string">internal_sub_string</a>(v: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, i: u64, j: u64): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x1_string_internal_index_of"></a>

## Function `internal_index_of`



<pre><code><b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_index_of">internal_index_of</a>(v: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, r: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string_internal_index_of">internal_index_of</a>(v: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, r: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): u64;
</code></pre>



</details>
