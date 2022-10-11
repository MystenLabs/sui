
<a name="0x2_bulletproofs"></a>

# Module `0x2::bulletproofs`



-  [Function `native_verify_full_range_proof`](#0x2_bulletproofs_native_verify_full_range_proof)
-  [Function `verify_full_range_proof`](#0x2_bulletproofs_verify_full_range_proof)


<pre><code><b>use</b> <a href="elliptic_curve.md#0x2_elliptic_curve">0x2::elliptic_curve</a>;
</code></pre>



<a name="0x2_bulletproofs_native_verify_full_range_proof"></a>

## Function `native_verify_full_range_proof`

Only bit_length = 64, 32, 16, 8 will work.


<pre><code><b>fun</b> <a href="bulletproofs.md#0x2_bulletproofs_native_verify_full_range_proof">native_verify_full_range_proof</a>(proof: &<a href="">vector</a>&lt;u8&gt;, commitment: &<a href="">vector</a>&lt;u8&gt;, bit_length: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="bulletproofs.md#0x2_bulletproofs_native_verify_full_range_proof">native_verify_full_range_proof</a>(proof: &<a href="">vector</a>&lt;u8&gt;, commitment: &<a href="">vector</a>&lt;u8&gt;, bit_length: u64);
</code></pre>



</details>

<a name="0x2_bulletproofs_verify_full_range_proof"></a>

## Function `verify_full_range_proof`

@param proof: The bulletproof
@param commitment: The commitment which we are trying to verify the range proof for
@param bit_length: The bit length that we prove the committed value is whithin. Note that bit_length must be either 64, 32, 16, or 8.

If the range proof is valid, execution succeeds, else panics.


<pre><code><b>public</b> <b>fun</b> <a href="bulletproofs.md#0x2_bulletproofs_verify_full_range_proof">verify_full_range_proof</a>(proof: &<a href="">vector</a>&lt;u8&gt;, commitment: &<a href="elliptic_curve.md#0x2_elliptic_curve_RistrettoPoint">elliptic_curve::RistrettoPoint</a>, bit_length: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="bulletproofs.md#0x2_bulletproofs_verify_full_range_proof">verify_full_range_proof</a>(proof: &<a href="">vector</a>&lt;u8&gt;, commitment: &RistrettoPoint, bit_length: u64) {
    <a href="bulletproofs.md#0x2_bulletproofs_native_verify_full_range_proof">native_verify_full_range_proof</a>(proof, &ec::bytes(commitment), bit_length)
}
</code></pre>



</details>
