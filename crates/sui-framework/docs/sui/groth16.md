---
title: Module `sui::groth16`
---



-  [Struct `Curve`](#sui_groth16_Curve)
-  [Struct `PreparedVerifyingKey`](#sui_groth16_PreparedVerifyingKey)
-  [Struct `PublicProofInputs`](#sui_groth16_PublicProofInputs)
-  [Struct `ProofPoints`](#sui_groth16_ProofPoints)
-  [Constants](#@Constants_0)
-  [Function `bls12381`](#sui_groth16_bls12381)
-  [Function `bn254`](#sui_groth16_bn254)
-  [Function `pvk_from_bytes`](#sui_groth16_pvk_from_bytes)
-  [Function `pvk_to_bytes`](#sui_groth16_pvk_to_bytes)
-  [Function `public_proof_inputs_from_bytes`](#sui_groth16_public_proof_inputs_from_bytes)
-  [Function `proof_points_from_bytes`](#sui_groth16_proof_points_from_bytes)
-  [Function `prepare_verifying_key`](#sui_groth16_prepare_verifying_key)
-  [Function `prepare_verifying_key_internal`](#sui_groth16_prepare_verifying_key_internal)
-  [Function `verify_groth16_proof`](#sui_groth16_verify_groth16_proof)
-  [Function `verify_groth16_proof_internal`](#sui_groth16_verify_groth16_proof_internal)


<pre><code></code></pre>



<a name="sui_groth16_Curve"></a>

## Struct `Curve`

Represents an elliptic curve construction to be used in the verifier. Currently we support BLS12-381 and BN254.
This should be given as the first parameter to <code><a href="../sui/groth16.md#sui_groth16_prepare_verifying_key">prepare_verifying_key</a></code> or <code><a href="../sui/groth16.md#sui_groth16_verify_groth16_proof">verify_groth16_proof</a></code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/groth16.md#sui_groth16_Curve">Curve</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: u8</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_groth16_PreparedVerifyingKey"></a>

## Struct `PreparedVerifyingKey`

A <code><a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a></code> consisting of four components in serialized form.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>vk_gamma_abc_g1_bytes: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>alpha_g1_beta_g2_bytes: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>gamma_g2_neg_pc_bytes: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>delta_g2_neg_pc_bytes: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_groth16_PublicProofInputs"></a>

## Struct `PublicProofInputs`

A <code><a href="../sui/groth16.md#sui_groth16_PublicProofInputs">PublicProofInputs</a></code> wrapper around its serialized bytes.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/groth16.md#sui_groth16_PublicProofInputs">PublicProofInputs</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>bytes: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_groth16_ProofPoints"></a>

## Struct `ProofPoints`

A <code><a href="../sui/groth16.md#sui_groth16_ProofPoints">ProofPoints</a></code> wrapper around the serialized form of three proof points.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/groth16.md#sui_groth16_ProofPoints">ProofPoints</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>bytes: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_groth16_EInvalidCurve"></a>



<pre><code><b>const</b> <a href="../sui/groth16.md#sui_groth16_EInvalidCurve">EInvalidCurve</a>: u64 = 1;
</code></pre>



<a name="sui_groth16_EInvalidScalar"></a>



<pre><code><b>const</b> <a href="../sui/groth16.md#sui_groth16_EInvalidScalar">EInvalidScalar</a>: u64 = 3;
</code></pre>



<a name="sui_groth16_EInvalidVerifyingKey"></a>



<pre><code><b>const</b> <a href="../sui/groth16.md#sui_groth16_EInvalidVerifyingKey">EInvalidVerifyingKey</a>: u64 = 0;
</code></pre>



<a name="sui_groth16_ETooManyPublicInputs"></a>



<pre><code><b>const</b> <a href="../sui/groth16.md#sui_groth16_ETooManyPublicInputs">ETooManyPublicInputs</a>: u64 = 2;
</code></pre>



<a name="sui_groth16_MaxPublicInputs"></a>



<pre><code><b>const</b> <a href="../sui/groth16.md#sui_groth16_MaxPublicInputs">MaxPublicInputs</a>: u64 = 8;
</code></pre>



<a name="sui_groth16_bls12381"></a>

## Function `bls12381`

Return the <code><a href="../sui/groth16.md#sui_groth16_Curve">Curve</a></code> value indicating that the BLS12-381 construction should be used in a given function.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bls12381.md#sui_bls12381">bls12381</a>(): <a href="../sui/groth16.md#sui_groth16_Curve">sui::groth16::Curve</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bls12381.md#sui_bls12381">bls12381</a>(): <a href="../sui/groth16.md#sui_groth16_Curve">Curve</a> { <a href="../sui/groth16.md#sui_groth16_Curve">Curve</a> { id: 0 } }
</code></pre>



</details>

<a name="sui_groth16_bn254"></a>

## Function `bn254`

Return the <code><a href="../sui/groth16.md#sui_groth16_Curve">Curve</a></code> value indicating that the BN254 construction should be used in a given function.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_bn254">bn254</a>(): <a href="../sui/groth16.md#sui_groth16_Curve">sui::groth16::Curve</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_bn254">bn254</a>(): <a href="../sui/groth16.md#sui_groth16_Curve">Curve</a> { <a href="../sui/groth16.md#sui_groth16_Curve">Curve</a> { id: 1 } }
</code></pre>



</details>

<a name="sui_groth16_pvk_from_bytes"></a>

## Function `pvk_from_bytes`

Creates a <code><a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a></code> from bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_pvk_from_bytes">pvk_from_bytes</a>(vk_gamma_abc_g1_bytes: vector&lt;u8&gt;, alpha_g1_beta_g2_bytes: vector&lt;u8&gt;, gamma_g2_neg_pc_bytes: vector&lt;u8&gt;, delta_g2_neg_pc_bytes: vector&lt;u8&gt;): <a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">sui::groth16::PreparedVerifyingKey</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_pvk_from_bytes">pvk_from_bytes</a>(
    vk_gamma_abc_g1_bytes: vector&lt;u8&gt;,
    alpha_g1_beta_g2_bytes: vector&lt;u8&gt;,
    gamma_g2_neg_pc_bytes: vector&lt;u8&gt;,
    delta_g2_neg_pc_bytes: vector&lt;u8&gt;,
): <a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a> {
    <a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a> {
        vk_gamma_abc_g1_bytes,
        alpha_g1_beta_g2_bytes,
        gamma_g2_neg_pc_bytes,
        delta_g2_neg_pc_bytes,
    }
}
</code></pre>



</details>

<a name="sui_groth16_pvk_to_bytes"></a>

## Function `pvk_to_bytes`

Returns bytes of the four components of the <code><a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_pvk_to_bytes">pvk_to_bytes</a>(pvk: <a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">sui::groth16::PreparedVerifyingKey</a>): vector&lt;vector&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_pvk_to_bytes">pvk_to_bytes</a>(pvk: <a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a>): vector&lt;vector&lt;u8&gt;&gt; {
    vector[
        pvk.vk_gamma_abc_g1_bytes,
        pvk.alpha_g1_beta_g2_bytes,
        pvk.gamma_g2_neg_pc_bytes,
        pvk.delta_g2_neg_pc_bytes,
    ]
}
</code></pre>



</details>

<a name="sui_groth16_public_proof_inputs_from_bytes"></a>

## Function `public_proof_inputs_from_bytes`

Creates a <code><a href="../sui/groth16.md#sui_groth16_PublicProofInputs">PublicProofInputs</a></code> wrapper from bytes. The <code>bytes</code> parameter should be a concatenation of a number of
32 bytes scalar field elements to be used as public inputs in little-endian format to a circuit.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_public_proof_inputs_from_bytes">public_proof_inputs_from_bytes</a>(bytes: vector&lt;u8&gt;): <a href="../sui/groth16.md#sui_groth16_PublicProofInputs">sui::groth16::PublicProofInputs</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_public_proof_inputs_from_bytes">public_proof_inputs_from_bytes</a>(bytes: vector&lt;u8&gt;): <a href="../sui/groth16.md#sui_groth16_PublicProofInputs">PublicProofInputs</a> {
    <b>assert</b>!(bytes.length() % 32 == 0, <a href="../sui/groth16.md#sui_groth16_EInvalidScalar">EInvalidScalar</a>);
    <b>assert</b>!(bytes.length() / 32 &lt;= <a href="../sui/groth16.md#sui_groth16_MaxPublicInputs">MaxPublicInputs</a>, <a href="../sui/groth16.md#sui_groth16_ETooManyPublicInputs">ETooManyPublicInputs</a>);
    <a href="../sui/groth16.md#sui_groth16_PublicProofInputs">PublicProofInputs</a> { bytes }
}
</code></pre>



</details>

<a name="sui_groth16_proof_points_from_bytes"></a>

## Function `proof_points_from_bytes`

Creates a Groth16 <code><a href="../sui/groth16.md#sui_groth16_ProofPoints">ProofPoints</a></code> from bytes.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_proof_points_from_bytes">proof_points_from_bytes</a>(bytes: vector&lt;u8&gt;): <a href="../sui/groth16.md#sui_groth16_ProofPoints">sui::groth16::ProofPoints</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_proof_points_from_bytes">proof_points_from_bytes</a>(bytes: vector&lt;u8&gt;): <a href="../sui/groth16.md#sui_groth16_ProofPoints">ProofPoints</a> {
    <a href="../sui/groth16.md#sui_groth16_ProofPoints">ProofPoints</a> { bytes }
}
</code></pre>



</details>

<a name="sui_groth16_prepare_verifying_key"></a>

## Function `prepare_verifying_key`

@param curve: What elliptic curve construction to use. See <code><a href="../sui/bls12381.md#sui_bls12381">bls12381</a></code> and <code><a href="../sui/groth16.md#sui_groth16_bn254">bn254</a></code>.
@param verifying_key: An Arkworks canonical compressed serialization of a verifying key.

Returns four vectors of bytes representing the four components of a prepared verifying key.
This step computes one pairing e(P, Q), and binds the verification to one particular proof statement.
This can be used as inputs for the <code><a href="../sui/groth16.md#sui_groth16_verify_groth16_proof">verify_groth16_proof</a></code> function.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_prepare_verifying_key">prepare_verifying_key</a>(curve: &<a href="../sui/groth16.md#sui_groth16_Curve">sui::groth16::Curve</a>, verifying_key: &vector&lt;u8&gt;): <a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">sui::groth16::PreparedVerifyingKey</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_prepare_verifying_key">prepare_verifying_key</a>(curve: &<a href="../sui/groth16.md#sui_groth16_Curve">Curve</a>, verifying_key: &vector&lt;u8&gt;): <a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a> {
    <a href="../sui/groth16.md#sui_groth16_prepare_verifying_key_internal">prepare_verifying_key_internal</a>(curve.id, verifying_key)
}
</code></pre>



</details>

<a name="sui_groth16_prepare_verifying_key_internal"></a>

## Function `prepare_verifying_key_internal`

Native functions that flattens the inputs into an array and passes to the Rust native function. May abort with <code><a href="../sui/groth16.md#sui_groth16_EInvalidVerifyingKey">EInvalidVerifyingKey</a></code> or <code><a href="../sui/groth16.md#sui_groth16_EInvalidCurve">EInvalidCurve</a></code>.


<pre><code><b>fun</b> <a href="../sui/groth16.md#sui_groth16_prepare_verifying_key_internal">prepare_verifying_key_internal</a>(curve: u8, verifying_key: &vector&lt;u8&gt;): <a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">sui::groth16::PreparedVerifyingKey</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_prepare_verifying_key_internal">prepare_verifying_key_internal</a>(
    curve: u8,
    verifying_key: &vector&lt;u8&gt;,
): <a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a>;
</code></pre>



</details>

<a name="sui_groth16_verify_groth16_proof"></a>

## Function `verify_groth16_proof`

@param curve: What elliptic curve construction to use. See the <code><a href="../sui/bls12381.md#sui_bls12381">bls12381</a></code> and <code><a href="../sui/groth16.md#sui_groth16_bn254">bn254</a></code> functions.
@param prepared_verifying_key: Consists of four vectors of bytes representing the four components of a prepared verifying key.
@param public_proof_inputs: Represent inputs that are public.
@param proof_points: Represent three proof points.

Returns a boolean indicating whether the proof is valid.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_verify_groth16_proof">verify_groth16_proof</a>(curve: &<a href="../sui/groth16.md#sui_groth16_Curve">sui::groth16::Curve</a>, prepared_verifying_key: &<a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">sui::groth16::PreparedVerifyingKey</a>, public_proof_inputs: &<a href="../sui/groth16.md#sui_groth16_PublicProofInputs">sui::groth16::PublicProofInputs</a>, proof_points: &<a href="../sui/groth16.md#sui_groth16_ProofPoints">sui::groth16::ProofPoints</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_verify_groth16_proof">verify_groth16_proof</a>(
    curve: &<a href="../sui/groth16.md#sui_groth16_Curve">Curve</a>,
    prepared_verifying_key: &<a href="../sui/groth16.md#sui_groth16_PreparedVerifyingKey">PreparedVerifyingKey</a>,
    public_proof_inputs: &<a href="../sui/groth16.md#sui_groth16_PublicProofInputs">PublicProofInputs</a>,
    proof_points: &<a href="../sui/groth16.md#sui_groth16_ProofPoints">ProofPoints</a>,
): bool {
    <a href="../sui/groth16.md#sui_groth16_verify_groth16_proof_internal">verify_groth16_proof_internal</a>(
        curve.id,
        &prepared_verifying_key.vk_gamma_abc_g1_bytes,
        &prepared_verifying_key.alpha_g1_beta_g2_bytes,
        &prepared_verifying_key.gamma_g2_neg_pc_bytes,
        &prepared_verifying_key.delta_g2_neg_pc_bytes,
        &public_proof_inputs.bytes,
        &proof_points.bytes,
    )
}
</code></pre>



</details>

<a name="sui_groth16_verify_groth16_proof_internal"></a>

## Function `verify_groth16_proof_internal`

Native functions that flattens the inputs into arrays of vectors and passed to the Rust native function. May abort with <code><a href="../sui/groth16.md#sui_groth16_EInvalidCurve">EInvalidCurve</a></code> or <code><a href="../sui/groth16.md#sui_groth16_ETooManyPublicInputs">ETooManyPublicInputs</a></code>.


<pre><code><b>fun</b> <a href="../sui/groth16.md#sui_groth16_verify_groth16_proof_internal">verify_groth16_proof_internal</a>(curve: u8, vk_gamma_abc_g1_bytes: &vector&lt;u8&gt;, alpha_g1_beta_g2_bytes: &vector&lt;u8&gt;, gamma_g2_neg_pc_bytes: &vector&lt;u8&gt;, delta_g2_neg_pc_bytes: &vector&lt;u8&gt;, public_proof_inputs: &vector&lt;u8&gt;, proof_points: &vector&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/groth16.md#sui_groth16_verify_groth16_proof_internal">verify_groth16_proof_internal</a>(
    curve: u8,
    vk_gamma_abc_g1_bytes: &vector&lt;u8&gt;,
    alpha_g1_beta_g2_bytes: &vector&lt;u8&gt;,
    gamma_g2_neg_pc_bytes: &vector&lt;u8&gt;,
    delta_g2_neg_pc_bytes: &vector&lt;u8&gt;,
    public_proof_inputs: &vector&lt;u8&gt;,
    proof_points: &vector&lt;u8&gt;,
): bool;
</code></pre>



</details>
