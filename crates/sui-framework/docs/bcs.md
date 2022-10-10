
<a name="0x2_bcs"></a>

# Module `0x2::bcs`



-  [Constants](#@Constants_0)
-  [Function `peel_address`](#0x2_bcs_peel_address)
-  [Function `peel_bool`](#0x2_bcs_peel_bool)
-  [Function `peel_u8`](#0x2_bcs_peel_u8)
-  [Function `peel_u64`](#0x2_bcs_peel_u64)
-  [Function `peel_u128`](#0x2_bcs_peel_u128)
-  [Function `peel_vec_length`](#0x2_bcs_peel_vec_length)


<pre><code><b>use</b> <a href="">0x1::vector</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_bcs_EOutOfRange"></a>

For when bytes length is less than required for deserialization.


<pre><code><b>const</b> <a href="bcs.md#0x2_bcs_EOutOfRange">EOutOfRange</a>: u64 = 0;
</code></pre>



<a name="0x2_bcs_peel_address"></a>

## Function `peel_address`

Read address from the bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_address">peel_address</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_address">peel_address</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): <b>address</b> {
    <b>assert</b>!(v::length(<a href="bcs.md#0x2_bcs">bcs</a>) &gt;= 20, <a href="bcs.md#0x2_bcs_EOutOfRange">EOutOfRange</a>);
    <b>let</b> (_value, i) = (0, 0);
    <b>while</b> (i &lt; 20) {
        <b>let</b> _ = v::remove(<a href="bcs.md#0x2_bcs">bcs</a>, 0);
        i = i + 1;
    };
    @0x0
}
</code></pre>



</details>

<a name="0x2_bcs_peel_bool"></a>

## Function `peel_bool`

Read a <code>bool</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_bool">peel_bool</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_bool">peel_bool</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): bool {
    <b>let</b> value = <a href="bcs.md#0x2_bcs_peel_u8">peel_u8</a>(<a href="bcs.md#0x2_bcs">bcs</a>);
    (value != 0)
}
</code></pre>



</details>

<a name="0x2_bcs_peel_u8"></a>

## Function `peel_u8`

Read <code>u8</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u8">peel_u8</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u8">peel_u8</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u8 {
    <b>assert</b>!(v::length(<a href="bcs.md#0x2_bcs">bcs</a>) &gt;= 1, <a href="bcs.md#0x2_bcs_EOutOfRange">EOutOfRange</a>);
    v::remove(<a href="bcs.md#0x2_bcs">bcs</a>, 0)
}
</code></pre>



</details>

<a name="0x2_bcs_peel_u64"></a>

## Function `peel_u64`

Read <code>u64</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u64">peel_u64</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u64">peel_u64</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u64 {
    <b>assert</b>!(v::length(<a href="bcs.md#0x2_bcs">bcs</a>) &gt;= 8, <a href="bcs.md#0x2_bcs_EOutOfRange">EOutOfRange</a>);
    <b>let</b> (l_value, r_value, i) = (0u64, 0u64, 0);

    // Read first 4 LE bytes (u32)
    <b>while</b> (i &lt; 4) {
        <b>let</b> byte = (v::remove(<a href="bcs.md#0x2_bcs">bcs</a>, 0) <b>as</b> u64);
        l_value = l_value + (byte &lt;&lt; ((8 * (i)) <b>as</b> u8));
        i = i + 1;
    };

    <b>let</b> i = 0;

    // Read second 4 bytes of the U64, also u32 LE
    <b>while</b> (i &lt; 4) {
        <b>let</b> byte = (v::remove(<a href="bcs.md#0x2_bcs">bcs</a>, 0) <b>as</b> u64);
        r_value = r_value + (byte &lt;&lt; ((8 * (i)) <b>as</b> u8));
        i = i + 1;
    };

    // Swap LHS and RHS of initial bytes
    (r_value &lt;&lt; 32) | l_value
}
</code></pre>



</details>

<a name="0x2_bcs_peel_u128"></a>

## Function `peel_u128`

Read <code>u128</code> value from bcs-serialized bytes.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u128">peel_u128</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u128
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_u128">peel_u128</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u128 {
    <b>assert</b>!(v::length(<a href="bcs.md#0x2_bcs">bcs</a>) &gt;= 16, <a href="bcs.md#0x2_bcs_EOutOfRange">EOutOfRange</a>);
    <b>let</b> (l_value, r_value) = (<a href="bcs.md#0x2_bcs_peel_u64">peel_u64</a>(<a href="bcs.md#0x2_bcs">bcs</a>), <a href="bcs.md#0x2_bcs_peel_u64">peel_u64</a>(<a href="bcs.md#0x2_bcs">bcs</a>));
    ((r_value <b>as</b> u128) &lt;&lt; 64) | (l_value <b>as</b> u128)
}
</code></pre>



</details>

<a name="0x2_bcs_peel_vec_length"></a>

## Function `peel_vec_length`

Read ULEB bytes expecting a vector length. Result should
then be used to perform <code>peel_*</code> operation LEN times.


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_length">peel_vec_length</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bcs.md#0x2_bcs_peel_vec_length">peel_vec_length</a>(<a href="bcs.md#0x2_bcs">bcs</a>: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;): u64 {
    v::reverse(<a href="bcs.md#0x2_bcs">bcs</a>);
    <b>let</b> (total, shift) = (0u64, 0);
    <b>while</b> (<b>true</b>) {
        <b>let</b> byte = (v::pop_back(<a href="bcs.md#0x2_bcs">bcs</a>) <b>as</b> u64);
        total = total | ((byte & 0x7f) &lt;&lt; shift);
        <b>if</b> ((byte & 0x80) == 0) {
            <b>break</b>
        };
        shift = shift + 7;
    };
    v::reverse(<a href="bcs.md#0x2_bcs">bcs</a>);
    total
}
</code></pre>



</details>
