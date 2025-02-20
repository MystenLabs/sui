---
title: Module `deepbook::math`
---



-  [Constants](#@Constants_0)
-  [Function `unsafe_mul`](#deepbook_math_unsafe_mul)
-  [Function `unsafe_mul_round`](#deepbook_math_unsafe_mul_round)
-  [Function `mul`](#deepbook_math_mul)
-  [Function `mul_round`](#deepbook_math_mul_round)
-  [Function `unsafe_div`](#deepbook_math_unsafe_div)
-  [Function `unsafe_div_round`](#deepbook_math_unsafe_div_round)
-  [Function `div_round`](#deepbook_math_div_round)
-  [Function `count_leading_zeros`](#deepbook_math_count_leading_zeros)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="deepbook_math_EUnderflow"></a>



<pre><code><b>const</b> <a href="../deepbook/math.md#deepbook_math_EUnderflow">EUnderflow</a>: u64 = 1;
</code></pre>



<a name="deepbook_math_FLOAT_SCALING"></a>

scaling setting for float


<pre><code><b>const</b> <a href="../deepbook/math.md#deepbook_math_FLOAT_SCALING">FLOAT_SCALING</a>: u64 = 1000000000;
</code></pre>



<a name="deepbook_math_FLOAT_SCALING_U128"></a>



<pre><code><b>const</b> <a href="../deepbook/math.md#deepbook_math_FLOAT_SCALING_U128">FLOAT_SCALING_U128</a>: u128 = 1000000000;
</code></pre>



<a name="deepbook_math_unsafe_mul"></a>

## Function `unsafe_mul`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/math.md#deepbook_math_unsafe_mul">unsafe_mul</a>(x: u64, y: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/math.md#deepbook_math_unsafe_mul">unsafe_mul</a>(x: u64, y: u64): u64 {
    <b>let</b> (_, result) = <a href="../deepbook/math.md#deepbook_math_unsafe_mul_round">unsafe_mul_round</a>(x, y);
    result
}
</code></pre>



</details>

<a name="deepbook_math_unsafe_mul_round"></a>

## Function `unsafe_mul_round`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/math.md#deepbook_math_unsafe_mul_round">unsafe_mul_round</a>(x: u64, y: u64): (bool, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/math.md#deepbook_math_unsafe_mul_round">unsafe_mul_round</a>(x: u64, y: u64): (bool, u64) {
    <b>let</b> x = x <b>as</b> u128;
    <b>let</b> y = y <b>as</b> u128;
    <b>let</b> <b>mut</b> is_round_down = <b>true</b>;
    <b>if</b> ((x * y) % <a href="../deepbook/math.md#deepbook_math_FLOAT_SCALING_U128">FLOAT_SCALING_U128</a> == 0) is_round_down = <b>false</b>;
    (is_round_down, (x * y / <a href="../deepbook/math.md#deepbook_math_FLOAT_SCALING_U128">FLOAT_SCALING_U128</a>) <b>as</b> u64)
}
</code></pre>



</details>

<a name="deepbook_math_mul"></a>

## Function `mul`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/math.md#deepbook_math_mul">mul</a>(x: u64, y: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/math.md#deepbook_math_mul">mul</a>(x: u64, y: u64): u64 {
    <b>let</b> (_, result) = <a href="../deepbook/math.md#deepbook_math_unsafe_mul_round">unsafe_mul_round</a>(x, y);
    <b>assert</b>!(result &gt; 0, <a href="../deepbook/math.md#deepbook_math_EUnderflow">EUnderflow</a>);
    result
}
</code></pre>



</details>

<a name="deepbook_math_mul_round"></a>

## Function `mul_round`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/math.md#deepbook_math_mul_round">mul_round</a>(x: u64, y: u64): (bool, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/math.md#deepbook_math_mul_round">mul_round</a>(x: u64, y: u64): (bool, u64) {
    <b>let</b> (is_round_down, result) = <a href="../deepbook/math.md#deepbook_math_unsafe_mul_round">unsafe_mul_round</a>(x, y);
    <b>assert</b>!(result &gt; 0, <a href="../deepbook/math.md#deepbook_math_EUnderflow">EUnderflow</a>);
    (is_round_down, result)
}
</code></pre>



</details>

<a name="deepbook_math_unsafe_div"></a>

## Function `unsafe_div`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/math.md#deepbook_math_unsafe_div">unsafe_div</a>(x: u64, y: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/math.md#deepbook_math_unsafe_div">unsafe_div</a>(x: u64, y: u64): u64 {
    <b>let</b> (_, result) = <a href="../deepbook/math.md#deepbook_math_unsafe_div_round">unsafe_div_round</a>(x, y);
    result
}
</code></pre>



</details>

<a name="deepbook_math_unsafe_div_round"></a>

## Function `unsafe_div_round`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/math.md#deepbook_math_unsafe_div_round">unsafe_div_round</a>(x: u64, y: u64): (bool, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/math.md#deepbook_math_unsafe_div_round">unsafe_div_round</a>(x: u64, y: u64): (bool, u64) {
    <b>let</b> x = x <b>as</b> u128;
    <b>let</b> y = y <b>as</b> u128;
    <b>let</b> <b>mut</b> is_round_down = <b>true</b>;
    <b>if</b> ((x * (<a href="../deepbook/math.md#deepbook_math_FLOAT_SCALING">FLOAT_SCALING</a> <b>as</b> u128) % y) == 0) is_round_down = <b>false</b>;
    (is_round_down, (x * (<a href="../deepbook/math.md#deepbook_math_FLOAT_SCALING">FLOAT_SCALING</a> <b>as</b> u128) / y) <b>as</b> u64)
}
</code></pre>



</details>

<a name="deepbook_math_div_round"></a>

## Function `div_round`



<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/math.md#deepbook_math_div_round">div_round</a>(x: u64, y: u64): (bool, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/math.md#deepbook_math_div_round">div_round</a>(x: u64, y: u64): (bool, u64) {
    <b>let</b> (is_round_down, result) = <a href="../deepbook/math.md#deepbook_math_unsafe_div_round">unsafe_div_round</a>(x, y);
    <b>assert</b>!(result &gt; 0, <a href="../deepbook/math.md#deepbook_math_EUnderflow">EUnderflow</a>);
    (is_round_down, result)
}
</code></pre>



</details>

<a name="deepbook_math_count_leading_zeros"></a>

## Function `count_leading_zeros`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/math.md#deepbook_math_count_leading_zeros">count_leading_zeros</a>(x: u128): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/math.md#deepbook_math_count_leading_zeros">count_leading_zeros</a>(<b>mut</b> x: u128): u8 {
    <b>if</b> (x == 0) {
        128
    } <b>else</b> {
        <b>let</b> <b>mut</b> n: u8 = 0;
        <b>if</b> (x & 0xFFFFFFFFFFFFFFFF0000000000000000 == 0) {
            // x's higher 64 is all zero, shift the lower part over
            x = x &lt;&lt; 64;
            n = n + 64;
        };
        <b>if</b> (x & 0xFFFFFFFF000000000000000000000000 == 0) {
            // x's higher 32 is all zero, shift the lower part over
            x = x &lt;&lt; 32;
            n = n + 32;
        };
        <b>if</b> (x & 0xFFFF0000000000000000000000000000 == 0) {
            // x's higher 16 is all zero, shift the lower part over
            x = x &lt;&lt; 16;
            n = n + 16;
        };
        <b>if</b> (x & 0xFF000000000000000000000000000000 == 0) {
            // x's higher 8 is all zero, shift the lower part over
            x = x &lt;&lt; 8;
            n = n + 8;
        };
        <b>if</b> (x & 0xF0000000000000000000000000000000 == 0) {
            // x's higher 4 is all zero, shift the lower part over
            x = x &lt;&lt; 4;
            n = n + 4;
        };
        <b>if</b> (x & 0xC0000000000000000000000000000000 == 0) {
            // x's higher 2 is all zero, shift the lower part over
            x = x &lt;&lt; 2;
            n = n + 2;
        };
        <b>if</b> (x & 0x80000000000000000000000000000000 == 0) {
            n = n + 1;
        };
        n
    }
}
</code></pre>



</details>
