
<a name="0x1_hash"></a>

# Module `0x1::hash`

Module which defines SHA hashes for byte vectors.

The functions in this module are natively declared both in the Move runtime
as in the Move prover's prelude.


-  [Function `sha2_256`](#0x1_hash_sha2_256)
-  [Function `sha3_256`](#0x1_hash_sha3_256)


<pre><code></code></pre>



<a name="0x1_hash_sha2_256"></a>

## Function `sha2_256`



<pre><code><b>public</b> <b>fun</b> <a href="hash.md#0x1_hash_sha2_256">sha2_256</a>(data: <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>public</b> <b>fun</b> <a href="hash.md#0x1_hash_sha2_256">sha2_256</a>(data: <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x1_hash_sha3_256"></a>

## Function `sha3_256`



<pre><code><b>public</b> <b>fun</b> <a href="hash.md#0x1_hash_sha3_256">sha3_256</a>(data: <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>public</b> <b>fun</b> <a href="hash.md#0x1_hash_sha3_256">sha3_256</a>(data: <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>


[//]: # ("File containing references which can be used from documentation")
