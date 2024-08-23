---
title: Module `0x2::math`
---

DEPRECATED, use the each integer type's individual module instead, e.g. <code>std::u64</code>


-  [Function `max`](#0x2_math_max)
-  [Function `min`](#0x2_math_min)
-  [Function `diff`](#0x2_math_diff)
-  [Function `pow`](#0x2_math_pow)
-  [Function `sqrt`](#0x2_math_sqrt)
-  [Function `sqrt_u128`](#0x2_math_sqrt_u128)
-  [Function `divide_and_round_up`](#0x2_math_divide_and_round_up)


<pre><code><b>use</b> <a href="../move-stdlib/u128.md#0x1_u128">0x1::u128</a>;
<b>use</b> <a href="../move-stdlib/u64.md#0x1_u64">0x1::u64</a>;
</code></pre>



<a name="0x2_math_max"></a>

## Function `max`

DEPRECATED, use <code>std::u64::max</code> instead


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_max">max</a>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, y: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_max">max</a>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, y: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    x.<a href="math.md#0x2_math_max">max</a>(y)
}
</code></pre>



</details>

<a name="0x2_math_min"></a>

## Function `min`

DEPRECATED, use <code>std::u64::min</code> instead


<pre><code><b>public</b> <b>fun</b> <b>min</b>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, y: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <b>min</b>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, y: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    x.<b>min</b>(y)
}
</code></pre>



</details>

<a name="0x2_math_diff"></a>

## Function `diff`

DEPRECATED, use <code>std::u64::diff</code> instead


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_diff">diff</a>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, y: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_diff">diff</a>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, y: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    x.<a href="math.md#0x2_math_diff">diff</a>(y)
}
</code></pre>



</details>

<a name="0x2_math_pow"></a>

## Function `pow`

DEPRECATED, use <code>std::u64::pow</code> instead


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_pow">pow</a>(base: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, exponent: u8): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_pow">pow</a>(base: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, exponent: u8): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    base.<a href="math.md#0x2_math_pow">pow</a>(exponent)
}
</code></pre>



</details>

<a name="0x2_math_sqrt"></a>

## Function `sqrt`

DEPRECATED, use <code>std::u64::sqrt</code> instead


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_sqrt">sqrt</a>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_sqrt">sqrt</a>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    x.<a href="math.md#0x2_math_sqrt">sqrt</a>()
}
</code></pre>



</details>

<a name="0x2_math_sqrt_u128"></a>

## Function `sqrt_u128`

DEPRECATED, use <code>std::u128::sqrt</code> instead


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_sqrt_u128">sqrt_u128</a>(x: <a href="../move-stdlib/u128.md#0x1_u128">u128</a>): <a href="../move-stdlib/u128.md#0x1_u128">u128</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_sqrt_u128">sqrt_u128</a>(x: <a href="../move-stdlib/u128.md#0x1_u128">u128</a>): <a href="../move-stdlib/u128.md#0x1_u128">u128</a> {
   x.<a href="math.md#0x2_math_sqrt">sqrt</a>()
}
</code></pre>



</details>

<a name="0x2_math_divide_and_round_up"></a>

## Function `divide_and_round_up`

DEPRECATED, use <code>std::u64::divide_and_round_up</code> instead


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_divide_and_round_up">divide_and_round_up</a>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, y: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_divide_and_round_up">divide_and_round_up</a>(x: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, y: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    x.<a href="math.md#0x2_math_divide_and_round_up">divide_and_round_up</a>(y)
}
</code></pre>



</details>
