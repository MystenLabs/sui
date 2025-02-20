
<a name="std_macros"></a>

# Module `std::macros`

This module holds shared implementation of macros used in <code>std</code>


-  [Macro function `num_max`](#std_macros_num_max)
-  [Macro function `num_min`](#std_macros_num_min)
-  [Macro function `num_diff`](#std_macros_num_diff)
-  [Macro function `num_divide_and_round_up`](#std_macros_num_divide_and_round_up)
-  [Macro function `num_pow`](#std_macros_num_pow)
-  [Macro function `num_sqrt`](#std_macros_num_sqrt)
-  [Macro function `range_do`](#std_macros_range_do)
-  [Macro function `range_do_eq`](#std_macros_range_do_eq)
-  [Macro function `do`](#std_macros_do)
-  [Macro function `do_eq`](#std_macros_do_eq)


<pre><code></code></pre>



<a name="std_macros_num_max"></a>

## Macro function `num_max`



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

## Macro function `num_min`



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

## Macro function `num_diff`



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

## Macro function `num_divide_and_round_up`



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

## Macro function `num_pow`



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

## Macro function `num_sqrt`



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

<a name="std_macros_range_do"></a>

## Macro function `range_do`



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

## Macro function `range_do_eq`



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

## Macro function `do`



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

## Macro function `do_eq`



<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_do_eq">do_eq</a>($stop: _, $f: |_| -&gt; ())
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/macros.md#std_macros_do_eq">do_eq</a>($stop: _, $f: |_|) {
    <a href="../std/macros.md#std_macros_range_do_eq">range_do_eq</a>!(0, $stop, $f)
}
</code></pre>



</details>


[//]: # ("File containing references which can be used from documentation")
