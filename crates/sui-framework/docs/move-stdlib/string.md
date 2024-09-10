---
title: Module `0x1::string`
---

The <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> module defines the <code><a href="../move-stdlib/string.md#0x1_string_String">String</a></code> type which represents UTF8 encoded
strings.


-  [Struct `String`](#0x1_string_String)
-  [Constants](#@Constants_0)
-  [Function `utf8`](#0x1_string_utf8)
-  [Function `from_ascii`](#0x1_string_from_ascii)
-  [Function `to_ascii`](#0x1_string_to_ascii)
-  [Function `try_utf8`](#0x1_string_try_utf8)
-  [Function `as_bytes`](#0x1_string_as_bytes)
-  [Function `into_bytes`](#0x1_string_into_bytes)
-  [Function `is_empty`](#0x1_string_is_empty)
-  [Function `length`](#0x1_string_length)
-  [Function `append`](#0x1_string_append)
-  [Function `append_utf8`](#0x1_string_append_utf8)
-  [Function `insert`](#0x1_string_insert)
-  [Function `substring`](#0x1_string_substring)
-  [Function `index_of`](#0x1_string_index_of)
-  [Function `internal_check_utf8`](#0x1_string_internal_check_utf8)
-  [Function `internal_is_char_boundary`](#0x1_string_internal_is_char_boundary)
-  [Function `internal_sub_string`](#0x1_string_internal_sub_string)
-  [Function `internal_index_of`](#0x1_string_internal_index_of)
-  [Function `bytes`](#0x1_string_bytes)
-  [Function `sub_string`](#0x1_string_sub_string)


<pre><code><b>use</b> <a href="../move-stdlib/ascii.md#0x1_ascii">0x1::ascii</a>;
<b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="0x1_string_String"></a>

## Struct `String`

A <code><a href="../move-stdlib/string.md#0x1_string_String">String</a></code> holds a sequence of bytes which is guaranteed to be in utf8
format.


<pre><code><b>struct</b> <a href="../move-stdlib/string.md#0x1_string_String">String</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x1_string_EInvalidIndex"></a>

Index out of range.


<pre><code><b>const</b> <a href="../move-stdlib/string.md#0x1_string_EInvalidIndex">EInvalidIndex</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0x1_string_EInvalidUTF8"></a>

An invalid UTF8 encoding.


<pre><code><b>const</b> <a href="../move-stdlib/string.md#0x1_string_EInvalidUTF8">EInvalidUTF8</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x1_string_utf8"></a>

## Function `utf8`

Creates a new string from a sequence of bytes. Aborts if the bytes do
not represent valid utf8.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_utf8">utf8</a>(bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_utf8">utf8</a>(bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/string.md#0x1_string_String">String</a> {
    <b>assert</b>!(<a href="../move-stdlib/string.md#0x1_string_internal_check_utf8">internal_check_utf8</a>(&bytes), <a href="../move-stdlib/string.md#0x1_string_EInvalidUTF8">EInvalidUTF8</a>);
    <a href="../move-stdlib/string.md#0x1_string_String">String</a> { bytes }
}
</code></pre>



</details>

<a name="0x1_string_from_ascii"></a>

## Function `from_ascii`

Convert an ASCII string to a UTF8 string


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_from_ascii">from_ascii</a>(s: <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_from_ascii">from_ascii</a>(s: <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../move-stdlib/string.md#0x1_string_String">String</a> {
    <a href="../move-stdlib/string.md#0x1_string_String">String</a> { bytes: s.<a href="../move-stdlib/string.md#0x1_string_into_bytes">into_bytes</a>() }
}
</code></pre>



</details>

<a name="0x1_string_to_ascii"></a>

## Function `to_ascii`

Convert an UTF8 string to an ASCII string.
Aborts if <code>s</code> is not valid ASCII


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_to_ascii">to_ascii</a>(s: <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>): <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_to_ascii">to_ascii</a>(s: <a href="../move-stdlib/string.md#0x1_string_String">String</a>): <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a> {
    <b>let</b> <a href="../move-stdlib/string.md#0x1_string_String">String</a> { bytes } = s;
    bytes.to_ascii_string()
}
</code></pre>



</details>

<a name="0x1_string_try_utf8"></a>

## Function `try_utf8`

Tries to create a new string from a sequence of bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_try_utf8">try_utf8</a>(bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../move-stdlib/string.md#0x1_string_String">string::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_try_utf8">try_utf8</a>(bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): Option&lt;<a href="../move-stdlib/string.md#0x1_string_String">String</a>&gt; {
    <b>if</b> (<a href="../move-stdlib/string.md#0x1_string_internal_check_utf8">internal_check_utf8</a>(&bytes)) <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(<a href="../move-stdlib/string.md#0x1_string_String">String</a> { bytes })
    <b>else</b> <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>()
}
</code></pre>



</details>

<a name="0x1_string_as_bytes"></a>

## Function `as_bytes`

Returns a reference to the underlying byte vector.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_as_bytes">as_bytes</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">string::String</a>): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_as_bytes">as_bytes</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">String</a>): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    &s.bytes
}
</code></pre>



</details>

<a name="0x1_string_into_bytes"></a>

## Function `into_bytes`

Unpack the <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> to get its underlying bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_into_bytes">into_bytes</a>(s: <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_into_bytes">into_bytes</a>(s: <a href="../move-stdlib/string.md#0x1_string_String">String</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    <b>let</b> <a href="../move-stdlib/string.md#0x1_string_String">String</a> { bytes } = s;
    bytes
}
</code></pre>



</details>

<a name="0x1_string_is_empty"></a>

## Function `is_empty`

Checks whether this string is empty.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_is_empty">is_empty</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">string::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_is_empty">is_empty</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">String</a>): bool {
    s.bytes.<a href="../move-stdlib/string.md#0x1_string_is_empty">is_empty</a>()
}
</code></pre>



</details>

<a name="0x1_string_length"></a>

## Function `length`

Returns the length of this string, in bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_length">length</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">string::String</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_length">length</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">String</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    s.bytes.<a href="../move-stdlib/string.md#0x1_string_length">length</a>()
}
</code></pre>



</details>

<a name="0x1_string_append"></a>

## Function `append`

Appends a string.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_append">append</a>(s: &<b>mut</b> <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>, r: <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_append">append</a>(s: &<b>mut</b> <a href="../move-stdlib/string.md#0x1_string_String">String</a>, r: <a href="../move-stdlib/string.md#0x1_string_String">String</a>) {
    s.bytes.<a href="../move-stdlib/string.md#0x1_string_append">append</a>(r.bytes)
}
</code></pre>



</details>

<a name="0x1_string_append_utf8"></a>

## Function `append_utf8`

Appends bytes which must be in valid utf8 format.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_append_utf8">append_utf8</a>(s: &<b>mut</b> <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>, bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_append_utf8">append_utf8</a>(s: &<b>mut</b> <a href="../move-stdlib/string.md#0x1_string_String">String</a>, bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;) {
    s.<a href="../move-stdlib/string.md#0x1_string_append">append</a>(<a href="../move-stdlib/string.md#0x1_string_utf8">utf8</a>(bytes))
}
</code></pre>



</details>

<a name="0x1_string_insert"></a>

## Function `insert`

Insert the other string at the byte index in given string. The index
must be at a valid utf8 char boundary.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_insert">insert</a>(s: &<b>mut</b> <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>, at: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, o: <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_insert">insert</a>(s: &<b>mut</b> <a href="../move-stdlib/string.md#0x1_string_String">String</a>, at: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, o: <a href="../move-stdlib/string.md#0x1_string_String">String</a>) {
    <b>let</b> bytes = &s.bytes;
    <b>assert</b>!(
        at &lt;= bytes.<a href="../move-stdlib/string.md#0x1_string_length">length</a>() && <a href="../move-stdlib/string.md#0x1_string_internal_is_char_boundary">internal_is_char_boundary</a>(bytes, at),
        <a href="../move-stdlib/string.md#0x1_string_EInvalidIndex">EInvalidIndex</a>,
    );
    <b>let</b> l = s.<a href="../move-stdlib/string.md#0x1_string_length">length</a>();
    <b>let</b> <b>mut</b> front = s.<a href="../move-stdlib/string.md#0x1_string_substring">substring</a>(0, at);
    <b>let</b> end = s.<a href="../move-stdlib/string.md#0x1_string_substring">substring</a>(at, l);
    front.<a href="../move-stdlib/string.md#0x1_string_append">append</a>(o);
    front.<a href="../move-stdlib/string.md#0x1_string_append">append</a>(end);
    *s = front;
}
</code></pre>



</details>

<a name="0x1_string_substring"></a>

## Function `substring`

Returns a sub-string using the given byte indices, where <code>i</code> is the first
byte position and <code>j</code> is the start of the first byte not included (or the
length of the string). The indices must be at valid utf8 char boundaries,
guaranteeing that the result is valid utf8.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_substring">substring</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">string::String</a>, i: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, j: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_substring">substring</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">String</a>, i: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, j: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/string.md#0x1_string_String">String</a> {
    <b>let</b> bytes = &s.bytes;
    <b>let</b> l = bytes.<a href="../move-stdlib/string.md#0x1_string_length">length</a>();
    <b>assert</b>!(
        j &lt;= l &&
        i &lt;= j &&
        <a href="../move-stdlib/string.md#0x1_string_internal_is_char_boundary">internal_is_char_boundary</a>(bytes, i) &&
        <a href="../move-stdlib/string.md#0x1_string_internal_is_char_boundary">internal_is_char_boundary</a>(bytes, j),
        <a href="../move-stdlib/string.md#0x1_string_EInvalidIndex">EInvalidIndex</a>,
    );
    <a href="../move-stdlib/string.md#0x1_string_String">String</a> { bytes: <a href="../move-stdlib/string.md#0x1_string_internal_sub_string">internal_sub_string</a>(bytes, i, j) }
}
</code></pre>



</details>

<a name="0x1_string_index_of"></a>

## Function `index_of`

Computes the index of the first occurrence of a string. Returns <code>s.<a href="../move-stdlib/string.md#0x1_string_length">length</a>()</code>
if no occurrence found.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_index_of">index_of</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">string::String</a>, r: &<a href="../move-stdlib/string.md#0x1_string_String">string::String</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_index_of">index_of</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">String</a>, r: &<a href="../move-stdlib/string.md#0x1_string_String">String</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <a href="../move-stdlib/string.md#0x1_string_internal_index_of">internal_index_of</a>(&s.bytes, &r.bytes)
}
</code></pre>



</details>

<a name="0x1_string_internal_check_utf8"></a>

## Function `internal_check_utf8`



<pre><code><b>fun</b> <a href="../move-stdlib/string.md#0x1_string_internal_check_utf8">internal_check_utf8</a>(v: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_internal_check_utf8">internal_check_utf8</a>(v: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>

<a name="0x1_string_internal_is_char_boundary"></a>

## Function `internal_is_char_boundary`



<pre><code><b>fun</b> <a href="../move-stdlib/string.md#0x1_string_internal_is_char_boundary">internal_is_char_boundary</a>(v: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, i: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_internal_is_char_boundary">internal_is_char_boundary</a>(v: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, i: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): bool;
</code></pre>



</details>

<a name="0x1_string_internal_sub_string"></a>

## Function `internal_sub_string`



<pre><code><b>fun</b> <a href="../move-stdlib/string.md#0x1_string_internal_sub_string">internal_sub_string</a>(v: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, i: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, j: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_internal_sub_string">internal_sub_string</a>(v: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, i: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, j: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x1_string_internal_index_of"></a>

## Function `internal_index_of`



<pre><code><b>fun</b> <a href="../move-stdlib/string.md#0x1_string_internal_index_of">internal_index_of</a>(v: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, r: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_internal_index_of">internal_index_of</a>(v: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, r: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>;
</code></pre>



</details>

<a name="0x1_string_bytes"></a>

## Function `bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_bytes">bytes</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">string::String</a>): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_bytes">bytes</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">String</a>): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; { s.<a href="../move-stdlib/string.md#0x1_string_as_bytes">as_bytes</a>() }
</code></pre>



</details>

<a name="0x1_string_sub_string"></a>

## Function `sub_string`



<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_sub_string">sub_string</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">string::String</a>, i: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, j: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string_sub_string">sub_string</a>(s: &<a href="../move-stdlib/string.md#0x1_string_String">String</a>, i: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, j: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/string.md#0x1_string_String">String</a> {
    s.<a href="../move-stdlib/string.md#0x1_string_substring">substring</a>(i, j)
}
</code></pre>



</details>
