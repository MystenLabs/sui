
<a name="0x1_type_name"></a>

# Module `0x1::type_name`



-  [Struct `TypeName`](#0x1_type_name_TypeName)
-  [Constants](#@Constants_0)
-  [Function `get`](#0x1_type_name_get)
-  [Function `get_with_original_ids`](#0x1_type_name_get_with_original_ids)
-  [Function `borrow_string`](#0x1_type_name_borrow_string)
-  [Function `get_address`](#0x1_type_name_get_address)
-  [Function `get_module`](#0x1_type_name_get_module)
-  [Function `into_string`](#0x1_type_name_into_string)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/address.md#0x1_address">0x1::address</a>;
<b>use</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii">0x1::ascii</a>;
</code></pre>



<a name="0x1_type_name_TypeName"></a>

## Struct `TypeName`



<pre><code><b>struct</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">TypeName</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>name: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x1_type_name_ASCII_COLON"></a>



<pre><code><b>const</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_ASCII_COLON">ASCII_COLON</a>: u8 = 58;
</code></pre>



<a name="0x1_type_name_get"></a>

## Function `get`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_get">get</a>&lt;T&gt;(): <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_get">get</a>&lt;T&gt;(): <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">TypeName</a>;
</code></pre>



</details>

<a name="0x1_type_name_get_with_original_ids"></a>

## Function `get_with_original_ids`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_get_with_original_ids">get_with_original_ids</a>&lt;T&gt;(): <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_get_with_original_ids">get_with_original_ids</a>&lt;T&gt;(): <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">TypeName</a>;
</code></pre>



</details>

<a name="0x1_type_name_borrow_string"></a>

## Function `borrow_string`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_borrow_string">borrow_string</a>(self: &<a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>): &<a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_borrow_string">borrow_string</a>(self: &<a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">TypeName</a>): &String {
    &self.name
}
</code></pre>



</details>

<a name="0x1_type_name_get_address"></a>

## Function `get_address`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_get_address">get_address</a>(self: &<a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_get_address">get_address</a>(self: &<a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">TypeName</a>): String {
    // Base16 (<a href="../../dependencies/move-stdlib/string.md#0x1_string">string</a>) representation of an <b>address</b> <b>has</b> 2 symbols per byte.
    <b>let</b> len = <a href="../../dependencies/move-stdlib/address.md#0x1_address_length">address::length</a>() * 2;
    <b>let</b> str_bytes = <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_as_bytes">ascii::as_bytes</a>(&self.name);
    <b>let</b> addr_bytes = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>let</b> i = 0;

    // Read `len` bytes from the type name and push them <b>to</b> addr_bytes.
    <b>while</b> (i &lt; len) {
        <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(
            &<b>mut</b> addr_bytes,
            *<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(str_bytes, i)
        );
        i = i + 1;
    };

    <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_string">ascii::string</a>(addr_bytes)
}
</code></pre>



</details>

<a name="0x1_type_name_get_module"></a>

## Function `get_module`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_get_module">get_module</a>(self: &<a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_get_module">get_module</a>(self: &<a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">TypeName</a>): String {
    // Starts after <b>address</b> and a double colon: `&lt;addr <b>as</b> HEX&gt;::`
    <b>let</b> i = <a href="../../dependencies/move-stdlib/address.md#0x1_address_length">address::length</a>() * 2 + 2;
    <b>let</b> str_bytes = <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_as_bytes">ascii::as_bytes</a>(&self.name);
    <b>let</b> module_name = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>[];

    <b>loop</b> {
        <b>let</b> char = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(str_bytes, i);
        <b>if</b> (char != &<a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_ASCII_COLON">ASCII_COLON</a>) {
            <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> module_name, *char);
            i = i + 1;
        } <b>else</b> {
            <b>break</b>
        }
    };

    <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_string">ascii::string</a>(module_name)
}
</code></pre>



</details>

<a name="0x1_type_name_into_string"></a>

## Function `into_string`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_into_string">into_string</a>(self: <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_into_string">into_string</a>(self: <a href="../../dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">TypeName</a>): String {
    self.name
}
</code></pre>



</details>
