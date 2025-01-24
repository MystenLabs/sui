---
title: Module `sui::group_ops`
---

Generic Move and native functions for group operations.


-  [Struct `Element`](#sui_group_ops_Element)
-  [Constants](#@Constants_0)
-  [Function `bytes`](#sui_group_ops_bytes)
-  [Function `equal`](#sui_group_ops_equal)
-  [Function `from_bytes`](#sui_group_ops_from_bytes)
-  [Function `add`](#sui_group_ops_add)
-  [Function `sub`](#sui_group_ops_sub)
-  [Function `mul`](#sui_group_ops_mul)
-  [Function `div`](#sui_group_ops_div)
-  [Function `hash_to`](#sui_group_ops_hash_to)
-  [Function `multi_scalar_multiplication`](#sui_group_ops_multi_scalar_multiplication)
-  [Function `pairing`](#sui_group_ops_pairing)
-  [Function `convert`](#sui_group_ops_convert)
-  [Function `sum`](#sui_group_ops_sum)
-  [Function `internal_validate`](#sui_group_ops_internal_validate)
-  [Function `internal_add`](#sui_group_ops_internal_add)
-  [Function `internal_sub`](#sui_group_ops_internal_sub)
-  [Function `internal_mul`](#sui_group_ops_internal_mul)
-  [Function `internal_div`](#sui_group_ops_internal_div)
-  [Function `internal_hash_to`](#sui_group_ops_internal_hash_to)
-  [Function `internal_multi_scalar_mul`](#sui_group_ops_internal_multi_scalar_mul)
-  [Function `internal_pairing`](#sui_group_ops_internal_pairing)
-  [Function `internal_convert`](#sui_group_ops_internal_convert)
-  [Function `internal_sum`](#sui_group_ops_internal_sum)
-  [Function `set_as_prefix`](#sui_group_ops_set_as_prefix)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
</code></pre>



<a name="sui_group_ops_Element"></a>

## Struct `Element`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_group_ops_EInputTooLong"></a>



<pre><code><b>const</b> <a href="../sui/group_ops.md#sui_group_ops_EInputTooLong">EInputTooLong</a>: u64 = 2;
</code></pre>



<a name="sui_group_ops_EInvalidBufferLength"></a>



<pre><code><b>const</b> <a href="../sui/group_ops.md#sui_group_ops_EInvalidBufferLength">EInvalidBufferLength</a>: u64 = 3;
</code></pre>



<a name="sui_group_ops_EInvalidInput"></a>



<pre><code><b>const</b> <a href="../sui/group_ops.md#sui_group_ops_EInvalidInput">EInvalidInput</a>: u64 = 1;
</code></pre>



<a name="sui_group_ops_ENotSupported"></a>



<pre><code><b>const</b> <a href="../sui/group_ops.md#sui_group_ops_ENotSupported">ENotSupported</a>: u64 = 0;
</code></pre>



<a name="sui_group_ops_bytes"></a>

## Function `bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>&lt;G&gt;(e: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>&lt;G&gt;(e: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt;): &vector&lt;u8&gt; {
    &e.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>
}
</code></pre>



</details>

<a name="sui_group_ops_equal"></a>

## Function `equal`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_equal">equal</a>&lt;G&gt;(e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_equal">equal</a>&lt;G&gt;(e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt;): bool {
    &e1.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a> == &e2.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>
}
</code></pre>



</details>

<a name="sui_group_ops_from_bytes"></a>

## Function `from_bytes`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_from_bytes">from_bytes</a>&lt;G&gt;(type_: u8, <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: &vector&lt;u8&gt;, is_trusted: bool): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_from_bytes">from_bytes</a>&lt;G&gt;(type_: u8, <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: &vector&lt;u8&gt;, is_trusted: bool): <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; {
    <b>assert</b>!(is_trusted || <a href="../sui/group_ops.md#sui_group_ops_internal_validate">internal_validate</a>(type_, <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>), <a href="../sui/group_ops.md#sui_group_ops_EInvalidInput">EInvalidInput</a>);
    <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; { <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: *<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a> }
}
</code></pre>



</details>

<a name="sui_group_ops_add"></a>

## Function `add`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_add">add</a>&lt;G&gt;(type_: u8, e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_add">add</a>&lt;G&gt;(type_: u8, e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; { <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: <a href="../sui/group_ops.md#sui_group_ops_internal_add">internal_add</a>(type_, &e1.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>, &e2.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>) }
}
</code></pre>



</details>

<a name="sui_group_ops_sub"></a>

## Function `sub`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_sub">sub</a>&lt;G&gt;(type_: u8, e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_sub">sub</a>&lt;G&gt;(type_: u8, e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; { <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: <a href="../sui/group_ops.md#sui_group_ops_internal_sub">internal_sub</a>(type_, &e1.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>, &e2.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>) }
}
</code></pre>



</details>

<a name="sui_group_ops_mul"></a>

## Function `mul`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_mul">mul</a>&lt;S, G&gt;(type_: u8, scalar: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;S&gt;, e: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_mul">mul</a>&lt;S, G&gt;(type_: u8, scalar: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;S&gt;, e: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; { <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: <a href="../sui/group_ops.md#sui_group_ops_internal_mul">internal_mul</a>(type_, &scalar.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>, &e.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>) }
}
</code></pre>



</details>

<a name="sui_group_ops_div"></a>

## Function `div`

Fails if scalar = 0. Else returns 1/scalar * e.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_div">div</a>&lt;S, G&gt;(type_: u8, scalar: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;S&gt;, e: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_div">div</a>&lt;S, G&gt;(type_: u8, scalar: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;S&gt;, e: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; { <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: <a href="../sui/group_ops.md#sui_group_ops_internal_div">internal_div</a>(type_, &scalar.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>, &e.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>) }
}
</code></pre>



</details>

<a name="sui_group_ops_hash_to"></a>

## Function `hash_to`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_hash_to">hash_to</a>&lt;G&gt;(type_: u8, m: &vector&lt;u8&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_hash_to">hash_to</a>&lt;G&gt;(type_: u8, m: &vector&lt;u8&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; { <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: <a href="../sui/group_ops.md#sui_group_ops_internal_hash_to">internal_hash_to</a>(type_, m) }
}
</code></pre>



</details>

<a name="sui_group_ops_multi_scalar_multiplication"></a>

## Function `multi_scalar_multiplication`

Aborts with <code><a href="../sui/group_ops.md#sui_group_ops_EInputTooLong">EInputTooLong</a></code> if the vectors are too long.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_multi_scalar_multiplication">multi_scalar_multiplication</a>&lt;S, G&gt;(type_: u8, scalars: &vector&lt;<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;S&gt;&gt;, elements: &vector&lt;<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_multi_scalar_multiplication">multi_scalar_multiplication</a>&lt;S, G&gt;(
    type_: u8,
    scalars: &vector&lt;<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;S&gt;&gt;,
    elements: &vector&lt;<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt;&gt;,
): <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; {
    <b>assert</b>!(scalars.length() &gt; 0, <a href="../sui/group_ops.md#sui_group_ops_EInvalidInput">EInvalidInput</a>);
    <b>assert</b>!(scalars.length() == elements.length(), <a href="../sui/group_ops.md#sui_group_ops_EInvalidInput">EInvalidInput</a>);
    <b>let</b> <b>mut</b> scalars_bytes: vector&lt;u8&gt; = vector[];
    <b>let</b> <b>mut</b> elements_bytes: vector&lt;u8&gt; = vector[];
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; scalars.length()) {
        <b>let</b> scalar_vec = scalars[i];
        scalars_bytes.append(scalar_vec.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>);
        <b>let</b> element_vec = elements[i];
        elements_bytes.append(element_vec.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>);
        i = i + 1;
    };
    <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; { <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: <a href="../sui/group_ops.md#sui_group_ops_internal_multi_scalar_mul">internal_multi_scalar_mul</a>(type_, &scalars_bytes, &elements_bytes) }
}
</code></pre>



</details>

<a name="sui_group_ops_pairing"></a>

## Function `pairing`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_pairing">pairing</a>&lt;G1, G2, G3&gt;(type_: u8, e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G1&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G2&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G3&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_pairing">pairing</a>&lt;G1, G2, G3&gt;(
    type_: u8,
    e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G1&gt;,
    e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G2&gt;,
): <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G3&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G3&gt; { <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: <a href="../sui/group_ops.md#sui_group_ops_internal_pairing">internal_pairing</a>(type_, &e1.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>, &e2.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>) }
}
</code></pre>



</details>

<a name="sui_group_ops_convert"></a>

## Function `convert`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_convert">convert</a>&lt;From, To&gt;(from_type_: u8, to_type_: u8, e: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;From&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;To&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_convert">convert</a>&lt;From, To&gt;(from_type_: u8, to_type_: u8, e: &<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;From&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;To&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;To&gt; { <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: <a href="../sui/group_ops.md#sui_group_ops_internal_convert">internal_convert</a>(from_type_, to_type_, &e.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>) }
}
</code></pre>



</details>

<a name="sui_group_ops_sum"></a>

## Function `sum`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_sum">sum</a>&lt;G&gt;(type_: u8, terms: &vector&lt;<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_sum">sum</a>&lt;G&gt;(type_: u8, terms: &vector&lt;<a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt;&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_Element">Element</a>&lt;G&gt; { <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: <a href="../sui/group_ops.md#sui_group_ops_internal_sum">internal_sum</a>(type_, &(*terms).map!(|x| x.<a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>)) }
}
</code></pre>



</details>

<a name="sui_group_ops_internal_validate"></a>

## Function `internal_validate`



<pre><code><b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_validate">internal_validate</a>(type_: u8, <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: &vector&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_validate">internal_validate</a>(type_: u8, <a href="../sui/group_ops.md#sui_group_ops_bytes">bytes</a>: &vector&lt;u8&gt;): bool;
</code></pre>



</details>

<a name="sui_group_ops_internal_add"></a>

## Function `internal_add`



<pre><code><b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_add">internal_add</a>(type_: u8, e1: &vector&lt;u8&gt;, e2: &vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_add">internal_add</a>(type_: u8, e1: &vector&lt;u8&gt;, e2: &vector&lt;u8&gt;): vector&lt;u8&gt;;
</code></pre>



</details>

<a name="sui_group_ops_internal_sub"></a>

## Function `internal_sub`



<pre><code><b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_sub">internal_sub</a>(type_: u8, e1: &vector&lt;u8&gt;, e2: &vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_sub">internal_sub</a>(type_: u8, e1: &vector&lt;u8&gt;, e2: &vector&lt;u8&gt;): vector&lt;u8&gt;;
</code></pre>



</details>

<a name="sui_group_ops_internal_mul"></a>

## Function `internal_mul`



<pre><code><b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_mul">internal_mul</a>(type_: u8, e1: &vector&lt;u8&gt;, e2: &vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_mul">internal_mul</a>(type_: u8, e1: &vector&lt;u8&gt;, e2: &vector&lt;u8&gt;): vector&lt;u8&gt;;
</code></pre>



</details>

<a name="sui_group_ops_internal_div"></a>

## Function `internal_div`



<pre><code><b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_div">internal_div</a>(type_: u8, e1: &vector&lt;u8&gt;, e2: &vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_div">internal_div</a>(type_: u8, e1: &vector&lt;u8&gt;, e2: &vector&lt;u8&gt;): vector&lt;u8&gt;;
</code></pre>



</details>

<a name="sui_group_ops_internal_hash_to"></a>

## Function `internal_hash_to`



<pre><code><b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_hash_to">internal_hash_to</a>(type_: u8, m: &vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_hash_to">internal_hash_to</a>(type_: u8, m: &vector&lt;u8&gt;): vector&lt;u8&gt;;
</code></pre>



</details>

<a name="sui_group_ops_internal_multi_scalar_mul"></a>

## Function `internal_multi_scalar_mul`



<pre><code><b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_multi_scalar_mul">internal_multi_scalar_mul</a>(type_: u8, scalars: &vector&lt;u8&gt;, elements: &vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_multi_scalar_mul">internal_multi_scalar_mul</a>(
    type_: u8,
    scalars: &vector&lt;u8&gt;,
    elements: &vector&lt;u8&gt;,
): vector&lt;u8&gt;;
</code></pre>



</details>

<a name="sui_group_ops_internal_pairing"></a>

## Function `internal_pairing`



<pre><code><b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_pairing">internal_pairing</a>(type_: u8, e1: &vector&lt;u8&gt;, e2: &vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_pairing">internal_pairing</a>(type_: u8, e1: &vector&lt;u8&gt;, e2: &vector&lt;u8&gt;): vector&lt;u8&gt;;
</code></pre>



</details>

<a name="sui_group_ops_internal_convert"></a>

## Function `internal_convert`



<pre><code><b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_convert">internal_convert</a>(from_type_: u8, to_type_: u8, e: &vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_convert">internal_convert</a>(from_type_: u8, to_type_: u8, e: &vector&lt;u8&gt;): vector&lt;u8&gt;;
</code></pre>



</details>

<a name="sui_group_ops_internal_sum"></a>

## Function `internal_sum`



<pre><code><b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_sum">internal_sum</a>(type_: u8, e: &vector&lt;vector&lt;u8&gt;&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_internal_sum">internal_sum</a>(type_: u8, e: &vector&lt;vector&lt;u8&gt;&gt;): vector&lt;u8&gt;;
</code></pre>



</details>

<a name="sui_group_ops_set_as_prefix"></a>

## Function `set_as_prefix`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_set_as_prefix">set_as_prefix</a>(x: u64, big_endian: bool, buffer: &<b>mut</b> vector&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/group_ops.md#sui_group_ops_set_as_prefix">set_as_prefix</a>(x: u64, big_endian: bool, buffer: &<b>mut</b> vector&lt;u8&gt;) {
    <b>let</b> buffer_len = buffer.length();
    <b>assert</b>!(buffer_len &gt; 7, <a href="../sui/group_ops.md#sui_group_ops_EInvalidBufferLength">EInvalidBufferLength</a>);
    <b>let</b> x_as_bytes = <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(&x); // little endian
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; 8) {
        <b>let</b> position = <b>if</b> (big_endian) {
            buffer_len - i - 1
        } <b>else</b> {
            i
        };
        *(&<b>mut</b> buffer[position]) = x_as_bytes[i];
        i = i + 1;
    };
}
</code></pre>



</details>
