
<a name="0x2_devnet_nft"></a>

# Module `0x2::devnet_nft`

A minimalist example to demonstrate how to create an NFT like object
on Sui. The user should be able to use the wallet command line tool
(https://docs.sui.io/build/wallet) to mint an NFT. For example,
<code>wallet example-nft --<a href="devnet_nft.md#0x2_devnet_nft_name">name</a> &lt;Name&gt; --<a href="devnet_nft.md#0x2_devnet_nft_description">description</a> &lt;Description&gt; --<a href="url.md#0x2_url">url</a> &lt;URL&gt;</code>
MUSTFIX: Remove this module from framework.


-  [Resource `DevNetNFT`](#0x2_devnet_nft_DevNetNFT)
-  [Struct `MintNFTEvent`](#0x2_devnet_nft_MintNFTEvent)
-  [Function `mint`](#0x2_devnet_nft_mint)
-  [Function `update_description`](#0x2_devnet_nft_update_description)
-  [Function `burn`](#0x2_devnet_nft_burn)
-  [Function `name`](#0x2_devnet_nft_name)
-  [Function `description`](#0x2_devnet_nft_description)
-  [Function `url`](#0x2_devnet_nft_url)


<pre><code><b>use</b> <a href="">0x1::string</a>;
<b>use</b> <a href="event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="url.md#0x2_url">0x2::url</a>;
</code></pre>



<a name="0x2_devnet_nft_DevNetNFT"></a>

## Resource `DevNetNFT`

An example NFT that can be minted by anybody


<pre><code><b>struct</b> <a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">DevNetNFT</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>name: <a href="_String">string::String</a></code>
</dt>
<dd>
 Name for the token
</dd>
<dt>
<code>description: <a href="_String">string::String</a></code>
</dt>
<dd>
 Description of the token
</dd>
<dt>
<code><a href="url.md#0x2_url">url</a>: <a href="url.md#0x2_url_Url">url::Url</a></code>
</dt>
<dd>
 URL for the token
</dd>
</dl>


</details>

<a name="0x2_devnet_nft_MintNFTEvent"></a>

## Struct `MintNFTEvent`



<pre><code><b>struct</b> <a href="devnet_nft.md#0x2_devnet_nft_MintNFTEvent">MintNFTEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>object_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>creator: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>name: <a href="_String">string::String</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_devnet_nft_mint"></a>

## Function `mint`

Create a new devnet_nft


<pre><code><b>public</b> entry <b>fun</b> <a href="devnet_nft.md#0x2_devnet_nft_mint">mint</a>(name: <a href="">vector</a>&lt;u8&gt;, description: <a href="">vector</a>&lt;u8&gt;, <a href="url.md#0x2_url">url</a>: <a href="">vector</a>&lt;u8&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="devnet_nft.md#0x2_devnet_nft_mint">mint</a>(
    name: <a href="">vector</a>&lt;u8&gt;,
    description: <a href="">vector</a>&lt;u8&gt;,
    <a href="url.md#0x2_url">url</a>: <a href="">vector</a>&lt;u8&gt;,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> nft = <a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">DevNetNFT</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        name: <a href="_utf8">string::utf8</a>(name),
        description: <a href="_utf8">string::utf8</a>(description),
        <a href="url.md#0x2_url">url</a>: <a href="url.md#0x2_url_new_unsafe_from_bytes">url::new_unsafe_from_bytes</a>(<a href="url.md#0x2_url">url</a>)
    };
    <b>let</b> sender = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="devnet_nft.md#0x2_devnet_nft_MintNFTEvent">MintNFTEvent</a> {
        object_id: <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&nft.id),
        creator: sender,
        name: nft.name,
    });
    <a href="transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(nft, sender);
}
</code></pre>



</details>

<a name="0x2_devnet_nft_update_description"></a>

## Function `update_description`

Update the <code>description</code> of <code>nft</code> to <code>new_description</code>


<pre><code><b>public</b> entry <b>fun</b> <a href="devnet_nft.md#0x2_devnet_nft_update_description">update_description</a>(nft: &<b>mut</b> <a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">devnet_nft::DevNetNFT</a>, new_description: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="devnet_nft.md#0x2_devnet_nft_update_description">update_description</a>(
    nft: &<b>mut</b> <a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">DevNetNFT</a>,
    new_description: <a href="">vector</a>&lt;u8&gt;,
) {
    nft.description = <a href="_utf8">string::utf8</a>(new_description)
}
</code></pre>



</details>

<a name="0x2_devnet_nft_burn"></a>

## Function `burn`

Permanently delete <code>nft</code>


<pre><code><b>public</b> entry <b>fun</b> <a href="devnet_nft.md#0x2_devnet_nft_burn">burn</a>(nft: <a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">devnet_nft::DevNetNFT</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="devnet_nft.md#0x2_devnet_nft_burn">burn</a>(nft: <a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">DevNetNFT</a>) {
    <b>let</b> <a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">DevNetNFT</a> { id, name: _, description: _, <a href="url.md#0x2_url">url</a>: _ } = nft;
    <a href="object.md#0x2_object_delete">object::delete</a>(id)
}
</code></pre>



</details>

<a name="0x2_devnet_nft_name"></a>

## Function `name`

Get the NFT's <code>name</code>


<pre><code><b>public</b> <b>fun</b> <a href="devnet_nft.md#0x2_devnet_nft_name">name</a>(nft: &<a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">devnet_nft::DevNetNFT</a>): &<a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="devnet_nft.md#0x2_devnet_nft_name">name</a>(nft: &<a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">DevNetNFT</a>): &<a href="_String">string::String</a> {
    &nft.name
}
</code></pre>



</details>

<a name="0x2_devnet_nft_description"></a>

## Function `description`

Get the NFT's <code>description</code>


<pre><code><b>public</b> <b>fun</b> <a href="devnet_nft.md#0x2_devnet_nft_description">description</a>(nft: &<a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">devnet_nft::DevNetNFT</a>): &<a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="devnet_nft.md#0x2_devnet_nft_description">description</a>(nft: &<a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">DevNetNFT</a>): &<a href="_String">string::String</a> {
    &nft.description
}
</code></pre>



</details>

<a name="0x2_devnet_nft_url"></a>

## Function `url`

Get the NFT's <code><a href="url.md#0x2_url">url</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url">url</a>(nft: &<a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">devnet_nft::DevNetNFT</a>): &<a href="url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url">url</a>(nft: &<a href="devnet_nft.md#0x2_devnet_nft_DevNetNFT">DevNetNFT</a>): &Url {
    &nft.<a href="url.md#0x2_url">url</a>
}
</code></pre>



</details>
