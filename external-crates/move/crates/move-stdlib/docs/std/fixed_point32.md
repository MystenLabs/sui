
<a name="std_fixed_point32"></a>

# Module `std::fixed_point32`

Defines a fixed-point numeric type with a 32-bit integer part and
a 32-bit fractional part.


-  [Struct `FixedPoint32`](#std_fixed_point32_FixedPoint32)
-  [Constants](#@Constants_0)
-  [Function `multiply_u64`](#std_fixed_point32_multiply_u64)
-  [Function `divide_u64`](#std_fixed_point32_divide_u64)
-  [Function `create_from_rational`](#std_fixed_point32_create_from_rational)
-  [Function `create_from_raw_value`](#std_fixed_point32_create_from_raw_value)
-  [Function `get_raw_value`](#std_fixed_point32_get_raw_value)
-  [Function `is_zero`](#std_fixed_point32_is_zero)


<pre><code></code></pre>



<a name="std_fixed_point32_FixedPoint32"></a>

## Struct `FixedPoint32`

Define a fixed-point numeric type with 32 fractional bits.
This is just a u64 integer but it is wrapped in a struct to
make a unique type. This is a binary representation, so decimal
values may not be exactly representable, but it provides more
than 9 decimal digits of precision both before and after the
decimal point (18 digits total). For comparison, double precision
floating-point has less than 16 decimal digits of precision, so
be careful about using floating-point to convert these values to
decimal.


<pre><code><b>public</b> <b>struct</b> <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">FixedPoint32</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>value: <a href="../std/u64.md#std_u64">u64</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="std_fixed_point32_EDENOMINATOR"></a>

The denominator provided was zero


<pre><code><b>const</b> <a href="../std/fixed_point32.md#std_fixed_point32_EDENOMINATOR">EDENOMINATOR</a>: <a href="../std/u64.md#std_u64">u64</a> = 65537;
</code></pre>



<a name="std_fixed_point32_EDIVISION"></a>

The quotient value would be too large to be held in a <code><a href="../std/u64.md#std_u64">u64</a></code>


<pre><code><b>const</b> <a href="../std/fixed_point32.md#std_fixed_point32_EDIVISION">EDIVISION</a>: <a href="../std/u64.md#std_u64">u64</a> = 131074;
</code></pre>



<a name="std_fixed_point32_EDIVISION_BY_ZERO"></a>

A division by zero was encountered


<pre><code><b>const</b> <a href="../std/fixed_point32.md#std_fixed_point32_EDIVISION_BY_ZERO">EDIVISION_BY_ZERO</a>: <a href="../std/u64.md#std_u64">u64</a> = 65540;
</code></pre>



<a name="std_fixed_point32_EMULTIPLICATION"></a>

The multiplied value would be too large to be held in a <code><a href="../std/u64.md#std_u64">u64</a></code>


<pre><code><b>const</b> <a href="../std/fixed_point32.md#std_fixed_point32_EMULTIPLICATION">EMULTIPLICATION</a>: <a href="../std/u64.md#std_u64">u64</a> = 131075;
</code></pre>



<a name="std_fixed_point32_ERATIO_OUT_OF_RANGE"></a>

The computed ratio when converting to a <code><a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">FixedPoint32</a></code> would be unrepresentable


<pre><code><b>const</b> <a href="../std/fixed_point32.md#std_fixed_point32_ERATIO_OUT_OF_RANGE">ERATIO_OUT_OF_RANGE</a>: <a href="../std/u64.md#std_u64">u64</a> = 131077;
</code></pre>



<a name="std_fixed_point32_MAX_U64"></a>

> TODO: This is a basic constant and should be provided somewhere centrally in the framework.


<pre><code><b>const</b> <a href="../std/fixed_point32.md#std_fixed_point32_MAX_U64">MAX_U64</a>: <a href="../std/u128.md#std_u128">u128</a> = 18446744073709551615;
</code></pre>



<a name="std_fixed_point32_multiply_u64"></a>

## Function `multiply_u64`

Multiply a u64 integer by a fixed-point number, truncating any
fractional part of the product. This will abort if the product
overflows.


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_multiply_u64">multiply_u64</a>(val: <a href="../std/u64.md#std_u64">u64</a>, multiplier: <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">std::fixed_point32::FixedPoint32</a>): <a href="../std/u64.md#std_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_multiply_u64">multiply_u64</a>(val: <a href="../std/u64.md#std_u64">u64</a>, multiplier: <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">FixedPoint32</a>): <a href="../std/u64.md#std_u64">u64</a> {
    // The product of two 64 bit values <b>has</b> 128 bits, so perform the
    // multiplication with <a href="../std/u128.md#std_u128">u128</a> types and keep the full 128 bit product
    // to avoid losing accuracy.
    <b>let</b> unscaled_product = val <b>as</b> <a href="../std/u128.md#std_u128">u128</a> * (multiplier.value <b>as</b> <a href="../std/u128.md#std_u128">u128</a>);
    // The unscaled product <b>has</b> 32 fractional bits (from the multiplier)
    // so rescale it by shifting away the low bits.
    <b>let</b> product = unscaled_product &gt;&gt; 32;
    // Check whether the value is too large.
    <b>assert</b>!(product &lt;= <a href="../std/fixed_point32.md#std_fixed_point32_MAX_U64">MAX_U64</a>, <a href="../std/fixed_point32.md#std_fixed_point32_EMULTIPLICATION">EMULTIPLICATION</a>);
    product <b>as</b> <a href="../std/u64.md#std_u64">u64</a>
}
</code></pre>



</details>

<a name="std_fixed_point32_divide_u64"></a>

## Function `divide_u64`

Divide a u64 integer by a fixed-point number, truncating any
fractional part of the quotient. This will abort if the divisor
is zero or if the quotient overflows.


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_divide_u64">divide_u64</a>(val: <a href="../std/u64.md#std_u64">u64</a>, divisor: <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">std::fixed_point32::FixedPoint32</a>): <a href="../std/u64.md#std_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_divide_u64">divide_u64</a>(val: <a href="../std/u64.md#std_u64">u64</a>, divisor: <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">FixedPoint32</a>): <a href="../std/u64.md#std_u64">u64</a> {
    // Check <b>for</b> division by zero.
    <b>assert</b>!(divisor.value != 0, <a href="../std/fixed_point32.md#std_fixed_point32_EDIVISION_BY_ZERO">EDIVISION_BY_ZERO</a>);
    // First convert to 128 bits and then shift left to
    // add 32 fractional zero bits to the dividend.
    <b>let</b> scaled_value = val <b>as</b> <a href="../std/u128.md#std_u128">u128</a> &lt;&lt; 32;
    <b>let</b> quotient = scaled_value / (divisor.value <b>as</b> <a href="../std/u128.md#std_u128">u128</a>);
    // Check whether the value is too large.
    <b>assert</b>!(quotient &lt;= <a href="../std/fixed_point32.md#std_fixed_point32_MAX_U64">MAX_U64</a>, <a href="../std/fixed_point32.md#std_fixed_point32_EDIVISION">EDIVISION</a>);
    // the value may be too large, which will cause the cast to fail
    // with an arithmetic error.
    quotient <b>as</b> <a href="../std/u64.md#std_u64">u64</a>
}
</code></pre>



</details>

<a name="std_fixed_point32_create_from_rational"></a>

## Function `create_from_rational`

Create a fixed-point value from a rational number specified by its
numerator and denominator. Calling this function should be preferred
for using <code><a href="../std/fixed_point32.md#std_fixed_point32_create_from_raw_value">Self::create_from_raw_value</a></code> which is also available.
This will abort if the denominator is zero. It will also
abort if the numerator is nonzero and the ratio is not in the range
2^-32 .. 2^32-1. When specifying decimal fractions, be careful about
rounding errors: if you round to display N digits after the decimal
point, you can use a denominator of 10^N to avoid numbers where the
very small imprecision in the binary representation could change the
rounding, e.g., 0.0125 will round down to 0.012 instead of up to 0.013.


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_create_from_rational">create_from_rational</a>(numerator: <a href="../std/u64.md#std_u64">u64</a>, denominator: <a href="../std/u64.md#std_u64">u64</a>): <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">std::fixed_point32::FixedPoint32</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_create_from_rational">create_from_rational</a>(numerator: <a href="../std/u64.md#std_u64">u64</a>, denominator: <a href="../std/u64.md#std_u64">u64</a>): <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">FixedPoint32</a> {
    // If the denominator is zero, this will <b>abort</b>.
    // Scale the numerator to have 64 fractional bits and the denominator
    // to have 32 fractional bits, so that the quotient will have 32
    // fractional bits.
    <b>let</b> scaled_numerator = numerator <b>as</b> <a href="../std/u128.md#std_u128">u128</a> &lt;&lt; 64;
    <b>let</b> scaled_denominator = denominator <b>as</b> <a href="../std/u128.md#std_u128">u128</a> &lt;&lt; 32;
    <b>assert</b>!(scaled_denominator != 0, <a href="../std/fixed_point32.md#std_fixed_point32_EDENOMINATOR">EDENOMINATOR</a>);
    <b>let</b> quotient = scaled_numerator / scaled_denominator;
    <b>assert</b>!(quotient != 0 || numerator == 0, <a href="../std/fixed_point32.md#std_fixed_point32_ERATIO_OUT_OF_RANGE">ERATIO_OUT_OF_RANGE</a>);
    // Return the quotient <b>as</b> a fixed-point number. We first need to check whether the cast
    // can succeed.
    <b>assert</b>!(quotient &lt;= <a href="../std/fixed_point32.md#std_fixed_point32_MAX_U64">MAX_U64</a>, <a href="../std/fixed_point32.md#std_fixed_point32_ERATIO_OUT_OF_RANGE">ERATIO_OUT_OF_RANGE</a>);
    <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">FixedPoint32</a> { value: quotient <b>as</b> <a href="../std/u64.md#std_u64">u64</a> }
}
</code></pre>



</details>

<a name="std_fixed_point32_create_from_raw_value"></a>

## Function `create_from_raw_value`

Create a fixedpoint value from a raw value.


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_create_from_raw_value">create_from_raw_value</a>(value: <a href="../std/u64.md#std_u64">u64</a>): <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">std::fixed_point32::FixedPoint32</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_create_from_raw_value">create_from_raw_value</a>(value: <a href="../std/u64.md#std_u64">u64</a>): <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">FixedPoint32</a> {
    <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">FixedPoint32</a> { value }
}
</code></pre>



</details>

<a name="std_fixed_point32_get_raw_value"></a>

## Function `get_raw_value`

Accessor for the raw u64 value. Other less common operations, such as
adding or subtracting FixedPoint32 values, can be done using the raw
values directly.


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_get_raw_value">get_raw_value</a>(num: <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">std::fixed_point32::FixedPoint32</a>): <a href="../std/u64.md#std_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_get_raw_value">get_raw_value</a>(num: <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">FixedPoint32</a>): <a href="../std/u64.md#std_u64">u64</a> {
    num.value
}
</code></pre>



</details>

<a name="std_fixed_point32_is_zero"></a>

## Function `is_zero`

Returns true if the ratio is zero.


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_is_zero">is_zero</a>(num: <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">std::fixed_point32::FixedPoint32</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/fixed_point32.md#std_fixed_point32_is_zero">is_zero</a>(num: <a href="../std/fixed_point32.md#std_fixed_point32_FixedPoint32">FixedPoint32</a>): bool {
    num.value == 0
}
</code></pre>



</details>


[//]: # ("File containing references which can be used from documentation")
