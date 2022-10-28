
<a name="0x2_hmac"></a>

# Module `0x2::hmac`



-  [Function `native_hmac_sha3_256`](#0x2_hmac_native_hmac_sha3_256)
-  [Function `hmac_sha3_256`](#0x2_hmac_hmac_sha3_256)


<pre><code><b>use</b> <a href="digest.md#0x2_digest">0x2::digest</a>;
</code></pre>



<a name="0x2_hmac_native_hmac_sha3_256"></a>

## Function `native_hmac_sha3_256`

@param key: HMAC key, arbitrary bytes.
@param msg: message to sign, arbitrary bytes.
A native move wrapper around the HMAC-SHA3-256. Returns the digest.


<pre><code><b>fun</b> <a href="hmac.md#0x2_hmac_native_hmac_sha3_256">native_hmac_sha3_256</a>(key: &<a href="">vector</a>&lt;u8&gt;, msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="hmac.md#0x2_hmac_native_hmac_sha3_256">native_hmac_sha3_256</a>(key: &<a href="">vector</a>&lt;u8&gt;, msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_hmac_hmac_sha3_256"></a>

## Function `hmac_sha3_256`

@param key: HMAC key, arbitrary bytes.
@param msg: message to sign, arbitrary bytes.
Returns the 32 bytes digest of HMAC-SHA3-256(key, msg).


<pre><code><b>public</b> <b>fun</b> <a href="hmac.md#0x2_hmac_hmac_sha3_256">hmac_sha3_256</a>(key: &<a href="">vector</a>&lt;u8&gt;, msg: &<a href="">vector</a>&lt;u8&gt;): <a href="digest.md#0x2_digest_Sha3256Digest">digest::Sha3256Digest</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="hmac.md#0x2_hmac_hmac_sha3_256">hmac_sha3_256</a>(key: &<a href="">vector</a>&lt;u8&gt;, msg: &<a href="">vector</a>&lt;u8&gt;): <a href="digest.md#0x2_digest_Sha3256Digest">digest::Sha3256Digest</a> {
    <a href="digest.md#0x2_digest_sha3_256_digest_from_bytes">digest::sha3_256_digest_from_bytes</a>(<a href="hmac.md#0x2_hmac_native_hmac_sha3_256">native_hmac_sha3_256</a>(key, msg))
}
</code></pre>



</details>
