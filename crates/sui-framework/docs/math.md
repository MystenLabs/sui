
<a name="0x2_math"></a>

# Module `0x2::math`

Basic math for nicer programmability


-  [Function `max`](#0x2_math_max)
-  [Function `min`](#0x2_math_min)
-  [Function `diff`](#0x2_math_diff)
-  [Function `pow`](#0x2_math_pow)
-  [Function `sqrt`](#0x2_math_sqrt)
-  [Function `sqrt_u128`](#0x2_math_sqrt_u128)
-  [Function `divide_and_round_up`](#0x2_math_divide_and_round_up)


<pre><code></code></pre>



<a name="0x2_math_max"></a>

## Function `max`

Return the larger of <code>x</code> and <code>y</code>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_max">max</a>(x: u64, y: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_max">max</a>(x: u64, y: u64): u64 {
    <b>if</b> (x &gt; y) {
        x
    } <b>else</b> {
        y
    }
}
</code></pre>



</details>

<a name="0x2_math_min"></a>

## Function `min`

Return the smaller of <code>x</code> and <code>y</code>


<pre><code><b>public</b> <b>fun</b> <b>min</b>(x: u64, y: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <b>min</b>(x: u64, y: u64): u64 {
    <b>if</b> (x &lt; y) {
        x
    } <b>else</b> {
        y
    }
}
</code></pre>



</details>

<a name="0x2_math_diff"></a>

## Function `diff`

Return the absolute value of x - y


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_diff">diff</a>(x: u64, y: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_diff">diff</a>(x: u64, y: u64): u64 {
    <b>if</b> (x &gt; y) {
        x - y
    } <b>else</b> {
        y - x
    }
}
</code></pre>



</details>

<a name="0x2_math_pow"></a>

## Function `pow`

Return the value of a base raised to a power


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_pow">pow</a>(base: u64, exponent: u8): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_pow">pow</a>(base: u64, exponent: u8): u64 {
    <b>let</b> res = 1;
    <b>while</b> (exponent &gt;= 1) {
        <b>if</b> (exponent % 2 == 0) {
            base = base * base;
            exponent = exponent / 2;
        } <b>else</b> {
            res = res * base;
            exponent = exponent - 1;
        }
    };

    res
}
</code></pre>



</details>

<a name="0x2_math_sqrt"></a>

## Function `sqrt`

Get a nearest lower integer Square Root for <code>x</code>. Given that this
function can only operate with integers, it is impossible
to get perfect (or precise) integer square root for some numbers.

Example:
```
math::sqrt(9) => 3
math::sqrt(8) => 2 // the nearest lower square root is 4;
```

In integer math, one of the possible ways to get results with more
precision is to use higher values or temporarily multiply the
value by some bigger number. Ideally if this is a square of 10 or 100.

Example:
```
math::sqrt(8) => 2;
math::sqrt(8 * 10000) => 282;
// now we can use this value as if it was 2.82;
// but to get the actual result, this value needs
// to be divided by 100 (because sqrt(10000)).


math::sqrt(8 * 1000000) => 2828; // same as above, 2828 / 1000 (2.828)
```


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_sqrt">sqrt</a>(x: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_sqrt">sqrt</a>(x: u64): u64 {
    <b>let</b> bit = 1u128 &lt;&lt; 64;
    <b>let</b> res = 0u128;
    <b>let</b> x = (x <b>as</b> u128);

    <b>while</b> (bit != 0) {
        <b>if</b> (x &gt;= res + bit) {
            x = x - (res + bit);
            res = (res &gt;&gt; 1) + bit;
        } <b>else</b> {
            res = res &gt;&gt; 1;
        };
        bit = bit &gt;&gt; 2;
    };

    (res <b>as</b> u64)
}
</code></pre>



</details>

<a name="0x2_math_sqrt_u128"></a>

## Function `sqrt_u128`

Similar to math::sqrt, but for u128 numbers. Get a nearest lower integer Square Root for <code>x</code>. Given that this
function can only operate with integers, it is impossible
to get perfect (or precise) integer square root for some numbers.

Example:
```
math::sqrt_u128(9) => 3
math::sqrt_u128(8) => 2 // the nearest lower square root is 4;
```

In integer math, one of the possible ways to get results with more
precision is to use higher values or temporarily multiply the
value by some bigger number. Ideally if this is a square of 10 or 100.

Example:
```
math::sqrt_u128(8) => 2;
math::sqrt_u128(8 * 10000) => 282;
// now we can use this value as if it was 2.82;
// but to get the actual result, this value needs
// to be divided by 100 (because sqrt_u128(10000)).


math::sqrt_u128(8 * 1000000) => 2828; // same as above, 2828 / 1000 (2.828)
```


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_sqrt_u128">sqrt_u128</a>(x: u128): u128
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_sqrt_u128">sqrt_u128</a>(x: u128): u128 {
    <b>let</b> bit = 1u256 &lt;&lt; 128;
    <b>let</b> res = 0u256;
    <b>let</b> x = (x <b>as</b> u256);

    <b>while</b> (bit != 0) {
        <b>if</b> (x &gt;= res + bit) {
            x = x - (res + bit);
            res = (res &gt;&gt; 1) + bit;
        } <b>else</b> {
            res = res &gt;&gt; 1;
        };
        bit = bit &gt;&gt; 2;
    };

    (res <b>as</b> u128)
}
</code></pre>



</details>

<a name="0x2_math_divide_and_round_up"></a>

## Function `divide_and_round_up`

Calculate x / y, but round up the result.


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_divide_and_round_up">divide_and_round_up</a>(x: u64, y: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="math.md#0x2_math_divide_and_round_up">divide_and_round_up</a>(x: u64, y: u64): u64 {
    <b>if</b> (x % y == 0) {
        x / y
    } <b>else</b> {
        x / y + 1
    }
}
</code></pre>



</details>
