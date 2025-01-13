---
title: Module `0x2::nitro_attestation`
---



-  [Function `verify_nitro_attestation_internal`](#0x2_nitro_attestation_verify_nitro_attestation_internal)
-  [Function `verify_nitro_attestation`](#0x2_nitro_attestation_verify_nitro_attestation)


<pre><code><b>use</b> <a href="clock.md#0x2_clock">0x2::clock</a>;
</code></pre>



<a name="0x2_nitro_attestation_verify_nitro_attestation_internal"></a>

## Function `verify_nitro_attestation_internal`

Internal native function


<pre><code><b>fun</b> <a href="nitro_attestation.md#0x2_nitro_attestation_verify_nitro_attestation_internal">verify_nitro_attestation_internal</a>(attestation: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, current_timestamp: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="nitro_attestation.md#0x2_nitro_attestation_verify_nitro_attestation_internal">verify_nitro_attestation_internal</a>(
    attestation: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    current_timestamp: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;;
</code></pre>



</details>

<a name="0x2_nitro_attestation_verify_nitro_attestation"></a>

## Function `verify_nitro_attestation`

@param attestation: attesttaion documents bytes data.
@param clock: the clock object.

Returns parsed pcrs after verifying the attestation.


<pre><code><b>public</b> <b>fun</b> <a href="nitro_attestation.md#0x2_nitro_attestation_verify_nitro_attestation">verify_nitro_attestation</a>(attestation: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, <a href="clock.md#0x2_clock">clock</a>: &<a href="clock.md#0x2_clock_Clock">clock::Clock</a>): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="nitro_attestation.md#0x2_nitro_attestation_verify_nitro_attestation">verify_nitro_attestation</a>(
    attestation: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    <a href="clock.md#0x2_clock">clock</a>: &Clock
): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;&gt; {
    <a href="nitro_attestation.md#0x2_nitro_attestation_verify_nitro_attestation_internal">verify_nitro_attestation_internal</a>(attestation, <a href="clock.md#0x2_clock_timestamp_ms">clock::timestamp_ms</a>(<a href="clock.md#0x2_clock">clock</a>))
}
</code></pre>



</details>
