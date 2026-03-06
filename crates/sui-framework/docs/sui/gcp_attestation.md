---
title: Module `sui::gcp_attestation`
---



-  [Struct `GcpAttestationDocument`](#sui_gcp_attestation_GcpAttestationDocument)
-  [Constants](#@Constants_0)
-  [Function `verify_gcp_attestation`](#sui_gcp_attestation_verify_gcp_attestation)
-  [Function `iss`](#sui_gcp_attestation_iss)
-  [Function `sub`](#sui_gcp_attestation_sub)
-  [Function `aud`](#sui_gcp_attestation_aud)
-  [Function `exp`](#sui_gcp_attestation_exp)
-  [Function `iat`](#sui_gcp_attestation_iat)
-  [Function `eat_nonce`](#sui_gcp_attestation_eat_nonce)
-  [Function `secboot`](#sui_gcp_attestation_secboot)
-  [Function `hwmodel`](#sui_gcp_attestation_hwmodel)
-  [Function `swname`](#sui_gcp_attestation_swname)
-  [Function `dbgstat`](#sui_gcp_attestation_dbgstat)
-  [Function `swversion`](#sui_gcp_attestation_swversion)
-  [Function `image_digest`](#sui_gcp_attestation_image_digest)
-  [Function `image_reference`](#sui_gcp_attestation_image_reference)
-  [Function `restart_policy`](#sui_gcp_attestation_restart_policy)
-  [Function `verify_gcp_attestation_internal`](#sui_gcp_attestation_verify_gcp_attestation_internal)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/u64.md#std_u64">std::u64</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/authenticator_state.md#sui_authenticator_state">sui::authenticator_state</a>;
<b>use</b> <a href="../sui/clock.md#sui_clock">sui::clock</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_gcp_attestation_GcpAttestationDocument"></a>

## Struct `GcpAttestationDocument`

Verified claims extracted from a GCP Confidential Spaces attestation JWT.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_iss">iss</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
 JWT issuer (always https://confidentialcomputing.googleapis.com).
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_sub">sub</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
 Subject identifier for the workload.
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_aud">aud</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
 Audience claim.
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_exp">exp</a>: u64</code>
</dt>
<dd>
 Expiration time, seconds since Unix epoch.
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_iat">iat</a>: u64</code>
</dt>
<dd>
 Issued-at time, seconds since Unix epoch.
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_eat_nonce">eat_nonce</a>: vector&lt;vector&lt;u8&gt;&gt;</code>
</dt>
<dd>
 EAT nonce values (GCP allows multiple).
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_secboot">secboot</a>: bool</code>
</dt>
<dd>
 Whether secure boot was enabled.
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_hwmodel">hwmodel</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
 Hardware model (e.g., GCP_AMD_SEV).
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_swname">swname</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
 Software name (e.g., CONFIDENTIAL_SPACE).
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_dbgstat">dbgstat</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
 Debug status (e.g., disabled-since-boot).
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_swversion">swversion</a>: vector&lt;vector&lt;u8&gt;&gt;</code>
</dt>
<dd>
 Software version strings.
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_image_digest">image_digest</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
 Container image digest.
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_image_reference">image_reference</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
 Container image reference.
</dd>
<dt>
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_restart_policy">restart_policy</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
 Container restart policy.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_gcp_attestation_ENotSupportedError"></a>

Error that the feature is not available on this network.


<pre><code><b>const</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_ENotSupportedError">ENotSupportedError</a>: u64 = 0;
</code></pre>



<a name="sui_gcp_attestation_EParseError"></a>

Error that the attestation input failed to be parsed.


<pre><code><b>const</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_EParseError">EParseError</a>: u64 = 1;
</code></pre>



<a name="sui_gcp_attestation_EVerifyError"></a>

Error that the attestation failed to be verified.


<pre><code><b>const</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_EVerifyError">EVerifyError</a>: u64 = 2;
</code></pre>



<a name="sui_gcp_attestation_GCP_ISS"></a>

The GCP Confidential Spaces token issuer; used to scope the JWK lookup.


<pre><code><b>const</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_GCP_ISS">GCP_ISS</a>: vector&lt;u8&gt; = vector[104, 116, 116, 112, 115, 58, 47, 47, 99, 111, 110, 102, 105, 100, 101, 110, 116, 105, 97, 108, 99, 111, 109, 112, 117, 116, 105, 110, 103, 46, 103, 111, 111, 103, 108, 101, 97, 112, 105, 115, 46, 99, 111, 109];
</code></pre>



<a name="sui_gcp_attestation_verify_gcp_attestation"></a>

## Function `verify_gcp_attestation`

Verify a GCP Confidential Spaces attestation JWT and return the extracted claims.

The RSA public key is looked up from <code>auth_state</code> using the GCP issuer and the
supplied <code>kid</code>, ensuring the key is consensus-validated rather than caller-controlled.

@param token: The RS256 JWT token bytes (UTF-8 encoded header.payload.signature).
@param auth_state: The on-chain AuthenticatorState containing trusted GCP JWKs.
@param kid: The key ID from the JWT header, identifying which trusted key to use.
@param clock: The clock object used to check token expiry.

Aborts with ENotSupportedError if the feature is disabled,
EParseError if the token cannot be parsed,
EVerifyError if the signature or claims are invalid, or if <code>kid</code> is not found in
<code>auth_state</code> for the GCP issuer.


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_verify_gcp_attestation">verify_gcp_attestation</a>(<a href="../sui/token.md#sui_token">token</a>: vector&lt;u8&gt;, auth_state: &<a href="../sui/authenticator_state.md#sui_authenticator_state_AuthenticatorState">sui::authenticator_state::AuthenticatorState</a>, kid: vector&lt;u8&gt;, <a href="../sui/clock.md#sui_clock">clock</a>: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>): <a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_verify_gcp_attestation">verify_gcp_attestation</a>(
    <a href="../sui/token.md#sui_token">token</a>: vector&lt;u8&gt;,
    auth_state: &AuthenticatorState,
    kid: vector&lt;u8&gt;,
    <a href="../sui/clock.md#sui_clock">clock</a>: &Clock,
): <a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a> {
    <b>let</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_iss">iss</a> = string::utf8(<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GCP_ISS">GCP_ISS</a>);
    <b>let</b> kid_str = string::utf8(kid);
    <b>let</b> jwk_opt = <a href="../sui/authenticator_state.md#sui_authenticator_state_get_jwk_by_kid">authenticator_state::get_jwk_by_kid</a>(auth_state, <a href="../sui/gcp_attestation.md#sui_gcp_attestation_iss">iss</a>, kid_str);
    <b>assert</b>!(jwk_opt.is_some(), <a href="../sui/gcp_attestation.md#sui_gcp_attestation_EVerifyError">EVerifyError</a>);
    <b>let</b> jwk = jwk_opt.destroy_some();
    // jwk_n and jwk_e are base64url-encoded strings (<b>as</b> stored in AuthenticatorState).
    // The <b>native</b> decodes them internally before RSA verification.
    <a href="../sui/gcp_attestation.md#sui_gcp_attestation_verify_gcp_attestation_internal">verify_gcp_attestation_internal</a>(
        &<a href="../sui/token.md#sui_token">token</a>,
        <a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_n">authenticator_state::jwk_n</a>(&jwk).as_bytes(),
        <a href="../sui/authenticator_state.md#sui_authenticator_state_jwk_e">authenticator_state::jwk_e</a>(&jwk).as_bytes(),
        <a href="../sui/clock.md#sui_clock">clock</a>.timestamp_ms(),
    )
}
</code></pre>



</details>

<a name="sui_gcp_attestation_iss"></a>

## Function `iss`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_iss">iss</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_iss">iss</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): &vector&lt;u8&gt; {
    &doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_iss">iss</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_sub"></a>

## Function `sub`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_sub">sub</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_sub">sub</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): &vector&lt;u8&gt; {
    &doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_sub">sub</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_aud"></a>

## Function `aud`

Returns the first audience value. When the token carries multiple audiences,
only the first is returned; callers requiring multi-audience validation should
use the raw JWT claims directly.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_aud">aud</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_aud">aud</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): &vector&lt;u8&gt; {
    &doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_aud">aud</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_exp"></a>

## Function `exp`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_exp">exp</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_exp">exp</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): u64 {
    doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_exp">exp</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_iat"></a>

## Function `iat`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_iat">iat</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_iat">iat</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): u64 {
    doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_iat">iat</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_eat_nonce"></a>

## Function `eat_nonce`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_eat_nonce">eat_nonce</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): &vector&lt;vector&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_eat_nonce">eat_nonce</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): &vector&lt;vector&lt;u8&gt;&gt; {
    &doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_eat_nonce">eat_nonce</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_secboot"></a>

## Function `secboot`

Returns whether secure boot was enabled. Defaults to <code><b>false</b></code> when the
<code><a href="../sui/gcp_attestation.md#sui_gcp_attestation_secboot">secboot</a></code> field is absent from the token; callers that require secure boot
must assert this value is <code><b>true</b></code> rather than treating absence as enabled.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_secboot">secboot</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_secboot">secboot</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): bool {
    doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_secboot">secboot</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_hwmodel"></a>

## Function `hwmodel`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_hwmodel">hwmodel</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_hwmodel">hwmodel</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): &vector&lt;u8&gt; {
    &doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_hwmodel">hwmodel</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_swname"></a>

## Function `swname`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_swname">swname</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_swname">swname</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): &vector&lt;u8&gt; {
    &doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_swname">swname</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_dbgstat"></a>

## Function `dbgstat`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_dbgstat">dbgstat</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_dbgstat">dbgstat</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): &vector&lt;u8&gt; {
    &doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_dbgstat">dbgstat</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_swversion"></a>

## Function `swversion`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_swversion">swversion</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): &vector&lt;vector&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_swversion">swversion</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): &vector&lt;vector&lt;u8&gt;&gt; {
    &doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_swversion">swversion</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_image_digest"></a>

## Function `image_digest`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_image_digest">image_digest</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_image_digest">image_digest</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): &vector&lt;u8&gt; {
    &doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_image_digest">image_digest</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_image_reference"></a>

## Function `image_reference`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_image_reference">image_reference</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_image_reference">image_reference</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): &vector&lt;u8&gt; {
    &doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_image_reference">image_reference</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_restart_policy"></a>

## Function `restart_policy`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_restart_policy">restart_policy</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_restart_policy">restart_policy</a>(doc: &<a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>): &vector&lt;u8&gt; {
    &doc.<a href="../sui/gcp_attestation.md#sui_gcp_attestation_restart_policy">restart_policy</a>
}
</code></pre>



</details>

<a name="sui_gcp_attestation_verify_gcp_attestation_internal"></a>

## Function `verify_gcp_attestation_internal`

Internal native function.


<pre><code><b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_verify_gcp_attestation_internal">verify_gcp_attestation_internal</a>(<a href="../sui/token.md#sui_token">token</a>: &vector&lt;u8&gt;, jwk_n_b64: &vector&lt;u8&gt;, jwk_e_b64: &vector&lt;u8&gt;, current_timestamp_ms: u64): <a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">sui::gcp_attestation::GcpAttestationDocument</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/gcp_attestation.md#sui_gcp_attestation_verify_gcp_attestation_internal">verify_gcp_attestation_internal</a>(
    <a href="../sui/token.md#sui_token">token</a>: &vector&lt;u8&gt;,
    jwk_n_b64: &vector&lt;u8&gt;,
    jwk_e_b64: &vector&lt;u8&gt;,
    current_timestamp_ms: u64,
): <a href="../sui/gcp_attestation.md#sui_gcp_attestation_GcpAttestationDocument">GcpAttestationDocument</a>;
</code></pre>



</details>
