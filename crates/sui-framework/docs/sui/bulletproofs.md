---
title: Module `sui::bulletproofs`
---

ZK Range proofs (bulletproofs)


-  [Function `verify_range_proof`](#sui_bulletproofs_verify_range_proof)
-  [Function `verify_bulletproof_ristretto255`](#sui_bulletproofs_verify_bulletproof_ristretto255)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/debug.md#std_debug">std::debug</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/group_ops.md#sui_group_ops">sui::group_ops</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/ristretto255.md#sui_ristretto255">sui::ristretto255</a>;
</code></pre>



<a name="sui_bulletproofs_verify_range_proof"></a>

## Function `verify_range_proof`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/bulletproofs.md#sui_bulletproofs_verify_range_proof">verify_range_proof</a>(proof: &vector&lt;u8&gt;, range: u8, commitment: &<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_Point">sui::ristretto255::Point</a>&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/bulletproofs.md#sui_bulletproofs_verify_range_proof">verify_range_proof</a>(proof: &vector&lt;u8&gt;, range: u8, commitment: &Element&lt;Point&gt;): bool {
    <a href="../sui/bulletproofs.md#sui_bulletproofs_verify_bulletproof_ristretto255">verify_bulletproof_ristretto255</a>(proof, range, commitment.bytes())
}
</code></pre>



</details>

<a name="sui_bulletproofs_verify_bulletproof_ristretto255"></a>

## Function `verify_bulletproof_ristretto255`



<pre><code><b>fun</b> <a href="../sui/bulletproofs.md#sui_bulletproofs_verify_bulletproof_ristretto255">verify_bulletproof_ristretto255</a>(proof: &vector&lt;u8&gt;, range: u8, commitment: &vector&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/bulletproofs.md#sui_bulletproofs_verify_bulletproof_ristretto255">verify_bulletproof_ristretto255</a>(proof: &vector&lt;u8&gt;, range: u8, commitment: &vector&lt;u8&gt;): bool;
</code></pre>



</details>
