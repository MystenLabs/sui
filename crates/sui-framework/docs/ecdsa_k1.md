
<a name="0x2_ecdsa_k1"></a>

# Module `0x2::ecdsa_k1`



-  [Constants](#@Constants_0)
-  [Function `ecrecover`](#0x2_ecdsa_k1_ecrecover)
-  [Function `decompress_pubkey`](#0x2_ecdsa_k1_decompress_pubkey)
-  [Function `keccak256`](#0x2_ecdsa_k1_keccak256)
-  [Function `secp256k1_verify`](#0x2_ecdsa_k1_secp256k1_verify)
-  [Function `secp256k1_verify_recoverable`](#0x2_ecdsa_k1_secp256k1_verify_recoverable)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_ecdsa_k1_EFailToRecoverPubKey"></a>



<pre><code><b>const</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_EFailToRecoverPubKey">EFailToRecoverPubKey</a>: u64 = 0;
</code></pre>



<a name="0x2_ecdsa_k1_EInvalidSignature"></a>



<pre><code><b>const</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_EInvalidSignature">EInvalidSignature</a>: u64 = 1;
</code></pre>



<a name="0x2_ecdsa_k1_ecrecover"></a>

## Function `ecrecover`

@param signature: A 65-bytes signature in form (r, s, v) that is signed using
Secp256k1. Reference implementation on signature generation using RFC6979:
https://github.com/MystenLabs/narwhal/blob/5d6f6df8ccee94446ff88786c0dbbc98be7cfc09/crypto/src/secp256k1.rs
The accepted v values are {0, 1, 2, 3}.

@param hashed_msg: the hashed 32-bytes message. The message must be hashed instead
of plain text to be secure.

If the signature is valid, return the corresponding recovered Secpk256k1 public
key, otherwise throw error. This is similar to ecrecover in Ethereum, can only be
applied to Secp256k1 signatures.


<pre><code><b>public</b> <b>fun</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_ecrecover">ecrecover</a>(signature: &<a href="">vector</a>&lt;u8&gt;, hashed_msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_ecrecover">ecrecover</a>(signature: &<a href="">vector</a>&lt;u8&gt;, hashed_msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_ecdsa_k1_decompress_pubkey"></a>

## Function `decompress_pubkey`

@param pubkey: A 33-bytes compressed public key, a prefix either 0x02 or 0x03 and a 256-bit integer.

If the compressed public key is valid, return the 65-bytes uncompressed public key,
otherwise throw error.


<pre><code><b>public</b> <b>fun</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_decompress_pubkey">decompress_pubkey</a>(pubkey: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_decompress_pubkey">decompress_pubkey</a>(pubkey: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_ecdsa_k1_keccak256"></a>

## Function `keccak256`

@param data: arbitrary bytes data to hash
Hash the input bytes using keccak256 and returns 32 bytes.


<pre><code><b>public</b> <b>fun</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_keccak256">keccak256</a>(data: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_keccak256">keccak256</a>(data: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_ecdsa_k1_secp256k1_verify"></a>

## Function `secp256k1_verify`

@param signature: A 64-bytes signature in form (r, s) that is signed using
Secp256k1. This is an non-recoverable signature without recovery id.
Reference implementation on signature generation using RFC6979:
https://github.com/MystenLabs/fastcrypto/blob/74aec4886e62122a5b769464c2bea5f803cf8ecc/fastcrypto/src/secp256k1/mod.rs#L193

@param public_key: The public key to verify the signature against
@param hashed_msg: The hashed 32-bytes message, same as what the signature is signed against.

If the signature is valid to the pubkey and hashed message, return true. Else false.


<pre><code><b>public</b> <b>fun</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_secp256k1_verify">secp256k1_verify</a>(signature: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, hashed_msg: &<a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_secp256k1_verify">secp256k1_verify</a>(signature: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, hashed_msg: &<a href="">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>

<a name="0x2_ecdsa_k1_secp256k1_verify_recoverable"></a>

## Function `secp256k1_verify_recoverable`

@param signature: A 65-bytes signature in form (r, s, v) that is signed using
Secp256k1. This is an recoverable signature with recovery id denoted as v.
Reference implementation on signature generation using RFC6979:
https://github.com/MystenLabs/fastcrypto/blob/74aec4886e62122a5b769464c2bea5f803cf8ecc/fastcrypto/src/secp256k1/mod.rs#L193

@param public_key: The public key to verify the signature against
@param hashed_msg: The hashed 32-bytes message, same as what the signature is signed against.

If the signature is valid to the pubkey and hashed message, return true. Else false.


<pre><code><b>public</b> <b>fun</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_secp256k1_verify_recoverable">secp256k1_verify_recoverable</a>(signature: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, hashed_msg: &<a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="ecdsa_k1.md#0x2_ecdsa_k1_secp256k1_verify_recoverable">secp256k1_verify_recoverable</a>(signature: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, hashed_msg: &<a href="">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>
