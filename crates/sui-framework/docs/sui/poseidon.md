---
title: Module `sui::poseidon`
---

Module which defines instances of the poseidon hash functions.


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


<a name="sui_poseidon_BN254_MAX"></a>

The field size for BN254 curve.


<pre><code><b>const</b> <a href="../sui/poseidon.md#sui_poseidon_BN254_MAX">BN254_MAX</a>: u256 = 21888242871839275222246405745257275088548364400416034343698204186575808495617;
</code></pre>



<a name="sui_poseidon_EEmptyInput"></a>

Error if an empty vector is passed as input.


<pre><code><b>const</b> <a href="../sui/poseidon.md#sui_poseidon_EEmptyInput">EEmptyInput</a>: u64 = 1;
</code></pre>



<a name="sui_poseidon_ENonCanonicalInput"></a>

Error if any of the inputs are larger than or equal to the BN254 field size.


<pre><code><b>const</b> <a href="../sui/poseidon.md#sui_poseidon_ENonCanonicalInput">ENonCanonicalInput</a>: u64 = 0;
</code></pre>



<a name="sui_poseidon_poseidon_bn254"></a>

## Function `poseidon_bn254`

@param data: Vector of BN254 field elements to hash.

Hash the inputs using poseidon_bn254 and returns a BN254 field element.

Each element has to be a BN254 field element in canonical representation so it must be smaller than the BN254
scalar field size which is 21888242871839275222246405745257275088548364400416034343698204186575808495617.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/poseidon.md#sui_poseidon_poseidon_bn254">poseidon_bn254</a>(data: &vector&lt;u256&gt;): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/poseidon.md#sui_poseidon_poseidon_bn254">poseidon_bn254</a>(data: &vector&lt;u256&gt;): u256 {
    <b>let</b> (<b>mut</b> i, <b>mut</b> b, l) = (0, vector[], data.length());
    <b>assert</b>!(l &gt; 0, <a href="../sui/poseidon.md#sui_poseidon_EEmptyInput">EEmptyInput</a>);
    <b>while</b> (i &lt; l) {
        <b>let</b> field_element = &data[i];
        <b>assert</b>!(*field_element &lt; <a href="../sui/poseidon.md#sui_poseidon_BN254_MAX">BN254_MAX</a>, <a href="../sui/poseidon.md#sui_poseidon_ENonCanonicalInput">ENonCanonicalInput</a>);
        b.push_back(<a href="../sui/bcs.md#sui_bcs_to_bytes">bcs::to_bytes</a>(&data[i]));
        i = i + 1;
    };
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
