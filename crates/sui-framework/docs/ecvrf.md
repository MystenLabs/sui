
<a name="0x2_ecvrf"></a>

# Module `0x2::ecvrf`



-  [Function `native_ecvrf_verify`](#0x2_ecvrf_native_ecvrf_verify)
-  [Function `ecvrf_verify`](#0x2_ecvrf_ecvrf_verify)


<pre><code></code></pre>



<a name="0x2_ecvrf_native_ecvrf_verify"></a>

## Function `native_ecvrf_verify`

@param hash: The hash/output from a ECVRF to be verified.
@param alpha_string: Input/seed to the ECVRF used to generate the output.
@param public_key: The public key corresponding to the private key used to generate the output.
@param proof: The proof of validity of the output.
A native move wrapper around the Ristretto ECVRF. Returns true if the proof is valid and corresponds to the given output.


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

@param hash: The hash/output from a ECVRF to be verified.
@param alpha_string: Input/seed to the ECVRF used to generate the output.
@param public_key: The public key corresponding to the private key used to generate the output.
@param proof: The proof of validity of the output.
Verify a proof for a Ristretto ECVRF. Returns true if the proof is valid and corresponds to the given output.


<pre><code><b>public</b> <b>fun</b> <a href="ecvrf.md#0x2_ecvrf_ecvrf_verify">ecvrf_verify</a>(<a href="">hash</a>: &<a href="">vector</a>&lt;u8&gt;, alpha_string: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, proof: &<a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="ecvrf.md#0x2_ecvrf_ecvrf_verify">ecvrf_verify</a>(<a href="">hash</a>: &<a href="">vector</a>&lt;u8&gt;, alpha_string: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, proof: &<a href="">vector</a>&lt;u8&gt;): bool {
    <a href="ecvrf.md#0x2_ecvrf_native_ecvrf_verify">native_ecvrf_verify</a>(<a href="">hash</a>, alpha_string, public_key, proof)
}
</code></pre>



</details>
