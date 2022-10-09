
<a name="0x2_digest"></a>

# Module `0x2::digest`

Sui types for message digests.


-  [Struct `Sha256Digest`](#0x2_digest_Sha256Digest)
-  [Constants](#@Constants_0)
-  [Function `new_sha256_digest`](#0x2_digest_new_sha256_digest)
-  [Function `sha256_digest`](#0x2_digest_sha256_digest)


<pre><code></code></pre>



<a name="0x2_digest_Sha256Digest"></a>

## Struct `Sha256Digest`

Sha256Digest: An immutable wrapper of SHA256_DIGEST_VECTOR_LENGTH bytes.


<pre><code><b>struct</b> <a href="digest.md#0x2_digest_Sha256Digest">Sha256Digest</a> <b>has</b> <b>copy</b>, drop, store
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



<a name="0x2_digest_SHA256_DIGEST_VECTOR_LENGTH"></a>

Length of the vector<u8> representing a SHA256 digest.


<pre><code><b>const</b> <a href="digest.md#0x2_digest_SHA256_DIGEST_VECTOR_LENGTH">SHA256_DIGEST_VECTOR_LENGTH</a>: u64 = 32;
</code></pre>



<a name="0x2_digest_new_sha256_digest"></a>

## Function `new_sha256_digest`

Create a <code><a href="digest.md#0x2_digest_Sha256Digest">Sha256Digest</a></code> from bytes. Aborts if <code>bytes</code> is not of length 32.


<pre><code><b>public</b> <b>fun</b> <a href="digest.md#0x2_digest_new_sha256_digest">new_sha256_digest</a>(<a href="digest.md#0x2_digest">digest</a>: <a href="">vector</a>&lt;u8&gt;): <a href="digest.md#0x2_digest_Sha256Digest">digest::Sha256Digest</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="digest.md#0x2_digest_new_sha256_digest">new_sha256_digest</a>(<a href="digest.md#0x2_digest">digest</a>: <a href="">vector</a>&lt;u8&gt;): <a href="digest.md#0x2_digest_Sha256Digest">Sha256Digest</a> {
    <b>assert</b>!(<a href="_length">vector::length</a>(&<a href="digest.md#0x2_digest">digest</a>) == <a href="digest.md#0x2_digest_SHA256_DIGEST_VECTOR_LENGTH">SHA256_DIGEST_VECTOR_LENGTH</a>, <a href="digest.md#0x2_digest_EHashLengthMismatch">EHashLengthMismatch</a>);
    <a href="digest.md#0x2_digest_Sha256Digest">Sha256Digest</a> { <a href="digest.md#0x2_digest">digest</a> }
}
</code></pre>



</details>

<a name="0x2_digest_sha256_digest"></a>

## Function `sha256_digest`

Get the digest.


<pre><code><b>public</b> <b>fun</b> <a href="digest.md#0x2_digest_sha256_digest">sha256_digest</a>(self: &<a href="digest.md#0x2_digest_Sha256Digest">digest::Sha256Digest</a>): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="digest.md#0x2_digest_sha256_digest">sha256_digest</a>(self: &<a href="digest.md#0x2_digest_Sha256Digest">Sha256Digest</a>): <a href="">vector</a>&lt;u8&gt; {
    self.<a href="digest.md#0x2_digest">digest</a>
}
</code></pre>



</details>
