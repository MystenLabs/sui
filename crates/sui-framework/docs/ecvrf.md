
<a name="0x2_ecvrf"></a>

# Module `0x2::ecvrf`



-  [Function `native_ecvrf_verify`](#0x2_ecvrf_native_ecvrf_verify)
-  [Function `ecvrf_verify`](#0x2_ecvrf_ecvrf_verify)


<pre><code></code></pre>



<a name="0x2_ecvrf_native_ecvrf_verify"></a>

## Function `native_ecvrf_verify`



<pre><code><b>fun</b> <a href="ecvrf.md#0x2_ecvrf_native_ecvrf_verify">native_ecvrf_verify</a>(<a href="">hash</a>: &<a href="">vector</a>&lt;u8&gt;, alpha_string: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, proof: &<a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="ecvrf.md#0x2_ecvrf_native_ecvrf_verify">native_ecvrf_verify</a>(<a href="">hash</a>: &<a href="">vector</a>&lt;u8&gt;, alpha_string: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, proof: &<a href="">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
</code></pre>



</details>

<a name="0x2_ecvrf_ecvrf_verify"></a>

## Function `ecvrf_verify`

@param key: HMAC key, arbitrary bytes.
@param msg: message to sign, arbitrary bytes.
Returns the 32 bytes digest of HMAC-SHA3-256(key, msg).


<pre><code><b>public</b> <b>fun</b> <a href="ecvrf.md#0x2_ecvrf_ecvrf_verify">ecvrf_verify</a>(<a href="">hash</a>: &<a href="">vector</a>&lt;u8&gt;, alpha_string: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, proof: &<a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ecvrf.md#0x2_ecvrf_ecvrf_verify">ecvrf_verify</a>(<a href="">hash</a>: &<a href="">vector</a>&lt;u8&gt;, alpha_string: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, proof: &<a href="">vector</a>&lt;u8&gt;): bool {
    <a href="ecvrf.md#0x2_ecvrf_native_ecvrf_verify">native_ecvrf_verify</a>(<a href="">hash</a>, alpha_string, public_key, proof)
}
</code></pre>



</details>
