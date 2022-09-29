
<a name="0x2_ecdsa"></a>

# Module `0x2::ecdsa`



-  [Function `ecrecover`](#0x2_ecdsa_ecrecover)
-  [Function `decompress_pubkey`](#0x2_ecdsa_decompress_pubkey)
-  [Function `keccak256`](#0x2_ecdsa_keccak256)
-  [Function `secp256k1_verify`](#0x2_ecdsa_secp256k1_verify)


<pre><code></code></pre>



<a name="0x2_ecdsa_ecrecover"></a>

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


<pre><code><b>public</b> <b>fun</b> <a href="ecdsa.md#0x2_ecdsa_ecrecover">ecrecover</a>(signature: &<a href="">vector</a>&lt;u8&gt;, hashed_msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="ecdsa.md#0x2_ecdsa_ecrecover">ecrecover</a>(signature: &<a href="">vector</a>&lt;u8&gt;, hashed_msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_ecdsa_decompress_pubkey"></a>

## Function `decompress_pubkey`

@param pubkey: A 33-bytes compressed public key, a prefix either 0x02 or 0x03 and a 256-bit integer.

If the compressed public key is valid, return the 65-bytes uncompressed public key,
otherwise throw error.


<pre><code><b>public</b> <b>fun</b> <a href="ecdsa.md#0x2_ecdsa_decompress_pubkey">decompress_pubkey</a>(pubkey: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="ecdsa.md#0x2_ecdsa_decompress_pubkey">decompress_pubkey</a>(pubkey: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_ecdsa_keccak256"></a>

## Function `keccak256`

@param data: arbitrary bytes data to hash
Hash the input bytes using keccak256 and returns 32 bytes.


<pre><code><b>public</b> <b>fun</b> <a href="ecdsa.md#0x2_ecdsa_keccak256">keccak256</a>(data: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="ecdsa.md#0x2_ecdsa_keccak256">keccak256</a>(data: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_ecdsa_secp256k1_verify"></a>

## Function `secp256k1_verify`

@param signature: A 65-bytes signature in form (r, s, v) that is signed using
Secp256k1. Reference implementation on signature generation using RFC6979:
https://github.com/MystenLabs/narwhal/blob/5d6f6df8ccee94446ff88786c0dbbc98be7cfc09/crypto/src/secp256k1.rs

@param public_key: The public key to verify the signature against
@param hashed_msg: The hashed 32-bytes message, same as what the signature is signed against.

If the signature is valid to the pubkey and hashed message, return true. Else false.


<pre><code><b>public</b> <b>fun</b> <a href="ecdsa.md#0x2_ecdsa_secp256k1_verify">secp256k1_verify</a>(signature: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, hashed_msg: &<a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="ecdsa.md#0x2_ecdsa_secp256k1_verify">secp256k1_verify</a>(signature: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, hashed_msg: &<a href="">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>
