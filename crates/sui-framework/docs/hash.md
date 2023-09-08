
<a name="0x2_hash"></a>

# Module `0x2::hash`

Module which defines hash functions. Note that Sha-256 and Sha3-256 is available in the std::hash module in the
standard library.


-  [Constants](#@Constants_0)
-  [Function `blake2b256`](#0x2_hash_blake2b256)
-  [Function `keccak256`](#0x2_hash_keccak256)
-  [Function `poseidon_bn254`](#0x2_hash_poseidon_bn254)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_hash_EInvalidPoseidonInput"></a>

Error if the input to the Poseidon hash function is invalid: Either if
more than 32 inputs are provided or if any of the inputs are larger than
the BN254 field size.


<pre><code><b>const</b> <a href="hash.md#0x2_hash_EInvalidPoseidonInput">EInvalidPoseidonInput</a>: u64 = 0;
</code></pre>



<a name="0x2_hash_blake2b256"></a>

## Function `blake2b256`

@param data: Arbitrary binary data to hash
Hash the input bytes using Blake2b-256 and returns 32 bytes.


<pre><code><b>public</b> <b>fun</b> <a href="hash.md#0x2_hash_blake2b256">blake2b256</a>(data: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>public</b> <b>fun</b> <a href="hash.md#0x2_hash_blake2b256">blake2b256</a>(data: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] <b>true</b>;
</code></pre>



</details>

<a name="0x2_hash_keccak256"></a>

## Function `keccak256`

@param data: Arbitrary binary data to hash
Hash the input bytes using keccak256 and returns 32 bytes.


<pre><code><b>public</b> <b>fun</b> <a href="hash.md#0x2_hash_keccak256">keccak256</a>(data: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>public</b> <b>fun</b> <a href="hash.md#0x2_hash_keccak256">keccak256</a>(data: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] <b>true</b>;
</code></pre>



</details>

<a name="0x2_hash_poseidon_bn254"></a>

## Function `poseidon_bn254`

@param data: Vector of BN254 field elements to hash.
Hash the inputs using poseidon_bn254 and returns a BN254 field element.
The number of inputs cannot exceed 32 and each element has to be a BN254
field element in canonical representation so they cannot be larger than
the BN254 field size, p = 0x2523648240000001BA344D80000000086121000000000013A700000000000013.


<pre><code><b>public</b> <b>fun</b> <a href="hash.md#0x2_hash_poseidon_bn254">poseidon_bn254</a>(data: &<a href="">vector</a>&lt;u256&gt;): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>public</b> <b>fun</b> <a href="hash.md#0x2_hash_poseidon_bn254">poseidon_bn254</a>(data: &<a href="">vector</a>&lt;u256&gt;): u256;
</code></pre>



</details>
