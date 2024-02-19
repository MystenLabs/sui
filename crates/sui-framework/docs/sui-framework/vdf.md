
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


<a name="0x2_vdf_EInvalidInput"></a>

Error code for invalid input


<pre><code><b>const</b> <a href="vdf.md#0x2_vdf_EInvalidInput">EInvalidInput</a>: u64 = 0;
</code></pre>



<a name="0x2_vdf_MAX_INPUT_LENGTH"></a>

The largest allowed byte length of the input to the VDF.


<pre><code><b>const</b> <a href="vdf.md#0x2_vdf_MAX_INPUT_LENGTH">MAX_INPUT_LENGTH</a>: u64 = 384;
</code></pre>



<a name="0x2_vdf_hash_to_input"></a>

## Function `hash_to_input`

Hash an arbitrary binary <code>message</code> to a class group element to be used as input for <code>vdf_verify</code>.

The <code>discriminant</code> defines what class group to use and should be the same as used in <code>vdf_verify</code>. The
<code>discriminant</code> should be encoded as a big-endian encoding of the negation of the negative discriminant.


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_hash_to_input">hash_to_input</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, message: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_hash_to_input">hash_to_input</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, message: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    // We allow up <b>to</b> 3072 bit discriminants
    <b>assert</b>!(std::vector::length(discriminant) &lt;= <a href="vdf.md#0x2_vdf_MAX_INPUT_LENGTH">MAX_INPUT_LENGTH</a>, <a href="vdf.md#0x2_vdf_EInvalidInput">EInvalidInput</a>);
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

Verify the output and proof of a VDF with the given number of iterations. The <code>input</code>, <code>output</code> and <code>proof</code>
are all class group elements represented by triples <code>(a,b,c)</code> such that <code>b^2 + 4ac = discriminant</code>. They should
be encoded in the following format:

<code>a_len</code> (2 bytes, big endian) | <code>a</code> as unsigned big endian bytes | <code>b_len</code> (2 bytes, big endian) | <code>b</code> as signed
big endian bytes

Note that <code>c</code> is omitted because it may be computed from <code>a</code> and <code>b</code> and <code>discriminant</code>.

The <code>discriminant</code> defines what class group to use and should be the same as used in <code>hash_to_input</code>. The
<code>discriminant</code> should be encoded as a big-endian encoding of the negation of the negative discriminant.

This uses Wesolowski's VDF construction over imaginary class groups


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_vdf_verify">vdf_verify</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, input: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, output: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, iterations: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vdf.md#0x2_vdf_vdf_verify">vdf_verify</a>(discriminant: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, input: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, output: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, iterations: u64): bool {
    // We allow up <b>to</b> 3072 bit discriminants
    <b>assert</b>!(std::vector::length(discriminant) &lt;= <a href="vdf.md#0x2_vdf_MAX_INPUT_LENGTH">MAX_INPUT_LENGTH</a>, <a href="vdf.md#0x2_vdf_EInvalidInput">EInvalidInput</a>);
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
