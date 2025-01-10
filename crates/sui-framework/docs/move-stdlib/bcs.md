---
title: Module `0x1::bcs`
---

Utility for converting a Move value to its binary representation in BCS (Binary Canonical
Serialization). BCS is the binary encoding for Move resources and other non-module values
published on-chain. See https://github.com/diem/bcs#binary-canonical-serialization-bcs for more
details on BCS.


-  [Function `to_bytes`](#0x1_bcs_to_bytes)


<pre><code></code></pre>



<a name="0x1_bcs_to_bytes"></a>

## Function `to_bytes`

Return the binary representation of <code>v</code> in BCS (Binary Canonical Serialization) format


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/bcs.md#0x1_bcs_to_bytes">to_bytes</a>&lt;MoveValue&gt;(v: &MoveValue): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../move-stdlib/bcs.md#0x1_bcs_to_bytes">to_bytes</a>&lt;MoveValue&gt;(v: &MoveValue): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;;
</code></pre>



</details>
