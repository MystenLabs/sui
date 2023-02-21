
<a name="0x0_address_spec"></a>

# Module `0x0::address_spec`



-  [Constants](#@Constants_0)
-  [Function `from_bytes`](#0x0_address_spec_from_bytes)
-  [Function `to_u256`](#0x0_address_spec_to_u256)
-  [Function `from_u256`](#0x0_address_spec_from_u256)


<pre><code><b>use</b> <a href="address.md#0x2_address">0x2::address</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x0_address_spec_MAX"></a>



<pre><code><b>const</b> <a href="address_spec.md#0x0_address_spec_MAX">MAX</a>: u256 = 1461501637330902918203684832716283019655932542975;
</code></pre>



<a name="0x0_address_spec_from_bytes"></a>

## Function `from_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="address_spec.md#0x0_address_spec_from_bytes">from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="address_spec.md#0x0_address_spec_from_bytes">from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <b>address</b> {
    <a href="address.md#0x2_address_from_bytes">address::from_bytes</a>(bytes)
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>aborts_if</b> len(bytes) != 20;
<b>ensures</b> result == <a href="address.md#0x2_address_from_bytes">address::from_bytes</a>(bytes);
<b>let</b> addr = @0x89b9f9d1fadc027cf9532d6f99041522;
<b>let</b> expected_output = x"0000000089b9f9d1fadc027cf9532d6f99041522";
<b>aborts_if</b> len(expected_output) != 20;
<b>aborts_if</b> <a href="address.md#0x2_address_from_bytes">address::from_bytes</a>(expected_output) != addr;
</code></pre>



</details>

<a name="0x0_address_spec_to_u256"></a>

## Function `to_u256`



<pre><code><b>public</b> <b>fun</b> <a href="address_spec.md#0x0_address_spec_to_u256">to_u256</a>(a: <b>address</b>): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="address_spec.md#0x0_address_spec_to_u256">to_u256</a>(a: <b>address</b>): u256 {
    <a href="address.md#0x2_address_to_u256">address::to_u256</a>(a)
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>aborts_if</b> <b>false</b>;
<b>ensures</b> <a href="address.md#0x2_address_from_u256">address::from_u256</a>(result) == a;
</code></pre>



</details>

<a name="0x0_address_spec_from_u256"></a>

## Function `from_u256`



<pre><code><b>public</b> <b>fun</b> <a href="address_spec.md#0x0_address_spec_from_u256">from_u256</a>(n: u256): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="address_spec.md#0x0_address_spec_from_u256">from_u256</a>(n: u256): <b>address</b> {
    <a href="address.md#0x2_address_from_u256">address::from_u256</a>(n)
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>aborts_if</b> n &gt; <a href="address_spec.md#0x0_address_spec_MAX">MAX</a>;
<b>ensures</b> <a href="address.md#0x2_address_to_u256">address::to_u256</a>(result) == n;
</code></pre>



</details>
