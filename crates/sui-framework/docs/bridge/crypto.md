---
title: Module `bridge::crypto`
---



-  [Function `ecdsa_pub_key_to_eth_address`](#bridge_crypto_ecdsa_pub_key_to_eth_address)


<pre><code><b>use</b> <a href="../sui/ecdsa_k1.md#sui_ecdsa_k1">sui::ecdsa_k1</a>;
<b>use</b> <a href="../sui/hash.md#sui_hash">sui::hash</a>;
</code></pre>



<a name="bridge_crypto_ecdsa_pub_key_to_eth_address"></a>

## Function `ecdsa_pub_key_to_eth_address`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../bridge/crypto.md#bridge_crypto_ecdsa_pub_key_to_eth_address">ecdsa_pub_key_to_eth_address</a>(compressed_pub_key: &vector&lt;u8&gt;): vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../bridge/crypto.md#bridge_crypto_ecdsa_pub_key_to_eth_address">ecdsa_pub_key_to_eth_address</a>(compressed_pub_key: &vector&lt;u8&gt;): vector&lt;u8&gt; {
    // Decompress pub key
    <b>let</b> decompressed = ecdsa_k1::decompress_pubkey(compressed_pub_key);
    // Skip the first byte
    <b>let</b> (<b>mut</b> i, <b>mut</b> decompressed_64) = (1, vector[]);
    <b>while</b> (i &lt; 65) {
        decompressed_64.push_back(decompressed[i]);
        i = i + 1;
    };
    // Hash
    <b>let</b> hash = keccak256(&decompressed_64);
    // Take last 20 bytes
    <b>let</b> <b>mut</b> <b>address</b> = vector[];
    <b>let</b> <b>mut</b> i = 12;
    <b>while</b> (i &lt; 32) {
        <b>address</b>.push_back(hash[i]);
        i = i + 1;
    };
    <b>address</b>
}
</code></pre>



</details>
