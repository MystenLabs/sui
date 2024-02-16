
<a name="0xb_crypto"></a>

# Module `0xb::crypto`



-  [Function `ecdsa_pub_key_to_eth_address`](#0xb_crypto_ecdsa_pub_key_to_eth_address)


<pre><code><b>use</b> <a href="dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1">0x2::ecdsa_k1</a>;
<b>use</b> <a href="dependencies/sui-framework/hash.md#0x2_hash">0x2::hash</a>;
</code></pre>



<a name="0xb_crypto_ecdsa_pub_key_to_eth_address"></a>

## Function `ecdsa_pub_key_to_eth_address`



<pre><code><b>public</b> <b>fun</b> <a href="crypto.md#0xb_crypto_ecdsa_pub_key_to_eth_address">ecdsa_pub_key_to_eth_address</a>(compressed_pub_key: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="crypto.md#0xb_crypto_ecdsa_pub_key_to_eth_address">ecdsa_pub_key_to_eth_address</a>(compressed_pub_key: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; {
    // Decompress pub key
    <b>let</b> decompressed = <a href="dependencies/sui-framework/ecdsa_k1.md#0x2_ecdsa_k1_decompress_pubkey">ecdsa_k1::decompress_pubkey</a>(&compressed_pub_key);

    // Remove first byte
    <b>let</b> (i, decompressed_64) = (1, <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>[]);
    <b>while</b> (i &lt; 65) {
        <b>let</b> value = <a href="dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&decompressed, i);
        <a href="dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> decompressed_64, *value);
        i = i + 1;
    };

    // Hash
    <b>let</b> <a href="dependencies/sui-framework/hash.md#0x2_hash">hash</a> = keccak256(&decompressed_64);

    // Take last 20 bytes
    <b>let</b> <b>address</b> = <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>let</b> i = 12;
    <b>while</b> (i &lt; 32) {
        <a href="dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(&<b>mut</b> <b>address</b>, *<a href="dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&<a href="dependencies/sui-framework/hash.md#0x2_hash">hash</a>, i));
        i = i + 1;
    };
    <b>address</b>
}
</code></pre>



</details>
