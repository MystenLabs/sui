---
title: Module `sui::halo2_kzg`
---



-  [Constants](#@Constants_0)
-  [Function `abi_version`](#sui_halo2_kzg_abi_version)
-  [Function `kzg_gwc`](#sui_halo2_kzg_kzg_gwc)
-  [Function `kzg_shplonk`](#sui_halo2_kzg_kzg_shplonk)
-  [Function `verify_proof`](#sui_halo2_kzg_verify_proof)
-  [Function `verify_proof_internal`](#sui_halo2_kzg_verify_proof_internal)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="sui_halo2_kzg_EUnsupportedKzgVariant"></a>



<pre><code><b>const</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_EUnsupportedKzgVariant">EUnsupportedKzgVariant</a>: u64 = 1;
</code></pre>



<a name="sui_halo2_kzg_EInvalidDigestLength"></a>



<pre><code><b>const</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_EInvalidDigestLength">EInvalidDigestLength</a>: u64 = 2;
</code></pre>



<a name="sui_halo2_kzg_KZG_GWC"></a>



<pre><code><b>const</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_KZG_GWC">KZG_GWC</a>: u8 = 0;
</code></pre>



<a name="sui_halo2_kzg_KZG_SHPLONK"></a>



<pre><code><b>const</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_KZG_SHPLONK">KZG_SHPLONK</a>: u8 = 1;
</code></pre>



<a name="sui_halo2_kzg_ABI_VERSION"></a>



<pre><code><b>const</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_ABI_VERSION">ABI_VERSION</a>: u64 = 1;
</code></pre>



<a name="sui_halo2_kzg_abi_version"></a>

## Function `abi_version`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_abi_version">abi_version</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_abi_version">abi_version</a>(): u64 { <a href="../sui/halo2_kzg.md#sui_halo2_kzg_ABI_VERSION">ABI_VERSION</a> }
</code></pre>



</details>

<a name="sui_halo2_kzg_kzg_gwc"></a>

## Function `kzg_gwc`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_kzg_gwc">kzg_gwc</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_kzg_gwc">kzg_gwc</a>(): u8 { <a href="../sui/halo2_kzg.md#sui_halo2_kzg_KZG_GWC">KZG_GWC</a> }
</code></pre>



</details>

<a name="sui_halo2_kzg_kzg_shplonk"></a>

## Function `kzg_shplonk`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_kzg_shplonk">kzg_shplonk</a>(): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_kzg_shplonk">kzg_shplonk</a>(): u8 { <a href="../sui/halo2_kzg.md#sui_halo2_kzg_KZG_SHPLONK">KZG_SHPLONK</a> }
</code></pre>



</details>

<a name="sui_halo2_kzg_verify_proof"></a>

## Function `verify_proof`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_verify_proof">verify_proof</a>(params: vector&lt;u8&gt;, params_digest: vector&lt;u8&gt;, vk: vector&lt;u8&gt;, vk_digest: vector&lt;u8&gt;, circuit_info: vector&lt;u8&gt;, circuit_info_digest: vector&lt;u8&gt;, public_inputs: vector&lt;u8&gt;, proof: vector&lt;u8&gt;, kzg_variant: u8, k_present: bool, k: u32): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_verify_proof">verify_proof</a>(
    params: vector&lt;u8&gt;,
    params_digest: vector&lt;u8&gt;,
    vk: vector&lt;u8&gt;,
    vk_digest: vector&lt;u8&gt;,
    circuit_info: vector&lt;u8&gt;,
    circuit_info_digest: vector&lt;u8&gt;,
    public_inputs: vector&lt;u8&gt;,
    proof: vector&lt;u8&gt;,
    kzg_variant: u8,
    k_present: bool,
    k: u32,
): bool {
    <b>assert</b>!(params_digest.length() == 32, <a href="../sui/halo2_kzg.md#sui_halo2_kzg_EInvalidDigestLength">EInvalidDigestLength</a>);
    <b>assert</b>!(vk_digest.length() == 32, <a href="../sui/halo2_kzg.md#sui_halo2_kzg_EInvalidDigestLength">EInvalidDigestLength</a>);
    <b>assert</b>!(circuit_info_digest.length() == 32, <a href="../sui/halo2_kzg.md#sui_halo2_kzg_EInvalidDigestLength">EInvalidDigestLength</a>);
    <b>assert</b>!(
        kzg_variant == <a href="../sui/halo2_kzg.md#sui_halo2_kzg_KZG_GWC">KZG_GWC</a> || kzg_variant == <a href="../sui/halo2_kzg.md#sui_halo2_kzg_KZG_SHPLONK">KZG_SHPLONK</a>,
        <a href="../sui/halo2_kzg.md#sui_halo2_kzg_EUnsupportedKzgVariant">EUnsupportedKzgVariant</a>,
    );
    <a href="../sui/halo2_kzg.md#sui_halo2_kzg_verify_proof_internal">verify_proof_internal</a>(
        params,
        params_digest,
        vk,
        vk_digest,
        circuit_info,
        circuit_info_digest,
        public_inputs,
        proof,
        kzg_variant,
        k_present,
        k,
    )
}
</code></pre>



</details>

<a name="sui_halo2_kzg_verify_proof_internal"></a>

## Function `verify_proof_internal`



<pre><code><b>fun</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_verify_proof_internal">verify_proof_internal</a>(params: vector&lt;u8&gt;, params_digest: vector&lt;u8&gt;, vk: vector&lt;u8&gt;, vk_digest: vector&lt;u8&gt;, circuit_info: vector&lt;u8&gt;, circuit_info_digest: vector&lt;u8&gt;, public_inputs: vector&lt;u8&gt;, proof: vector&lt;u8&gt;, kzg_variant: u8, k_present: bool, k: u32): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/halo2_kzg.md#sui_halo2_kzg_verify_proof_internal">verify_proof_internal</a>(
    params: vector&lt;u8&gt;,
    params_digest: vector&lt;u8&gt;,
    vk: vector&lt;u8&gt;,
    vk_digest: vector&lt;u8&gt;,
    circuit_info: vector&lt;u8&gt;,
    circuit_info_digest: vector&lt;u8&gt;,
    public_inputs: vector&lt;u8&gt;,
    proof: vector&lt;u8&gt;,
    kzg_variant: u8,
    k_present: bool,
    k: u32,
): bool;
</code></pre>



</details>
