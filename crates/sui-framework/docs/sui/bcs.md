---
title: Module `sui::bcs`
---

This module implements BCS (de)serialization in Move.
Full specification can be found here: https://github.com/diem/bcs

Short summary (for Move-supported types):

- address - sequence of X bytes
- bool - byte with 0 or 1
- u8 - a single u8 byte
- u16 / u32 / u64 / u128 / u256 - LE bytes
- vector - ULEB128 length + LEN elements
- option - first byte bool: None (0) or Some (1), then value

Usage example:
```
/// This function reads u8 and u64 value from the input
/// and returns the rest of the bytes.
fun deserialize(bytes: vector<u8>): (u8, u64, vector<u8>) {
use sui::bcs::{Self, BCS};

let prepared: BCS = bcs::new(bytes);
let (u8_value, u64_value) = (
prepared.peel_u8(),
prepared.peel_u64()
);

// unpack bcs struct
let leftovers = prepared.into_remainder_bytes();

(u8_value, u64_value, leftovers)
}
```


-  [Struct `BCS`](#sui_bcs_BCS)
-  [Constants](#@Constants_0)
-  [Function `to_bytes`](#sui_bcs_to_bytes)
-  [Function `new`](#sui_bcs_new)
-  [Function `into_remainder_bytes`](#sui_bcs_into_remainder_bytes)
-  [Function `peel_address`](#sui_bcs_peel_address)
-  [Function `peel_bool`](#sui_bcs_peel_bool)
-  [Function `peel_u8`](#sui_bcs_peel_u8)
-  [Macro function `peel_num`](#sui_bcs_peel_num)
-  [Function `peel_u16`](#sui_bcs_peel_u16)
-  [Function `peel_u32`](#sui_bcs_peel_u32)
-  [Function `peel_u64`](#sui_bcs_peel_u64)
-  [Function `peel_u128`](#sui_bcs_peel_u128)
-  [Function `peel_u256`](#sui_bcs_peel_u256)
-  [Function `peel_vec_length`](#sui_bcs_peel_vec_length)
-  [Macro function `peel_vec`](#sui_bcs_peel_vec)
-  [Function `peel_vec_address`](#sui_bcs_peel_vec_address)
-  [Function `peel_vec_bool`](#sui_bcs_peel_vec_bool)
-  [Function `peel_vec_u8`](#sui_bcs_peel_vec_u8)
-  [Function `peel_vec_vec_u8`](#sui_bcs_peel_vec_vec_u8)
-  [Function `peel_vec_u16`](#sui_bcs_peel_vec_u16)
-  [Function `peel_vec_u32`](#sui_bcs_peel_vec_u32)
-  [Function `peel_vec_u64`](#sui_bcs_peel_vec_u64)
-  [Function `peel_vec_u128`](#sui_bcs_peel_vec_u128)
-  [Function `peel_vec_u256`](#sui_bcs_peel_vec_u256)
-  [Function `peel_enum_tag`](#sui_bcs_peel_enum_tag)
-  [Macro function `peel_option`](#sui_bcs_peel_option)
-  [Function `peel_option_address`](#sui_bcs_peel_option_address)
-  [Function `peel_option_bool`](#sui_bcs_peel_option_bool)
-  [Function `peel_option_u8`](#sui_bcs_peel_option_u8)
-  [Function `peel_option_u16`](#sui_bcs_peel_option_u16)
-  [Function `peel_option_u32`](#sui_bcs_peel_option_u32)
-  [Function `peel_option_u64`](#sui_bcs_peel_option_u64)
-  [Function `peel_option_u128`](#sui_bcs_peel_option_u128)
-  [Function `peel_option_u256`](#sui_bcs_peel_option_u256)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
</code></pre>



<a name="sui_bcs_BCS"></a>

## Struct `BCS`

A helper struct that saves resources on operations. For better
vector performance, it stores reversed bytes of the BCS and
enables use of <code>vector::pop_back</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>bytes: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_bcs_ELenOutOfRange"></a>

For when ULEB byte is out of range (or not found).


<pre><code><b>const</b> <a href="../sui/bcs.md#sui_bcs_ELenOutOfRange">ELenOutOfRange</a>: u64 = 2;
</code></pre>



<a name="sui_bcs_ENotBool"></a>

For when the boolean value different than <code>0</code> or <code>1</code>.


<pre><code><b>const</b> <a href="../sui/bcs.md#sui_bcs_ENotBool">ENotBool</a>: u64 = 1;
</code></pre>



<a name="sui_bcs_EOutOfRange"></a>

For when bytes length is less than required for deserialization.


<pre><code><b>const</b> <a href="../sui/bcs.md#sui_bcs_EOutOfRange">EOutOfRange</a>: u64 = 0;
</code></pre>



<a name="sui_bcs_to_bytes"></a>

## Function `to_bytes`

Get BCS serialized bytes for any value.
Re-exports stdlib <code><a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_to_bytes">to_bytes</a>&lt;T&gt;(value: &T): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_to_bytes">to_bytes</a>&lt;T&gt;(value: &T): vector&lt;u8&gt; {
    <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(value)
}
</code></pre>



</details>

<a name="sui_bcs_new"></a>

## Function `new`

Creates a new instance of BCS wrapper that holds inversed
bytes for better performance.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_new">new</a>(bytes: vector&lt;u8&gt;): <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_new">new</a>(<b>mut</b> bytes: vector&lt;u8&gt;): <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a> {
    bytes.reverse();
    <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a> { bytes }
}
</code></pre>



</details>

<a name="sui_bcs_into_remainder_bytes"></a>

## Function `into_remainder_bytes`

Unpack the <code><a href="../sui/bcs.md#sui_bcs_BCS">BCS</a></code> struct returning the leftover bytes.
Useful for passing the data further after partial deserialization.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_into_remainder_bytes">into_remainder_bytes</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_into_remainder_bytes">into_remainder_bytes</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): vector&lt;u8&gt; {
    <b>let</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a> { <b>mut</b> bytes } = <a href="../sui/bcs.md#sui_bcs">bcs</a>;
    bytes.reverse();
    bytes
}
</code></pre>



</details>

<a name="sui_bcs_peel_address"></a>

## Function `peel_address`

Read address from the bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_address">peel_address</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_address">peel_address</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): <b>address</b> {
    <b>assert</b>!(<a href="../sui/bcs.md#sui_bcs">bcs</a>.bytes.length() &gt;= <a href="../sui/address.md#sui_address_length">address::length</a>(), <a href="../sui/bcs.md#sui_bcs_EOutOfRange">EOutOfRange</a>);
    <b>let</b> (<b>mut</b> addr_bytes, <b>mut</b> i) = (vector[], 0);
    <b>while</b> (i &lt; <a href="../sui/address.md#sui_address_length">address::length</a>()) {
        addr_bytes.push_back(<a href="../sui/bcs.md#sui_bcs">bcs</a>.bytes.pop_back());
        i = i + 1;
    };
    <a href="../sui/address.md#sui_address_from_bytes">address::from_bytes</a>(addr_bytes)
}
</code></pre>



</details>

<a name="sui_bcs_peel_bool"></a>

## Function `peel_bool`

Read a <code>bool</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_bool">peel_bool</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_bool">peel_bool</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): bool {
    <b>let</b> value = <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u8">peel_u8</a>();
    <b>if</b> (value == 0) <b>false</b>
    <b>else</b> <b>if</b> (value == 1) <b>true</b>
    <b>else</b> <b>abort</b> <a href="../sui/bcs.md#sui_bcs_ENotBool">ENotBool</a>
}
</code></pre>



</details>

<a name="sui_bcs_peel_u8"></a>

## Function `peel_u8`

Read <code>u8</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u8">peel_u8</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u8">peel_u8</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): u8 {
    <b>assert</b>!(<a href="../sui/bcs.md#sui_bcs">bcs</a>.bytes.length() &gt;= 1, <a href="../sui/bcs.md#sui_bcs_EOutOfRange">EOutOfRange</a>);
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.bytes.pop_back()
}
</code></pre>



</details>

<a name="sui_bcs_peel_num"></a>

## Macro function `peel_num`



<pre><code><b>macro</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_num">peel_num</a>&lt;$I, $T&gt;($<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>, $len: u64, $bits: $I): $T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>macro</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_num">peel_num</a>&lt;$I, $T&gt;($<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>, $len: u64, $bits: $I): $T {
    <b>let</b> <a href="../sui/bcs.md#sui_bcs">bcs</a> = $<a href="../sui/bcs.md#sui_bcs">bcs</a>;
    <b>assert</b>!(<a href="../sui/bcs.md#sui_bcs">bcs</a>.bytes.length() &gt;= $len, <a href="../sui/bcs.md#sui_bcs_EOutOfRange">EOutOfRange</a>);
    <b>let</b> <b>mut</b> value: $T = 0;
    <b>let</b> <b>mut</b> i: $I = 0;
    <b>let</b> bits = $bits;
    <b>while</b> (i &lt; bits) {
        <b>let</b> byte = <a href="../sui/bcs.md#sui_bcs">bcs</a>.bytes.pop_back() <b>as</b> $T;
        value = value + (byte &lt;&lt; (i <b>as</b> u8));
        i = i + 8;
    };
    value
}
</code></pre>



</details>

<a name="sui_bcs_peel_u16"></a>

## Function `peel_u16`

Read <code>u16</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u16">peel_u16</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): u16
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u16">peel_u16</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): u16 {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_num">peel_num</a>!(2, 16u8)
}
</code></pre>



</details>

<a name="sui_bcs_peel_u32"></a>

## Function `peel_u32`

Read <code>u32</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u32">peel_u32</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): u32
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u32">peel_u32</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): u32 {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_num">peel_num</a>!(4, 32u8)
}
</code></pre>



</details>

<a name="sui_bcs_peel_u64"></a>

## Function `peel_u64`

Read <code>u64</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u64">peel_u64</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u64">peel_u64</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): u64 {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_num">peel_num</a>!(8, 64u8)
}
</code></pre>



</details>

<a name="sui_bcs_peel_u128"></a>

## Function `peel_u128`

Read <code>u128</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u128">peel_u128</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): u128
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u128">peel_u128</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): u128 {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_num">peel_num</a>!(16, 128u8)
}
</code></pre>



</details>

<a name="sui_bcs_peel_u256"></a>

## Function `peel_u256`

Read <code>u256</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u256">peel_u256</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_u256">peel_u256</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): u256 {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_num">peel_num</a>!(32, 256u16)
}
</code></pre>



</details>

<a name="sui_bcs_peel_vec_length"></a>

## Function `peel_vec_length`

Read ULEB bytes expecting a vector length. Result should
then be used to perform <code>peel_*</code> operation LEN times.

In BCS <code>vector</code> length is implemented with ULEB128;
See more here: https://en.wikipedia.org/wiki/LEB128


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_length">peel_vec_length</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_length">peel_vec_length</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): u64 {
    <b>let</b> (<b>mut</b> total, <b>mut</b> shift, <b>mut</b> len) = (0u64, 0, 0);
    <b>loop</b> {
        <b>assert</b>!(len &lt;= 4, <a href="../sui/bcs.md#sui_bcs_ELenOutOfRange">ELenOutOfRange</a>);
        <b>let</b> byte = <a href="../sui/bcs.md#sui_bcs">bcs</a>.bytes.pop_back() <b>as</b> u64;
        len = len + 1;
        total = total | ((byte & 0x7f) &lt;&lt; shift);
        <b>if</b> ((byte & 0x80) == 0) <b>break</b>;
        shift = shift + 7;
    };
    total
}
</code></pre>



</details>

<a name="sui_bcs_peel_vec"></a>

## Macro function `peel_vec`

Peel <code>vector&lt;$T&gt;</code> from serialized bytes, where <code>$peel: |&<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>| -&gt; $T</code> gives the
functionality of peeling each value.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec">peel_vec</a>&lt;$T&gt;($<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>, $peel: |&<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>| -&gt; $T): vector&lt;$T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec">peel_vec</a>&lt;$T&gt;($<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>, $peel: |&<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>| -&gt; $T): vector&lt;$T&gt; {
    <b>let</b> <a href="../sui/bcs.md#sui_bcs">bcs</a> = $<a href="../sui/bcs.md#sui_bcs">bcs</a>;
    <b>let</b> len = <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec_length">peel_vec_length</a>();
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> <b>mut</b> res = vector[];
    <b>while</b> (i &lt; len) {
        res.push_back($peel(<a href="../sui/bcs.md#sui_bcs">bcs</a>));
        i = i + 1;
    };
    res
}
</code></pre>



</details>

<a name="sui_bcs_peel_vec_address"></a>

## Function `peel_vec_address`

Peel a vector of <code><b>address</b></code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_address">peel_vec_address</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): vector&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_address">peel_vec_address</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): vector&lt;<b>address</b>&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec">peel_vec</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_address">peel_address</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_vec_bool"></a>

## Function `peel_vec_bool`

Peel a vector of <code><b>address</b></code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_bool">peel_vec_bool</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): vector&lt;bool&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_bool">peel_vec_bool</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): vector&lt;bool&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec">peel_vec</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_bool">peel_bool</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_vec_u8"></a>

## Function `peel_vec_u8`

Peel a vector of <code>u8</code> (eg string) from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u8">peel_vec_u8</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u8">peel_vec_u8</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): vector&lt;u8&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec">peel_vec</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u8">peel_u8</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_vec_vec_u8"></a>

## Function `peel_vec_vec_u8`

Peel a <code>vector&lt;vector&lt;u8&gt;&gt;</code> (eg vec of string) from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_vec_u8">peel_vec_vec_u8</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): vector&lt;vector&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_vec_u8">peel_vec_vec_u8</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): vector&lt;vector&lt;u8&gt;&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec">peel_vec</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec_u8">peel_vec_u8</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_vec_u16"></a>

## Function `peel_vec_u16`

Peel a vector of <code>u16</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u16">peel_vec_u16</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): vector&lt;u16&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u16">peel_vec_u16</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): vector&lt;u16&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec">peel_vec</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u16">peel_u16</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_vec_u32"></a>

## Function `peel_vec_u32`

Peel a vector of <code>u32</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u32">peel_vec_u32</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): vector&lt;u32&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u32">peel_vec_u32</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): vector&lt;u32&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec">peel_vec</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u32">peel_u32</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_vec_u64"></a>

## Function `peel_vec_u64`

Peel a vector of <code>u64</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u64">peel_vec_u64</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): vector&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u64">peel_vec_u64</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): vector&lt;u64&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec">peel_vec</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u64">peel_u64</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_vec_u128"></a>

## Function `peel_vec_u128`

Peel a vector of <code>u128</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u128">peel_vec_u128</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): vector&lt;u128&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u128">peel_vec_u128</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): vector&lt;u128&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec">peel_vec</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u128">peel_u128</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_vec_u256"></a>

## Function `peel_vec_u256`

Peel a vector of <code>u256</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u256">peel_vec_u256</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): vector&lt;u256&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_vec_u256">peel_vec_u256</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): vector&lt;u256&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec">peel_vec</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u256">peel_u256</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_enum_tag"></a>

## Function `peel_enum_tag`

Peel enum from serialized bytes, where <code>$f</code> takes a <code>tag</code> value and returns
the corresponding enum variant. Move enums are limited to 127 variants,
however the tag can be any <code>u32</code> value.

Example:
```rust
let my_enum = match (bcs.peel_enum_tag()) {
0 => Enum::Empty,
1 => Enum::U8(bcs.peel_u8()),
2 => Enum::U16(bcs.peel_u16()),
3 => Enum::Struct { a: bcs.peel_address(), b: bcs.peel_u8() },
_ => abort,
};
```


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_enum_tag">peel_enum_tag</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): u32
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_enum_tag">peel_enum_tag</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): u32 {
    <b>let</b> tag = <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_vec_length">peel_vec_length</a>();
    <b>assert</b>!(tag &lt;= <a href="../std/u32.md#std_u32_max_value">std::u32::max_value</a>!() <b>as</b> u64, <a href="../sui/bcs.md#sui_bcs_EOutOfRange">EOutOfRange</a>);
    tag <b>as</b> u32
}
</code></pre>



</details>

<a name="sui_bcs_peel_option"></a>

## Macro function `peel_option`

Peel <code>Option&lt;$T&gt;</code> from serialized bytes, where <code>$peel: |&<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>| -&gt; $T</code> gives the
functionality of peeling the inner value.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option">peel_option</a>&lt;$T&gt;($<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>, $peel: |&<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>| -&gt; $T): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option">peel_option</a>&lt;$T&gt;($<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>, $peel: |&<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>| -&gt; $T): Option&lt;$T&gt; {
    <b>let</b> <a href="../sui/bcs.md#sui_bcs">bcs</a> = $<a href="../sui/bcs.md#sui_bcs">bcs</a>;
    <b>if</b> (<a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_bool">peel_bool</a>()) option::some($peel(<a href="../sui/bcs.md#sui_bcs">bcs</a>))
    <b>else</b> option::none()
}
</code></pre>



</details>

<a name="sui_bcs_peel_option_address"></a>

## Function `peel_option_address`

Peel <code>Option&lt;<b>address</b>&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_address">peel_option_address</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_address">peel_option_address</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): Option&lt;<b>address</b>&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_option">peel_option</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_address">peel_address</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_option_bool"></a>

## Function `peel_option_bool`

Peel <code>Option&lt;bool&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_bool">peel_option_bool</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;bool&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_bool">peel_option_bool</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): Option&lt;bool&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_option">peel_option</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_bool">peel_bool</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_option_u8"></a>

## Function `peel_option_u8`

Peel <code>Option&lt;u8&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u8">peel_option_u8</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u8">peel_option_u8</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): Option&lt;u8&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_option">peel_option</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u8">peel_u8</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_option_u16"></a>

## Function `peel_option_u16`

Peel <code>Option&lt;u16&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u16">peel_option_u16</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u16&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u16">peel_option_u16</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): Option&lt;u16&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_option">peel_option</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u16">peel_u16</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_option_u32"></a>

## Function `peel_option_u32`

Peel <code>Option&lt;u32&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u32">peel_option_u32</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u32&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u32">peel_option_u32</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): Option&lt;u32&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_option">peel_option</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u32">peel_u32</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_option_u64"></a>

## Function `peel_option_u64`

Peel <code>Option&lt;u64&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u64">peel_option_u64</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u64">peel_option_u64</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): Option&lt;u64&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_option">peel_option</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u64">peel_u64</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_option_u128"></a>

## Function `peel_option_u128`

Peel <code>Option&lt;u128&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u128">peel_option_u128</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u128&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u128">peel_option_u128</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): Option&lt;u128&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_option">peel_option</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u128">peel_u128</a>())
}
</code></pre>



</details>

<a name="sui_bcs_peel_option_u256"></a>

## Function `peel_option_u256`

Peel <code>Option&lt;u256&gt;</code> from serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u256">peel_option_u256</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">sui::bcs::BCS</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u256&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bcs.md#sui_bcs_peel_option_u256">peel_option_u256</a>(<a href="../sui/bcs.md#sui_bcs">bcs</a>: &<b>mut</b> <a href="../sui/bcs.md#sui_bcs_BCS">BCS</a>): Option&lt;u256&gt; {
    <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_option">peel_option</a>!(|<a href="../sui/bcs.md#sui_bcs">bcs</a>| <a href="../sui/bcs.md#sui_bcs">bcs</a>.<a href="../sui/bcs.md#sui_bcs_peel_u256">peel_u256</a>())
}
</code></pre>



</details>
