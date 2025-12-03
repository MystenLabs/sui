---
title: Module `sui::poseidon`
---

Module which defines instances of the poseidon hash functions. Available in Devnet and Testnet.


-  [Constants](#@Constants_0)
-  [Function `poseidon_bn254`](#sui_poseidon_poseidon_bn254)
-  [Function `poseidon_bn254_internal`](#sui_poseidon_poseidon_bn254_internal)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="sui_poseidon_ENonCanonicalInput"></a>

Error if any of the inputs are larger than or equal to the BN254 field size.


<pre><code><b>const</b> <a href="../sui/poseidon.md#sui_poseidon_ENonCanonicalInput">ENonCanonicalInput</a>: u64 = 0;
</code></pre>



<a name="sui_poseidon_EEmptyInput"></a>

Error if an empty vector is passed as input.


<pre><code><b>const</b> <a href="../sui/poseidon.md#sui_poseidon_EEmptyInput">EEmptyInput</a>: u64 = 1;
</code></pre>



<a name="sui_poseidon_ETooManyInputs"></a>

Error if more than MAX_INPUTS inputs are given.


<pre><code><b>const</b> <a href="../sui/poseidon.md#sui_poseidon_ETooManyInputs">ETooManyInputs</a>: u64 = 2;
</code></pre>



<a name="sui_poseidon_BN254_MAX"></a>

The field size for BN254 curve.


<pre><code><b>const</b> <a href="../sui/poseidon.md#sui_poseidon_BN254_MAX">BN254_MAX</a>: u256 = 21888242871839275222246405745257275088548364400416034343698204186575808495617;
</code></pre>



<a name="sui_poseidon_MAX_INPUTS"></a>

The maximum number of inputs for the poseidon_bn254 function.


<pre><code><b>const</b> <a href="../sui/poseidon.md#sui_poseidon_MAX_INPUTS">MAX_INPUTS</a>: u64 = 16;
</code></pre>



<a name="sui_poseidon_poseidon_bn254"></a>

## Function `poseidon_bn254`

@param data: Vector of BN254 field elements to hash.

Hash the inputs using poseidon_bn254 and returns a BN254 field element.

Each element has to be a BN254 field element in canonical representation so it must be smaller than the BN254
scalar field size which is 21888242871839275222246405745257275088548364400416034343698204186575808495617.

This function supports between 1 and 16 inputs. If you need to hash more than 16 inputs, some implementations
instead returns the root of a k-ary Merkle tree with the inputs as leafs, but since this is not standardized,
we leave that to the caller to implement if needed.

If the input is empty, the function will abort with EEmptyInput.
If more than 16 inputs are provided, the function will abort with ETooManyInputs.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/poseidon.md#sui_poseidon_poseidon_bn254">poseidon_bn254</a>(data: &vector&lt;u256&gt;): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/poseidon.md#sui_poseidon_poseidon_bn254">poseidon_bn254</a>(data: &vector&lt;u256&gt;): u256 {
    <b>assert</b>!(data.length() &gt; 0, <a href="../sui/poseidon.md#sui_poseidon_EEmptyInput">EEmptyInput</a>);
    <b>assert</b>!(data.length() &lt;= <a href="../sui/poseidon.md#sui_poseidon_MAX_INPUTS">MAX_INPUTS</a>, <a href="../sui/poseidon.md#sui_poseidon_ETooManyInputs">ETooManyInputs</a>);
    <b>let</b> b = data.map_ref!(|e| {
        <b>assert</b>!(*e &lt; <a href="../sui/poseidon.md#sui_poseidon_BN254_MAX">BN254_MAX</a>, <a href="../sui/poseidon.md#sui_poseidon_ENonCanonicalInput">ENonCanonicalInput</a>);
        <a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(e)
    });
    <b>let</b> binary_output = <a href="../sui/poseidon.md#sui_poseidon_poseidon_bn254_internal">poseidon_bn254_internal</a>(&b);
    <a href="../sui/bcs.md#sui_bcs_new">bcs::new</a>(binary_output).peel_u256()
}
</code></pre>



</details>

<a name="sui_poseidon_poseidon_bn254_internal"></a>

## Function `poseidon_bn254_internal`

@param data: Vector of BN254 field elements in little-endian representation.

Hash the inputs using poseidon_bn254 and returns a BN254 field element in little-endian representation.


<pre><code><b>fun</b> <a href="../sui/poseidon.md#sui_poseidon_poseidon_bn254_internal">poseidon_bn254_internal</a>(data: &vector&lt;vector&lt;u8&gt;&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/poseidon.md#sui_poseidon_poseidon_bn254_internal">poseidon_bn254_internal</a>(data: &vector&lt;vector&lt;u8&gt;&gt;): vector&lt;u8&gt;;
</code></pre>



</details>
