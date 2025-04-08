---
title: Module `sui::math`
---

DEPRECATED, use the each integer type's individual module instead, e.g. <code><a href="../std/u64.md#std_u64">std::u64</a></code>


-  [Function `max`](#sui_math_max)
-  [Function `min`](#sui_math_min)
-  [Function `diff`](#sui_math_diff)
-  [Function `pow`](#sui_math_pow)
-  [Function `sqrt`](#sui_math_sqrt)
-  [Function `sqrt_u128`](#sui_math_sqrt_u128)
-  [Function `divide_and_round_up`](#sui_math_divide_and_round_up)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/u128.md#std_u128">std::u128</a>;
<b>use</b> <a href="../std/u64.md#std_u64">std::u64</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
</code></pre>



<a name="sui_math_max"></a>

## Function `max`

DEPRECATED, use <code><a href="../std/u64.md#std_u64_max">std::u64::max</a></code> instead


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_max">max</a>(x: u64, y: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_max">max</a>(x: u64, y: u64): u64 {
    x.<a href="../sui/math.md#sui_math_max">max</a>(y)
}
</code></pre>



</details>

<a name="sui_math_min"></a>

## Function `min`

DEPRECATED, use <code><a href="../std/u64.md#std_u64_min">std::u64::min</a></code> instead


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_min">min</a>(x: u64, y: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_min">min</a>(x: u64, y: u64): u64 {
    x.<a href="../sui/math.md#sui_math_min">min</a>(y)
}
</code></pre>



</details>

<a name="sui_math_diff"></a>

## Function `diff`

DEPRECATED, use <code><a href="../std/u64.md#std_u64_diff">std::u64::diff</a></code> instead


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_diff">diff</a>(x: u64, y: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_diff">diff</a>(x: u64, y: u64): u64 {
    x.<a href="../sui/math.md#sui_math_diff">diff</a>(y)
}
</code></pre>



</details>

<a name="sui_math_pow"></a>

## Function `pow`

DEPRECATED, use <code><a href="../std/u64.md#std_u64_pow">std::u64::pow</a></code> instead


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_pow">pow</a>(base: u64, exponent: u8): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_pow">pow</a>(base: u64, exponent: u8): u64 {
    base.<a href="../sui/math.md#sui_math_pow">pow</a>(exponent)
}
</code></pre>



</details>

<a name="sui_math_sqrt"></a>

## Function `sqrt`

DEPRECATED, use <code><a href="../std/u64.md#std_u64_sqrt">std::u64::sqrt</a></code> instead


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_sqrt">sqrt</a>(x: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_sqrt">sqrt</a>(x: u64): u64 {
    x.<a href="../sui/math.md#sui_math_sqrt">sqrt</a>()
}
</code></pre>



</details>

<a name="sui_math_sqrt_u128"></a>

## Function `sqrt_u128`

DEPRECATED, use <code><a href="../std/u128.md#std_u128_sqrt">std::u128::sqrt</a></code> instead


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_sqrt_u128">sqrt_u128</a>(x: u128): u128
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_sqrt_u128">sqrt_u128</a>(x: u128): u128 {
    x.<a href="../sui/math.md#sui_math_sqrt">sqrt</a>()
}
</code></pre>



</details>

<a name="sui_math_divide_and_round_up"></a>

## Function `divide_and_round_up`

DEPRECATED, use <code><a href="../std/u64.md#std_u64_divide_and_round_up">std::u64::divide_and_round_up</a></code> instead


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_divide_and_round_up">divide_and_round_up</a>(x: u64, y: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/math.md#sui_math_divide_and_round_up">divide_and_round_up</a>(x: u64, y: u64): u64 {
    x.<a href="../sui/math.md#sui_math_divide_and_round_up">divide_and_round_up</a>(y)
}
</code></pre>



</details>
