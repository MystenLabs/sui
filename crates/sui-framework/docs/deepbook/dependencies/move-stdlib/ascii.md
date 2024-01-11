
<a name="0x1_ascii"></a>

# Module `0x1::ascii`



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
-  [Function `as_bytes`](#0x1_ascii_as_bytes)
-  [Function `into_bytes`](#0x1_ascii_into_bytes)
-  [Function `byte`](#0x1_ascii_byte)
-  [Function `is_valid_char`](#0x1_ascii_is_valid_char)
-  [Function `is_printable_char`](#0x1_ascii_is_printable_char)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
</code></pre>



<a name="0x1_ascii_String"></a>

## Struct `String`



<pre><code><b>struct</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">String</a> <b>has</b> <b>copy</b>, drop, store
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

<a name="0x1_ascii_Char"></a>

## Struct `Char`



<pre><code><b>struct</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">Char</a> <b>has</b> <b>copy</b>, drop, store
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


<a name="0x1_ascii_EINVALID_ASCII_CHARACTER"></a>



<pre><code><b>const</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_EINVALID_ASCII_CHARACTER">EINVALID_ASCII_CHARACTER</a>: u64 = 65536;
</code></pre>



<a name="0x1_ascii_char"></a>

## Function `char`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_char">char</a>(byte: u8): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">ascii::Char</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_char">char</a>(byte: u8): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">Char</a> {
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(byte), <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_EINVALID_ASCII_CHARACTER">EINVALID_ASCII_CHARACTER</a>);
    <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">Char</a> { byte }
}
</code></pre>



</details>

<a name="0x1_ascii_string"></a>

## Function `string`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">String</a> {
   <b>let</b> x = <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_try_string">try_string</a>(bytes);
   <b>assert</b>!(
        <a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&x),
        <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_EINVALID_ASCII_CHARACTER">EINVALID_ASCII_CHARACTER</a>
   );
   <a href="../../dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(x)
}
</code></pre>



</details>

<a name="0x1_ascii_try_string"></a>

## Function `try_string`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_try_string">try_string</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_try_string">try_string</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): Option&lt;<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">String</a>&gt; {
    <b>let</b> len = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&bytes);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> possible_byte = *<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&bytes, i);
        <b>if</b> (!<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(possible_byte)) <b>return</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>();
        i = i + 1;
    };
    <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">String</a> { bytes })
}
</code></pre>



</details>

<a name="0x1_ascii_all_characters_printable"></a>

## Function `all_characters_printable`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_all_characters_printable">all_characters_printable</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: &<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_all_characters_printable">all_characters_printable</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: &<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">String</a>): bool {
    <b>let</b> len = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(&<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>.bytes);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> byte = *<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>.bytes, i);
        <b>if</b> (!<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(byte)) <b>return</b> <b>false</b>;
        i = i + 1;
    };
    <b>true</b>
}
</code></pre>



</details>

<a name="0x1_ascii_push_char"></a>

## Function `push_char`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_push_char">push_char</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: &<b>mut</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>, char: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">ascii::Char</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_push_char">push_char</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: &<b>mut</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">String</a>, char: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">Char</a>) {
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>.bytes, char.byte);
}
</code></pre>



</details>

<a name="0x1_ascii_pop_char"></a>

## Function `pop_char`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_pop_char">pop_char</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: &<b>mut</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">ascii::Char</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_pop_char">pop_char</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: &<b>mut</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">String</a>): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">Char</a> {
    <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">Char</a> { byte: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> <a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>.bytes) }
}
</code></pre>



</details>

<a name="0x1_ascii_length"></a>

## Function `length`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_length">length</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: &<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_length">length</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: &<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">String</a>): u64 {
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_as_bytes">as_bytes</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>))
}
</code></pre>



</details>

<a name="0x1_ascii_as_bytes"></a>

## Function `as_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_as_bytes">as_bytes</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: &<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_as_bytes">as_bytes</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: &<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">String</a>): &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
   &<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>.bytes
}
</code></pre>



</details>

<a name="0x1_ascii_into_bytes"></a>

## Function `into_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_into_bytes">into_bytes</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_into_bytes">into_bytes</a>(<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">String</a>): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
   <b>let</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">String</a> { bytes } = <a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>;
   bytes
}
</code></pre>



</details>

<a name="0x1_ascii_byte"></a>

## Function `byte`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_byte">byte</a>(char: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">ascii::Char</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_byte">byte</a>(char: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">Char</a>): u8 {
   <b>let</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_Char">Char</a> { byte } = char;
   byte
}
</code></pre>



</details>

<a name="0x1_ascii_is_valid_char"></a>

## Function `is_valid_char`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(b: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(b: u8): bool {
   b &lt;= 0x7F
}
</code></pre>



</details>

<a name="0x1_ascii_is_printable_char"></a>

## Function `is_printable_char`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(byte: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(byte: u8): bool {
   byte &gt;= 0x20 && // Disallow metacharacters
   <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_byte">byte</a> &lt;= 0x7E // Don't allow DEL metacharacter
}
</code></pre>



</details>
