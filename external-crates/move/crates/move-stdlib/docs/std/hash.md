
<a name="std_hash"></a>

# Module `std::hash`

Module which defines SHA hashes for byte vectors.

The functions in this module are natively declared both in the Move runtime
as in the Move prover's prelude.


-  [Function `sha2_256`](#std_hash_sha2_256)
-  [Function `sha3_256`](#std_hash_sha3_256)


<pre><code></code></pre>



<a name="std_hash_sha2_256"></a>

## Function `sha2_256`



<pre><code><b>public</b> <b>fun</b> <a href="../std/hash.md#std_hash_sha2_256">sha2_256</a>(data: <a href="../std/vector.md#std_vector">vector</a>&lt;<a href="../std/u8.md#std_u8">u8</a>&gt;): <a href="../std/vector.md#std_vector">vector</a>&lt;<a href="../std/u8.md#std_u8">u8</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>public</b> <b>fun</b> <a href="../std/hash.md#std_hash_sha2_256">sha2_256</a>(data: <a href="../std/vector.md#std_vector">vector</a>&lt;<a href="../std/u8.md#std_u8">u8</a>&gt;): <a href="../std/vector.md#std_vector">vector</a>&lt;<a href="../std/u8.md#std_u8">u8</a>&gt;;
</code></pre>



</details>

<a name="std_hash_sha3_256"></a>

## Function `sha3_256`



<pre><code><b>public</b> <b>fun</b> <a href="../std/hash.md#std_hash_sha3_256">sha3_256</a>(data: <a href="../std/vector.md#std_vector">vector</a>&lt;<a href="../std/u8.md#std_u8">u8</a>&gt;): <a href="../std/vector.md#std_vector">vector</a>&lt;<a href="../std/u8.md#std_u8">u8</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>public</b> <b>fun</b> <a href="../std/hash.md#std_hash_sha3_256">sha3_256</a>(data: <a href="../std/vector.md#std_vector">vector</a>&lt;<a href="../std/u8.md#std_u8">u8</a>&gt;): <a href="../std/vector.md#std_vector">vector</a>&lt;<a href="../std/u8.md#std_u8">u8</a>&gt;;
</code></pre>



</details>


[//]: # ("File containing references which can be used from documentation")
