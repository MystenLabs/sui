---
title: Module `sui::zklogin_verified_issuer`
---



-  [Struct `VerifiedIssuer`](#sui_zklogin_verified_issuer_VerifiedIssuer)
-  [Constants](#@Constants_0)
-  [Function `owner`](#sui_zklogin_verified_issuer_owner)
-  [Function `issuer`](#sui_zklogin_verified_issuer_issuer)
-  [Function `delete`](#sui_zklogin_verified_issuer_delete)
-  [Function `verify_zklogin_issuer`](#sui_zklogin_verified_issuer_verify_zklogin_issuer)
-  [Function `check_zklogin_issuer`](#sui_zklogin_verified_issuer_check_zklogin_issuer)
-  [Function `check_zklogin_issuer_internal`](#sui_zklogin_verified_issuer_check_zklogin_issuer_internal)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="sui_zklogin_verified_issuer_VerifiedIssuer"></a>

## Struct `VerifiedIssuer`

Possession of a VerifiedIssuer proves that the user's address was created using zklogin and with the given issuer
(identity provider).


<pre><code><b>public</b> <b>struct</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
 The ID of this VerifiedIssuer
</dd>
<dt>
<code><a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_owner">owner</a>: <b>address</b></code>
</dt>
<dd>
 The address this VerifiedID is associated with
</dd>
<dt>
<code><a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 The issuer
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_zklogin_verified_issuer_EInvalidInput"></a>

Error if the proof consisting of the inputs provided to the verification function is invalid.


<pre><code><b>const</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_EInvalidInput">EInvalidInput</a>: u64 = 0;
</code></pre>



<a name="sui_zklogin_verified_issuer_EInvalidProof"></a>

Error if the proof consisting of the inputs provided to the verification function is invalid.


<pre><code><b>const</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_EInvalidProof">EInvalidProof</a>: u64 = 1;
</code></pre>



<a name="sui_zklogin_verified_issuer_owner"></a>

## Function `owner`

Returns the address associated with the given VerifiedIssuer


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_owner">owner</a>(verified_issuer: &<a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_VerifiedIssuer">sui::zklogin_verified_issuer::VerifiedIssuer</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_owner">owner</a>(verified_issuer: &<a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a>): <b>address</b> {
    verified_issuer.<a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_owner">owner</a>
}
</code></pre>



</details>

<a name="sui_zklogin_verified_issuer_issuer"></a>

## Function `issuer`

Returns the issuer associated with the given VerifiedIssuer


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>(verified_issuer: &<a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_VerifiedIssuer">sui::zklogin_verified_issuer::VerifiedIssuer</a>): &<a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>(verified_issuer: &<a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a>): &String {
    &verified_issuer.<a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>
}
</code></pre>



</details>

<a name="sui_zklogin_verified_issuer_delete"></a>

## Function `delete`

Delete a VerifiedIssuer


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_delete">delete</a>(verified_issuer: <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_VerifiedIssuer">sui::zklogin_verified_issuer::VerifiedIssuer</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_delete">delete</a>(verified_issuer: <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a>) {
    <b>let</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a> { id, <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_owner">owner</a>: _, <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>: _ } = verified_issuer;
    id.<a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_delete">delete</a>();
}
</code></pre>



</details>

<a name="sui_zklogin_verified_issuer_verify_zklogin_issuer"></a>

## Function `verify_zklogin_issuer`

Verify that the caller's address was created using zklogin with the given issuer. If so, a VerifiedIssuer object
with the issuers id transferred to the caller.

Aborts with <code><a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_EInvalidProof">EInvalidProof</a></code> if the verification fails.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_verify_zklogin_issuer">verify_zklogin_issuer</a>(address_seed: u256, <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_verify_zklogin_issuer">verify_zklogin_issuer</a>(address_seed: u256, <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>: String, ctx: &<b>mut</b> TxContext) {
    <b>let</b> sender = ctx.sender();
    <b>assert</b>!(<a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_check_zklogin_issuer">check_zklogin_issuer</a>(sender, address_seed, &<a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>), <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_EInvalidProof">EInvalidProof</a>);
    <a href="../sui/transfer.md#sui_transfer_transfer">transfer::transfer</a>(
        <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_VerifiedIssuer">VerifiedIssuer</a> {
            id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
            <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_owner">owner</a>: sender,
            <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>,
        },
        sender,
    )
}
</code></pre>



</details>

<a name="sui_zklogin_verified_issuer_check_zklogin_issuer"></a>

## Function `check_zklogin_issuer`

Returns true if <code><b>address</b></code> was created using zklogin with the given issuer and address seed.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_check_zklogin_issuer">check_zklogin_issuer</a>(<b>address</b>: <b>address</b>, address_seed: u256, <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>: &<a href="../std/string.md#std_string_String">std::string::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_check_zklogin_issuer">check_zklogin_issuer</a>(<b>address</b>: <b>address</b>, address_seed: u256, <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>: &String): bool {
    <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_check_zklogin_issuer_internal">check_zklogin_issuer_internal</a>(<b>address</b>, address_seed, <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>.as_bytes())
}
</code></pre>



</details>

<a name="sui_zklogin_verified_issuer_check_zklogin_issuer_internal"></a>

## Function `check_zklogin_issuer_internal`

Returns true if <code><b>address</b></code> was created using zklogin with the given issuer and address seed.

Aborts with <code><a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_EInvalidInput">EInvalidInput</a></code> if the <code>iss</code> input is not a valid UTF-8 string.


<pre><code><b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_check_zklogin_issuer_internal">check_zklogin_issuer_internal</a>(<b>address</b>: <b>address</b>, address_seed: u256, <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>: &vector&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_check_zklogin_issuer_internal">check_zklogin_issuer_internal</a>(
    <b>address</b>: <b>address</b>,
    address_seed: u256,
    <a href="../sui/zklogin_verified_issuer.md#sui_zklogin_verified_issuer_issuer">issuer</a>: &vector&lt;u8&gt;,
): bool;
</code></pre>



</details>
