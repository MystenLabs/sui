---
title: Module `sui::zklogin_verified_id`
---



-  [Struct `VerifiedID`](#sui_zklogin_verified_id_VerifiedID)
-  [Constants](#@Constants_0)
-  [Function `owner`](#sui_zklogin_verified_id_owner)
-  [Function `key_claim_name`](#sui_zklogin_verified_id_key_claim_name)
-  [Function `key_claim_value`](#sui_zklogin_verified_id_key_claim_value)
-  [Function `issuer`](#sui_zklogin_verified_id_issuer)
-  [Function `audience`](#sui_zklogin_verified_id_audience)
-  [Function `delete`](#sui_zklogin_verified_id_delete)
-  [Function `verify_zklogin_id`](#sui_zklogin_verified_id_verify_zklogin_id)
-  [Function `check_zklogin_id`](#sui_zklogin_verified_id_check_zklogin_id)
-  [Function `check_zklogin_id_internal`](#sui_zklogin_verified_id_check_zklogin_id_internal)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="sui_zklogin_verified_id_VerifiedID"></a>

## Struct `VerifiedID`

Possession of a VerifiedID proves that the user's address was created using zklogin and the given parameters.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">VerifiedID</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
 The ID of this VerifiedID
</dd>
<dt>
<code><a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_owner">owner</a>: <b>address</b></code>
</dt>
<dd>
 The address this VerifiedID is associated with
</dd>
<dt>
<code><a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_name">key_claim_name</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 The name of the key claim
</dd>
<dt>
<code><a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_value">key_claim_value</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 The value of the key claim
</dd>
<dt>
<code><a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_issuer">issuer</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 The issuer
</dd>
<dt>
<code><a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_audience">audience</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 The audience (wallet)
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_zklogin_verified_id_EFunctionDisabled"></a>



<pre><code><b>const</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_EFunctionDisabled">EFunctionDisabled</a>: u64 = 0;
</code></pre>



<a name="sui_zklogin_verified_id_owner"></a>

## Function `owner`

Returns the address associated with the given VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_owner">owner</a>(verified_id: &<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">sui::zklogin_verified_id::VerifiedID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_owner">owner</a>(verified_id: &<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">VerifiedID</a>): <b>address</b> {
    verified_id.<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_owner">owner</a>
}
</code></pre>



</details>

<a name="sui_zklogin_verified_id_key_claim_name"></a>

## Function `key_claim_name`

Returns the name of the key claim associated with the given VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_name">key_claim_name</a>(verified_id: &<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">sui::zklogin_verified_id::VerifiedID</a>): &<a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_name">key_claim_name</a>(verified_id: &<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">VerifiedID</a>): &String {
    &verified_id.<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_name">key_claim_name</a>
}
</code></pre>



</details>

<a name="sui_zklogin_verified_id_key_claim_value"></a>

## Function `key_claim_value`

Returns the value of the key claim associated with the given VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_value">key_claim_value</a>(verified_id: &<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">sui::zklogin_verified_id::VerifiedID</a>): &<a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_value">key_claim_value</a>(verified_id: &<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">VerifiedID</a>): &String {
    &verified_id.<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_value">key_claim_value</a>
}
</code></pre>



</details>

<a name="sui_zklogin_verified_id_issuer"></a>

## Function `issuer`

Returns the issuer associated with the given VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_issuer">issuer</a>(verified_id: &<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">sui::zklogin_verified_id::VerifiedID</a>): &<a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_issuer">issuer</a>(verified_id: &<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">VerifiedID</a>): &String {
    &verified_id.<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_issuer">issuer</a>
}
</code></pre>



</details>

<a name="sui_zklogin_verified_id_audience"></a>

## Function `audience`

Returns the audience (wallet) associated with the given VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_audience">audience</a>(verified_id: &<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">sui::zklogin_verified_id::VerifiedID</a>): &<a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_audience">audience</a>(verified_id: &<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">VerifiedID</a>): &String {
    &verified_id.<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_audience">audience</a>
}
</code></pre>



</details>

<a name="sui_zklogin_verified_id_delete"></a>

## Function `delete`

Delete a VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_delete">delete</a>(verified_id: <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">sui::zklogin_verified_id::VerifiedID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_delete">delete</a>(verified_id: <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">VerifiedID</a>) {
    <b>let</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_VerifiedID">VerifiedID</a> { id, <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_owner">owner</a>: _, <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_name">key_claim_name</a>: _, <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_value">key_claim_value</a>: _, <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_issuer">issuer</a>: _, <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_audience">audience</a>: _ } =
        verified_id;
    id.<a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_delete">delete</a>();
}
</code></pre>



</details>

<a name="sui_zklogin_verified_id_verify_zklogin_id"></a>

## Function `verify_zklogin_id`

This function has been disabled.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_verify_zklogin_id">verify_zklogin_id</a>(_key_claim_name: <a href="../std/string.md#std_string_String">std::string::String</a>, _key_claim_value: <a href="../std/string.md#std_string_String">std::string::String</a>, _issuer: <a href="../std/string.md#std_string_String">std::string::String</a>, _audience: <a href="../std/string.md#std_string_String">std::string::String</a>, _pin_hash: u256, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_verify_zklogin_id">verify_zklogin_id</a>(
    _key_claim_name: String,
    _key_claim_value: String,
    _issuer: String,
    _audience: String,
    _pin_hash: u256,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(<b>false</b>, <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_EFunctionDisabled">EFunctionDisabled</a>);
}
</code></pre>



</details>

<a name="sui_zklogin_verified_id_check_zklogin_id"></a>

## Function `check_zklogin_id`

This function has been disabled.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_check_zklogin_id">check_zklogin_id</a>(_address: <b>address</b>, _key_claim_name: &<a href="../std/string.md#std_string_String">std::string::String</a>, _key_claim_value: &<a href="../std/string.md#std_string_String">std::string::String</a>, _issuer: &<a href="../std/string.md#std_string_String">std::string::String</a>, _audience: &<a href="../std/string.md#std_string_String">std::string::String</a>, _pin_hash: u256): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_check_zklogin_id">check_zklogin_id</a>(
    _address: <b>address</b>,
    _key_claim_name: &String,
    _key_claim_value: &String,
    _issuer: &String,
    _audience: &String,
    _pin_hash: u256,
): bool {
    <b>assert</b>!(<b>false</b>, <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_EFunctionDisabled">EFunctionDisabled</a>);
    <b>false</b>
}
</code></pre>



</details>

<a name="sui_zklogin_verified_id_check_zklogin_id_internal"></a>

## Function `check_zklogin_id_internal`

Returns true if <code><b>address</b></code> was created using zklogin and the given parameters.

Aborts with <code>EInvalidInput</code> if any of <code>kc_name</code>, <code>kc_value</code>, <code>iss</code> and <code>aud</code> is not a properly encoded UTF-8
string or if the inputs are longer than the allowed upper bounds: <code>kc_name</code> must be at most 32 characters,
<code>kc_value</code> must be at most 115 characters and <code>aud</code> must be at most 145 characters.


<pre><code><b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_check_zklogin_id_internal">check_zklogin_id_internal</a>(<b>address</b>: <b>address</b>, <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_name">key_claim_name</a>: &vector&lt;u8&gt;, <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_value">key_claim_value</a>: &vector&lt;u8&gt;, <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_issuer">issuer</a>: &vector&lt;u8&gt;, <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_audience">audience</a>: &vector&lt;u8&gt;, pin_hash: u256): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_check_zklogin_id_internal">check_zklogin_id_internal</a>(
    <b>address</b>: <b>address</b>,
    <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_name">key_claim_name</a>: &vector&lt;u8&gt;,
    <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_key_claim_value">key_claim_value</a>: &vector&lt;u8&gt;,
    <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_issuer">issuer</a>: &vector&lt;u8&gt;,
    <a href="../sui/zklogin_verified_id.md#sui_zklogin_verified_id_audience">audience</a>: &vector&lt;u8&gt;,
    pin_hash: u256,
): bool;
</code></pre>



</details>
