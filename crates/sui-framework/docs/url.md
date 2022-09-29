
<a name="0x2_url"></a>

# Module `0x2::url`

URL: standard Uniform Resource Locator string
Url: Sui type which wraps a URL
UrlCommitment: Sui type which wraps a Url but also includes an immutable commitment
to the hash of the resource at the given URL


-  [Struct `Url`](#0x2_url_Url)
-  [Struct `UrlCommitment`](#0x2_url_UrlCommitment)
-  [Constants](#@Constants_0)
-  [Function `new_unsafe`](#0x2_url_new_unsafe)
-  [Function `new_unsafe_from_bytes`](#0x2_url_new_unsafe_from_bytes)
-  [Function `new_unsafe_url_commitment`](#0x2_url_new_unsafe_url_commitment)
-  [Function `inner_url`](#0x2_url_inner_url)
-  [Function `update`](#0x2_url_update)
-  [Function `url_commitment_resource_hash`](#0x2_url_url_commitment_resource_hash)
-  [Function `url_commitment_inner_url`](#0x2_url_url_commitment_inner_url)
-  [Function `url_commitment_update`](#0x2_url_url_commitment_update)


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
</code></pre>



<a name="0x2_url_Url"></a>

## Struct `Url`

Represents an arbitrary URL. Clients rendering values of this type should fetch the resource at <code><a href="url.md#0x2_url">url</a></code> and render it using a to-be-defined Sui standard.


<pre><code><b>struct</b> <a href="url.md#0x2_url_Url">Url</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="url.md#0x2_url">url</a>: <a href="_String">ascii::String</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_url_UrlCommitment"></a>

## Struct `UrlCommitment`

Represents an arbitrary URL plus an immutable commitment to the underlying
resource hash. Clients rendering values of this type should fetch the resource at <code><a href="url.md#0x2_url">url</a></code>, and then compare it against <code>resource_hash</code> using a to-be-defined Sui standard, and (if the two match) render the value using the <code><a href="url.md#0x2_url_Url">Url</a></code> standard.


<pre><code><b>struct</b> <a href="url.md#0x2_url_UrlCommitment">UrlCommitment</a> <b>has</b> <b>copy</b>, drop, store
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
<code>resource_hash: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_url_EHashLengthMismatch"></a>

Error code when the length of the hash vector is not HASH_VECTOR_LENGTH


<pre><code><b>const</b> <a href="url.md#0x2_url_EHashLengthMismatch">EHashLengthMismatch</a>: u64 = 0;
</code></pre>



<a name="0x2_url_HASH_VECTOR_LENGTH"></a>

Length of the vector<u8> representing a resource hash


<pre><code><b>const</b> <a href="url.md#0x2_url_HASH_VECTOR_LENGTH">HASH_VECTOR_LENGTH</a>: u64 = 32;
</code></pre>



<a name="0x2_url_new_unsafe"></a>

## Function `new_unsafe`

Create a <code><a href="url.md#0x2_url_Url">Url</a></code>, with no validation


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_unsafe">new_unsafe</a>(<a href="url.md#0x2_url">url</a>: <a href="_String">ascii::String</a>): <a href="url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_unsafe">new_unsafe</a>(<a href="url.md#0x2_url">url</a>: String): <a href="url.md#0x2_url_Url">Url</a> {
    <a href="url.md#0x2_url_Url">Url</a> { <a href="url.md#0x2_url">url</a> }
}
</code></pre>



</details>

<a name="0x2_url_new_unsafe_from_bytes"></a>

## Function `new_unsafe_from_bytes`

Create a <code><a href="url.md#0x2_url_Url">Url</a></code> with no validation from bytes
Note: this will abort if <code>bytes</code> is not valid ASCII


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_unsafe_from_bytes">new_unsafe_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_unsafe_from_bytes">new_unsafe_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="url.md#0x2_url_Url">Url</a> {
    <b>let</b> <a href="url.md#0x2_url">url</a> = <a href="_string">ascii::string</a>(bytes);
    <a href="url.md#0x2_url_Url">Url</a> { <a href="url.md#0x2_url">url</a> }
}
</code></pre>



</details>

<a name="0x2_url_new_unsafe_url_commitment"></a>

## Function `new_unsafe_url_commitment`

Create a <code><a href="url.md#0x2_url_UrlCommitment">UrlCommitment</a></code>, and set the immutable hash


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_unsafe_url_commitment">new_unsafe_url_commitment</a>(<a href="url.md#0x2_url">url</a>: <a href="url.md#0x2_url_Url">url::Url</a>, resource_hash: <a href="">vector</a>&lt;u8&gt;): <a href="url.md#0x2_url_UrlCommitment">url::UrlCommitment</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_unsafe_url_commitment">new_unsafe_url_commitment</a>(<a href="url.md#0x2_url">url</a>: <a href="url.md#0x2_url_Url">Url</a>, resource_hash: <a href="">vector</a>&lt;u8&gt;): <a href="url.md#0x2_url_UrlCommitment">UrlCommitment</a> {
    // Length must be exact
    <b>assert</b>!(<a href="_length">vector::length</a>(&resource_hash) == <a href="url.md#0x2_url_HASH_VECTOR_LENGTH">HASH_VECTOR_LENGTH</a>, <a href="url.md#0x2_url_EHashLengthMismatch">EHashLengthMismatch</a>);

    <a href="url.md#0x2_url_UrlCommitment">UrlCommitment</a> { <a href="url.md#0x2_url">url</a>, resource_hash }
}
</code></pre>



</details>

<a name="0x2_url_inner_url"></a>

## Function `inner_url`

Get inner URL


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_inner_url">inner_url</a>(self: &<a href="url.md#0x2_url_Url">url::Url</a>): <a href="_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_inner_url">inner_url</a>(self: &<a href="url.md#0x2_url_Url">Url</a>): String{
    self.<a href="url.md#0x2_url">url</a>
}
</code></pre>



</details>

<a name="0x2_url_update"></a>

## Function `update`

Update the inner URL


<pre><code><b>public</b> <b>fun</b> <b>update</b>(self: &<b>mut</b> <a href="url.md#0x2_url_Url">url::Url</a>, <a href="url.md#0x2_url">url</a>: <a href="_String">ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <b>update</b>(self: &<b>mut</b> <a href="url.md#0x2_url_Url">Url</a>, <a href="url.md#0x2_url">url</a>: String) {
    self.<a href="url.md#0x2_url">url</a> = <a href="url.md#0x2_url">url</a>;
}
</code></pre>



</details>

<a name="0x2_url_url_commitment_resource_hash"></a>

## Function `url_commitment_resource_hash`

Get the hash of the resource at the URL
We enforce that the hash is immutable


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_url_commitment_resource_hash">url_commitment_resource_hash</a>(self: &<a href="url.md#0x2_url_UrlCommitment">url::UrlCommitment</a>): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_url_commitment_resource_hash">url_commitment_resource_hash</a>(self: &<a href="url.md#0x2_url_UrlCommitment">UrlCommitment</a>): <a href="">vector</a>&lt;u8&gt; {
    self.resource_hash
}
</code></pre>



</details>

<a name="0x2_url_url_commitment_inner_url"></a>

## Function `url_commitment_inner_url`

Get inner URL


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_url_commitment_inner_url">url_commitment_inner_url</a>(self: &<a href="url.md#0x2_url_UrlCommitment">url::UrlCommitment</a>): <a href="_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_url_commitment_inner_url">url_commitment_inner_url</a>(self: &<a href="url.md#0x2_url_UrlCommitment">UrlCommitment</a>): String{
    self.<a href="url.md#0x2_url">url</a>.<a href="url.md#0x2_url">url</a>
}
</code></pre>



</details>

<a name="0x2_url_url_commitment_update"></a>

## Function `url_commitment_update`

Update the URL, but the hash of the object at the URL must never change


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_url_commitment_update">url_commitment_update</a>(self: &<b>mut</b> <a href="url.md#0x2_url_UrlCommitment">url::UrlCommitment</a>, <a href="url.md#0x2_url">url</a>: <a href="_String">ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_url_commitment_update">url_commitment_update</a>(self: &<b>mut</b> <a href="url.md#0x2_url_UrlCommitment">UrlCommitment</a>, <a href="url.md#0x2_url">url</a>: String) {
    <b>update</b>(&<b>mut</b> self.<a href="url.md#0x2_url">url</a>, <a href="url.md#0x2_url">url</a>)
}
</code></pre>



</details>
