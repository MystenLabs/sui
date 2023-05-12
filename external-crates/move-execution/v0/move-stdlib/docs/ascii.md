
<a name="0x1_ascii"></a>

# Module `0x1::ascii`

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
-  [Function `as_bytes`](#0x1_ascii_as_bytes)
-  [Function `into_bytes`](#0x1_ascii_into_bytes)
-  [Function `byte`](#0x1_ascii_byte)
-  [Function `is_valid_char`](#0x1_ascii_is_valid_char)
-  [Function `is_printable_char`](#0x1_ascii_is_printable_char)


<pre><code><b>use</b> <a href="option.md#0x1_option">0x1::option</a>;
</code></pre>



<a name="0x1_ascii_String"></a>

## Struct `String`

The <code><a href="ascii.md#0x1_ascii_String">String</a></code> struct holds a vector of bytes that all represent
valid ASCII characters. Note that these ASCII characters may not all
be printable. To determine if a <code><a href="ascii.md#0x1_ascii_String">String</a></code> contains only "printable"
characters you should use the <code>all_characters_printable</code> predicate
defined in this module.


<pre><code><b>struct</b> <a href="ascii.md#0x1_ascii_String">String</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>bytes: <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<details>
<summary>Specification</summary>



<pre><code><b>invariant</b> <b>forall</b> i in 0..len(bytes): <a href="ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(bytes[i]);
</code></pre>



</details>

<a name="0x1_ascii_Char"></a>

## Struct `Char`

An ASCII character.


<pre><code><b>struct</b> <a href="ascii.md#0x1_ascii_Char">Char</a> <b>has</b> <b>copy</b>, drop, store
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

<details>
<summary>Specification</summary>



<pre><code><b>invariant</b> <a href="ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(byte);
</code></pre>



</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x1_ascii_EINVALID_ASCII_CHARACTER"></a>

An invalid ASCII character was encountered when creating an ASCII string.


<pre><code><b>const</b> <a href="ascii.md#0x1_ascii_EINVALID_ASCII_CHARACTER">EINVALID_ASCII_CHARACTER</a>: u64 = 65536;
</code></pre>



<a name="0x1_ascii_char"></a>

## Function `char`

Convert a <code>byte</code> into a <code><a href="ascii.md#0x1_ascii_Char">Char</a></code> that is checked to make sure it is valid ASCII.


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_char">char</a>(byte: u8): <a href="ascii.md#0x1_ascii_Char">ascii::Char</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_char">char</a>(byte: u8): <a href="ascii.md#0x1_ascii_Char">Char</a> {
    <b>assert</b>!(<a href="ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(byte), <a href="ascii.md#0x1_ascii_EINVALID_ASCII_CHARACTER">EINVALID_ASCII_CHARACTER</a>);
    <a href="ascii.md#0x1_ascii_Char">Char</a> { byte }
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>aborts_if</b> !<a href="ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(byte) <b>with</b> <a href="ascii.md#0x1_ascii_EINVALID_ASCII_CHARACTER">EINVALID_ASCII_CHARACTER</a>;
</code></pre>



</details>

<a name="0x1_ascii_string"></a>

## Function `string`

Convert a vector of bytes <code>bytes</code> into an <code><a href="ascii.md#0x1_ascii_String">String</a></code>. Aborts if
<code>bytes</code> contains non-ASCII characters.


<pre><code><b>public</b> <b>fun</b> <a href="string.md#0x1_string">string</a>(bytes: <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="string.md#0x1_string">string</a>(bytes: <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="ascii.md#0x1_ascii_String">String</a> {
   <b>let</b> x = <a href="ascii.md#0x1_ascii_try_string">try_string</a>(bytes);
   <b>assert</b>!(
        <a href="option.md#0x1_option_is_some">option::is_some</a>(&x),
        <a href="ascii.md#0x1_ascii_EINVALID_ASCII_CHARACTER">EINVALID_ASCII_CHARACTER</a>
   );
   <a href="option.md#0x1_option_destroy_some">option::destroy_some</a>(x)
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>aborts_if</b> <b>exists</b> i in 0..len(bytes): !<a href="ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(bytes[i]) <b>with</b> <a href="ascii.md#0x1_ascii_EINVALID_ASCII_CHARACTER">EINVALID_ASCII_CHARACTER</a>;
</code></pre>



</details>

<a name="0x1_ascii_try_string"></a>

## Function `try_string`

Convert a vector of bytes <code>bytes</code> into an <code><a href="ascii.md#0x1_ascii_String">String</a></code>. Returns
<code>Some(&lt;ascii_string&gt;)</code> if the <code>bytes</code> contains all valid ASCII
characters. Otherwise returns <code>None</code>.


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_try_string">try_string</a>(bytes: <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="option.md#0x1_option_Option">option::Option</a>&lt;<a href="ascii.md#0x1_ascii_String">ascii::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_try_string">try_string</a>(bytes: <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;): Option&lt;<a href="ascii.md#0x1_ascii_String">String</a>&gt; {
   <b>let</b> len = <a href="vector.md#0x1_vector_length">vector::length</a>(&bytes);
   <b>let</b> i = 0;
   <b>while</b> ({
       <b>spec</b> {
           <b>invariant</b> i &lt;= len;
           <b>invariant</b> <b>forall</b> j in 0..i: <a href="ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(bytes[j]);
       };
       i &lt; len
   }) {
       <b>let</b> possible_byte = *<a href="vector.md#0x1_vector_borrow">vector::borrow</a>(&bytes, i);
       <b>if</b> (!<a href="ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(possible_byte)) <b>return</b> <a href="option.md#0x1_option_none">option::none</a>();
       i = i + 1;
   };
   <b>spec</b> {
       <b>assert</b> i == len;
       <b>assert</b> <b>forall</b> j in 0..len: <a href="ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(bytes[j]);
   };
   <a href="option.md#0x1_option_some">option::some</a>(<a href="ascii.md#0x1_ascii_String">String</a> { bytes })
}
</code></pre>



</details>

<a name="0x1_ascii_all_characters_printable"></a>

## Function `all_characters_printable`

Returns <code><b>true</b></code> if all characters in <code><a href="string.md#0x1_string">string</a></code> are printable characters
Returns <code><b>false</b></code> otherwise. Not all <code><a href="ascii.md#0x1_ascii_String">String</a></code>s are printable strings.


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_all_characters_printable">all_characters_printable</a>(<a href="string.md#0x1_string">string</a>: &<a href="ascii.md#0x1_ascii_String">ascii::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_all_characters_printable">all_characters_printable</a>(<a href="string.md#0x1_string">string</a>: &<a href="ascii.md#0x1_ascii_String">String</a>): bool {
   <b>let</b> len = <a href="vector.md#0x1_vector_length">vector::length</a>(&<a href="string.md#0x1_string">string</a>.bytes);
   <b>let</b> i = 0;
   <b>while</b> ({
       <b>spec</b> {
           <b>invariant</b> i &lt;= len;
           <b>invariant</b> <b>forall</b> j in 0..i: <a href="ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(<a href="string.md#0x1_string">string</a>.bytes[j]);
       };
       i &lt; len
   }) {
       <b>let</b> byte = *<a href="vector.md#0x1_vector_borrow">vector::borrow</a>(&<a href="string.md#0x1_string">string</a>.bytes, i);
       <b>if</b> (!<a href="ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(byte)) <b>return</b> <b>false</b>;
       i = i + 1;
   };
   <b>spec</b> {
       <b>assert</b> i == len;
       <b>assert</b> <b>forall</b> j in 0..len: <a href="ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(<a href="string.md#0x1_string">string</a>.bytes[j]);
   };
   <b>true</b>
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>ensures</b> result ==&gt; (<b>forall</b> j in 0..len(<a href="string.md#0x1_string">string</a>.bytes): <a href="ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(<a href="string.md#0x1_string">string</a>.bytes[j]));
</code></pre>



</details>

<a name="0x1_ascii_push_char"></a>

## Function `push_char`



<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_push_char">push_char</a>(<a href="string.md#0x1_string">string</a>: &<b>mut</b> <a href="ascii.md#0x1_ascii_String">ascii::String</a>, char: <a href="ascii.md#0x1_ascii_Char">ascii::Char</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_push_char">push_char</a>(<a href="string.md#0x1_string">string</a>: &<b>mut</b> <a href="ascii.md#0x1_ascii_String">String</a>, char: <a href="ascii.md#0x1_ascii_Char">Char</a>) {
    <a href="vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> <a href="string.md#0x1_string">string</a>.bytes, char.byte);
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>ensures</b> len(<a href="string.md#0x1_string">string</a>.bytes) == len(<b>old</b>(<a href="string.md#0x1_string">string</a>.bytes)) + 1;
</code></pre>



</details>

<a name="0x1_ascii_pop_char"></a>

## Function `pop_char`



<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_pop_char">pop_char</a>(<a href="string.md#0x1_string">string</a>: &<b>mut</b> <a href="ascii.md#0x1_ascii_String">ascii::String</a>): <a href="ascii.md#0x1_ascii_Char">ascii::Char</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_pop_char">pop_char</a>(<a href="string.md#0x1_string">string</a>: &<b>mut</b> <a href="ascii.md#0x1_ascii_String">String</a>): <a href="ascii.md#0x1_ascii_Char">Char</a> {
    <a href="ascii.md#0x1_ascii_Char">Char</a> { byte: <a href="vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> <a href="string.md#0x1_string">string</a>.bytes) }
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>ensures</b> len(<a href="string.md#0x1_string">string</a>.bytes) == len(<b>old</b>(<a href="string.md#0x1_string">string</a>.bytes)) - 1;
</code></pre>



</details>

<a name="0x1_ascii_length"></a>

## Function `length`



<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_length">length</a>(<a href="string.md#0x1_string">string</a>: &<a href="ascii.md#0x1_ascii_String">ascii::String</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_length">length</a>(<a href="string.md#0x1_string">string</a>: &<a href="ascii.md#0x1_ascii_String">String</a>): u64 {
    <a href="vector.md#0x1_vector_length">vector::length</a>(<a href="ascii.md#0x1_ascii_as_bytes">as_bytes</a>(<a href="string.md#0x1_string">string</a>))
}
</code></pre>



</details>

<a name="0x1_ascii_as_bytes"></a>

## Function `as_bytes`

Get the inner bytes of the <code><a href="string.md#0x1_string">string</a></code> as a reference


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_as_bytes">as_bytes</a>(<a href="string.md#0x1_string">string</a>: &<a href="ascii.md#0x1_ascii_String">ascii::String</a>): &<a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_as_bytes">as_bytes</a>(<a href="string.md#0x1_string">string</a>: &<a href="ascii.md#0x1_ascii_String">String</a>): &<a href="vector.md#0x1_vector">vector</a>&lt;u8&gt; {
   &<a href="string.md#0x1_string">string</a>.bytes
}
</code></pre>



</details>

<a name="0x1_ascii_into_bytes"></a>

## Function `into_bytes`

Unpack the <code><a href="string.md#0x1_string">string</a></code> to get its backing bytes


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_into_bytes">into_bytes</a>(<a href="string.md#0x1_string">string</a>: <a href="ascii.md#0x1_ascii_String">ascii::String</a>): <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_into_bytes">into_bytes</a>(<a href="string.md#0x1_string">string</a>: <a href="ascii.md#0x1_ascii_String">String</a>): <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt; {
   <b>let</b> <a href="ascii.md#0x1_ascii_String">String</a> { bytes } = <a href="string.md#0x1_string">string</a>;
   bytes
}
</code></pre>



</details>

<a name="0x1_ascii_byte"></a>

## Function `byte`

Unpack the <code>char</code> into its underlying byte.


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_byte">byte</a>(char: <a href="ascii.md#0x1_ascii_Char">ascii::Char</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_byte">byte</a>(char: <a href="ascii.md#0x1_ascii_Char">Char</a>): u8 {
   <b>let</b> <a href="ascii.md#0x1_ascii_Char">Char</a> { byte } = char;
   byte
}
</code></pre>



</details>

<a name="0x1_ascii_is_valid_char"></a>

## Function `is_valid_char`

Returns <code><b>true</b></code> if <code>b</code> is a valid ASCII character. Returns <code><b>false</b></code> otherwise.


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(b: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_is_valid_char">is_valid_char</a>(b: u8): bool {
   b &lt;= 0x7F
}
</code></pre>



</details>

<a name="0x1_ascii_is_printable_char"></a>

## Function `is_printable_char`

Returns <code><b>true</b></code> if <code>byte</code> is an printable ASCII character. Returns <code><b>false</b></code> otherwise.


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(byte: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ascii.md#0x1_ascii_is_printable_char">is_printable_char</a>(byte: u8): bool {
   byte &gt;= 0x20 && // Disallow metacharacters
   <a href="ascii.md#0x1_ascii_byte">byte</a> &lt;= 0x7E // Don't allow DEL metacharacter
}
</code></pre>



</details>


[//]: # ("File containing references which can be used from documentation")
