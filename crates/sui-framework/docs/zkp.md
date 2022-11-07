
<a name="0x2_zkp"></a>

# Module `0x2::zkp`



-  [Struct `Proof`](#0x2_zkp_Proof)
-  [Struct `PreparedVerifyingKey`](#0x2_zkp_PreparedVerifyingKey)
-  [Constants](#@Constants_0)
-  [Function `pvk_from_bytes`](#0x2_zkp_pvk_from_bytes)
-  [Function `proof_from_bytes`](#0x2_zkp_proof_from_bytes)
-  [Function `verify_groth16_proof`](#0x2_zkp_verify_groth16_proof)
-  [Function `internal_verify_groth16_proof`](#0x2_zkp_internal_verify_groth16_proof)


<pre><code></code></pre>



<a name="0x2_zkp_Proof"></a>

## Struct `Proof`

Proof<Bls12_381>


<pre><code><b>struct</b> <a href="zkp.md#0x2_zkp_Proof">Proof</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>bytes: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_zkp_PreparedVerifyingKey"></a>

## Struct `PreparedVerifyingKey`

PreparedVerifyingKey


<pre><code><b>struct</b> <a href="zkp.md#0x2_zkp_PreparedVerifyingKey">PreparedVerifyingKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>vk_gamma_abc_g1: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The element vk.gamma_abc_g1,
 aka the <code>[gamma^{-1} * (beta * a_i + alpha * b_i + c_i) * G]</code>, where i spans the public inputs
</dd>
<dt>
<code>alpha_g1_beta_g2: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The element <code>e(alpha * G, beta * H)</code> in <code>E::GT</code>. blst_fp12
</dd>
<dt>
<code>gamma_g2_neg_pc: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The element <code>- gamma * H</code> in <code>E::G2</code>, for use in pairings.
</dd>
<dt>
<code>delta_g2_neg_pc: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The element <code>- delta * H</code> in <code>E::G2</code>, for use in pairings.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_zkp_LENGTH"></a>

Length of the vector<u8> representing a SHA3-256 digest.


<pre><code><b>const</b> <a href="zkp.md#0x2_zkp_LENGTH">LENGTH</a>: u64 = 32;
</code></pre>



<a name="0x2_zkp_LengthMismatch"></a>

Error code when the length is invalid.


<pre><code><b>const</b> <a href="zkp.md#0x2_zkp_LengthMismatch">LengthMismatch</a>: u64 = 0;
</code></pre>



<a name="0x2_zkp_pvk_from_bytes"></a>

## Function `pvk_from_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_zkp_pvk_from_bytes">pvk_from_bytes</a>(vk_gamma_abc_g1: <a href="">vector</a>&lt;u8&gt;, alpha_g1_beta_g2: <a href="">vector</a>&lt;u8&gt;, gamma_g2_neg_pc: <a href="">vector</a>&lt;u8&gt;, delta_g2_neg_pc: <a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_zkp_PreparedVerifyingKey">zkp::PreparedVerifyingKey</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_zkp_pvk_from_bytes">pvk_from_bytes</a>(
    vk_gamma_abc_g1: <a href="">vector</a>&lt;u8&gt;,
    alpha_g1_beta_g2: <a href="">vector</a>&lt;u8&gt;,
    gamma_g2_neg_pc: <a href="">vector</a>&lt;u8&gt;,
    delta_g2_neg_pc: <a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_zkp_PreparedVerifyingKey">PreparedVerifyingKey</a> {
    <a href="zkp.md#0x2_zkp_PreparedVerifyingKey">PreparedVerifyingKey</a> { vk_gamma_abc_g1, alpha_g1_beta_g2, gamma_g2_neg_pc, delta_g2_neg_pc }
}
</code></pre>



</details>

<a name="0x2_zkp_proof_from_bytes"></a>

## Function `proof_from_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_zkp_proof_from_bytes">proof_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_zkp_Proof">zkp::Proof</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_zkp_proof_from_bytes">proof_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_zkp_Proof">Proof</a> {
    <a href="zkp.md#0x2_zkp_Proof">Proof</a> { bytes }
}
</code></pre>



</details>

<a name="0x2_zkp_verify_groth16_proof"></a>

## Function `verify_groth16_proof`

@param pvk: PreparedVerifyingKey

@param x

@param proof
Returns the validity of the Groth16 proof passed as argument.


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_zkp_verify_groth16_proof">verify_groth16_proof</a>(pvk: <a href="zkp.md#0x2_zkp_PreparedVerifyingKey">zkp::PreparedVerifyingKey</a>, x: <a href="">vector</a>&lt;u8&gt;, proof: <a href="zkp.md#0x2_zkp_Proof">zkp::Proof</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_zkp_verify_groth16_proof">verify_groth16_proof</a>(pvk: <a href="zkp.md#0x2_zkp_PreparedVerifyingKey">PreparedVerifyingKey</a>, x: <a href="">vector</a>&lt;u8&gt;, proof: <a href="zkp.md#0x2_zkp_Proof">Proof</a>): bool {
    <a href="zkp.md#0x2_zkp_internal_verify_groth16_proof">internal_verify_groth16_proof</a>(
        pvk.vk_gamma_abc_g1,
        pvk.alpha_g1_beta_g2,
        pvk.gamma_g2_neg_pc,
        pvk.delta_g2_neg_pc,
        x,
        proof.bytes
    )
}
</code></pre>



</details>

<a name="0x2_zkp_internal_verify_groth16_proof"></a>

## Function `internal_verify_groth16_proof`



<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_zkp_internal_verify_groth16_proof">internal_verify_groth16_proof</a>(vk_gamma_abc_g1_bytes: <a href="">vector</a>&lt;u8&gt;, alpha_g1_beta_g2_bytes: <a href="">vector</a>&lt;u8&gt;, gamma_g2_neg_pc_bytes: <a href="">vector</a>&lt;u8&gt;, delta_g2_neg_pc_bytes: <a href="">vector</a>&lt;u8&gt;, x_bytes: <a href="">vector</a>&lt;u8&gt;, proof_bytes: <a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="zkp.md#0x2_zkp_internal_verify_groth16_proof">internal_verify_groth16_proof</a>(
    vk_gamma_abc_g1_bytes: <a href="">vector</a>&lt;u8&gt;,
    alpha_g1_beta_g2_bytes: <a href="">vector</a>&lt;u8&gt;,
    gamma_g2_neg_pc_bytes: <a href="">vector</a>&lt;u8&gt;,
    delta_g2_neg_pc_bytes: <a href="">vector</a>&lt;u8&gt;,
    x_bytes: <a href="">vector</a>&lt;u8&gt;,
    proof_bytes: <a href="">vector</a>&lt;u8&gt;
): bool;
</code></pre>



</details>
