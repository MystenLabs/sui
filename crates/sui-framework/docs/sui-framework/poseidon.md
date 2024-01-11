
<a name="0x2_poseidon"></a>

# Module `0x2::poseidon`

Module which defines instances of the poseidon hash functions.


-  [Constants](#@Constants_0)
-  [Function `poseidon_bn254`](#0x2_poseidon_poseidon_bn254)
-  [Function `poseidon_bn254_internal`](#0x2_poseidon_poseidon_bn254_internal)


<pre><code><b>use</b> <a href="bcs.md#0x2_bcs">0x2::bcs</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_poseidon_BN254_MAX"></a>

The field size for BN254 curve.


<pre><code><b>const</b> <a href="poseidon.md#0x2_poseidon_BN254_MAX">BN254_MAX</a>: u256 = 21888242871839275222246405745257275088548364400416034343698204186575808495617;
</code></pre>



<a name="0x2_poseidon_EEmptyInput"></a>

Error if an empty vector is passed as input.


<pre><code><b>const</b> <a href="poseidon.md#0x2_poseidon_EEmptyInput">EEmptyInput</a>: u64 = 1;
</code></pre>



<a name="0x2_poseidon_ENonCanonicalInput"></a>

Error if any of the inputs are larger than or equal to the BN254 field size.


<pre><code><b>const</b> <a href="poseidon.md#0x2_poseidon_ENonCanonicalInput">ENonCanonicalInput</a>: u64 = 0;
</code></pre>



<a name="0x2_poseidon_poseidon_bn254"></a>

## Function `poseidon_bn254`

@param data: Vector of BN254 field elements to hash.

Hash the inputs using poseidon_bn254 and returns a BN254 field element.

Each element has to be a BN254 field element in canonical representation so it must be smaller than the BN254
scalar field size which is 21888242871839275222246405745257275088548364400416034343698204186575808495617.


<pre><code><b>public</b> <b>fun</b> <a href="poseidon.md#0x2_poseidon_poseidon_bn254">poseidon_bn254</a>(data: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u256&gt;): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="poseidon.md#0x2_poseidon_poseidon_bn254">poseidon_bn254</a>(data: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u256&gt;): u256 {
    <b>let</b> (i, b, l) = (0, <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>[], <a href="dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(data));
    <b>assert</b>!(l &gt; 0, <a href="poseidon.md#0x2_poseidon_EEmptyInput">EEmptyInput</a>);
    <b>while</b> (i &lt; l) {
        <b>let</b> field_element = <a href="dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(data, i);
        <b>assert</b>!(*field_element &lt; <a href="poseidon.md#0x2_poseidon_BN254_MAX">BN254_MAX</a>, <a href="poseidon.md#0x2_poseidon_ENonCanonicalInput">ENonCanonicalInput</a>);
        <a href="dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> b, <a href="dependencies/move-stdlib/bcs.md#0x1_bcs_to_bytes">bcs::to_bytes</a>(<a href="dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(data, i)));
        i = i + 1;
    };
    <b>let</b> binary_output = <a href="poseidon.md#0x2_poseidon_poseidon_bn254_internal">poseidon_bn254_internal</a>(&b);
    bcs::peel_u256(&<b>mut</b> bcs::new(binary_output))
}
</code></pre>



</details>

<a name="0x2_poseidon_poseidon_bn254_internal"></a>

## Function `poseidon_bn254_internal`

@param data: Vector of BN254 field elements in little-endian representation.

Hash the inputs using poseidon_bn254 and returns a BN254 field element in little-endian representation.


<pre><code><b>fun</b> <a href="poseidon.md#0x2_poseidon_poseidon_bn254_internal">poseidon_bn254_internal</a>(data: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="poseidon.md#0x2_poseidon_poseidon_bn254_internal">poseidon_bn254_internal</a>(data: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>
