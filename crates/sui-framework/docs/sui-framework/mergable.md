---
title: Module `0x2::mergable`
---



-  [Struct `Sum`](#0x2_mergable_Sum)
-  [Function `make_sum`](#0x2_mergable_make_sum)


<pre><code></code></pre>



<a name="0x2_mergable_Sum"></a>

## Struct `Sum`

A commutative sum type. It is represented as a u128, but can only
be created from a u64. This ensures that overflow is impossible unless
2^64 Sums are created and added together - a practical impossibility.


<pre><code><b>struct</b> <a href="../sui-framework/mergable.md#0x2_mergable_Sum">Sum</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>value: u128</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_mergable_make_sum"></a>

## Function `make_sum`



<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/mergable.md#0x2_mergable_make_sum">make_sum</a>(value: u64): <a href="../sui-framework/mergable.md#0x2_mergable_Sum">mergable::Sum</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/mergable.md#0x2_mergable_make_sum">make_sum</a>(value: u64): <a href="../sui-framework/mergable.md#0x2_mergable_Sum">Sum</a> {
    <a href="../sui-framework/mergable.md#0x2_mergable_Sum">Sum</a> { value: value <b>as</b> u128 }
}
</code></pre>



</details>
