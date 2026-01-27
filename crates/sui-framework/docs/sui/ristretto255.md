---
title: Module `sui::ristretto255`
---

Group operations of BLS12-381.


-  [Struct `Scalar`](#sui_ristretto255_Scalar)
-  [Struct `Point`](#sui_ristretto255_Point)
-  [Constants](#@Constants_0)
-  [Function `scalar_from_bytes`](#sui_ristretto255_scalar_from_bytes)
-  [Function `scalar_from_u64`](#sui_ristretto255_scalar_from_u64)
-  [Function `scalar_zero`](#sui_ristretto255_scalar_zero)
-  [Function `scalar_one`](#sui_ristretto255_scalar_one)
-  [Function `scalar_add`](#sui_ristretto255_scalar_add)
-  [Function `scalar_sub`](#sui_ristretto255_scalar_sub)
-  [Function `scalar_mul`](#sui_ristretto255_scalar_mul)
-  [Function `scalar_div`](#sui_ristretto255_scalar_div)
-  [Function `scalar_neg`](#sui_ristretto255_scalar_neg)
-  [Function `scalar_inv`](#sui_ristretto255_scalar_inv)
-  [Function `hash_to_scalar`](#sui_ristretto255_hash_to_scalar)
-  [Function `point_from_bytes`](#sui_ristretto255_point_from_bytes)
-  [Function `identity`](#sui_ristretto255_identity)
-  [Function `generator`](#sui_ristretto255_generator)
-  [Function `point_add`](#sui_ristretto255_point_add)
-  [Function `point_sub`](#sui_ristretto255_point_sub)
-  [Function `point_mul`](#sui_ristretto255_point_mul)
-  [Function `point_div`](#sui_ristretto255_point_div)
-  [Function `point_neg`](#sui_ristretto255_point_neg)
-  [Function `hash_to_point`](#sui_ristretto255_hash_to_point)
-  [Function `multi_scalar_multiplication`](#sui_ristretto255_multi_scalar_multiplication)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/debug.md#std_debug">std::debug</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/group_ops.md#sui_group_ops">sui::group_ops</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
</code></pre>



<a name="sui_ristretto255_Scalar"></a>

## Struct `Scalar`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_ristretto255_Point"></a>

## Struct `Point`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_ristretto255_SCALAR_ZERO_BYTES"></a>



<pre><code><b>const</b> <a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_ZERO_BYTES">SCALAR_ZERO_BYTES</a>: vector&lt;u8&gt; = vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
</code></pre>



<a name="sui_ristretto255_SCALAR_ONE_BYTES"></a>



<pre><code><b>const</b> <a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_ONE_BYTES">SCALAR_ONE_BYTES</a>: vector&lt;u8&gt; = vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
</code></pre>



<a name="sui_ristretto255_IDENTITY_BYTES"></a>



<pre><code><b>const</b> <a href="../sui/ristretto255.md#sui_ristretto255_IDENTITY_BYTES">IDENTITY_BYTES</a>: vector&lt;u8&gt; = vector[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
</code></pre>



<a name="sui_ristretto255_GENERATOR_BYTES"></a>



<pre><code><b>const</b> <a href="../sui/ristretto255.md#sui_ristretto255_GENERATOR_BYTES">GENERATOR_BYTES</a>: vector&lt;u8&gt; = vector[226, 242, 174, 10, 106, 188, 78, 113, 168, 132, 169, 97, 197, 0, 81, 95, 88, 227, 11, 106, 165, 130, 221, 141, 182, 166, 89, 69, 224, 141, 45, 118];
</code></pre>



<a name="sui_ristretto255_SCALAR_TYPE"></a>



<pre><code><b>const</b> <a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_TYPE">SCALAR_TYPE</a>: u8 = 5;
</code></pre>



<a name="sui_ristretto255_POINT_TYPE"></a>



<pre><code><b>const</b> <a href="../sui/ristretto255.md#sui_ristretto255_POINT_TYPE">POINT_TYPE</a>: u8 = 6;
</code></pre>



<a name="sui_ristretto255_scalar_from_bytes"></a>

## Function `scalar_from_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_from_bytes">scalar_from_bytes</a>(bytes: &vector&lt;u8&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_from_bytes">scalar_from_bytes</a>(bytes: &vector&lt;u8&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_from_bytes">group_ops::from_bytes</a>(<a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_TYPE">SCALAR_TYPE</a>, *bytes, <b>false</b>)
}
</code></pre>



</details>

<a name="sui_ristretto255_scalar_from_u64"></a>

## Function `scalar_from_u64`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_from_u64">scalar_from_u64</a>(x: u64): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_from_u64">scalar_from_u64</a>(x: u64): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt; {
    <b>let</b> <b>mut</b> bytes = <a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_ZERO_BYTES">SCALAR_ZERO_BYTES</a>;
    <a href="../sui/group_ops.md#sui_group_ops_set_as_prefix">group_ops::set_as_prefix</a>(x, <b>true</b>, &<b>mut</b> bytes);
    <a href="../sui/group_ops.md#sui_group_ops_from_bytes">group_ops::from_bytes</a>(<a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_TYPE">SCALAR_TYPE</a>, bytes, <b>true</b>)
}
</code></pre>



</details>

<a name="sui_ristretto255_scalar_zero"></a>

## Function `scalar_zero`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_zero">scalar_zero</a>(): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_zero">scalar_zero</a>(): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_from_bytes">group_ops::from_bytes</a>(<a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_TYPE">SCALAR_TYPE</a>, <a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_ZERO_BYTES">SCALAR_ZERO_BYTES</a>, <b>true</b>)
}
</code></pre>



</details>

<a name="sui_ristretto255_scalar_one"></a>

## Function `scalar_one`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_one">scalar_one</a>(): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_one">scalar_one</a>(): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_from_bytes">group_ops::from_bytes</a>(<a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_TYPE">SCALAR_TYPE</a>, <a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_ONE_BYTES">SCALAR_ONE_BYTES</a>, <b>true</b>)
}
</code></pre>



</details>

<a name="sui_ristretto255_scalar_add"></a>

## Function `scalar_add`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_add">scalar_add</a>(e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_add">scalar_add</a>(e1: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;, e2: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt; {
    <a href="../std/debug.md#std_debug_print">std::debug::print</a>(e1);
    <a href="../std/debug.md#std_debug_print">std::debug::print</a>(e2);
    <a href="../sui/group_ops.md#sui_group_ops_add">group_ops::add</a>(<a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_TYPE">SCALAR_TYPE</a>, e1, e2)
}
</code></pre>



</details>

<a name="sui_ristretto255_scalar_sub"></a>

## Function `scalar_sub`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_sub">scalar_sub</a>(e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_sub">scalar_sub</a>(e1: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;, e2: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_sub">group_ops::sub</a>(<a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_TYPE">SCALAR_TYPE</a>, e1, e2)
}
</code></pre>



</details>

<a name="sui_ristretto255_scalar_mul"></a>

## Function `scalar_mul`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_mul">scalar_mul</a>(e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_mul">scalar_mul</a>(e1: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;, e2: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_mul">group_ops::mul</a>(<a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_TYPE">SCALAR_TYPE</a>, e1, e2)
}
</code></pre>



</details>

<a name="sui_ristretto255_scalar_div"></a>

## Function `scalar_div`

Returns e2/e1, fails if a is zero.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_div">scalar_div</a>(e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_div">scalar_div</a>(e1: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;, e2: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_div">group_ops::div</a>(<a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_TYPE">SCALAR_TYPE</a>, e1, e2)
}
</code></pre>



</details>

<a name="sui_ristretto255_scalar_neg"></a>

## Function `scalar_neg`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_neg">scalar_neg</a>(e: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_neg">scalar_neg</a>(e: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt; {
    <a href="../sui/ristretto255.md#sui_ristretto255_scalar_sub">scalar_sub</a>(&<a href="../sui/ristretto255.md#sui_ristretto255_scalar_zero">scalar_zero</a>(), e)
}
</code></pre>



</details>

<a name="sui_ristretto255_scalar_inv"></a>

## Function `scalar_inv`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_inv">scalar_inv</a>(e: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_scalar_inv">scalar_inv</a>(e: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt; {
    <a href="../sui/ristretto255.md#sui_ristretto255_scalar_div">scalar_div</a>(e, &<a href="../sui/ristretto255.md#sui_ristretto255_scalar_one">scalar_one</a>())
}
</code></pre>



</details>

<a name="sui_ristretto255_hash_to_scalar"></a>

## Function `hash_to_scalar`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_hash_to_scalar">hash_to_scalar</a>(m: &vector&lt;u8&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_hash_to_scalar">hash_to_scalar</a>(m: &vector&lt;u8&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_hash_to">group_ops::hash_to</a>(<a href="../sui/ristretto255.md#sui_ristretto255_SCALAR_TYPE">SCALAR_TYPE</a>, m)
}
</code></pre>



</details>

<a name="sui_ristretto255_point_from_bytes"></a>

## Function `point_from_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_from_bytes">point_from_bytes</a>(bytes: &vector&lt;u8&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_from_bytes">point_from_bytes</a>(bytes: &vector&lt;u8&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_from_bytes">group_ops::from_bytes</a>(<a href="../sui/ristretto255.md#sui_ristretto255_POINT_TYPE">POINT_TYPE</a>, *bytes, <b>false</b>)
}
</code></pre>



</details>

<a name="sui_ristretto255_identity"></a>

## Function `identity`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_identity">identity</a>(): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_identity">identity</a>(): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_from_bytes">group_ops::from_bytes</a>(<a href="../sui/ristretto255.md#sui_ristretto255_POINT_TYPE">POINT_TYPE</a>, <a href="../sui/ristretto255.md#sui_ristretto255_IDENTITY_BYTES">IDENTITY_BYTES</a>, <b>true</b>)
}
</code></pre>



</details>

<a name="sui_ristretto255_generator"></a>

## Function `generator`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_generator">generator</a>(): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_generator">generator</a>(): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_from_bytes">group_ops::from_bytes</a>(<a href="../sui/ristretto255.md#sui_ristretto255_POINT_TYPE">POINT_TYPE</a>, <a href="../sui/ristretto255.md#sui_ristretto255_GENERATOR_BYTES">GENERATOR_BYTES</a>, <b>true</b>)
}
</code></pre>



</details>

<a name="sui_ristretto255_point_add"></a>

## Function `point_add`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_add">point_add</a>(e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_add">point_add</a>(e1: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt;, e2: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_add">group_ops::add</a>(<a href="../sui/ristretto255.md#sui_ristretto255_POINT_TYPE">POINT_TYPE</a>, e1, e2)
}
</code></pre>



</details>

<a name="sui_ristretto255_point_sub"></a>

## Function `point_sub`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_sub">point_sub</a>(e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_sub">point_sub</a>(e1: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt;, e2: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_sub">group_ops::sub</a>(<a href="../sui/ristretto255.md#sui_ristretto255_POINT_TYPE">POINT_TYPE</a>, e1, e2)
}
</code></pre>



</details>

<a name="sui_ristretto255_point_mul"></a>

## Function `point_mul`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_mul">point_mul</a>(e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_mul">point_mul</a>(e1: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;, e2: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_mul">group_ops::mul</a>(<a href="../sui/ristretto255.md#sui_ristretto255_POINT_TYPE">POINT_TYPE</a>, e1, e2)
}
</code></pre>



</details>

<a name="sui_ristretto255_point_div"></a>

## Function `point_div`

Returns e2 / e1, fails if scalar is zero.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_div">point_div</a>(e1: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;, e2: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_div">point_div</a>(e1: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;, e2: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_div">group_ops::div</a>(<a href="../sui/ristretto255.md#sui_ristretto255_POINT_TYPE">POINT_TYPE</a>, e1, e2)
}
</code></pre>



</details>

<a name="sui_ristretto255_point_neg"></a>

## Function `point_neg`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_neg">point_neg</a>(e: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_point_neg">point_neg</a>(e: &Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt; {
    <a href="../sui/ristretto255.md#sui_ristretto255_point_sub">point_sub</a>(&<a href="../sui/ristretto255.md#sui_ristretto255_identity">identity</a>(), e)
}
</code></pre>



</details>

<a name="sui_ristretto255_hash_to_point"></a>

## Function `hash_to_point`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_hash_to_point">hash_to_point</a>(m: &vector&lt;u8&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_hash_to_point">hash_to_point</a>(m: &vector&lt;u8&gt;): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_hash_to">group_ops::hash_to</a>(<a href="../sui/ristretto255.md#sui_ristretto255_POINT_TYPE">POINT_TYPE</a>, m)
}
</code></pre>



</details>

<a name="sui_ristretto255_multi_scalar_multiplication"></a>

## Function `multi_scalar_multiplication`

Let 'scalars' be the vector [s1, s2, ..., sn] and 'elements' be the vector [e1, e2, ..., en].
Returns s1*e1 + s2*e2 + ... + sn*en.
Aborts with <code>EInputTooLong</code> if the vectors are larger than 32 (may increase in the future).


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_multi_scalar_multiplication">multi_scalar_multiplication</a>(scalars: &vector&lt;<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">sui::ristretto255::Scalar</a>&gt;&gt;, elements: &vector&lt;<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;&gt;): <a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ristretto255.md#sui_ristretto255_multi_scalar_multiplication">multi_scalar_multiplication</a>(
    scalars: &vector&lt;Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Scalar">Scalar</a>&gt;&gt;,
    elements: &vector&lt;Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt;&gt;,
): Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">Point</a>&gt; {
    <a href="../sui/group_ops.md#sui_group_ops_multi_scalar_multiplication">group_ops::multi_scalar_multiplication</a>(<a href="../sui/ristretto255.md#sui_ristretto255_POINT_TYPE">POINT_TYPE</a>, scalars, elements)
}
</code></pre>



</details>
