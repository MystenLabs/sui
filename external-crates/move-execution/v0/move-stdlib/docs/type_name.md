
<a name="0x1_type_name"></a>

# Module `0x1::type_name`

Functionality for converting Move types into values. Use with care!


-  [Struct `TypeName`](#0x1_type_name_TypeName)
-  [Function `get`](#0x1_type_name_get)
-  [Function `borrow_string`](#0x1_type_name_borrow_string)
-  [Function `into_string`](#0x1_type_name_into_string)


<pre><code><b>use</b> <a href="ascii.md#0x1_ascii">0x1::ascii</a>;
</code></pre>



<a name="0x1_type_name_TypeName"></a>

## Struct `TypeName`



<pre><code><b>struct</b> <a href="type_name.md#0x1_type_name_TypeName">TypeName</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>name: <a href="ascii.md#0x1_ascii_String">ascii::String</a></code>
</dt>
<dd>
 String representation of the type. All types are represented
 using their source syntax:
 "u8", "u64", "u128", "bool", "address", "vector", "signer" for ground types.
 Struct types are represented as fully qualified type names; e.g.
 <code>00000000000000000000000000000001::string::String</code> or
 <code>0000000000000000000000000000000a::module_name1::type_name1&lt;0000000000000000000000000000000a::module_name2::type_name2&lt;u64&gt;&gt;</code>
 Addresses are hex-encoded lowercase values of length ADDRESS_LENGTH (16, 20, or 32 depending on the Move platform)
</dd>
</dl>


</details>

<a name="0x1_type_name_get"></a>

## Function `get`

Return a value representation of the type <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="type_name.md#0x1_type_name_get">get</a>&lt;T&gt;(): <a href="type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="type_name.md#0x1_type_name_get">get</a>&lt;T&gt;(): <a href="type_name.md#0x1_type_name_TypeName">TypeName</a>;
</code></pre>



</details>

<a name="0x1_type_name_borrow_string"></a>

## Function `borrow_string`

Get the String representation of <code>self</code>


<pre><code><b>public</b> <b>fun</b> <a href="type_name.md#0x1_type_name_borrow_string">borrow_string</a>(self: &<a href="type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>): &<a href="ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="type_name.md#0x1_type_name_borrow_string">borrow_string</a>(self: &<a href="type_name.md#0x1_type_name_TypeName">TypeName</a>): &String {
    &self.name
}
</code></pre>



</details>

<a name="0x1_type_name_into_string"></a>

## Function `into_string`

Convert <code>self</code> into its inner String


<pre><code><b>public</b> <b>fun</b> <a href="type_name.md#0x1_type_name_into_string">into_string</a>(self: <a href="type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>): <a href="ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="type_name.md#0x1_type_name_into_string">into_string</a>(self: <a href="type_name.md#0x1_type_name_TypeName">TypeName</a>): String {
    self.name
}
</code></pre>



</details>


[//]: # ("File containing references which can be used from documentation")
