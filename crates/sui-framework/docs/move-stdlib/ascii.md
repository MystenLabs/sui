---
title: Module `0x1::ascii`
---

The <code>ASCII</code> module defines basic string and char newtypes in Move that verify
that characters are valid ASCII, and that strings consist of only valid ASCII characters.


-  [Struct `String`](#0x1_ascii_String)
-  [Struct `Char`](#0x1_ascii_Char)
-  [Constants](#@Constants_0)
-  [Function `char`](#0x1_ascii_char)
-  [Function `string`](#0x1_ascii_string)
-  [Function `try_string`](#0x1_ascii_try_string)
-  [Function `all_characters_printable`](#0x1_ascii_all_characters_printable)
-  [Function `push_char`](#0x1_ascii_push_char)
-  [Function `pop_char`](#0x1_ascii_pop_char)
-  [Function `length`](#0x1_ascii_length)
-  [Function `append`](#0x1_ascii_append)
-  [Function `insert`](#0x1_ascii_insert)
-  [Function `substring`](#0x1_ascii_substring)
-  [Function `as_bytes`](#0x1_ascii_as_bytes)
-  [Function `into_bytes`](#0x1_ascii_into_bytes)
-  [Function `byte`](#0x1_ascii_byte)
-  [Function `is_valid_char`](#0x1_ascii_is_valid_char)
-  [Function `is_printable_char`](#0x1_ascii_is_printable_char)
-  [Function `is_empty`](#0x1_ascii_is_empty)
-  [Function `to_uppercase`](#0x1_ascii_to_uppercase)
-  [Function `to_lowercase`](#0x1_ascii_to_lowercase)
-  [Function `index_of`](#0x1_ascii_index_of)
-  [Function `char_to_uppercase`](#0x1_ascii_char_to_uppercase)
-  [Function `char_to_lowercase`](#0x1_ascii_char_to_lowercase)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="0x1_ascii_String"></a>

## Struct `String`

The <code><a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a></code> struct holds a vector of bytes that all represent
valid ASCII characters. Note that these ASCII characters may not all
be printable. To determine if a <code><a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a></code> contains only "printable"
characters you should use the <code>all_characters_printable</code> predicate
defined in this module.


<pre><code><b>struct</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> <b>has</b> <b>copy</b>, drop, store
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

<a name="0x1_ascii_Char"></a>

## Struct `Char`

An ASCII character.


<pre><code><b>struct</b> <a href="../move-stdlib/ascii.md#0x1_ascii_Char">Char</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>byte: u8</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x1_ascii_EInvalidASCIICharacter"></a>

An invalid ASCII character was encountered when creating an ASCII string.


<pre><code><b>const</b> <a href="../move-stdlib/ascii.md#0x1_ascii_EInvalidASCIICharacter">EInvalidASCIICharacter</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 65536;
</code></pre>



<a name="0x1_ascii_EInvalidIndex"></a>

An invalid index was encountered when creating a substring.


<pre><code><b>const</b> <a href="../move-stdlib/ascii.md#0x1_ascii_EInvalidIndex">EInvalidIndex</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 65537;
</code></pre>



<a name="0x1_ascii_char"></a>

## Function `char`

Convert a <code>byte</code> into a <code><a href="../move-stdlib/ascii.md#0x1_ascii_Char">Char</a></code> that is checked to make sure it is valid ASCII.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_char">char</a>(byte: u8): <a href="../move-stdlib/ascii.md#0x1_ascii_Char">ascii::Char</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_char">char</a>(byte: u8): <a href="../move-stdlib/ascii.md#0x1_ascii_Char">Char</a> {
    <b>assert</b>!(<a href="../move-stdlib/ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(byte), <a href="../move-stdlib/ascii.md#0x1_ascii_EInvalidASCIICharacter">EInvalidASCIICharacter</a>);
    <a href="../move-stdlib/ascii.md#0x1_ascii_Char">Char</a> { byte }
}
</code></pre>



</details>

<a name="0x1_ascii_string"></a>

## Function `string`

Convert a vector of bytes <code>bytes</code> into an <code><a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a></code>. Aborts if
<code>bytes</code> contains non-ASCII characters.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string">string</a>(bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/string.md#0x1_string">string</a>(bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> {
    <b>let</b> x = <a href="../move-stdlib/ascii.md#0x1_ascii_try_string">try_string</a>(bytes);
    <b>assert</b>!(x.is_some(), <a href="../move-stdlib/ascii.md#0x1_ascii_EInvalidASCIICharacter">EInvalidASCIICharacter</a>);
    x.destroy_some()
}
</code></pre>



</details>

<a name="0x1_ascii_try_string"></a>

## Function `try_string`

Convert a vector of bytes <code>bytes</code> into an <code><a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a></code>. Returns
<code>Some(&lt;ascii_string&gt;)</code> if the <code>bytes</code> contains all valid ASCII
characters. Otherwise returns <code>None</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_try_string">try_string</a>(bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_try_string">try_string</a>(bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): Option&lt;<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>&gt; {
    <b>let</b> is_valid = bytes.all!(|byte| <a href="../move-stdlib/ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(*byte));
    <b>if</b> (is_valid) <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> { bytes })
    <b>else</b> <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>()
}
</code></pre>



</details>

<a name="0x1_ascii_all_characters_printable"></a>

## Function `all_characters_printable`

Returns <code><b>true</b></code> if all characters in <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> are printable characters
Returns <code><b>false</b></code> otherwise. Not all <code><a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a></code>s are printable strings.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_all_characters_printable">all_characters_printable</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_all_characters_printable">all_characters_printable</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>): bool {
    <a href="../move-stdlib/string.md#0x1_string">string</a>.bytes.all!(|byte| <a href="../move-stdlib/ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(*byte))
}
</code></pre>



</details>

<a name="0x1_ascii_push_char"></a>

## Function `push_char`

Push a <code><a href="../move-stdlib/ascii.md#0x1_ascii_Char">Char</a></code> to the end of the <code><a href="../move-stdlib/string.md#0x1_string">string</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_push_char">push_char</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<b>mut</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>, char: <a href="../move-stdlib/ascii.md#0x1_ascii_Char">ascii::Char</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_push_char">push_char</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<b>mut</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>, char: <a href="../move-stdlib/ascii.md#0x1_ascii_Char">Char</a>) {
    <a href="../move-stdlib/string.md#0x1_string">string</a>.bytes.push_back(char.byte);
}
</code></pre>



</details>

<a name="0x1_ascii_pop_char"></a>

## Function `pop_char`

Pop a <code><a href="../move-stdlib/ascii.md#0x1_ascii_Char">Char</a></code> from the end of the <code><a href="../move-stdlib/string.md#0x1_string">string</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_pop_char">pop_char</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<b>mut</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../move-stdlib/ascii.md#0x1_ascii_Char">ascii::Char</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_pop_char">pop_char</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<b>mut</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>): <a href="../move-stdlib/ascii.md#0x1_ascii_Char">Char</a> {
    <a href="../move-stdlib/ascii.md#0x1_ascii_Char">Char</a> { byte: <a href="../move-stdlib/string.md#0x1_string">string</a>.bytes.pop_back() }
}
</code></pre>



</details>

<a name="0x1_ascii_length"></a>

## Function `length`

Returns the length of the <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> in bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <a href="../move-stdlib/string.md#0x1_string">string</a>.<a href="../move-stdlib/ascii.md#0x1_ascii_as_bytes">as_bytes</a>().<a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>()
}
</code></pre>



</details>

<a name="0x1_ascii_append"></a>

## Function `append`

Append the <code>other</code> string to the end of <code><a href="../move-stdlib/string.md#0x1_string">string</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_append">append</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<b>mut</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>, other: <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_append">append</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<b>mut</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>, other: <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>) {
    <a href="../move-stdlib/string.md#0x1_string">string</a>.bytes.<a href="../move-stdlib/ascii.md#0x1_ascii_append">append</a>(other.<a href="../move-stdlib/ascii.md#0x1_ascii_into_bytes">into_bytes</a>())
}
</code></pre>



</details>

<a name="0x1_ascii_insert"></a>

## Function `insert`

Insert the <code>other</code> string at the <code>at</code> index of <code><a href="../move-stdlib/string.md#0x1_string">string</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_insert">insert</a>(s: &<b>mut</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>, at: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, o: <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_insert">insert</a>(s: &<b>mut</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>, at: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, o: <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>) {
    <b>assert</b>!(at &lt;= s.<a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>(), <a href="../move-stdlib/ascii.md#0x1_ascii_EInvalidIndex">EInvalidIndex</a>);
    o.<a href="../move-stdlib/ascii.md#0x1_ascii_into_bytes">into_bytes</a>().destroy!(|e| s.bytes.<a href="../move-stdlib/ascii.md#0x1_ascii_insert">insert</a>(e, at));
}
</code></pre>



</details>

<a name="0x1_ascii_substring"></a>

## Function `substring`

Copy the slice of the <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> from <code>i</code> to <code>j</code> into a new <code><a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_substring">substring</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>, i: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, j: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_substring">substring</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>, i: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, j: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> {
    <b>assert</b>!(i &lt;= j && j &lt;= <a href="../move-stdlib/string.md#0x1_string">string</a>.<a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>(), <a href="../move-stdlib/ascii.md#0x1_ascii_EInvalidIndex">EInvalidIndex</a>);
    <b>let</b> <b>mut</b> bytes = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    i.range_do!(j, |i| bytes.push_back(<a href="../move-stdlib/string.md#0x1_string">string</a>.bytes[i]));
    <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> { bytes }
}
</code></pre>



</details>

<a name="0x1_ascii_as_bytes"></a>

## Function `as_bytes`

Get the inner bytes of the <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> as a reference


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_as_bytes">as_bytes</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_as_bytes">as_bytes</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    &<a href="../move-stdlib/string.md#0x1_string">string</a>.bytes
}
</code></pre>



</details>

<a name="0x1_ascii_into_bytes"></a>

## Function `into_bytes`

Unpack the <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> to get its backing bytes


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_into_bytes">into_bytes</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_into_bytes">into_bytes</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    <b>let</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> { bytes } = <a href="../move-stdlib/string.md#0x1_string">string</a>;
    bytes
}
</code></pre>



</details>

<a name="0x1_ascii_byte"></a>

## Function `byte`

Unpack the <code>char</code> into its underlying bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_byte">byte</a>(char: <a href="../move-stdlib/ascii.md#0x1_ascii_Char">ascii::Char</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_byte">byte</a>(char: <a href="../move-stdlib/ascii.md#0x1_ascii_Char">Char</a>): u8 {
    <b>let</b> <a href="../move-stdlib/ascii.md#0x1_ascii_Char">Char</a> { byte } = char;
    byte
}
</code></pre>



</details>

<a name="0x1_ascii_is_valid_char"></a>

## Function `is_valid_char`

Returns <code><b>true</b></code> if <code>b</code> is a valid ASCII character.
Returns <code><b>false</b></code> otherwise.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(b: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(b: u8): bool {
    b &lt;= 0x7F
}
</code></pre>



</details>

<a name="0x1_ascii_is_printable_char"></a>

## Function `is_printable_char`

Returns <code><b>true</b></code> if <code>byte</code> is an printable ASCII character.
Returns <code><b>false</b></code> otherwise.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(byte: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(byte: u8): bool {
    byte &gt;= 0x20 && // Disallow metacharacters
    <a href="../move-stdlib/ascii.md#0x1_ascii_byte">byte</a> &lt;= 0x7E // Don't allow DEL metacharacter
}
</code></pre>



</details>

<a name="0x1_ascii_is_empty"></a>

## Function `is_empty`

Returns <code><b>true</b></code> if <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> is empty.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_is_empty">is_empty</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_is_empty">is_empty</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>): bool {
    <a href="../move-stdlib/string.md#0x1_string">string</a>.bytes.<a href="../move-stdlib/ascii.md#0x1_ascii_is_empty">is_empty</a>()
}
</code></pre>



</details>

<a name="0x1_ascii_to_uppercase"></a>

## Function `to_uppercase`

Convert a <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> to its uppercase equivalent.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_to_uppercase">to_uppercase</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_to_uppercase">to_uppercase</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>): <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> {
    <b>let</b> bytes = <a href="../move-stdlib/string.md#0x1_string">string</a>.<a href="../move-stdlib/ascii.md#0x1_ascii_as_bytes">as_bytes</a>().map_ref!(|byte| <a href="../move-stdlib/ascii.md#0x1_ascii_char_to_uppercase">char_to_uppercase</a>(*byte));
    <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> { bytes }
}
</code></pre>



</details>

<a name="0x1_ascii_to_lowercase"></a>

## Function `to_lowercase`

Convert a <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> to its lowercase equivalent.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_to_lowercase">to_lowercase</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_to_lowercase">to_lowercase</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>): <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> {
    <b>let</b> bytes = <a href="../move-stdlib/string.md#0x1_string">string</a>.<a href="../move-stdlib/ascii.md#0x1_ascii_as_bytes">as_bytes</a>().map_ref!(|byte| <a href="../move-stdlib/ascii.md#0x1_ascii_char_to_lowercase">char_to_lowercase</a>(*byte));
    <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> { bytes }
}
</code></pre>



</details>

<a name="0x1_ascii_index_of"></a>

## Function `index_of`

Computes the index of the first occurrence of the <code>substr</code> in the <code><a href="../move-stdlib/string.md#0x1_string">string</a></code>.
Returns the length of the <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> if the <code>substr</code> is not found.
Returns 0 if the <code>substr</code> is empty.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_index_of">index_of</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>, substr: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_index_of">index_of</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>, substr: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> (n, m) = (<a href="../move-stdlib/string.md#0x1_string">string</a>.<a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>(), substr.<a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>());
    <b>if</b> (n &lt; m) <b>return</b> n;
    <b>while</b> (i &lt;= n - m) {
        <b>let</b> <b>mut</b> j = 0;
        <b>while</b> (j &lt; m && <a href="../move-stdlib/string.md#0x1_string">string</a>.bytes[i + j] == substr.bytes[j]) j = j + 1;
        <b>if</b> (j == m) <b>return</b> i;
        i = i + 1;
    };
    n
}
</code></pre>



</details>

<a name="0x1_ascii_char_to_uppercase"></a>

## Function `char_to_uppercase`

Convert a <code>char</code> to its lowercase equivalent.


<pre><code><b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_char_to_uppercase">char_to_uppercase</a>(byte: u8): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_char_to_uppercase">char_to_uppercase</a>(byte: u8): u8 {
    <b>if</b> (byte &gt;= 0x61 && <a href="../move-stdlib/ascii.md#0x1_ascii_byte">byte</a> &lt;= 0x7A) byte - 0x20
    <b>else</b> byte
}
</code></pre>



</details>

<a name="0x1_ascii_char_to_lowercase"></a>

## Function `char_to_lowercase`

Convert a <code>char</code> to its lowercase equivalent.


<pre><code><b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_char_to_lowercase">char_to_lowercase</a>(byte: u8): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_char_to_lowercase">char_to_lowercase</a>(byte: u8): u8 {
    <b>if</b> (byte &gt;= 0x41 && <a href="../move-stdlib/ascii.md#0x1_ascii_byte">byte</a> &lt;= 0x5A) byte + 0x20
    <b>else</b> byte
}
</code></pre>



</details>
