
<a name="0x2_hmac"></a>

# Module `0x2::hmac`



-  [Function `hmac_sha3_256`](#0x2_hmac_hmac_sha3_256)
-  [Module Specification](#@Module_Specification_0)


<pre><code></code></pre>



<a name="0x2_hmac_hmac_sha3_256"></a>

## Function `hmac_sha3_256`

@param key: HMAC key, arbitrary bytes.
@param msg: message to sign, arbitrary bytes.
Returns the 32 bytes digest of HMAC-SHA3-256(key, msg).


<pre><code><b>public</b> <b>fun</b> <a href="hmac.md#0x2_hmac_hmac_sha3_256">hmac_sha3_256</a>(key: &<a href="">vector</a>&lt;u8&gt;, msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="hmac.md#0x2_hmac_hmac_sha3_256">hmac_sha3_256</a>(key: &<a href="">vector</a>&lt;u8&gt;, msg: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="@Module_Specification_0"></a>

## Module Specification



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] <b>true</b>;
</code></pre>
