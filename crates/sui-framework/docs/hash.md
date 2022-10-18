
<a name="0x2_hash"></a>

# Module `0x2::hash`



-  [Function `hmac_sha2_256`](#0x2_hash_hmac_sha2_256)


<pre><code></code></pre>



<a name="0x2_hash_hmac_sha2_256"></a>

## Function `hmac_sha2_256`

@param key: HMAC key, arbitrary bytes.
@param msg: message to sign, arbitrary bytes.
Returns the 32 bytes output of HMAC-SHA2-256(key, msg).


<pre><code><b>public</b> <b>fun</b> <a href="hash.md#0x2_hash_hmac_sha2_256">hmac_sha2_256</a>(key: &<a href="">vector</a>&lt;u8&gt;, msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="hash.md#0x2_hash_hmac_sha2_256">hmac_sha2_256</a>(key: &<a href="">vector</a>&lt;u8&gt;, msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>
