
<a name="0x2_ecdsa_k1"></a>

# Module `0x2::ecdsa_k1`



-  [Constants](#@Constants_0)
-  [Function `secp256k1_ecrecover`](#0x2_ecdsa_k1_secp256k1_ecrecover)
-  [Function `decompress_pubkey`](#0x2_ecdsa_k1_decompress_pubkey)
-  [Function `secp256k1_verify`](#0x2_ecdsa_k1_secp256k1_verify)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_ecdsa_k1_EFailToRecoverPubKey"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_EFailToRecoverPubKey">EFailToRecoverPubKey</a>: u64 = 0;
</code></pre>



<a name="0x2_ecdsa_k1_EInvalidPubKey"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_EInvalidPubKey">EInvalidPubKey</a>: u64 = 2;
</code></pre>



<a name="0x2_ecdsa_k1_EInvalidSignature"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_EInvalidSignature">EInvalidSignature</a>: u64 = 1;
</code></pre>



<a name="0x2_ecdsa_k1_KECCAK256"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_KECCAK256">KECCAK256</a>: u8 = 0;
</code></pre>



<a name="0x2_ecdsa_k1_SHA256"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_SHA256">SHA256</a>: u8 = 1;
</code></pre>



<a name="0x2_ecdsa_k1_secp256k1_ecrecover"></a>

## Function `secp256k1_ecrecover`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_secp256k1_ecrecover">secp256k1_ecrecover</a>(signature: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, msg: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="../../dependencies/sui-framework/hash.md#0x2_hash">hash</a>: u8): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_secp256k1_ecrecover">secp256k1_ecrecover</a>(signature: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, msg: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="../../dependencies/sui-framework/hash.md#0x2_hash">hash</a>: u8): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_ecdsa_k1_decompress_pubkey"></a>

## Function `decompress_pubkey`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_decompress_pubkey">decompress_pubkey</a>(pubkey: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_decompress_pubkey">decompress_pubkey</a>(pubkey: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_ecdsa_k1_secp256k1_verify"></a>

## Function `secp256k1_verify`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_secp256k1_verify">secp256k1_verify</a>(signature: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, public_key: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, msg: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="../../dependencies/sui-framework/hash.md#0x2_hash">hash</a>: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_secp256k1_verify">secp256k1_verify</a>(signature: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, public_key: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, msg: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="../../dependencies/sui-framework/hash.md#0x2_hash">hash</a>: u8): bool;
</code></pre>



</details>
