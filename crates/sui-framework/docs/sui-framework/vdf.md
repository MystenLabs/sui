---
title: Module `0x2::vdf`
---



-  [Constants](#@Constants_0)
-  [Function `hash_to_input`](#0x2_vdf_hash_to_input)
-  [Function `hash_to_input_internal`](#0x2_vdf_hash_to_input_internal)
-  [Function `vdf_verify`](#0x2_vdf_vdf_verify)
-  [Function `vdf_verify_internal`](#0x2_vdf_vdf_verify_internal)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_vdf_EInvalidInput"></a>



<pre><code><b>const</b> <a href="vdf.md#0x2_vdf_EInvalidInput">EInvalidInput</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_vdf_hash_to_input"></a>

## Function `hash_to_input`

Hash an arbitrary binary <code>message</code> to a class group element to be used as input for <code>vdf_verify</code>.


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_hash_to_input">hash_to_input</a>(message: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_hash_to_input">hash_to_input</a>(message: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    <a href="vdf.md#0x2_vdf_hash_to_input_internal">hash_to_input_internal</a>(message)
}
</code></pre>



</details>

<a name="0x2_vdf_hash_to_input_internal"></a>

## Function `hash_to_input_internal`

The internal functions for <code>hash_to_input</code>.


<pre><code><b>fun</b> <a href="vdf.md#0x2_vdf_hash_to_input_internal">hash_to_input_internal</a>(message: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="vdf.md#0x2_vdf_hash_to_input_internal">hash_to_input_internal</a>(message: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_vdf_vdf_verify"></a>

## Function `vdf_verify`

Verify the output and proof of a VDF with the given number of iterations. The <code>input</code>, <code>output</code> and <code>proof</code>
are all class group elements represented by triples <code>(a,b,c)</code> such that <code>b^2 - 4ac = discriminant</code>. The are expected
to be encoded as a BCS encoding of a triple of byte arrays, each being the big-endian twos-complement encoding of
a, b and c in that order.

This uses Wesolowski's VDF construction over imaginary class groups as described in Wesolowski (2020),
'Efficient Verifiable Delay Functions.', J. Cryptol. 33, and is compatible with the VDF implementation in
fastcrypto.

The discriminant for the class group is pre-computed and fixed. See how this was generated in the fastcrypto-vdf
crate. The final selection of the discriminant for Mainnet will be computed and announced under a nothing-up-my-sleeve
process.


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_vdf_verify">vdf_verify</a>(input: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, output: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, iterations: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_vdf_verify">vdf_verify</a>(input: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, output: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, iterations: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): bool {
    <a href="vdf.md#0x2_vdf_vdf_verify_internal">vdf_verify_internal</a>(input, output, proof, iterations)
}
</code></pre>



</details>

<a name="0x2_vdf_vdf_verify_internal"></a>

## Function `vdf_verify_internal`

The internal functions for <code>vdf_verify_internal</code>.


<pre><code><b>fun</b> <a href="vdf.md#0x2_vdf_vdf_verify_internal">vdf_verify_internal</a>(input: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, output: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, iterations: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="vdf.md#0x2_vdf_vdf_verify_internal">vdf_verify_internal</a>(input: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, output: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, iterations: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): bool;
</code></pre>



</details>
