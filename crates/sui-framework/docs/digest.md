
<a name="0x2_digest"></a>

# Module `0x2::digest`

Sui types for message digests.


-  [Struct `Sha2256Digest`](#0x2_digest_Sha2256Digest)
-  [Constants](#@Constants_0)
-  [Function `sha2_256_digest_from_bytes`](#0x2_digest_sha2_256_digest_from_bytes)
-  [Function `sha2_256_digest_to_bytes`](#0x2_digest_sha2_256_digest_to_bytes)


<pre><code></code></pre>



<a name="0x2_digest_Sha2256Digest"></a>

## Struct `Sha2256Digest`

Sha2256Digest: An immutable wrapper of SHA2256_DIGEST_VECTOR_LENGTH bytes.


<pre><code><b>struct</b> <a href="digest.md#0x2_digest_Sha2256Digest">Sha2256Digest</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="digest.md#0x2_digest">digest</a>: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_digest_EHashLengthMismatch"></a>

Error code when the length of the digest vector is invalid.


<pre><code><b>const</b> <a href="digest.md#0x2_digest_EHashLengthMismatch">EHashLengthMismatch</a>: u64 = 0;
</code></pre>



<a name="0x2_digest_SHA2_256_DIGEST_VECTOR_LENGTH"></a>

Length of the vector<u8> representing a SHA2-256 digest.


<pre><code><b>const</b> <a href="digest.md#0x2_digest_SHA2_256_DIGEST_VECTOR_LENGTH">SHA2_256_DIGEST_VECTOR_LENGTH</a>: u64 = 32;
</code></pre>



<a name="0x2_digest_sha2_256_digest_from_bytes"></a>

## Function `sha2_256_digest_from_bytes`

Create a <code><a href="digest.md#0x2_digest_Sha2256Digest">Sha2256Digest</a></code> from bytes. Aborts if <code>bytes</code> is not of length 32.


<pre><code><b>public</b> <b>fun</b> <a href="digest.md#0x2_digest_sha2_256_digest_from_bytes">sha2_256_digest_from_bytes</a>(<a href="digest.md#0x2_digest">digest</a>: <a href="">vector</a>&lt;u8&gt;): <a href="digest.md#0x2_digest_Sha2256Digest">digest::Sha2256Digest</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="digest.md#0x2_digest_sha2_256_digest_from_bytes">sha2_256_digest_from_bytes</a>(<a href="digest.md#0x2_digest">digest</a>: <a href="">vector</a>&lt;u8&gt;): <a href="digest.md#0x2_digest_Sha2256Digest">Sha2256Digest</a> {
    <b>assert</b>!(<a href="_length">vector::length</a>(&<a href="digest.md#0x2_digest">digest</a>) == <a href="digest.md#0x2_digest_SHA2_256_DIGEST_VECTOR_LENGTH">SHA2_256_DIGEST_VECTOR_LENGTH</a>, <a href="digest.md#0x2_digest_EHashLengthMismatch">EHashLengthMismatch</a>);
    <a href="digest.md#0x2_digest_Sha2256Digest">Sha2256Digest</a> { <a href="digest.md#0x2_digest">digest</a> }
}
</code></pre>



</details>

<a name="0x2_digest_sha2_256_digest_to_bytes"></a>

## Function `sha2_256_digest_to_bytes`

Get the digest.


<pre><code><b>public</b> <b>fun</b> <a href="digest.md#0x2_digest_sha2_256_digest_to_bytes">sha2_256_digest_to_bytes</a>(self: &<a href="digest.md#0x2_digest_Sha2256Digest">digest::Sha2256Digest</a>): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="digest.md#0x2_digest_sha2_256_digest_to_bytes">sha2_256_digest_to_bytes</a>(self: &<a href="digest.md#0x2_digest_Sha2256Digest">Sha2256Digest</a>): <a href="">vector</a>&lt;u8&gt; {
    self.<a href="digest.md#0x2_digest">digest</a>
}
</code></pre>



</details>
