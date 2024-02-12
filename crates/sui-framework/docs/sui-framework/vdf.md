
<a name="0x2_vdf"></a>

# Module `0x2::vdf`



-  [Constants](#@Constants_0)
-  [Function `hash_to_input`](#0x2_vdf_hash_to_input)
-  [Function `hash_to_input_internal`](#0x2_vdf_hash_to_input_internal)
-  [Function `vdf_verify`](#0x2_vdf_vdf_verify)
-  [Function `vdf_verify_internal`](#0x2_vdf_vdf_verify_internal)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_vdf_EINVALID_INPUT"></a>



<pre><code><b>const</b> <a href="vdf.md#0x2_vdf_EINVALID_INPUT">EINVALID_INPUT</a>: u64 = 0;
</code></pre>



<a name="0x2_vdf_hash_to_input"></a>

## Function `hash_to_input`

Hash an arbitrary binary <code>message</code> to an input for the VDF.


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_hash_to_input">hash_to_input</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, message: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_hash_to_input">hash_to_input</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, message: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    // We allow up <b>to</b> 3072 bit discriminants
    <b>assert</b>!(std::vector::length(discriminant) &lt;= 384, <a href="vdf.md#0x2_vdf_EINVALID_INPUT">EINVALID_INPUT</a>);
    <a href="vdf.md#0x2_vdf_hash_to_input_internal">hash_to_input_internal</a>(discriminant, message)
}
</code></pre>



</details>

<a name="0x2_vdf_hash_to_input_internal"></a>

## Function `hash_to_input_internal`



<pre><code><b>fun</b> <a href="vdf.md#0x2_vdf_hash_to_input_internal">hash_to_input_internal</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, message: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b>  <b>fun</b> <a href="vdf.md#0x2_vdf_hash_to_input_internal">hash_to_input_internal</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, message: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_vdf_vdf_verify"></a>

## Function `vdf_verify`

Verify the output and proof of a VDF with the given number of iterations.


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_vdf_verify">vdf_verify</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, input: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, output: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, iterations: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_vdf_verify">vdf_verify</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, input: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, output: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, iterations: u64): bool {
    // We allow up <b>to</b> 3072 bit discriminants
    <b>assert</b>!(std::vector::length(discriminant) &lt;= 384, <a href="vdf.md#0x2_vdf_EINVALID_INPUT">EINVALID_INPUT</a>);
    <a href="vdf.md#0x2_vdf_vdf_verify_internal">vdf_verify_internal</a>(discriminant, input, output, proof, iterations)
}
</code></pre>



</details>

<a name="0x2_vdf_vdf_verify_internal"></a>

## Function `vdf_verify_internal`



<pre><code><b>fun</b> <a href="vdf.md#0x2_vdf_vdf_verify_internal">vdf_verify_internal</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, input: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, output: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, iterations: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="vdf.md#0x2_vdf_vdf_verify_internal">vdf_verify_internal</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, input: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, output: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, iterations: u64): bool;
</code></pre>



</details>
