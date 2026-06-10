---
title: Module `sui::ecdsa_p384`
---



-  [Constants](#@Constants_0)
-  [Function `secp384r1_verify`](#sui_ecdsa_p384_secp384r1_verify)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="sui_ecdsa_p384_SHA256"></a>

Hash function flag for SHA-256, used by <code><a href="../sui/ecdsa_p384.md#sui_ecdsa_p384_secp384r1_verify">secp384r1_verify</a></code>.


<pre><code><b>const</b> <a href="../sui/ecdsa_p384.md#sui_ecdsa_p384_SHA256">SHA256</a>: u8 = 0;
</code></pre>



<a name="sui_ecdsa_p384_SHA384"></a>

Hash function flag for SHA-384, used by <code><a href="../sui/ecdsa_p384.md#sui_ecdsa_p384_secp384r1_verify">secp384r1_verify</a></code>.


<pre><code><b>const</b> <a href="../sui/ecdsa_p384.md#sui_ecdsa_p384_SHA384">SHA384</a>: u8 = 1;
</code></pre>



<a name="sui_ecdsa_p384_secp384r1_verify"></a>

## Function `secp384r1_verify`

@param signature: A 96-byte signature in the form <code>(r, s)</code> produced with Secp384r1 /
NIST P-384. This is the fixed-size encoding, not ASN.1/DER.
@param public_key: The SEC1-encoded public key to verify the signature against
(33-byte prefix <code>02</code>/<code>03</code> compressed, or 65-byte prefix <code>04</code> uncompressed).
@param msg: The raw message the signature is signed against (hashed internally).
@param hash: The hash function flag used when signing: 0 = SHA-256, 1 = SHA-384.

Verifies a NIST P-384 ECDSA signature. This accepts standard ECDSA signatures, including
high-S signatures, for X.509 / WebAuthn / Apple App Attest compatibility. Because the
signature encoding is malleable, callers that need a unique signature identifier must
canonicalize the signature before using its bytes as a nullifier or map key.

If the signature is valid for the public key and hashed message, returns true. Else false.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/ecdsa_p384.md#sui_ecdsa_p384_secp384r1_verify">secp384r1_verify</a>(signature: &vector&lt;u8&gt;, public_key: &vector&lt;u8&gt;, msg: &vector&lt;u8&gt;, <a href="../sui/hash.md#sui_hash">hash</a>: u8): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../sui/ecdsa_p384.md#sui_ecdsa_p384_secp384r1_verify">secp384r1_verify</a>(
    signature: &vector&lt;u8&gt;,
    public_key: &vector&lt;u8&gt;,
    msg: &vector&lt;u8&gt;,
    <a href="../sui/hash.md#sui_hash">hash</a>: u8,
): bool;
</code></pre>



</details>
