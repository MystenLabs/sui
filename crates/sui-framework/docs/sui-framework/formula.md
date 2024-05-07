---
title: Module `0x2::formula`
---



-  [Struct `Expr`](#0x2_formula_Expr)
-  [Struct `Formula`](#0x2_formula_Formula)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_formula_new)
-  [Function `div`](#0x2_formula_div)
-  [Function `mul`](#0x2_formula_mul)
-  [Function `add`](#0x2_formula_add)
-  [Function `sub`](#0x2_formula_sub)
-  [Function `scale`](#0x2_formula_scale)
-  [Function `sqrt`](#0x2_formula_sqrt)
-  [Function `calculate_u8`](#0x2_formula_calculate_u8)
-  [Function `calculate_u64`](#0x2_formula_calculate_u64)
-  [Function `calculate_u128`](#0x2_formula_calculate_u128)
-  [Function `log2_u256`](#0x2_formula_log2_u256)
-  [Function `min_u256`](#0x2_formula_min_u256)
-  [Function `sqrt_u256`](#0x2_formula_sqrt_u256)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="math.md#0x2_math">0x2::math</a>;
</code></pre>



<a name="0x2_formula_Expr"></a>

## Struct `Expr`



<pre><code><b>struct</b> <a href="formula.md#0x2_formula_Expr">Expr</a>&lt;T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>op: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>args: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_formula_Formula"></a>

## Struct `Formula`



<pre><code><b>struct</b> <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>expressions: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="formula.md#0x2_formula_Expr">formula::Expr</a>&lt;T&gt;&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>scaling: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_formula_EOverflow"></a>



<pre><code><b>const</b> <a href="formula.md#0x2_formula_EOverflow">EOverflow</a>: u64 = 0;
</code></pre>



<a name="0x2_formula_EDivideByZero"></a>



<pre><code><b>const</b> <a href="formula.md#0x2_formula_EDivideByZero">EDivideByZero</a>: u64 = 2;
</code></pre>



<a name="0x2_formula_EUnderflow"></a>



<pre><code><b>const</b> <a href="formula.md#0x2_formula_EUnderflow">EUnderflow</a>: u64 = 1;
</code></pre>



<a name="0x2_formula_new"></a>

## Function `new`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_new">new</a>&lt;T&gt;(): <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_new">new</a>&lt;T&gt;(): <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt; {
    <a href="formula.md#0x2_formula_Formula">Formula</a> { expressions: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[], scaling: <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>() }
}
</code></pre>



</details>

<a name="0x2_formula_div"></a>

## Function `div`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_div">div</a>&lt;T&gt;(self: <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;, other: T): <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_div">div</a>&lt;T&gt;(<b>mut</b> self: <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt;, other: T): <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt; {
    self.expressions.push_back(<a href="formula.md#0x2_formula_Expr">Expr</a> { op: b"div", args: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[ other ] });
    self
}
</code></pre>



</details>

<a name="0x2_formula_mul"></a>

## Function `mul`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_mul">mul</a>&lt;T&gt;(self: <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;, other: T): <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_mul">mul</a>&lt;T&gt;(<b>mut</b> self: <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt;, other: T): <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt; {
    self.expressions.push_back(<a href="formula.md#0x2_formula_Expr">Expr</a> { op: b"mul", args: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[ other ] });
    self
}
</code></pre>



</details>

<a name="0x2_formula_add"></a>

## Function `add`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_add">add</a>&lt;T&gt;(self: <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;, other: T): <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_add">add</a>&lt;T&gt;(<b>mut</b> self: <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt;, other: T): <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt; {
    self.expressions.push_back(<a href="formula.md#0x2_formula_Expr">Expr</a> { op: b"add", args: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[ other ] });
    self
}
</code></pre>



</details>

<a name="0x2_formula_sub"></a>

## Function `sub`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_sub">sub</a>&lt;T&gt;(self: <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;, other: T): <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_sub">sub</a>&lt;T&gt;(<b>mut</b> self: <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt;, other: T): <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt; {
    self.expressions.push_back(<a href="formula.md#0x2_formula_Expr">Expr</a> { op: b"sub", args: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[ other ] });
    self
}
</code></pre>



</details>

<a name="0x2_formula_scale"></a>

## Function `scale`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_scale">scale</a>&lt;T&gt;(self: <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;, scaling: T): <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_scale">scale</a>&lt;T&gt;(<b>mut</b> self: <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt;, scaling: T): <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt; {
    self.scaling.fill(scaling);
    self
}
</code></pre>



</details>

<a name="0x2_formula_sqrt"></a>

## Function `sqrt`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_sqrt">sqrt</a>&lt;T&gt;(self: <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;): <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_sqrt">sqrt</a>&lt;T&gt;(<b>mut</b> self: <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt;): <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;T&gt; {
    self.expressions.push_back(<a href="formula.md#0x2_formula_Expr">Expr</a> { op: b"sqrt", args: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[] });
    self
}
</code></pre>



</details>

<a name="0x2_formula_calculate_u8"></a>

## Function `calculate_u8`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_calculate_u8">calculate_u8</a>(self: <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;u8&gt;, value: u8): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_calculate_u8">calculate_u8</a>(self: <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;u8&gt;, value: u8): u8 {
    <b>let</b> <a href="formula.md#0x2_formula_Formula">Formula</a> { <b>mut</b> expressions, scaling: _ } = self;
    <b>let</b> <b>mut</b> result = value <b>as</b> u16;
    expressions.reverse();
    <b>while</b> (expressions.length() &gt; 0) {
        <b>let</b> <a href="formula.md#0x2_formula_Expr">Expr</a> { op, args } = expressions.pop_back();
        <b>if</b> (op == b"div") {
            result = result / (args[0] <b>as</b> u16);
        } <b>else</b> <b>if</b> (op == b"mul") {
            result = result * (args[0] <b>as</b> u16);
        } <b>else</b> <b>if</b> (op == b"add") {
            result = result + (args[0] <b>as</b> u16);
        } <b>else</b> <b>if</b> (op == b"sub") {
            result = result - (args[0] <b>as</b> u16);
        } <b>else</b> <b>if</b> (op == b"sqrt") {
            result = <a href="math.md#0x2_math_sqrt">math::sqrt</a>((result <b>as</b> u64) * 10000) <b>as</b> u16;
        }
    };

    <b>assert</b>!(result &lt; 255, <a href="formula.md#0x2_formula_EOverflow">EOverflow</a>);
    (result <b>as</b> u8)
}
</code></pre>



</details>

<a name="0x2_formula_calculate_u64"></a>

## Function `calculate_u64`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_calculate_u64">calculate_u64</a>(self: <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;u64&gt;, value: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_calculate_u64">calculate_u64</a>(self: <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;u64&gt;, value: u64): u64 {
    <b>let</b> <a href="formula.md#0x2_formula_Formula">Formula</a> { <b>mut</b> expressions, scaling: _ } = self;
    <b>let</b> <b>mut</b> result = value;
    expressions.reverse();
    <b>while</b> (expressions.length() &gt; 0) {
        <b>let</b> <a href="formula.md#0x2_formula_Expr">Expr</a> { op, args } = expressions.pop_back();
        <b>if</b> (op == b"div") {
            result = result / args[0];
        } <b>else</b> <b>if</b> (op == b"mul") {
            result = result * args[0];
        } <b>else</b> <b>if</b> (op == b"add") {
            result = result + args[0];
        } <b>else</b> <b>if</b> (op == b"sub") {
            result = result - args[0];
        } <b>else</b> <b>if</b> (op == b"sqrt") {
            result = <a href="math.md#0x2_math_sqrt">math::sqrt</a>(result * 10000);
        }
    };

    result
}
</code></pre>



</details>

<a name="0x2_formula_calculate_u128"></a>

## Function `calculate_u128`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_calculate_u128">calculate_u128</a>(self: <a href="formula.md#0x2_formula_Formula">formula::Formula</a>&lt;u128&gt;, value: u128): u128
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_calculate_u128">calculate_u128</a>(self: <a href="formula.md#0x2_formula_Formula">Formula</a>&lt;u128&gt;, value: u128): u128 {
    <b>let</b> <a href="formula.md#0x2_formula_Formula">Formula</a> { <b>mut</b> expressions, scaling } = self;
    <b>let</b> scaling = scaling.destroy_with_default(1 &lt;&lt; 64) <b>as</b> u256;
    <b>let</b> <b>mut</b> is_scaled = <b>false</b>;
    <b>let</b> <b>mut</b> result = (value <b>as</b> u256);

    expressions.reverse();

    <b>while</b> (expressions.length() &gt; 0) {
        <b>let</b> <a href="formula.md#0x2_formula_Expr">Expr</a> { op, args } = expressions.pop_back();
        <b>if</b> (op == b"div") {
            <b>assert</b>!(args[0] != 0, <a href="formula.md#0x2_formula_EDivideByZero">EDivideByZero</a>);
            <b>if</b> (is_scaled) {
                result = (result) / (args[0] <b>as</b> u256);
            } <b>else</b> {
                result = (result * scaling) / (args[0] <b>as</b> u256);
                is_scaled = <b>true</b>;
            }
        } <b>else</b> <b>if</b> (op == b"mul") {
            result = result * (args[0] <b>as</b> u256);
        } <b>else</b> <b>if</b> (op == b"add") {
            <b>if</b> (is_scaled) {
                result = result + (args[0] <b>as</b> u256 * scaling);
            } <b>else</b> {
                result = result + (args[0] <b>as</b> u256);
            }
        } <b>else</b> <b>if</b> (op == b"sub") {
            <b>if</b> (is_scaled) {
                <b>assert</b>!(result &gt;= (args[0] <b>as</b> u256 * scaling), <a href="formula.md#0x2_formula_EUnderflow">EUnderflow</a>);
                result = result - (args[0] <b>as</b> u256 * scaling);
            } <b>else</b> {
                <b>assert</b>!(result &gt;= (args[0] <b>as</b> u256), <a href="formula.md#0x2_formula_EUnderflow">EUnderflow</a>);
                result = result - (args[0] <b>as</b> u256);
            }
        } <b>else</b> <b>if</b> (op == b"sqrt") {
            <b>if</b> (is_scaled) {
                result = <a href="formula.md#0x2_formula_sqrt_u256">sqrt_u256</a>(result * scaling);
            } <b>else</b> {
                result = <a href="formula.md#0x2_formula_sqrt_u256">sqrt_u256</a>(result * scaling * scaling);
                is_scaled = <b>true</b>;
            }
        }
    };

    <b>if</b> (is_scaled) {
        result = result / scaling;
    };

    <b>assert</b>!(result &lt; 340_282_366_920_938_463_463_374_607_431_768_211_455u256, <a href="formula.md#0x2_formula_EOverflow">EOverflow</a>);

    result <b>as</b> u128
}
</code></pre>



</details>

<a name="0x2_formula_log2_u256"></a>

## Function `log2_u256`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_log2_u256">log2_u256</a>(x: u256): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_log2_u256">log2_u256</a>(<b>mut</b> x: u256): u8 {
    <b>let</b> <b>mut</b> result = 0;
    <b>if</b> (x &gt;&gt; 128 &gt; 0) {
        x = x &gt;&gt; 128;
        result = result + 128;
    };

    <b>if</b> (x &gt;&gt; 64 &gt; 0) {
        x = x &gt;&gt; 64;
        result = result + 64;
    };

    <b>if</b> (x &gt;&gt; 32 &gt; 0) {
        x = x &gt;&gt; 32;
        result = result + 32;
    };

    <b>if</b> (x &gt;&gt; 16 &gt; 0) {
        x = x &gt;&gt; 16;
        result = result + 16;
    };

    <b>if</b> (x &gt;&gt; 8 &gt; 0) {
        x = x &gt;&gt; 8;
        result = result + 8;
    };

    <b>if</b> (x &gt;&gt; 4 &gt; 0) {
        x = x &gt;&gt; 4;
        result = result + 4;
    };

    <b>if</b> (x &gt;&gt; 2 &gt; 0) {
        x = x &gt;&gt; 2;
        result = result + 2;
    };

    <b>if</b> (x &gt;&gt; 1 &gt; 0)
        result = result + 1;

    result
}
</code></pre>



</details>

<a name="0x2_formula_min_u256"></a>

## Function `min_u256`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_min_u256">min_u256</a>(x: u256, y: u256): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_min_u256">min_u256</a>(x: u256, y: u256): u256 {
    <b>if</b> (x &lt; y) {
        x
    } <b>else</b> {
        y
    }
}
</code></pre>



</details>

<a name="0x2_formula_sqrt_u256"></a>

## Function `sqrt_u256`



<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_sqrt_u256">sqrt_u256</a>(x: u256): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="formula.md#0x2_formula_sqrt_u256">sqrt_u256</a>(x: u256): u256 {
    <b>if</b> (x == 0) <b>return</b> 0;

    <b>let</b> <b>mut</b> result = 1 &lt;&lt; ((<a href="formula.md#0x2_formula_log2_u256">log2_u256</a>(x) &gt;&gt; 1) <b>as</b> u8);

    result = (result + x / result) &gt;&gt; 1;
    result = (result + x / result) &gt;&gt; 1;
    result = (result + x / result) &gt;&gt; 1;
    result = (result + x / result) &gt;&gt; 1;
    result = (result + x / result) &gt;&gt; 1;
    result = (result + x / result) &gt;&gt; 1;
    result = (result + x / result) &gt;&gt; 1;

    <a href="formula.md#0x2_formula_min_u256">min_u256</a>(result, x / result)
}
</code></pre>



</details>
