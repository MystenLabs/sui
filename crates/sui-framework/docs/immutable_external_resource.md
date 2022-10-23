
<a name="0x2_immutable_external_resource"></a>

# Module `0x2::immutable_external_resource`

Sui types for specifying off-chain/external resources.

The keywords "MUST", "MUST NOT", "SHOULD", "SHOULD NOT" and "MAY" below should be interpreted as described in
RFC 2119.


-  [Struct `ImmutableExternalResource`](#0x2_immutable_external_resource_ImmutableExternalResource)
-  [Function `new`](#0x2_immutable_external_resource_new)
-  [Function `digest`](#0x2_immutable_external_resource_digest)
-  [Function `url`](#0x2_immutable_external_resource_url)
-  [Function `update`](#0x2_immutable_external_resource_update)


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
<b>use</b> <a href="digest.md#0x2_digest">0x2::digest</a>;
<b>use</b> <a href="url.md#0x2_url">0x2::url</a>;
</code></pre>



<a name="0x2_immutable_external_resource_ImmutableExternalResource"></a>

## Struct `ImmutableExternalResource`

ImmutableExternalResource: An arbitrary, mutable URL plus an immutable digest of the resource.

Represents a resource that can move but must never change. Example use cases:
- NFT images.
- NFT metadata.

<code><a href="url.md#0x2_url">url</a></code> MUST follow RFC-3986. Clients MUST support (at least) the following schemes: ipfs, https.
<code><a href="digest.md#0x2_digest">digest</a></code> MUST be set to SHA3-256(content of resource at <code><a href="url.md#0x2_url">url</a></code>).

Clients of this type MUST fetch the resource at <code><a href="url.md#0x2_url">url</a></code>, compute its digest and compare it against <code><a href="digest.md#0x2_digest">digest</a></code>. If
the result is false, clients SHOULD indicate that to users or ignore the resource.


<pre><code><b>struct</b> <a href="immutable_external_resource.md#0x2_immutable_external_resource_ImmutableExternalResource">ImmutableExternalResource</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="url.md#0x2_url">url</a>: <a href="url.md#0x2_url_Url">url::Url</a></code>
</dt>
<dd>

</dd>
<dt>
<code><a href="digest.md#0x2_digest">digest</a>: <a href="digest.md#0x2_digest_Sha3256Digest">digest::Sha3256Digest</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_immutable_external_resource_new"></a>

## Function `new`

Create a <code><a href="immutable_external_resource.md#0x2_immutable_external_resource_ImmutableExternalResource">ImmutableExternalResource</a></code>, and set the immutable hash.


<pre><code><b>public</b> <b>fun</b> <a href="immutable_external_resource.md#0x2_immutable_external_resource_new">new</a>(<a href="url.md#0x2_url">url</a>: <a href="url.md#0x2_url_Url">url::Url</a>, <a href="digest.md#0x2_digest">digest</a>: <a href="digest.md#0x2_digest_Sha3256Digest">digest::Sha3256Digest</a>): <a href="immutable_external_resource.md#0x2_immutable_external_resource_ImmutableExternalResource">immutable_external_resource::ImmutableExternalResource</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="immutable_external_resource.md#0x2_immutable_external_resource_new">new</a>(<a href="url.md#0x2_url">url</a>: Url, <a href="digest.md#0x2_digest">digest</a>: Sha3256Digest): <a href="immutable_external_resource.md#0x2_immutable_external_resource_ImmutableExternalResource">ImmutableExternalResource</a> {
    <a href="immutable_external_resource.md#0x2_immutable_external_resource_ImmutableExternalResource">ImmutableExternalResource</a> { <a href="url.md#0x2_url">url</a>, <a href="digest.md#0x2_digest">digest</a> }
}
</code></pre>



</details>

<a name="0x2_immutable_external_resource_digest"></a>

## Function `digest`

Get the hash of the resource.


<pre><code><b>public</b> <b>fun</b> <a href="digest.md#0x2_digest">digest</a>(self: &<a href="immutable_external_resource.md#0x2_immutable_external_resource_ImmutableExternalResource">immutable_external_resource::ImmutableExternalResource</a>): <a href="digest.md#0x2_digest_Sha3256Digest">digest::Sha3256Digest</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="digest.md#0x2_digest">digest</a>(self: &<a href="immutable_external_resource.md#0x2_immutable_external_resource_ImmutableExternalResource">ImmutableExternalResource</a>): Sha3256Digest {
    self.<a href="digest.md#0x2_digest">digest</a>
}
</code></pre>



</details>

<a name="0x2_immutable_external_resource_url"></a>

## Function `url`

Get the URL of the resource.


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url">url</a>(self: &<a href="immutable_external_resource.md#0x2_immutable_external_resource_ImmutableExternalResource">immutable_external_resource::ImmutableExternalResource</a>): <a href="url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url">url</a>(self: &<a href="immutable_external_resource.md#0x2_immutable_external_resource_ImmutableExternalResource">ImmutableExternalResource</a>): Url {
    self.<a href="url.md#0x2_url">url</a>
}
</code></pre>



</details>

<a name="0x2_immutable_external_resource_update"></a>

## Function `update`

Update the URL, but the digest of the resource must never change.


<pre><code><b>public</b> <b>fun</b> <b>update</b>(self: &<b>mut</b> <a href="immutable_external_resource.md#0x2_immutable_external_resource_ImmutableExternalResource">immutable_external_resource::ImmutableExternalResource</a>, <a href="url.md#0x2_url">url</a>: <a href="url.md#0x2_url_Url">url::Url</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <b>update</b>(self: &<b>mut</b> <a href="immutable_external_resource.md#0x2_immutable_external_resource_ImmutableExternalResource">ImmutableExternalResource</a>, <a href="url.md#0x2_url">url</a>: Url) {
    sui::url::update(&<b>mut</b> self.<a href="url.md#0x2_url">url</a>, inner_url(&<a href="url.md#0x2_url">url</a>))
}
</code></pre>



</details>
