---
title: Module `sui::address`
---



-  [Constants](#@Constants_0)
-  [Function `to_u256`](#sui_address_to_u256)
-  [Function `from_u256`](#sui_address_from_u256)
-  [Function `from_bytes`](#sui_address_from_bytes)
-  [Function `to_bytes`](#sui_address_to_bytes)
-  [Function `to_ascii_string`](#sui_address_to_ascii_string)
-  [Function `to_string`](#sui_address_to_string)
-  [Function `from_ascii_bytes`](#sui_address_from_ascii_bytes)
-  [Function `hex_char_value`](#sui_address_hex_char_value)
-  [Function `length`](#sui_address_length)
-  [Function `max`](#sui_address_max)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="sui_address_EAddressParseError"></a>

Error from <code><a href="../sui/address.md#sui_address_from_bytes">from_bytes</a></code> when it is supplied too many or too few bytes.


<pre><code><b>const</b> <a href="../sui/address.md#sui_address_EAddressParseError">EAddressParseError</a>: u64 = 0;
</code></pre>



<a name="sui_address_LENGTH"></a>

The length of an address, in bytes


<pre><code><b>const</b> <a href="../sui/address.md#sui_address_LENGTH">LENGTH</a>: u64 = 32;
</code></pre>



<a name="sui_address_MAX"></a>



<pre><code><b>const</b> <a href="../sui/address.md#sui_address_MAX">MAX</a>: u256 = 115792089237316195423570985008687907853269984665640564039457584007913129639935;
</code></pre>



<a name="sui_address_to_u256"></a>

## Function `to_u256`

Convert <code>a</code> into a u256 by interpreting <code>a</code> as the bytes of a big-endian integer
(e.g., <code><a href="../sui/address.md#sui_address_to_u256">to_u256</a>(0x1) == 1</code>)


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_to_u256">to_u256</a>(a: <b>address</b>): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../sui/address.md#sui_address_to_u256">to_u256</a>(a: <b>address</b>): u256;
</code></pre>



</details>

<a name="sui_address_from_u256"></a>

## Function `from_u256`

Convert <code>n</code> into an address by encoding it as a big-endian integer (e.g., <code><a href="../sui/address.md#sui_address_from_u256">from_u256</a>(1) = @0x1</code>)
Aborts if <code>n</code> > <code>MAX_ADDRESS</code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_from_u256">from_u256</a>(n: u256): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../sui/address.md#sui_address_from_u256">from_u256</a>(n: u256): <b>address</b>;
</code></pre>



</details>

<a name="sui_address_from_bytes"></a>

## Function `from_bytes`

Convert <code>bytes</code> into an address.
Aborts with <code><a href="../sui/address.md#sui_address_EAddressParseError">EAddressParseError</a></code> if the length of <code>bytes</code> is not 32


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_from_bytes">from_bytes</a>(bytes: vector&lt;u8&gt;): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../sui/address.md#sui_address_from_bytes">from_bytes</a>(bytes: vector&lt;u8&gt;): <b>address</b>;
</code></pre>



</details>

<a name="sui_address_to_bytes"></a>

## Function `to_bytes`

Convert <code>a</code> into BCS-encoded bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_to_bytes">to_bytes</a>(a: <b>address</b>): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_to_bytes">to_bytes</a>(a: <b>address</b>): vector&lt;u8&gt; {
    <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(&a)
}
</code></pre>



</details>

<a name="sui_address_to_ascii_string"></a>

## Function `to_ascii_string`

Convert <code>a</code> to a hex-encoded ASCII string


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_to_ascii_string">to_ascii_string</a>(a: <b>address</b>): <a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_to_ascii_string">to_ascii_string</a>(a: <b>address</b>): ascii::String {
    <a href="../sui/hex.md#sui_hex_encode">hex::encode</a>(<a href="../sui/address.md#sui_address_to_bytes">to_bytes</a>(a)).<a href="../sui/address.md#sui_address_to_ascii_string">to_ascii_string</a>()
}
</code></pre>



</details>

<a name="sui_address_to_string"></a>

## Function `to_string`

Convert <code>a</code> to a hex-encoded string


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_to_string">to_string</a>(a: <b>address</b>): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_to_string">to_string</a>(a: <b>address</b>): string::String {
    <a href="../sui/address.md#sui_address_to_ascii_string">to_ascii_string</a>(a).<a href="../sui/address.md#sui_address_to_string">to_string</a>()
}
</code></pre>



</details>

<a name="sui_address_from_ascii_bytes"></a>

## Function `from_ascii_bytes`

Converts an ASCII string to an address, taking the numerical value for each character. The
string must be Base16 encoded, and thus exactly 64 characters long.
For example, the string "00000000000000000000000000000000000000000000000000000000DEADB33F"
will be converted to the address @0xDEADB33F.
Aborts with <code><a href="../sui/address.md#sui_address_EAddressParseError">EAddressParseError</a></code> if the length of <code>s</code> is not 64,
or if an invalid character is encountered.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_from_ascii_bytes">from_ascii_bytes</a>(bytes: &vector&lt;u8&gt;): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_from_ascii_bytes">from_ascii_bytes</a>(bytes: &vector&lt;u8&gt;): <b>address</b> {
    <b>assert</b>!(bytes.<a href="../sui/address.md#sui_address_length">length</a>() == 64, <a href="../sui/address.md#sui_address_EAddressParseError">EAddressParseError</a>);
    <b>let</b> <b>mut</b> hex_bytes = vector[];
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; 64) {
        <b>let</b> hi = <a href="../sui/address.md#sui_address_hex_char_value">hex_char_value</a>(bytes[i]);
        <b>let</b> lo = <a href="../sui/address.md#sui_address_hex_char_value">hex_char_value</a>(bytes[i+1]);
        hex_bytes.push_back((hi &lt;&lt; 4) | lo);
        i = i + 2;
    };
    <a href="../sui/address.md#sui_address_from_bytes">from_bytes</a>(hex_bytes)
}
</code></pre>



</details>

<a name="sui_address_hex_char_value"></a>

## Function `hex_char_value`



<pre><code><b>fun</b> <a href="../sui/address.md#sui_address_hex_char_value">hex_char_value</a>(c: u8): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/address.md#sui_address_hex_char_value">hex_char_value</a>(c: u8): u8 {
    <b>if</b> (c &gt;= 48 && c &lt;= 57) c - 48 // 0-9
    <b>else</b> <b>if</b> (c &gt;= 65 && c &lt;= 70) c - 55 // A-F
    <b>else</b> <b>if</b> (c &gt;= 97 && c &lt;= 102) c - 87 // a-f
    <b>else</b> <b>abort</b> <a href="../sui/address.md#sui_address_EAddressParseError">EAddressParseError</a>
}
</code></pre>



</details>

<a name="sui_address_length"></a>

## Function `length`

Length of a Sui address in bytes


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_length">length</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_length">length</a>(): u64 {
    <a href="../sui/address.md#sui_address_LENGTH">LENGTH</a>
}
</code></pre>



</details>

<a name="sui_address_max"></a>

## Function `max`

Largest possible address


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_max">max</a>(): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/address.md#sui_address_max">max</a>(): u256 {
    <a href="../sui/address.md#sui_address_MAX">MAX</a>
}
</code></pre>



</details>
