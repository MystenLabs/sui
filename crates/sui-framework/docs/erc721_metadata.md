
<a name="0x2_erc721_metadata"></a>

# Module `0x2::erc721_metadata`



-  [Struct `ERC721Metadata`](#0x2_erc721_metadata_ERC721Metadata)
-  [Struct `TokenID`](#0x2_erc721_metadata_TokenID)
-  [Function `new`](#0x2_erc721_metadata_new)
-  [Function `new_token_id`](#0x2_erc721_metadata_new_token_id)
-  [Function `token_id`](#0x2_erc721_metadata_token_id)
-  [Function `token_uri`](#0x2_erc721_metadata_token_uri)
-  [Function `name`](#0x2_erc721_metadata_name)


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
<b>use</b> <a href="">0x1::string</a>;
<b>use</b> <a href="url.md#0x2_url">0x2::url</a>;
</code></pre>



<a name="0x2_erc721_metadata_ERC721Metadata"></a>

## Struct `ERC721Metadata`

A wrapper type for the ERC721 metadata standard https://eips.ethereum.org/EIPS/eip-721


<pre><code><b>struct</b> <a href="erc721_metadata.md#0x2_erc721_metadata_ERC721Metadata">ERC721Metadata</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>token_id: <a href="erc721_metadata.md#0x2_erc721_metadata_TokenID">erc721_metadata::TokenID</a></code>
</dt>
<dd>
 The token id associated with the source contract on Ethereum
</dd>
<dt>
<code>name: <a href="_String">string::String</a></code>
</dt>
<dd>
 A descriptive name for a collection of NFTs in this contract.
 This corresponds to the <code><a href="erc721_metadata.md#0x2_erc721_metadata_name">name</a>()</code> method in the
 ERC721Metadata interface in EIP-721.
</dd>
<dt>
<code>token_uri: <a href="url.md#0x2_url_Url">url::Url</a></code>
</dt>
<dd>
 A distinct Uniform Resource Identifier (URI) for a given asset.
 This corresponds to the <code>tokenURI()</code> method in the ERC721Metadata
 interface in EIP-721.
</dd>
</dl>


</details>

<a name="0x2_erc721_metadata_TokenID"></a>

## Struct `TokenID`

An ERC721 token ID


<pre><code><b>struct</b> <a href="erc721_metadata.md#0x2_erc721_metadata_TokenID">TokenID</a> <b>has</b> <b>copy</b>, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_erc721_metadata_new"></a>

## Function `new`

Construct a new ERC721Metadata from the given inputs. Does not perform any validation
on <code>token_uri</code> or <code>name</code>


<pre><code><b>public</b> <b>fun</b> <a href="erc721_metadata.md#0x2_erc721_metadata_new">new</a>(token_id: <a href="erc721_metadata.md#0x2_erc721_metadata_TokenID">erc721_metadata::TokenID</a>, name: <a href="">vector</a>&lt;u8&gt;, token_uri: <a href="">vector</a>&lt;u8&gt;): <a href="erc721_metadata.md#0x2_erc721_metadata_ERC721Metadata">erc721_metadata::ERC721Metadata</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="erc721_metadata.md#0x2_erc721_metadata_new">new</a>(token_id: <a href="erc721_metadata.md#0x2_erc721_metadata_TokenID">TokenID</a>, name: <a href="">vector</a>&lt;u8&gt;, token_uri: <a href="">vector</a>&lt;u8&gt;): <a href="erc721_metadata.md#0x2_erc721_metadata_ERC721Metadata">ERC721Metadata</a> {
    // Note: this will <b>abort</b> <b>if</b> `token_uri` is not valid ASCII
    <b>let</b> uri_str = <a href="_string">ascii::string</a>(token_uri);
    <a href="erc721_metadata.md#0x2_erc721_metadata_ERC721Metadata">ERC721Metadata</a> {
        token_id,
        name: <a href="_utf8">string::utf8</a>(name),
        token_uri: <a href="url.md#0x2_url_new_unsafe">url::new_unsafe</a>(uri_str),
    }
}
</code></pre>



</details>

<a name="0x2_erc721_metadata_new_token_id"></a>

## Function `new_token_id`



<pre><code><b>public</b> <b>fun</b> <a href="erc721_metadata.md#0x2_erc721_metadata_new_token_id">new_token_id</a>(id: u64): <a href="erc721_metadata.md#0x2_erc721_metadata_TokenID">erc721_metadata::TokenID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="erc721_metadata.md#0x2_erc721_metadata_new_token_id">new_token_id</a>(id: u64): <a href="erc721_metadata.md#0x2_erc721_metadata_TokenID">TokenID</a> {
    <a href="erc721_metadata.md#0x2_erc721_metadata_TokenID">TokenID</a> { id }
}
</code></pre>



</details>

<a name="0x2_erc721_metadata_token_id"></a>

## Function `token_id`



<pre><code><b>public</b> <b>fun</b> <a href="erc721_metadata.md#0x2_erc721_metadata_token_id">token_id</a>(self: &<a href="erc721_metadata.md#0x2_erc721_metadata_ERC721Metadata">erc721_metadata::ERC721Metadata</a>): &<a href="erc721_metadata.md#0x2_erc721_metadata_TokenID">erc721_metadata::TokenID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="erc721_metadata.md#0x2_erc721_metadata_token_id">token_id</a>(self: &<a href="erc721_metadata.md#0x2_erc721_metadata_ERC721Metadata">ERC721Metadata</a>): &<a href="erc721_metadata.md#0x2_erc721_metadata_TokenID">TokenID</a> {
    &self.token_id
}
</code></pre>



</details>

<a name="0x2_erc721_metadata_token_uri"></a>

## Function `token_uri`



<pre><code><b>public</b> <b>fun</b> <a href="erc721_metadata.md#0x2_erc721_metadata_token_uri">token_uri</a>(self: &<a href="erc721_metadata.md#0x2_erc721_metadata_ERC721Metadata">erc721_metadata::ERC721Metadata</a>): &<a href="url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="erc721_metadata.md#0x2_erc721_metadata_token_uri">token_uri</a>(self: &<a href="erc721_metadata.md#0x2_erc721_metadata_ERC721Metadata">ERC721Metadata</a>): &Url {
    &self.token_uri
}
</code></pre>



</details>

<a name="0x2_erc721_metadata_name"></a>

## Function `name`



<pre><code><b>public</b> <b>fun</b> <a href="erc721_metadata.md#0x2_erc721_metadata_name">name</a>(self: &<a href="erc721_metadata.md#0x2_erc721_metadata_ERC721Metadata">erc721_metadata::ERC721Metadata</a>): &<a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="erc721_metadata.md#0x2_erc721_metadata_name">name</a>(self: &<a href="erc721_metadata.md#0x2_erc721_metadata_ERC721Metadata">ERC721Metadata</a>): &<a href="_String">string::String</a> {
    &self.name
}
</code></pre>



</details>
