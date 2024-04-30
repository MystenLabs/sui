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
-  [Function `sub_string`](#0x1_ascii_sub_string)
-  [Function `as_bytes`](#0x1_ascii_as_bytes)
-  [Function `into_bytes`](#0x1_ascii_into_bytes)
-  [Function `byte`](#0x1_ascii_byte)
-  [Function `is_valid_char`](#0x1_ascii_is_valid_char)
-  [Function `is_printable_char`](#0x1_ascii_is_printable_char)
-  [Function `is_empty`](#0x1_ascii_is_empty)
-  [Function `is_alphanumeric`](#0x1_ascii_is_alphanumeric)


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


<pre><code><b>const</b> <a href="../move-stdlib/ascii.md#0x1_ascii_EInvalidASCIICharacter">EInvalidASCIICharacter</a>: u64 = 65536;
</code></pre>



<a name="0x1_ascii_EInvalidIndex"></a>

An invalid index was encountered when creating a substring.


<pre><code><b>const</b> <a href="../move-stdlib/ascii.md#0x1_ascii_EInvalidIndex">EInvalidIndex</a>: u64 = 65537;
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
    <b>let</b> len = bytes.<a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> possible_byte = bytes[i];
        <b>if</b> (!<a href="../move-stdlib/ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(possible_byte)) <b>return</b> <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>();
        i = i + 1;
    };
    <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> { bytes })
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
    <b>let</b> len = <a href="../move-stdlib/string.md#0x1_string">string</a>.bytes.<a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> byte = <a href="../move-stdlib/string.md#0x1_string">string</a>.bytes[i];
        <b>if</b> (!<a href="../move-stdlib/ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(byte)) <b>return</b> <b>false</b>;
        i = i + 1;
    };
    <b>true</b>
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


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>): u64 {
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
    <a href="../move-stdlib/string.md#0x1_string">string</a>.bytes.<a href="../move-stdlib/ascii.md#0x1_ascii_append">append</a>(other.bytes)
}
</code></pre>



</details>

<a name="0x1_ascii_insert"></a>

## Function `insert`

Insert the <code>other</code> string at the <code>at</code> index of <code><a href="../move-stdlib/string.md#0x1_string">string</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_insert">insert</a>(s: &<b>mut</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>, at: u64, o: <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_insert">insert</a>(s: &<b>mut</b> <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>, at: u64, o: <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>) {
    <b>assert</b>!(at &lt;= s.<a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>(), <a href="../move-stdlib/ascii.md#0x1_ascii_EInvalidIndex">EInvalidIndex</a>);
    <b>let</b> l = s.<a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>();
    <b>let</b> <b>mut</b> front = s.<a href="../move-stdlib/ascii.md#0x1_ascii_sub_string">sub_string</a>(0, at);
    <b>let</b> end = s.<a href="../move-stdlib/ascii.md#0x1_ascii_sub_string">sub_string</a>(at, l);
    front.<a href="../move-stdlib/ascii.md#0x1_ascii_append">append</a>(o);
    front.<a href="../move-stdlib/ascii.md#0x1_ascii_append">append</a>(end);
    *s = front;
}
</code></pre>



</details>

<a name="0x1_ascii_sub_string"></a>

## Function `sub_string`

Copy the slice of the <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> from <code>i</code> to <code>j</code> into a new <code><a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_sub_string">sub_string</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>, i: u64, j: u64): <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_sub_string">sub_string</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>, <b>mut</b> i: u64, j: u64): <a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a> {
    <b>assert</b>!(i &lt;= j && j &lt;= <a href="../move-stdlib/string.md#0x1_string">string</a>.<a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>(), <a href="../move-stdlib/ascii.md#0x1_ascii_EInvalidIndex">EInvalidIndex</a>);
    <b>let</b> <b>mut</b> bytes = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>while</b> (i &lt; j) {
        bytes.push_back(<a href="../move-stdlib/string.md#0x1_string">string</a>.bytes[i]);
        i = i + 1;
    };
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

<a name="0x1_ascii_is_alphanumeric"></a>

## Function `is_alphanumeric`

Returns <code><b>true</b></code> if <code><a href="../move-stdlib/string.md#0x1_string">string</a></code> is an alphanumeric ASCII string.
Returns <code><b>false</b></code> otherwise.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_is_alphanumeric">is_alphanumeric</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/ascii.md#0x1_ascii_is_alphanumeric">is_alphanumeric</a>(<a href="../move-stdlib/string.md#0x1_string">string</a>: &<a href="../move-stdlib/ascii.md#0x1_ascii_String">String</a>): bool {
    <b>let</b> (<b>mut</b> i, len) = (0, <a href="../move-stdlib/string.md#0x1_string">string</a>.<a href="../move-stdlib/ascii.md#0x1_ascii_length">length</a>());
    <b>while</b> (i &lt; len) {
        <b>let</b> byte = <a href="../move-stdlib/string.md#0x1_string">string</a>.bytes[i];
        <b>let</b> is_alphanumeric =
            (0x41 &lt;= byte && <a href="../move-stdlib/ascii.md#0x1_ascii_byte">byte</a> &lt;= 0x5A) || // A-Z
            (0x61 &lt;= byte && <a href="../move-stdlib/ascii.md#0x1_ascii_byte">byte</a> &lt;= 0x7A) || // a-z
            (0x30 &lt;= byte && <a href="../move-stdlib/ascii.md#0x1_ascii_byte">byte</a> &lt;= 0x39); // 0-9
        <b>if</b> (!is_alphanumeric) <b>return</b> <b>false</b>;
        i = i + 1;
    };

    <b>true</b>
}
</code></pre>



</details>
