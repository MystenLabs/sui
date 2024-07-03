---
title: Module `0x2::group_ops`
---

Generic Move and native functions for group operations.


-  [Struct `Element`](#0x2_group_ops_Element)
-  [Constants](#@Constants_0)
-  [Function `bytes`](#0x2_group_ops_bytes)
-  [Function `equal`](#0x2_group_ops_equal)
-  [Function `from_bytes`](#0x2_group_ops_from_bytes)
-  [Function `add`](#0x2_group_ops_add)
-  [Function `sub`](#0x2_group_ops_sub)
-  [Function `mul`](#0x2_group_ops_mul)
-  [Function `div`](#0x2_group_ops_div)
-  [Function `hash_to`](#0x2_group_ops_hash_to)
-  [Function `multi_scalar_multiplication`](#0x2_group_ops_multi_scalar_multiplication)
-  [Function `pairing`](#0x2_group_ops_pairing)
-  [Function `internal_validate`](#0x2_group_ops_internal_validate)
-  [Function `internal_add`](#0x2_group_ops_internal_add)
-  [Function `internal_sub`](#0x2_group_ops_internal_sub)
-  [Function `internal_mul`](#0x2_group_ops_internal_mul)
-  [Function `internal_div`](#0x2_group_ops_internal_div)
-  [Function `internal_hash_to`](#0x2_group_ops_internal_hash_to)
-  [Function `internal_multi_scalar_mul`](#0x2_group_ops_internal_multi_scalar_mul)
-  [Function `internal_pairing`](#0x2_group_ops_internal_pairing)
-  [Function `set_as_prefix`](#0x2_group_ops_set_as_prefix)


<pre><code><b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="bcs.md#0x2_bcs">0x2::bcs</a>;
</code></pre>



<a name="0x2_group_ops_Element"></a>

## Struct `Element`



<pre><code><b>struct</b> <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;T&gt; <b>has</b> <b>copy</b>, drop, store
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

<a name="@Constants_0"></a>

## Constants


<a name="0x2_group_ops_EInvalidInput"></a>



<pre><code><b>const</b> <a href="group_ops.md#0x2_group_ops_EInvalidInput">EInvalidInput</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x2_group_ops_EInputTooLong"></a>



<pre><code><b>const</b> <a href="group_ops.md#0x2_group_ops_EInputTooLong">EInputTooLong</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0x2_group_ops_EInvalidBufferLength"></a>



<pre><code><b>const</b> <a href="group_ops.md#0x2_group_ops_EInvalidBufferLength">EInvalidBufferLength</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 3;
</code></pre>



<a name="0x2_group_ops_ENotSupported"></a>



<pre><code><b>const</b> <a href="group_ops.md#0x2_group_ops_ENotSupported">ENotSupported</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_group_ops_bytes"></a>

## Function `bytes`



<pre><code><b>public</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_bytes">bytes</a>&lt;G&gt;(e: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_bytes">bytes</a>&lt;G&gt;(e: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    &e.bytes
}
</code></pre>



</details>

<a name="0x2_group_ops_equal"></a>

## Function `equal`



<pre><code><b>public</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_equal">equal</a>&lt;G&gt;(e1: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_equal">equal</a>&lt;G&gt;(e1: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): bool {
    &e1.bytes == &e2.bytes
}
</code></pre>



</details>

<a name="0x2_group_ops_from_bytes"></a>

## Function `from_bytes`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_from_bytes">from_bytes</a>&lt;G&gt;(type_: u8, bytes: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, is_trusted: bool): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_from_bytes">from_bytes</a>&lt;G&gt;(type_: u8, bytes: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, is_trusted: bool): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <b>assert</b>!(is_trusted || <a href="group_ops.md#0x2_group_ops_internal_validate">internal_validate</a>(type_, bytes), <a href="group_ops.md#0x2_group_ops_EInvalidInput">EInvalidInput</a>);
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: *bytes }
}
</code></pre>



</details>

<a name="0x2_group_ops_add"></a>

## Function `add`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_add">add</a>&lt;G&gt;(type_: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_add">add</a>&lt;G&gt;(type_: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_add">internal_add</a>(type_, &e1.bytes, &e2.bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_sub"></a>

## Function `sub`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_sub">sub</a>&lt;G&gt;(type_: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_sub">sub</a>&lt;G&gt;(type_: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_sub">internal_sub</a>(type_, &e1.bytes, &e2.bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_mul"></a>

## Function `mul`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_mul">mul</a>&lt;S, G&gt;(type_: u8, scalar: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;S&gt;, e: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_mul">mul</a>&lt;S, G&gt;(type_: u8, scalar: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;S&gt;, e: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_mul">internal_mul</a>(type_, &scalar.bytes, &e.bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_div"></a>

## Function `div`

Fails if scalar = 0. Else returns 1/scalar * e.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_div">div</a>&lt;S, G&gt;(type_: u8, scalar: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;S&gt;, e: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_div">div</a>&lt;S, G&gt;(type_: u8, scalar: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;S&gt;, e: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_div">internal_div</a>(type_, &scalar.bytes, &e.bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_hash_to"></a>

## Function `hash_to`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_hash_to">hash_to</a>&lt;G&gt;(type_: u8, m: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_hash_to">hash_to</a>&lt;G&gt;(type_: u8, m: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_hash_to">internal_hash_to</a>(type_, m) }
}
</code></pre>



</details>

<a name="0x2_group_ops_multi_scalar_multiplication"></a>

## Function `multi_scalar_multiplication`

Aborts with <code><a href="group_ops.md#0x2_group_ops_EInputTooLong">EInputTooLong</a></code> if the vectors are too long.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_multi_scalar_multiplication">multi_scalar_multiplication</a>&lt;S, G&gt;(type_: u8, scalars: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;S&gt;&gt;, elements: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_multi_scalar_multiplication">multi_scalar_multiplication</a>&lt;S, G&gt;(type_: u8, scalars: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;S&gt;&gt;, elements: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <b>assert</b>!(scalars.length() &gt; 0, <a href="group_ops.md#0x2_group_ops_EInvalidInput">EInvalidInput</a>);
    <b>assert</b>!(scalars.length() == elements.length(), <a href="group_ops.md#0x2_group_ops_EInvalidInput">EInvalidInput</a>);

    <b>let</b> <b>mut</b> scalars_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>let</b> <b>mut</b> elements_bytes: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;  = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; scalars.length()) {
        <b>let</b> scalar_vec = scalars[i];
        scalars_bytes.append(scalar_vec.bytes);
        <b>let</b> element_vec = elements[i];
        elements_bytes.append(element_vec.bytes);
        i = i + 1;
    };
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_multi_scalar_mul">internal_multi_scalar_mul</a>(type_, &scalars_bytes, &elements_bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_pairing"></a>

## Function `pairing`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_pairing">pairing</a>&lt;G1, G2, G3&gt;(type_: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G1&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G2&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G3&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_pairing">pairing</a>&lt;G1, G2, G3&gt;(type_: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G1&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G2&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G3&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G3&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_pairing">internal_pairing</a>(type_, &e1.bytes, &e2.bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_internal_validate"></a>

## Function `internal_validate`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_validate">internal_validate</a>(type_: u8, bytes: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_validate">internal_validate</a>(type_: u8, bytes: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>

<a name="0x2_group_ops_internal_add"></a>

## Function `internal_add`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_add">internal_add</a>(type_: u8, e1: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, e2: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_add">internal_add</a>(type_: u8, e1: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, e2: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_sub"></a>

## Function `internal_sub`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_sub">internal_sub</a>(type_: u8, e1: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, e2: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_sub">internal_sub</a>(type_: u8, e1: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, e2: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_mul"></a>

## Function `internal_mul`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_mul">internal_mul</a>(type_: u8, e1: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, e2: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_mul">internal_mul</a>(type_: u8, e1: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, e2: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_div"></a>

## Function `internal_div`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_div">internal_div</a>(type_: u8, e1: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, e2: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_div">internal_div</a>(type_: u8, e1: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, e2: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_hash_to"></a>

## Function `internal_hash_to`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_hash_to">internal_hash_to</a>(type_: u8, m: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_hash_to">internal_hash_to</a>(type_: u8, m: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_multi_scalar_mul"></a>

## Function `internal_multi_scalar_mul`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_multi_scalar_mul">internal_multi_scalar_mul</a>(type_: u8, scalars: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, elements: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_multi_scalar_mul">internal_multi_scalar_mul</a>(type_: u8, scalars: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, elements: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_pairing"></a>

## Function `internal_pairing`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_pairing">internal_pairing</a>(type_: u8, e1: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, e2: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_pairing">internal_pairing</a>(type_: u8, e1: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, e2: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_set_as_prefix"></a>

## Function `set_as_prefix`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_set_as_prefix">set_as_prefix</a>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, big_endian: bool, buffer: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="package.md#0x2_package">package</a>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_set_as_prefix">set_as_prefix</a>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, big_endian: bool, buffer: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;) {
    <b>let</b> buffer_len = buffer.length();
    <b>assert</b>!(buffer_len &gt; 7, <a href="group_ops.md#0x2_group_ops_EInvalidBufferLength">EInvalidBufferLength</a>);
    <b>let</b> x_as_bytes = <a href="../move-stdlib/bcs.md#0x1_bcs_to_bytes">bcs::to_bytes</a>(&x); // little endian
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; 8) {
        <b>let</b> position = <b>if</b> (big_endian) { buffer_len - i - 1 } <b>else</b> { i };
        *(&<b>mut</b> buffer[position]) = x_as_bytes[i];
        i = i + 1;
    };
}
</code></pre>



</details>
