---
title: Module `0x2::zklogin_verified_issuer`
---



-  [Resource `VerifiedIssuer`](#0x2_zklogin_verified_issuer_VerifiedIssuer)
-  [Constants](#@Constants_0)
-  [Function `owner`](#0x2_zklogin_verified_issuer_owner)
-  [Function `issuer`](#0x2_zklogin_verified_issuer_issuer)
-  [Function `delete`](#0x2_zklogin_verified_issuer_delete)
-  [Function `verify_zklogin_issuer`](#0x2_zklogin_verified_issuer_verify_zklogin_issuer)
-  [Function `check_zklogin_issuer`](#0x2_zklogin_verified_issuer_check_zklogin_issuer)
-  [Function `check_zklogin_issuer_internal`](#0x2_zklogin_verified_issuer_check_zklogin_issuer_internal)


<pre><code><b>use</b> <a href="../move-stdlib/string.md#0x1_string">0x1::string</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_zklogin_verified_issuer_VerifiedIssuer"></a>

## Resource `VerifiedIssuer`

Possession of a VerifiedIssuer proves that the user's address was created using zklogin and with the given issuer
(identity provider).


<pre><code><b>struct</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>
 The ID of this VerifiedIssuer
</dd>
<dt>
<code>owner: <b>address</b></code>
</dt>
<dd>
 The address this VerifiedID is associated with
</dd>
<dt>
<code>issuer: <a href="../move-stdlib/string.md#0x1_string_String">string::String</a></code>
</dt>
<dd>
 The issuer
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_zklogin_verified_issuer_EInvalidInput"></a>

Error if the proof consisting of the inputs provided to the verification function is invalid.


<pre><code><b>const</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_EInvalidInput">EInvalidInput</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_zklogin_verified_issuer_EInvalidProof"></a>

Error if the proof consisting of the inputs provided to the verification function is invalid.


<pre><code><b>const</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_EInvalidProof">EInvalidProof</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x2_zklogin_verified_issuer_owner"></a>

## Function `owner`

Returns the address associated with the given VerifiedIssuer


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_owner">owner</a>(verified_issuer: &<a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_VerifiedIssuer">zklogin_verified_issuer::VerifiedIssuer</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_owner">owner</a>(verified_issuer: &<a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a>): <b>address</b> {
    verified_issuer.owner
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_issuer_issuer"></a>

## Function `issuer`

Returns the issuer associated with the given VerifiedIssuer


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_issuer">issuer</a>(verified_issuer: &<a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_VerifiedIssuer">zklogin_verified_issuer::VerifiedIssuer</a>): &<a href="../move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_issuer">issuer</a>(verified_issuer: &<a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a>): &String {
    &verified_issuer.issuer
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_issuer_delete"></a>

## Function `delete`

Delete a VerifiedIssuer


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_delete">delete</a>(verified_issuer: <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_VerifiedIssuer">zklogin_verified_issuer::VerifiedIssuer</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_delete">delete</a>(verified_issuer: <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a>) {
    <b>let</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a> { id, owner: _, issuer: _ } = verified_issuer;
    id.<a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_delete">delete</a>();
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_issuer_verify_zklogin_issuer"></a>

## Function `verify_zklogin_issuer`

Verify that the caller's address was created using zklogin with the given issuer. If so, a VerifiedIssuer object
with the issuers id transferred to the caller.

Aborts with <code><a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_EInvalidProof">EInvalidProof</a></code> if the verification fails.


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_verify_zklogin_issuer">verify_zklogin_issuer</a>(address_seed: u256, issuer: <a href="../move-stdlib/string.md#0x1_string_String">string::String</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_verify_zklogin_issuer">verify_zklogin_issuer</a>(
    address_seed: u256,
    issuer: String,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> sender = ctx.sender();
    <b>assert</b>!(<a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_check_zklogin_issuer">check_zklogin_issuer</a>(sender, address_seed, &issuer), <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_EInvalidProof">EInvalidProof</a>);
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(
        <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a> {
            id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
            owner: sender,
            issuer
        },
        sender
    )
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_issuer_check_zklogin_issuer"></a>

## Function `check_zklogin_issuer`

Returns true if <code><b>address</b></code> was created using zklogin with the given issuer and address seed.


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_check_zklogin_issuer">check_zklogin_issuer</a>(<b>address</b>: <b>address</b>, address_seed: u256, issuer: &<a href="../move-stdlib/string.md#0x1_string_String">string::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_check_zklogin_issuer">check_zklogin_issuer</a>(
    <b>address</b>: <b>address</b>,
    address_seed: u256,
    issuer: &String,
): bool {
    <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_check_zklogin_issuer_internal">check_zklogin_issuer_internal</a>(<b>address</b>, address_seed, issuer.as_bytes())
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_issuer_check_zklogin_issuer_internal"></a>

## Function `check_zklogin_issuer_internal`

Returns true if <code><b>address</b></code> was created using zklogin with the given issuer and address seed.

Aborts with <code><a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_EInvalidInput">EInvalidInput</a></code> if the <code>iss</code> input is not a valid UTF-8 string.


<pre><code><b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_check_zklogin_issuer_internal">check_zklogin_issuer_internal</a>(<b>address</b>: <b>address</b>, address_seed: u256, issuer: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="zklogin_verified_issuer.md#0x2_zklogin_verified_issuer_check_zklogin_issuer_internal">check_zklogin_issuer_internal</a>(
    <b>address</b>: <b>address</b>,
    address_seed: u256,
    issuer: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
): bool;
</code></pre>



</details>
