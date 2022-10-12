
<a name="0x2_hash"></a>

# Module `0x2::hash`



-  [Function `keccak256`](#0x2_hash_keccak256)


<pre><code></code></pre>



<a name="0x2_hash_keccak256"></a>

## Function `keccak256`

@param data: arbitrary bytes data to hash
Hash the input bytes using keccak256 and returns 32 bytes.


<pre><code><b>public</b> <b>fun</b> <a href="hash.md#0x2_hash_keccak256">keccak256</a>(data: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="hash.md#0x2_hash_keccak256">keccak256</a>(data: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>
