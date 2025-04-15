---
title: Module `sui::hmac`
---



-  [Function `hmac_sha3_256`](#sui_hmac_hmac_sha3_256)


<pre><code></code></pre>



<a name="sui_hmac_hmac_sha3_256"></a>

## Function `hmac_sha3_256`

@param key: HMAC key, arbitrary bytes.
@param msg: message to sign, arbitrary bytes.
Returns the 32 bytes digest of HMAC-SHA3-256(key, msg).


<pre><code><b>public</b> <b>fun</b> <a href="../sui/hmac.md#sui_hmac_hmac_sha3_256">hmac_sha3_256</a>(key: &vector&lt;u8&gt;, msg: &vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../sui/hmac.md#sui_hmac_hmac_sha3_256">hmac_sha3_256</a>(key: &vector&lt;u8&gt;, msg: &vector&lt;u8&gt;): vector&lt;u8&gt;;
</code></pre>



</details>
