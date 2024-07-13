---
title: Module `0x2::ecvrf`
---



-  [Constants](#@Constants_0)
-  [Function `ecvrf_verify`](#0x2_ecvrf_ecvrf_verify)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_ecvrf_EInvalidHashLength"></a>



<pre><code><b>const</b> <a href="ecvrf.md#0x2_ecvrf_EInvalidHashLength">EInvalidHashLength</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x2_ecvrf_EInvalidProofEncoding"></a>



<pre><code><b>const</b> <a href="ecvrf.md#0x2_ecvrf_EInvalidProofEncoding">EInvalidProofEncoding</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 3;
</code></pre>



<a name="0x2_ecvrf_EInvalidPublicKeyEncoding"></a>



<pre><code><b>const</b> <a href="ecvrf.md#0x2_ecvrf_EInvalidPublicKeyEncoding">EInvalidPublicKeyEncoding</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0x2_ecvrf_ecvrf_verify"></a>

## Function `ecvrf_verify`

@param hash: The hash/output from a ECVRF to be verified.
@param alpha_string: Input/seed to the ECVRF used to generate the output.
@param public_key: The public key corresponding to the private key used to generate the output.
@param proof: The proof of validity of the output.
Verify a proof for a Ristretto ECVRF. Returns true if the proof is valid and corresponds to the given output. May abort with <code><a href="ecvrf.md#0x2_ecvrf_EInvalidHashLength">EInvalidHashLength</a></code>, <code><a href="ecvrf.md#0x2_ecvrf_EInvalidPublicKeyEncoding">EInvalidPublicKeyEncoding</a></code> or <code><a href="ecvrf.md#0x2_ecvrf_EInvalidProofEncoding">EInvalidProofEncoding</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="ecvrf.md#0x2_ecvrf_ecvrf_verify">ecvrf_verify</a>(<a href="hash.md#0x2_hash">hash</a>: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, alpha_string: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, public_key: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="ecvrf.md#0x2_ecvrf_ecvrf_verify">ecvrf_verify</a>(<a href="hash.md#0x2_hash">hash</a>: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, alpha_string: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, public_key: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>
