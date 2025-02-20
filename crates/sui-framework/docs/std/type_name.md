---
title: Module `std::type_name`
---

Functionality for converting Move types into values. Use with care!


-  [Struct `TypeName`](#std_type_name_TypeName)
-  [Constants](#@Constants_0)
-  [Function `get`](#std_type_name_get)
-  [Function `get_with_original_ids`](#std_type_name_get_with_original_ids)
-  [Function `is_primitive`](#std_type_name_is_primitive)
-  [Function `borrow_string`](#std_type_name_borrow_string)
-  [Function `get_address`](#std_type_name_get_address)
-  [Function `get_module`](#std_type_name_get_module)
-  [Function `into_string`](#std_type_name_into_string)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
</code></pre>



<a name="std_type_name_TypeName"></a>

## Struct `TypeName`



<pre><code><b>public</b> <b>struct</b> <a href="../std/type_name.md#std_type_name_TypeName">TypeName</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>name: <a href="../std/ascii.md#std_ascii_String">std::ascii::String</a></code>
</dt>
<dd>
 String representation of the type. All types are represented
 using their source syntax:
 "u8", "u64", "bool", "address", "vector", and so on for primitive types.
 Struct types are represented as fully qualified type names; e.g.
 <code>00000000000000000000000000000001::string::String</code> or
 <code>0000000000000000000000000000000a::module_name1::type_name1&lt;0000000000000000000000000000000a::module_name2::type_name2&lt;<a href="../std/u64.md#std_u64">u64</a>&gt;&gt;</code>
 Addresses are hex-encoded lowercase values of length ADDRESS_LENGTH (16, 20, or 32 depending on the Move platform)
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="std_type_name_ASCII_C"></a>

ASCII Character code for the <code>c</code> (lowercase c) symbol.


<pre><code><b>const</b> <a href="../std/type_name.md#std_type_name_ASCII_C">ASCII_C</a>: <a href="../std/u8.md#std_u8">u8</a> = 99;
</code></pre>



<a name="std_type_name_ASCII_COLON"></a>

ASCII Character code for the <code>:</code> (colon) symbol.


<pre><code><b>const</b> <a href="../std/type_name.md#std_type_name_ASCII_COLON">ASCII_COLON</a>: <a href="../std/u8.md#std_u8">u8</a> = 58;
</code></pre>



<a name="std_type_name_ASCII_E"></a>

ASCII Character code for the <code>e</code> (lowercase e) symbol.


<pre><code><b>const</b> <a href="../std/type_name.md#std_type_name_ASCII_E">ASCII_E</a>: <a href="../std/u8.md#std_u8">u8</a> = 101;
</code></pre>



<a name="std_type_name_ASCII_O"></a>

ASCII Character code for the <code>o</code> (lowercase o) symbol.


<pre><code><b>const</b> <a href="../std/type_name.md#std_type_name_ASCII_O">ASCII_O</a>: <a href="../std/u8.md#std_u8">u8</a> = 111;
</code></pre>



<a name="std_type_name_ASCII_R"></a>

ASCII Character code for the <code>r</code> (lowercase r) symbol.


<pre><code><b>const</b> <a href="../std/type_name.md#std_type_name_ASCII_R">ASCII_R</a>: <a href="../std/u8.md#std_u8">u8</a> = 114;
</code></pre>



<a name="std_type_name_ASCII_T"></a>

ASCII Character code for the <code>t</code> (lowercase t) symbol.


<pre><code><b>const</b> <a href="../std/type_name.md#std_type_name_ASCII_T">ASCII_T</a>: <a href="../std/u8.md#std_u8">u8</a> = 116;
</code></pre>



<a name="std_type_name_ASCII_V"></a>

ASCII Character code for the <code>v</code> (lowercase v) symbol.


<pre><code><b>const</b> <a href="../std/type_name.md#std_type_name_ASCII_V">ASCII_V</a>: <a href="../std/u8.md#std_u8">u8</a> = 118;
</code></pre>



<a name="std_type_name_ENonModuleType"></a>

The type is not from a package/module. It is a primitive type.


<pre><code><b>const</b> <a href="../std/type_name.md#std_type_name_ENonModuleType">ENonModuleType</a>: <a href="../std/u64.md#std_u64">u64</a> = 0;
</code></pre>



<a name="std_type_name_get"></a>

## Function `get`

Return a value representation of the type <code>T</code>.  Package IDs
that appear in fully qualified type names in the output from
this function are defining IDs (the ID of the package in
storage that first introduced the type).


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_get">get</a>&lt;T&gt;(): <a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_get">get</a>&lt;T&gt;(): <a href="../std/type_name.md#std_type_name_TypeName">TypeName</a>;
</code></pre>



</details>

<a name="std_type_name_get_with_original_ids"></a>

## Function `get_with_original_ids`

Return a value representation of the type <code>T</code>.  Package IDs
that appear in fully qualified type names in the output from
this function are original IDs (the ID of the first version of
the package, even if the type in question was introduced in a
later upgrade).


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_get_with_original_ids">get_with_original_ids</a>&lt;T&gt;(): <a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_get_with_original_ids">get_with_original_ids</a>&lt;T&gt;(): <a href="../std/type_name.md#std_type_name_TypeName">TypeName</a>;
</code></pre>



</details>

<a name="std_type_name_is_primitive"></a>

## Function `is_primitive`

Returns true iff the TypeName represents a primitive type, i.e. one of
u8, u16, u32, u64, u128, u256, bool, address, vector.


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_is_primitive">is_primitive</a>(self: &<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_is_primitive">is_primitive</a>(self: &<a href="../std/type_name.md#std_type_name_TypeName">TypeName</a>): bool {
    <b>let</b> bytes = self.name.as_bytes();
    bytes == &b"bool" ||
        bytes == &b"<a href="../std/u8.md#std_u8">u8</a>" ||
        bytes == &b"<a href="../std/u16.md#std_u16">u16</a>" ||
        bytes == &b"<a href="../std/u32.md#std_u32">u32</a>" ||
        bytes == &b"<a href="../std/u64.md#std_u64">u64</a>" ||
        bytes == &b"<a href="../std/u128.md#std_u128">u128</a>" ||
        bytes == &b"<a href="../std/u256.md#std_u256">u256</a>" ||
        bytes == &b"<b>address</b>" ||
        (
            bytes.length() &gt;= 6 &&
            bytes[0] == <a href="../std/type_name.md#std_type_name_ASCII_V">ASCII_V</a> &&
            bytes[1] == <a href="../std/type_name.md#std_type_name_ASCII_E">ASCII_E</a> &&
            bytes[2] == <a href="../std/type_name.md#std_type_name_ASCII_C">ASCII_C</a> &&
            bytes[3] == <a href="../std/type_name.md#std_type_name_ASCII_T">ASCII_T</a> &&
            bytes[4] == <a href="../std/type_name.md#std_type_name_ASCII_O">ASCII_O</a> &&
            bytes[5] == <a href="../std/type_name.md#std_type_name_ASCII_R">ASCII_R</a>,
        )
}
</code></pre>



</details>

<a name="std_type_name_borrow_string"></a>

## Function `borrow_string`

Get the String representation of <code>self</code>


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_borrow_string">borrow_string</a>(self: &<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>): &<a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_borrow_string">borrow_string</a>(self: &<a href="../std/type_name.md#std_type_name_TypeName">TypeName</a>): &String {
    &self.name
}
</code></pre>



</details>

<a name="std_type_name_get_address"></a>

## Function `get_address`

Get Address string (Base16 encoded), first part of the TypeName.
Aborts if given a primitive type.


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_get_address">get_address</a>(self: &<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>): <a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_get_address">get_address</a>(self: &<a href="../std/type_name.md#std_type_name_TypeName">TypeName</a>): String {
    <b>assert</b>!(!self.<a href="../std/type_name.md#std_type_name_is_primitive">is_primitive</a>(), <a href="../std/type_name.md#std_type_name_ENonModuleType">ENonModuleType</a>);
    // Base16 (<a href="../std/string.md#std_string">string</a>) representation of an <b>address</b> <b>has</b> 2 symbols per byte.
    <b>let</b> len = <a href="../std/address.md#std_address_length">address::length</a>() * 2;
    <b>let</b> str_bytes = self.name.as_bytes();
    <b>let</b> <b>mut</b> addr_bytes = <a href="../std/vector.md#std_vector">vector</a>[];
    <b>let</b> <b>mut</b> i = 0;
    // Read `len` bytes from the type name and push them to addr_bytes.
    <b>while</b> (i &lt; len) {
        addr_bytes.push_back(str_bytes[i]);
        i = i + 1;
    };
    <a href="../std/ascii.md#std_ascii_string">ascii::string</a>(addr_bytes)
}
</code></pre>



</details>

<a name="std_type_name_get_module"></a>

## Function `get_module`

Get name of the module.
Aborts if given a primitive type.


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_get_module">get_module</a>(self: &<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>): <a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_get_module">get_module</a>(self: &<a href="../std/type_name.md#std_type_name_TypeName">TypeName</a>): String {
    <b>assert</b>!(!self.<a href="../std/type_name.md#std_type_name_is_primitive">is_primitive</a>(), <a href="../std/type_name.md#std_type_name_ENonModuleType">ENonModuleType</a>);
    // Starts after <b>address</b> and a double colon: `&lt;addr <b>as</b> HEX&gt;::`
    <b>let</b> <b>mut</b> i = <a href="../std/address.md#std_address_length">address::length</a>() * 2 + 2;
    <b>let</b> str_bytes = self.name.as_bytes();
    <b>let</b> <b>mut</b> module_name = <a href="../std/vector.md#std_vector">vector</a>[];
    <b>let</b> colon = <a href="../std/type_name.md#std_type_name_ASCII_COLON">ASCII_COLON</a>;
    <b>loop</b> {
        <b>let</b> char = &str_bytes[i];
        <b>if</b> (char != &colon) {
            module_name.push_back(*char);
            i = i + 1;
        } <b>else</b> {
            <b>break</b>
        }
    };
    <a href="../std/ascii.md#std_ascii_string">ascii::string</a>(module_name)
}
</code></pre>



</details>

<a name="std_type_name_into_string"></a>

## Function `into_string`

Convert <code>self</code> into its inner String


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_into_string">into_string</a>(self: <a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>): <a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/type_name.md#std_type_name_into_string">into_string</a>(self: <a href="../std/type_name.md#std_type_name_TypeName">TypeName</a>): String {
    self.name
}
</code></pre>



</details>
