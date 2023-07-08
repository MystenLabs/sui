
<a name="0x1_compare"></a>

# Module `0x1::compare`

Utilities for comparing Move values based on their representation in BCS.


-  [Constants](#@Constants_0)
-  [Function `cmp_bcs_bytes`](#0x1_compare_cmp_bcs_bytes)
-  [Function `cmp_u8`](#0x1_compare_cmp_u8)
-  [Function `cmp_u64`](#0x1_compare_cmp_u64)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x1_compare_EQUAL"></a>



<pre><code><b>const</b> <a href="compare.md#0x1_compare_EQUAL">EQUAL</a>: u8 = 0;
</code></pre>



<a name="0x1_compare_GREATER_THAN"></a>



<pre><code><b>const</b> <a href="compare.md#0x1_compare_GREATER_THAN">GREATER_THAN</a>: u8 = 2;
</code></pre>



<a name="0x1_compare_LESS_THAN"></a>



<pre><code><b>const</b> <a href="compare.md#0x1_compare_LESS_THAN">LESS_THAN</a>: u8 = 1;
</code></pre>



<a name="0x1_compare_cmp_bcs_bytes"></a>

## Function `cmp_bcs_bytes`

compare vectors <code>v1</code> and <code>v2</code> using (1) vector contents from right to left and then
(2) vector length to break ties.
Returns either <code><a href="compare.md#0x1_compare_EQUAL">EQUAL</a></code> (0u8), <code><a href="compare.md#0x1_compare_LESS_THAN">LESS_THAN</a></code> (1u8), or <code><a href="compare.md#0x1_compare_GREATER_THAN">GREATER_THAN</a></code> (2u8).

This function is designed to compare BCS (Binary Canonical Serialization)-encoded values
(i.e., vectors produced by <code><a href="_to_bytes">bcs::to_bytes</a></code>). A typical client will call
<code><a href="compare.md#0x1_compare_cmp_bcs_bytes">compare::cmp_bcs_bytes</a>(<a href="_to_bytes">bcs::to_bytes</a>(&t1), <a href="_to_bytes">bcs::to_bytes</a>(&t2))</code>. The comparison provides the
following guarantees w.r.t the original values t1 and t2:
- <code><a href="compare.md#0x1_compare_cmp_bcs_bytes">cmp_bcs_bytes</a>(<a href="">bcs</a>(t1), <a href="">bcs</a>(t2)) == <a href="compare.md#0x1_compare_LESS_THAN">LESS_THAN</a></code> iff <code><a href="compare.md#0x1_compare_cmp_bcs_bytes">cmp_bcs_bytes</a>(t2, t1) == <a href="compare.md#0x1_compare_GREATER_THAN">GREATER_THAN</a></code>
- <code>compare::cmp&lt;T&gt;(t1, t2) == <a href="compare.md#0x1_compare_EQUAL">EQUAL</a></code> iff <code>t1 == t2</code> and (similarly)
<code>compare::cmp&lt;T&gt;(t1, t2) != <a href="compare.md#0x1_compare_EQUAL">EQUAL</a></code> iff <code>t1 != t2</code>, where <code>==</code> and <code>!=</code> denote the Move
bytecode operations for polymorphic equality.
- for all primitive types <code>T</code> with <code>&lt;</code> and <code>&gt;</code> comparison operators exposed in Move bytecode
(<code>u8</code>, <code>u16</code>, <code>u32</code>, <code>u64</code>, <code>u128</code>, <code>u256</code>), we have
<code>compare_bcs_bytes(<a href="">bcs</a>(t1), <a href="">bcs</a>(t2)) == <a href="compare.md#0x1_compare_LESS_THAN">LESS_THAN</a></code> iff <code>t1 &lt; t2</code> and (similarly)
<code>compare_bcs_bytes(<a href="">bcs</a>(t1), <a href="">bcs</a>(t2)) == <a href="compare.md#0x1_compare_LESS_THAN">LESS_THAN</a></code> iff <code>t1 &gt; t2</code>.

For all other types, the order is whatever the BCS encoding of the type and the comparison
strategy above gives you. One case where the order might be surprising is the <code><b>address</b></code>
type.
CoreAddresses are 16 byte hex values that BCS encodes with the identity function. The right
to left, byte-by-byte comparison means that (for example)
<code>compare_bcs_bytes(<a href="">bcs</a>(0x01), <a href="">bcs</a>(0x10)) == <a href="compare.md#0x1_compare_LESS_THAN">LESS_THAN</a></code> (as you'd expect), but
<code>compare_bcs_bytes(<a href="">bcs</a>(0x100), <a href="">bcs</a>(0x001)) == <a href="compare.md#0x1_compare_LESS_THAN">LESS_THAN</a></code> (as you probably wouldn't expect).
Keep this in mind when using this function to compare addresses.

> TODO: there is currently no specification for this function, which causes no problem because it is not yet
> used in the Diem framework. However, should this functionality be needed in specification, a customized
> native abstraction is needed in the prover framework.


<pre><code><b>public</b> <b>fun</b> <a href="compare.md#0x1_compare_cmp_bcs_bytes">cmp_bcs_bytes</a>(v1: &<a href="">vector</a>&lt;u8&gt;, v2: &<a href="">vector</a>&lt;u8&gt;): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="compare.md#0x1_compare_cmp_bcs_bytes">cmp_bcs_bytes</a>(v1: &<a href="">vector</a>&lt;u8&gt;, v2: &<a href="">vector</a>&lt;u8&gt;): u8 {
    <b>let</b> i1 = <a href="_length">vector::length</a>(v1);
    <b>let</b> i2 = <a href="_length">vector::length</a>(v2);
    <b>let</b> len_cmp = <a href="compare.md#0x1_compare_cmp_u64">cmp_u64</a>(i1, i2);

    // BCS uses little endian encoding for all integer types, so we <b>choose</b> <b>to</b> <a href="compare.md#0x1_compare">compare</a> from left
    // <b>to</b> right. Going right <b>to</b> left would make the behavior of compare::cmp diverge from the
    // bytecode operators &lt; and &gt; on integer values (which would be confusing).
    <b>while</b> (i1 &gt; 0 && i2 &gt; 0) {
        i1 = i1 - 1;
        i2 = i2 - 1;
        <b>let</b> elem_cmp = <a href="compare.md#0x1_compare_cmp_u8">cmp_u8</a>(*<a href="_borrow">vector::borrow</a>(v1, i1), *<a href="_borrow">vector::borrow</a>(v2, i2));
        <b>if</b> (elem_cmp != 0) <b>return</b> elem_cmp
        // <b>else</b>, <a href="compare.md#0x1_compare">compare</a> next element
    };
    // all compared elements equal; <b>use</b> length comparion <b>to</b> <b>break</b> the tie
    len_cmp
}
</code></pre>



</details>

<a name="0x1_compare_cmp_u8"></a>

## Function `cmp_u8`

Compare two <code>u8</code>'s


<pre><code><b>fun</b> <a href="compare.md#0x1_compare_cmp_u8">cmp_u8</a>(i1: u8, i2: u8): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="compare.md#0x1_compare_cmp_u8">cmp_u8</a>(i1: u8, i2: u8): u8 {
    <b>if</b> (i1 == i2) <a href="compare.md#0x1_compare_EQUAL">EQUAL</a>
    <b>else</b> <b>if</b> (i1 &lt; i2) <a href="compare.md#0x1_compare_LESS_THAN">LESS_THAN</a>
    <b>else</b> <a href="compare.md#0x1_compare_GREATER_THAN">GREATER_THAN</a>
}
</code></pre>



</details>

<a name="0x1_compare_cmp_u64"></a>

## Function `cmp_u64`

Compare two <code>u64</code>'s


<pre><code><b>fun</b> <a href="compare.md#0x1_compare_cmp_u64">cmp_u64</a>(i1: u64, i2: u64): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="compare.md#0x1_compare_cmp_u64">cmp_u64</a>(i1: u64, i2: u64): u8 {
    <b>if</b> (i1 == i2) <a href="compare.md#0x1_compare_EQUAL">EQUAL</a>
    <b>else</b> <b>if</b> (i1 &lt; i2) <a href="compare.md#0x1_compare_LESS_THAN">LESS_THAN</a>
    <b>else</b> <a href="compare.md#0x1_compare_GREATER_THAN">GREATER_THAN</a>
}
</code></pre>



</details>
