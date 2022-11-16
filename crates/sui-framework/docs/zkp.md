
<a name="0x2_groth16"></a>

# Module `0x2::groth16`



-  [Struct `PreparedVerifyingKey`](#0x2_groth16_PreparedVerifyingKey)
-  [Struct `PublicProofInputs`](#0x2_groth16_PublicProofInputs)
-  [Struct `ProofPoints`](#0x2_groth16_ProofPoints)
-  [Function `pvk_from_bytes`](#0x2_groth16_pvk_from_bytes)
-  [Function `pvk_to_bytes`](#0x2_groth16_pvk_to_bytes)
-  [Function `public_proof_inputs_from_bytes`](#0x2_groth16_public_proof_inputs_from_bytes)
-  [Function `proof_points_from_bytes`](#0x2_groth16_proof_points_from_bytes)
-  [Function `prepare_verifying_key`](#0x2_groth16_prepare_verifying_key)
-  [Function `verify_groth16_proof`](#0x2_groth16_verify_groth16_proof)
-  [Function `verify_groth16_proof_internal`](#0x2_groth16_verify_groth16_proof_internal)


<pre><code></code></pre>



<a name="0x2_groth16_PreparedVerifyingKey"></a>

## Struct `PreparedVerifyingKey`

A <code><a href="zkp.md#0x2_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a></code> consisting of four components in serialized form.


<pre><code><b>struct</b> <a href="zkp.md#0x2_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>vk_gamma_abc_g1_bytes: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>alpha_g1_beta_g2_bytes: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>gamma_g2_neg_pc_bytes: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>delta_g2_neg_pc_bytes: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_groth16_PublicProofInputs"></a>

## Struct `PublicProofInputs`

A <code><a href="zkp.md#0x2_groth16_PublicProofInputs">PublicProofInputs</a></code> wrapper around its serialized bytes.


<pre><code><b>struct</b> <a href="zkp.md#0x2_groth16_PublicProofInputs">PublicProofInputs</a> <b>has</b> <b>copy</b>, drop, store
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

<a name="0x2_groth16_ProofPoints"></a>

## Struct `ProofPoints`

A <code><a href="zkp.md#0x2_groth16_ProofPoints">ProofPoints</a></code> wrapper around the serialized form of three proof points.


<pre><code><b>struct</b> <a href="zkp.md#0x2_groth16_ProofPoints">ProofPoints</a> <b>has</b> <b>copy</b>, drop, store
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

<a name="0x2_groth16_pvk_from_bytes"></a>

## Function `pvk_from_bytes`

Creates a <code><a href="zkp.md#0x2_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a></code> from bytes.


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_pvk_from_bytes">pvk_from_bytes</a>(vk_gamma_abc_g1_bytes: <a href="">vector</a>&lt;u8&gt;, alpha_g1_beta_g2_bytes: <a href="">vector</a>&lt;u8&gt;, gamma_g2_neg_pc_bytes: <a href="">vector</a>&lt;u8&gt;, delta_g2_neg_pc_bytes: <a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_groth16_PreparedVerifyingKey">groth16::PreparedVerifyingKey</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_pvk_from_bytes">pvk_from_bytes</a>(vk_gamma_abc_g1_bytes: <a href="">vector</a>&lt;u8&gt;, alpha_g1_beta_g2_bytes: <a href="">vector</a>&lt;u8&gt;, gamma_g2_neg_pc_bytes: <a href="">vector</a>&lt;u8&gt;, delta_g2_neg_pc_bytes: <a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a> {
    <a href="zkp.md#0x2_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a> {
        vk_gamma_abc_g1_bytes,
        alpha_g1_beta_g2_bytes,
        gamma_g2_neg_pc_bytes,
        delta_g2_neg_pc_bytes
    }
}
</code></pre>



</details>

<a name="0x2_groth16_pvk_to_bytes"></a>

## Function `pvk_to_bytes`

Returns bytes of the four components of the <code><a href="zkp.md#0x2_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_pvk_to_bytes">pvk_to_bytes</a>(pvk: <a href="zkp.md#0x2_groth16_PreparedVerifyingKey">groth16::PreparedVerifyingKey</a>): <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_pvk_to_bytes">pvk_to_bytes</a>(pvk: <a href="zkp.md#0x2_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a>): <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt; {
    <b>let</b> res = <a href="_empty">vector::empty</a>();
    <a href="_push_back">vector::push_back</a>(&<b>mut</b> res, pvk.vk_gamma_abc_g1_bytes);
    <a href="_push_back">vector::push_back</a>(&<b>mut</b> res, pvk.alpha_g1_beta_g2_bytes);
    <a href="_push_back">vector::push_back</a>(&<b>mut</b> res, pvk.gamma_g2_neg_pc_bytes);
    <a href="_push_back">vector::push_back</a>(&<b>mut</b> res, pvk.delta_g2_neg_pc_bytes);
    res
}
</code></pre>



</details>

<a name="0x2_groth16_public_proof_inputs_from_bytes"></a>

## Function `public_proof_inputs_from_bytes`

Creates a <code><a href="zkp.md#0x2_groth16_PublicProofInputs">PublicProofInputs</a></code> wrapper from bytes.


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_public_proof_inputs_from_bytes">public_proof_inputs_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_groth16_PublicProofInputs">groth16::PublicProofInputs</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_public_proof_inputs_from_bytes">public_proof_inputs_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_groth16_PublicProofInputs">PublicProofInputs</a> {
    <a href="zkp.md#0x2_groth16_PublicProofInputs">PublicProofInputs</a> { bytes }
}
</code></pre>



</details>

<a name="0x2_groth16_proof_points_from_bytes"></a>

## Function `proof_points_from_bytes`

Creates a Groth16 <code><a href="zkp.md#0x2_groth16_ProofPoints">ProofPoints</a></code> from bytes.


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_proof_points_from_bytes">proof_points_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_groth16_ProofPoints">groth16::ProofPoints</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_proof_points_from_bytes">proof_points_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_groth16_ProofPoints">ProofPoints</a> {
    <a href="zkp.md#0x2_groth16_ProofPoints">ProofPoints</a> { bytes }
}
</code></pre>



</details>

<a name="0x2_groth16_prepare_verifying_key"></a>

## Function `prepare_verifying_key`

@param veriyfing_key: An Arkworks canonical serialization of a verifying key.

Returns four vectors of bytes representing the four components of a prepared verifying key.
This step computes one pairing e(P, Q), and binds the verification to one particular proof statement.
This can be used as inputs for the <code>verify_groth16_proof</code> function.


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_prepare_verifying_key">prepare_verifying_key</a>(verifying_key: &<a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_groth16_PreparedVerifyingKey">groth16::PreparedVerifyingKey</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="zkp.md#0x2_groth16_prepare_verifying_key">prepare_verifying_key</a>(verifying_key: &<a href="">vector</a>&lt;u8&gt;): <a href="zkp.md#0x2_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a>;
</code></pre>



</details>

<a name="0x2_groth16_verify_groth16_proof"></a>

## Function `verify_groth16_proof`

@param prepared_verifying_key: Consists of four vectors of bytes representing the four components of a prepared verifying key.
@param public_proof_inputs: Represent inputs that are public.
@param proof_points: Represent three proof points.

Returns a boolean indicating whether the proof is valid.


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_verify_groth16_proof">verify_groth16_proof</a>(prepared_verifying_key: <a href="zkp.md#0x2_groth16_PreparedVerifyingKey">groth16::PreparedVerifyingKey</a>, public_proof_inputs: <a href="zkp.md#0x2_groth16_PublicProofInputs">groth16::PublicProofInputs</a>, proof_points: <a href="zkp.md#0x2_groth16_ProofPoints">groth16::ProofPoints</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_verify_groth16_proof">verify_groth16_proof</a>(prepared_verifying_key: <a href="zkp.md#0x2_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a>, public_proof_inputs: <a href="zkp.md#0x2_groth16_PublicProofInputs">PublicProofInputs</a>, proof_points: <a href="zkp.md#0x2_groth16_ProofPoints">ProofPoints</a>): bool {
    <a href="zkp.md#0x2_groth16_verify_groth16_proof_internal">verify_groth16_proof_internal</a>(
        &prepared_verifying_key.vk_gamma_abc_g1_bytes,
        &prepared_verifying_key.alpha_g1_beta_g2_bytes,
        &prepared_verifying_key.gamma_g2_neg_pc_bytes,
        &prepared_verifying_key.delta_g2_neg_pc_bytes,
        &public_proof_inputs.bytes,
        &proof_points.bytes
    )
}
</code></pre>



</details>

<a name="0x2_groth16_verify_groth16_proof_internal"></a>

## Function `verify_groth16_proof_internal`

Native functions that flattens the inputs into arrays of vectors and passed to the Rust native function.


<pre><code><b>public</b> <b>fun</b> <a href="zkp.md#0x2_groth16_verify_groth16_proof_internal">verify_groth16_proof_internal</a>(vk_gamma_abc_g1_bytes: &<a href="">vector</a>&lt;u8&gt;, alpha_g1_beta_g2_bytes: &<a href="">vector</a>&lt;u8&gt;, gamma_g2_neg_pc_bytes: &<a href="">vector</a>&lt;u8&gt;, delta_g2_neg_pc_bytes: &<a href="">vector</a>&lt;u8&gt;, public_proof_inputs: &<a href="">vector</a>&lt;u8&gt;, proof_points: &<a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="zkp.md#0x2_groth16_verify_groth16_proof_internal">verify_groth16_proof_internal</a>(vk_gamma_abc_g1_bytes: &<a href="">vector</a>&lt;u8&gt;, alpha_g1_beta_g2_bytes: &<a href="">vector</a>&lt;u8&gt;, gamma_g2_neg_pc_bytes: &<a href="">vector</a>&lt;u8&gt;, delta_g2_neg_pc_bytes: &<a href="">vector</a>&lt;u8&gt;, public_proof_inputs: &<a href="">vector</a>&lt;u8&gt;, proof_points: &<a href="">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>
