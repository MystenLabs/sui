---
title: Module `sui::rangeproofs`
---



-  [Constants](#@Constants_0)
-  [Function `verify_bulletproofs_with_dst_ristretto255`](#sui_rangeproofs_verify_bulletproofs_with_dst_ristretto255)
-  [Function `verify_bulletproofs_ristretto255`](#sui_rangeproofs_verify_bulletproofs_ristretto255)
-  [Function `verify_bulletproofs_with_dst_ristretto255_internal`](#sui_rangeproofs_verify_bulletproofs_with_dst_ristretto255_internal)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/group_ops.md#sui_group_ops">sui::group_ops</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/ristretto255.md#sui_ristretto255">sui::ristretto255</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="sui_rangeproofs_ENotSupported"></a>



<pre><code><b>const</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_ENotSupported">ENotSupported</a>: u64 = 0;
</code></pre>



<a name="sui_rangeproofs_EInvalidProof"></a>



<pre><code><b>const</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_EInvalidProof">EInvalidProof</a>: u64 = 1;
</code></pre>



<a name="sui_rangeproofs_EInvalidRange"></a>



<pre><code><b>const</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_EInvalidRange">EInvalidRange</a>: u64 = 2;
</code></pre>



<a name="sui_rangeproofs_EInvalidBatchSize"></a>



<pre><code><b>const</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_EInvalidBatchSize">EInvalidBatchSize</a>: u64 = 3;
</code></pre>



<a name="sui_rangeproofs_EUnsupportedVersion"></a>



<pre><code><b>const</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_EUnsupportedVersion">EUnsupportedVersion</a>: u64 = 4;
</code></pre>



<a name="sui_rangeproofs_EInvalidDst"></a>



<pre><code><b>const</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_EInvalidDst">EInvalidDst</a>: u64 = 5;
</code></pre>



<a name="sui_rangeproofs_MAX_DST_LENGTH"></a>



<pre><code><b>const</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_MAX_DST_LENGTH">MAX_DST_LENGTH</a>: u64 = 64;
</code></pre>



<a name="sui_rangeproofs_verify_bulletproofs_with_dst_ristretto255"></a>

## Function `verify_bulletproofs_with_dst_ristretto255`

Verify a range proof over the Ristretto255 curve that all committed values are in the range [0, 2^bits).
Currently, the only supported version is 0 which corresponds to the original Bulletproofs construction (https://eprint.iacr.org/2017/1066.pdf).
In the future, we may add support for newer versions of Bulletproofs, such as Bulletproofs+ or Bulletproofs++.

The format of the proof follows the specifications from https://github.com/dalek-cryptography/bulletproofs/blob/be67b6d5f5ad1c1f54d5511b52e6d645a1313d07/src/range_proof/mod.rs#L59-L76.

The <code>bits</code> parameter is the bit length of the range and must be one of 8, 16, 32, or 64.

The <code>commitments</code> are Pedersen commitments to the values used in the proof.
The number of commitments must be a power of two, but if needed, the input to the prover can be padded with trivial commitments to zero.
The number of commitments times <code>bits</code> can be at most 512.

The <code>dst</code> is a domain separation tag that is bound into the proof transcript. Provers and
verifiers must agree on the same <code>dst</code> for verification to succeed. It can be at most 64 bytes.

Enabled only on devnet and testnet.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_verify_bulletproofs_with_dst_ristretto255">verify_bulletproofs_with_dst_ristretto255</a>(proof: &vector&lt;u8&gt;, bits: u8, commitments: &vector&lt;<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_G">sui::ristretto255::G</a>&gt;&gt;, dst: &vector&lt;u8&gt;, version: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_verify_bulletproofs_with_dst_ristretto255">verify_bulletproofs_with_dst_ristretto255</a>(
    proof: &vector&lt;u8&gt;,
    bits: u8,
    commitments: &vector&lt;Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_G">ristretto255::G</a>&gt;&gt;,
    dst: &vector&lt;u8&gt;,
    version: u8,
): bool {
    <b>assert</b>!(dst.length() &lt;= <a href="../sui/rangeproofs.md#sui_rangeproofs_MAX_DST_LENGTH">MAX_DST_LENGTH</a>, <a href="../sui/rangeproofs.md#sui_rangeproofs_EInvalidDst">EInvalidDst</a>);
    match (version) {
        0 =&gt; <a href="../sui/rangeproofs.md#sui_rangeproofs_verify_bulletproofs_with_dst_ristretto255_internal">verify_bulletproofs_with_dst_ristretto255_internal</a>(
            proof,
            bits,
            &commitments.map_ref!(|c| *c.bytes()),
            dst,
        ),
        _ =&gt; <b>abort</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_EUnsupportedVersion">EUnsupportedVersion</a>,
    }
}
</code></pre>



</details>

<a name="sui_rangeproofs_verify_bulletproofs_ristretto255"></a>

## Function `verify_bulletproofs_ristretto255`

Disabled. This entry point always aborts; use <code><a href="../sui/rangeproofs.md#sui_rangeproofs_verify_bulletproofs_with_dst_ristretto255">verify_bulletproofs_with_dst_ristretto255</a></code>
instead.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_verify_bulletproofs_ristretto255">verify_bulletproofs_ristretto255</a>(_proof: &vector&lt;u8&gt;, _bits: u8, _commitments: &vector&lt;<a href="../sui/group_ops.md#sui_group_ops_Element">sui::group_ops::Element</a>&lt;<a href="../sui/ristretto255.md#sui_ristretto255_G">sui::ristretto255::G</a>&gt;&gt;, _version: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_verify_bulletproofs_ristretto255">verify_bulletproofs_ristretto255</a>(
    _proof: &vector&lt;u8&gt;,
    _bits: u8,
    _commitments: &vector&lt;Element&lt;<a href="../sui/ristretto255.md#sui_ristretto255_G">ristretto255::G</a>&gt;&gt;,
    _version: u8,
): bool {
    <b>abort</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_ENotSupported">ENotSupported</a>
}
</code></pre>



</details>

<a name="sui_rangeproofs_verify_bulletproofs_with_dst_ristretto255_internal"></a>

## Function `verify_bulletproofs_with_dst_ristretto255_internal`



<pre><code><b>fun</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_verify_bulletproofs_with_dst_ristretto255_internal">verify_bulletproofs_with_dst_ristretto255_internal</a>(proof: &vector&lt;u8&gt;, bits: u8, commitments: &vector&lt;vector&lt;u8&gt;&gt;, dst: &vector&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/rangeproofs.md#sui_rangeproofs_verify_bulletproofs_with_dst_ristretto255_internal">verify_bulletproofs_with_dst_ristretto255_internal</a>(
    proof: &vector&lt;u8&gt;,
    bits: u8,
    commitments: &vector&lt;vector&lt;u8&gt;&gt;,
    dst: &vector&lt;u8&gt;,
): bool;
</code></pre>



</details>
