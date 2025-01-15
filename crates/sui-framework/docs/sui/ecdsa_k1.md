---
title: Module `sui::ecdsa_k1`
---



-  [Constants](#@Constants_0)
-  [Function `secp256k1_ecrecover`](#sui_ecdsa_k1_secp256k1_ecrecover)
-  [Function `decompress_pubkey`](#sui_ecdsa_k1_decompress_pubkey)
-  [Function `secp256k1_verify`](#sui_ecdsa_k1_secp256k1_verify)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="sui_ecdsa_k1_EFailToRecoverPubKey"></a>

Error if the public key cannot be recovered from the signature.


<pre><code><b>const</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_EFailToRecoverPubKey">EFailToRecoverPubKey</a>: u64 = 0;
</code></pre>



<a name="sui_ecdsa_k1_EInvalidPubKey"></a>

Error if the public key is invalid.


<pre><code><b>const</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_EInvalidPubKey">EInvalidPubKey</a>: u64 = 2;
</code></pre>



<a name="sui_ecdsa_k1_EInvalidSignature"></a>

Error if the signature is invalid.


<pre><code><b>const</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_EInvalidSignature">EInvalidSignature</a>: u64 = 1;
</code></pre>



<a name="sui_ecdsa_k1_KECCAK256"></a>

Hash function name that are valid for ecrecover and secp256k1_verify.


<pre><code><b>const</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_KECCAK256">KECCAK256</a>: u8 = 0;
</code></pre>



<a name="sui_ecdsa_k1_SHA256"></a>



<pre><code><b>const</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_SHA256">SHA256</a>: u8 = 1;
</code></pre>



<a name="sui_ecdsa_k1_secp256k1_ecrecover"></a>

## Function `secp256k1_ecrecover`

@param signature: A 65-bytes signature in form (r, s, v) that is signed using
Secp256k1. Reference implementation on signature generation using RFC6979:
https://github.com/MystenLabs/narwhal/blob/5d6f6df8ccee94446ff88786c0dbbc98be7cfc09/crypto/src/secp256k1.rs
The accepted v values are {0, 1, 2, 3}.
@param msg: The message that the signature is signed against, this is raw message without hashing.
@param hash: The hash function used to hash the message when signing.

If the signature is valid, return the corresponding recovered Secpk256k1 public
key, otherwise throw error. This is similar to ecrecover in Ethereum, can only be
applied to Secp256k1 signatures. May abort with <code><a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_EFailToRecoverPubKey">EFailToRecoverPubKey</a></code> or <code><a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_EInvalidSignature">EInvalidSignature</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_secp256k1_ecrecover">secp256k1_ecrecover</a>(signature: &vector&lt;u8&gt;, msg: &vector&lt;u8&gt;, <a href="../sui/hash.md#sui_hash">hash</a>: u8): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_secp256k1_ecrecover">secp256k1_ecrecover</a>(
    signature: &vector&lt;u8&gt;,
    msg: &vector&lt;u8&gt;,
    <a href="../sui/hash.md#sui_hash">hash</a>: u8,
): vector&lt;u8&gt;;
</code></pre>



</details>

<a name="sui_ecdsa_k1_decompress_pubkey"></a>

## Function `decompress_pubkey`

@param pubkey: A 33-bytes compressed public key, a prefix either 0x02 or 0x03 and a 256-bit integer.

If the compressed public key is valid, return the 65-bytes uncompressed public key,
otherwise throw error. May abort with <code><a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_EInvalidPubKey">EInvalidPubKey</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_decompress_pubkey">decompress_pubkey</a>(pubkey: &vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_decompress_pubkey">decompress_pubkey</a>(pubkey: &vector&lt;u8&gt;): vector&lt;u8&gt;;
</code></pre>



</details>

<a name="sui_ecdsa_k1_secp256k1_verify"></a>

## Function `secp256k1_verify`

@param signature: A 64-bytes signature in form (r, s) that is signed using
Secp256k1. This is an non-recoverable signature without recovery id.
Reference implementation on signature generation using RFC6979:
https://github.com/MystenLabs/fastcrypto/blob/74aec4886e62122a5b769464c2bea5f803cf8ecc/fastcrypto/src/secp256k1/mod.rs#L193
@param public_key: The public key to verify the signature against
@param msg: The message that the signature is signed against, this is raw message without hashing.
@param hash: The hash function used to hash the message when signing.

If the signature is valid to the pubkey and hashed message, return true. Else false.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_secp256k1_verify">secp256k1_verify</a>(signature: &vector&lt;u8&gt;, public_key: &vector&lt;u8&gt;, msg: &vector&lt;u8&gt;, <a href="../sui/hash.md#sui_hash">hash</a>: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1_secp256k1_verify">secp256k1_verify</a>(
    signature: &vector&lt;u8&gt;,
    public_key: &vector&lt;u8&gt;,
    msg: &vector&lt;u8&gt;,
    <a href="../sui/hash.md#sui_hash">hash</a>: u8,
): bool;
</code></pre>



</details>
