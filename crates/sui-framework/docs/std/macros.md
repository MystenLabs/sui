---
title: Module `std::macros`
---

This module holds shared implementation of macros used in <code>std</code>


-  [Function `num_max`](#std_macros_num_max)
-  [Function `num_min`](#std_macros_num_min)
-  [Function `num_diff`](#std_macros_num_diff)
-  [Function `num_divide_and_round_up`](#std_macros_num_divide_and_round_up)
-  [Function `num_pow`](#std_macros_num_pow)
-  [Function `num_sqrt`](#std_macros_num_sqrt)
-  [Function `num_to_string`](#std_macros_num_to_string)
-  [Function `range_do`](#std_macros_range_do)
-  [Function `range_do_eq`](#std_macros_range_do_eq)
-  [Function `do`](#std_macros_do)
-  [Function `do_eq`](#std_macros_do_eq)
-  [Function `try_as_u8`](#std_macros_try_as_u8)
-  [Function `try_as_u16`](#std_macros_try_as_u16)
-  [Function `try_as_u32`](#std_macros_try_as_u32)
-  [Function `try_as_u64`](#std_macros_try_as_u64)
-  [Function `try_as_u128`](#std_macros_try_as_u128)
-  [Function `uq_from_quotient`](#std_macros_uq_from_quotient)
-  [Function `uq_from_int`](#std_macros_uq_from_int)
-  [Function `uq_add`](#std_macros_uq_add)
-  [Function `uq_sub`](#std_macros_uq_sub)
-  [Function `uq_to_int`](#std_macros_uq_to_int)
-  [Function `uq_int_mul`](#std_macros_uq_int_mul)
-  [Function `uq_int_div`](#std_macros_uq_int_div)


<pre><code></code></pre>



<a name="std_macros_num_max"></a>

## Function `num_max`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_max">num_max</a>($x: _, $y: _): _
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_max">num_max</a>($x: _, $y: _): _ {
    <b>let</b> x = $x;
    <b>let</b> y = $y;
    <b>if</b> (x &gt; y) x
    <b>else</b> y
}
</code></pre>



</details>

<a name="std_macros_num_min"></a>

## Function `num_min`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_min">num_min</a>($x: _, $y: _): _
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_min">num_min</a>($x: _, $y: _): _ {
    <b>let</b> x = $x;
    <b>let</b> y = $y;
    <b>if</b> (x &lt; y) x
    <b>else</b> y
}
</code></pre>



</details>

<a name="std_macros_num_diff"></a>

## Function `num_diff`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_diff">num_diff</a>($x: _, $y: _): _
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_diff">num_diff</a>($x: _, $y: _): _ {
    <b>let</b> x = $x;
    <b>let</b> y = $y;
    <b>if</b> (x &gt; y) x - y
    <b>else</b> y - x
}
</code></pre>



</details>

<a name="std_macros_num_divide_and_round_up"></a>

## Function `num_divide_and_round_up`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_divide_and_round_up">num_divide_and_round_up</a>($x: _, $y: _): _
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_divide_and_round_up">num_divide_and_round_up</a>($x: _, $y: _): _ {
    <b>let</b> x = $x;
    <b>let</b> y = $y;
    <b>if</b> (x % y == 0) x / y
    <b>else</b> x / y + 1
}
</code></pre>



</details>

<a name="std_macros_num_pow"></a>

## Function `num_pow`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_pow">num_pow</a>($base: _, $exponent: <a href="../std/u8.md#std_u8">u8</a>): _
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_pow">num_pow</a>($base: _, $exponent: <a href="../std/u8.md#std_u8">u8</a>): _ {
    <b>let</b> <b>mut</b> base = $base;
    <b>let</b> <b>mut</b> exponent = $exponent;
    <b>let</b> <b>mut</b> res = 1;
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

<a name="std_macros_num_sqrt"></a>

## Function `num_sqrt`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_sqrt">num_sqrt</a>&lt;$T, $U&gt;($x: $T, $bitsize: <a href="../std/u8.md#std_u8">u8</a>): $T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_sqrt">num_sqrt</a>&lt;$T, $U&gt;($x: $T, $bitsize: <a href="../std/u8.md#std_u8">u8</a>): $T {
    <b>let</b> x = $x;
    <b>let</b> <b>mut</b> bit = (1: $U) &lt;&lt; $bitsize;
    <b>let</b> <b>mut</b> res = (0: $U);
    <b>let</b> <b>mut</b> x = x <b>as</b> $U;
    <b>while</b> (bit != 0) {
        <b>if</b> (x &gt;= res + bit) {
            x = x - (res + bit);
            res = (res &gt;&gt; 1) + bit;
        } <b>else</b> {
            res = res &gt;&gt; 1;
        };
        bit = bit &gt;&gt; 2;
    };
    res <b>as</b> $T
}
</code></pre>



</details>

<a name="std_macros_num_to_string"></a>

## Function `num_to_string`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_to_string">num_to_string</a>($x: _): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_num_to_string">num_to_string</a>($x: _): String {
    <b>let</b> <b>mut</b> x = $x;
    <b>if</b> (x == 0) {
        <b>return</b> b"0".to_string()
    };
    <b>let</b> <b>mut</b> buffer = <a href="../std/vector.md#std_vector">vector</a>[];
    <b>while</b> (x != 0) {
        buffer.push_back(((48 + x % 10) <b>as</b> <a href="../std/u8.md#std_u8">u8</a>));
        x = x / 10;
    };
    buffer.reverse();
    buffer.to_string()
}
</code></pre>



</details>

<a name="std_macros_range_do"></a>

## Function `range_do`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_range_do">range_do</a>($start: _, $stop: _, $f: |_| -&gt; ())
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_range_do">range_do</a>($start: _, $stop: _, $f: |_|) {
    <b>let</b> <b>mut</b> i = $start;
    <b>let</b> stop = $stop;
    <b>while</b> (i &lt; stop) {
        $f(i);
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="std_macros_range_do_eq"></a>

## Function `range_do_eq`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_range_do_eq">range_do_eq</a>($start: _, $stop: _, $f: |_| -&gt; ())
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_range_do_eq">range_do_eq</a>($start: _, $stop: _, $f: |_|) {
    <b>let</b> <b>mut</b> i = $start;
    <b>let</b> stop = $stop;
    // we check `i &gt;= stop` inside the <b>loop</b> instead of `i &lt;= stop` <b>as</b> `<b>while</b>` condition to avoid
    // incrementing `i` past the MAX integer value.
    // Because of this, we need to check <b>if</b> `i &gt; stop` and <b>return</b> early--instead of letting the
    // <b>loop</b> bound handle it, like in the `<a href="../std/macros.md#std_macros_range_do">range_do</a>` <b>macro</b>.
    <b>if</b> (i &gt; stop) <b>return</b>;
    <b>loop</b> {
        $f(i);
        <b>if</b> (i &gt;= stop) <b>break</b>;
        i = i + 1;
    }
}
</code></pre>



</details>

<a name="std_macros_do"></a>

## Function `do`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_do">do</a>($stop: _, $f: |_| -&gt; ())
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_do">do</a>($stop: _, $f: |_|) {
    <a href="../std/macros.md#std_macros_range_do">range_do</a>!(0, $stop, $f)
}
</code></pre>



</details>

<a name="std_macros_do_eq"></a>

## Function `do_eq`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_do_eq">do_eq</a>($stop: _, $f: |_| -&gt; ())
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_do_eq">do_eq</a>($stop: _, $f: |_|) {
    <a href="../std/macros.md#std_macros_range_do_eq">range_do_eq</a>!(0, $stop, $f)
}
</code></pre>



</details>

<a name="std_macros_try_as_u8"></a>

## Function `try_as_u8`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_try_as_u8">try_as_u8</a>($x: _): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../std/u8.md#std_u8">u8</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_try_as_u8">try_as_u8</a>($x: _): Option&lt;<a href="../std/u8.md#std_u8">u8</a>&gt; {
    <b>let</b> x = $x;
    <b>if</b> (x &gt; 0xFF) <a href="../std/option.md#std_option_none">option::none</a>()
    <b>else</b> <a href="../std/option.md#std_option_some">option::some</a>(x <b>as</b> <a href="../std/u8.md#std_u8">u8</a>)
}
</code></pre>



</details>

<a name="std_macros_try_as_u16"></a>

## Function `try_as_u16`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_try_as_u16">try_as_u16</a>($x: _): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../std/u16.md#std_u16">u16</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_try_as_u16">try_as_u16</a>($x: _): Option&lt;<a href="../std/u16.md#std_u16">u16</a>&gt; {
    <b>let</b> x = $x;
    <b>if</b> (x &gt; 0xFFFF) <a href="../std/option.md#std_option_none">option::none</a>()
    <b>else</b> <a href="../std/option.md#std_option_some">option::some</a>(x <b>as</b> <a href="../std/u16.md#std_u16">u16</a>)
}
</code></pre>



</details>

<a name="std_macros_try_as_u32"></a>

## Function `try_as_u32`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_try_as_u32">try_as_u32</a>($x: _): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../std/u32.md#std_u32">u32</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_try_as_u32">try_as_u32</a>($x: _): Option&lt;<a href="../std/u32.md#std_u32">u32</a>&gt; {
    <b>let</b> x = $x;
    <b>if</b> (x &gt; 0xFFFF_FFFF) <a href="../std/option.md#std_option_none">option::none</a>()
    <b>else</b> <a href="../std/option.md#std_option_some">option::some</a>(x <b>as</b> <a href="../std/u32.md#std_u32">u32</a>)
}
</code></pre>



</details>

<a name="std_macros_try_as_u64"></a>

## Function `try_as_u64`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_try_as_u64">try_as_u64</a>($x: _): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../std/u64.md#std_u64">u64</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_try_as_u64">try_as_u64</a>($x: _): Option&lt;<a href="../std/u64.md#std_u64">u64</a>&gt; {
    <b>let</b> x = $x;
    <b>if</b> (x &gt; 0xFFFF_FFFF_FFFF_FFFF) <a href="../std/option.md#std_option_none">option::none</a>()
    <b>else</b> <a href="../std/option.md#std_option_some">option::some</a>(x <b>as</b> <a href="../std/u64.md#std_u64">u64</a>)
}
</code></pre>



</details>

<a name="std_macros_try_as_u128"></a>

## Function `try_as_u128`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_try_as_u128">try_as_u128</a>($x: _): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../std/u128.md#std_u128">u128</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_try_as_u128">try_as_u128</a>($x: _): Option&lt;<a href="../std/u128.md#std_u128">u128</a>&gt; {
    <b>let</b> x = $x;
    <b>if</b> (x &gt; 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF) <a href="../std/option.md#std_option_none">option::none</a>()
    <b>else</b> <a href="../std/option.md#std_option_some">option::some</a>(x <b>as</b> <a href="../std/u128.md#std_u128">u128</a>)
}
</code></pre>



</details>

<a name="std_macros_uq_from_quotient"></a>

## Function `uq_from_quotient`

Creates a fixed-point value from a quotient specified by its numerator and denominator.
<code>$T</code> is the underlying integer type for the fixed-point value, where <code>$T</code> has <code>$t_bits</code> bits.
<code>$U</code> is the type used for intermediate calculations, where <code>$U</code> is the next larger integer type.
<code>$max_t</code> is the maximum value that can be represented by <code>$T</code>.
<code>$t_bits</code> (as mentioned above) is the total number of bits in the fixed-point value (integer
plus fractional).
<code>$fractional_bits</code> is the number of fractional bits in the fixed-point value.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_from_quotient">uq_from_quotient</a>&lt;$T, $U&gt;($numerator: $T, $denominator: $T, $max_t: $T, $t_bits: <a href="../std/u8.md#std_u8">u8</a>, $fractional_bits: <a href="../std/u8.md#std_u8">u8</a>, $abort_denominator: _, $abort_quotient_too_small: _, $abort_quotient_too_large: _): $T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_from_quotient">uq_from_quotient</a>&lt;$T, $U&gt;(
    $numerator: $T,
    $denominator: $T,
    $max_t: $T,
    $t_bits: <a href="../std/u8.md#std_u8">u8</a>,
    $fractional_bits: <a href="../std/u8.md#std_u8">u8</a>,
    $abort_denominator: _,
    $abort_quotient_too_small: _,
    $abort_quotient_too_large: _,
): $T {
    <b>let</b> numerator = $numerator;
    <b>let</b> denominator = $denominator;
    <b>if</b> (denominator == 0) $abort_denominator;
    // Scale the numerator to have `$t_bits` fractional bits and the denominator to have
    // `$t_bits - $fractional_bits` fractional bits, so that the quotient will have
    // `$fractional_bits` fractional bits.
    <b>let</b> scaled_numerator = numerator <b>as</b> $U &lt;&lt; $t_bits;
    <b>let</b> scaled_denominator = denominator <b>as</b> $U &lt;&lt; ($t_bits - $fractional_bits);
    <b>let</b> quotient = scaled_numerator / scaled_denominator;
    // The quotient can only be zero <b>if</b> the numerator is also zero.
    <b>if</b> (quotient == 0 && numerator != 0) $abort_quotient_too_small;
    // Return the quotient <b>as</b> a fixed-point number. We first need to check whether the cast
    // can succeed.
    <b>if</b> (quotient &gt; $max_t <b>as</b> $U) $abort_quotient_too_large;
    quotient <b>as</b> $T
}
</code></pre>



</details>

<a name="std_macros_uq_from_int"></a>

## Function `uq_from_int`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_from_int">uq_from_int</a>&lt;$T, $U&gt;($integer: $T, $fractional_bits: <a href="../std/u8.md#std_u8">u8</a>): $U
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_from_int">uq_from_int</a>&lt;$T, $U&gt;($integer: $T, $fractional_bits: <a href="../std/u8.md#std_u8">u8</a>): $U {
    ($integer <b>as</b> $U) &lt;&lt; $fractional_bits
}
</code></pre>



</details>

<a name="std_macros_uq_add"></a>

## Function `uq_add`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_add">uq_add</a>&lt;$T, $U&gt;($a: $T, $b: $T, $max_t: $T, $abort_overflow: _): $T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_add">uq_add</a>&lt;$T, $U&gt;($a: $T, $b: $T, $max_t: $T, $abort_overflow: _): $T {
    <b>let</b> sum = $a <b>as</b> $U + ($b <b>as</b> $U);
    <b>if</b> (sum &gt; $max_t <b>as</b> $U) $abort_overflow;
    sum <b>as</b> $T
}
</code></pre>



</details>

<a name="std_macros_uq_sub"></a>

## Function `uq_sub`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_sub">uq_sub</a>&lt;$T&gt;($a: $T, $b: $T, $abort_overflow: _): $T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_sub">uq_sub</a>&lt;$T&gt;($a: $T, $b: $T, $abort_overflow: _): $T {
    <b>let</b> a = $a;
    <b>let</b> b = $b;
    <b>if</b> (a &lt; b) $abort_overflow;
    a - b
}
</code></pre>



</details>

<a name="std_macros_uq_to_int"></a>

## Function `uq_to_int`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_to_int">uq_to_int</a>&lt;$T, $U&gt;($a: $U, $fractional_bits: <a href="../std/u8.md#std_u8">u8</a>): $T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_to_int">uq_to_int</a>&lt;$T, $U&gt;($a: $U, $fractional_bits: <a href="../std/u8.md#std_u8">u8</a>): $T {
    ($a &gt;&gt; $fractional_bits) <b>as</b> $T
}
</code></pre>



</details>

<a name="std_macros_uq_int_mul"></a>

## Function `uq_int_mul`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_int_mul">uq_int_mul</a>&lt;$T, $U&gt;($val: $T, $multiplier: $T, $max_t: $T, $fractional_bits: <a href="../std/u8.md#std_u8">u8</a>, $abort_overflow: _): $T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_int_mul">uq_int_mul</a>&lt;$T, $U&gt;(
    $val: $T,
    $multiplier: $T,
    $max_t: $T,
    $fractional_bits: <a href="../std/u8.md#std_u8">u8</a>,
    $abort_overflow: _,
): $T {
    // The product of two `$T` bit values <b>has</b> the same number of bits <b>as</b> `$U`, so perform the
    // multiplication with `$U` types and keep the full `$U` bit product
    // to avoid losing accuracy.
    <b>let</b> unscaled_product = $val <b>as</b> $U * ($multiplier <b>as</b> $U);
    // The unscaled product <b>has</b> `$fractional_bits` fractional bits (from the multiplier)
    // so rescale it by shifting away the low bits.
    <b>let</b> product = unscaled_product &gt;&gt; $fractional_bits;
    // Check whether the value is too large.
    <b>if</b> (product &gt; $max_t <b>as</b> $U) $abort_overflow;
    product <b>as</b> $T
}
</code></pre>



</details>

<a name="std_macros_uq_int_div"></a>

## Function `uq_int_div`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_int_div">uq_int_div</a>&lt;$T, $U&gt;($val: $T, $divisor: $T, $max_t: $T, $fractional_bits: <a href="../std/u8.md#std_u8">u8</a>, $abort_division_by_zero: _, $abort_overflow: _): $T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_uq_int_div">uq_int_div</a>&lt;$T, $U&gt;(
    $val: $T,
    $divisor: $T,
    $max_t: $T,
    $fractional_bits: <a href="../std/u8.md#std_u8">u8</a>,
    $abort_division_by_zero: _,
    $abort_overflow: _,
): $T {
    <b>let</b> val = $val;
    <b>let</b> divisor = $divisor;
    // Check <b>for</b> division by zero.
    <b>if</b> (divisor == 0) $abort_division_by_zero;
    // First convert to $U to increase the number of bits to the next integer size
    // and then shift left to add `$fractional_bits` fractional zero bits to the dividend.
    <b>let</b> scaled_value = val <b>as</b> $U &lt;&lt; $fractional_bits;
    <b>let</b> quotient = scaled_value / (divisor <b>as</b> $U);
    // Check whether the value is too large.
    <b>if</b> (quotient &gt; $max_t <b>as</b> $U) $abort_overflow;
    quotient <b>as</b> $T
}
</code></pre>



</details>
