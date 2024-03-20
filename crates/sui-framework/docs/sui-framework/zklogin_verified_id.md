
<a name="0x2_zklogin_verified_id"></a>

# Module `0x2::zklogin_verified_id`



-  [Resource `VerifiedID`](#0x2_zklogin_verified_id_VerifiedID)
-  [Constants](#@Constants_0)
-  [Function `owner`](#0x2_zklogin_verified_id_owner)
-  [Function `key_claim_name`](#0x2_zklogin_verified_id_key_claim_name)
-  [Function `key_claim_value`](#0x2_zklogin_verified_id_key_claim_value)
-  [Function `issuer`](#0x2_zklogin_verified_id_issuer)
-  [Function `audience`](#0x2_zklogin_verified_id_audience)
-  [Function `delete`](#0x2_zklogin_verified_id_delete)
-  [Function `verify_zklogin_id`](#0x2_zklogin_verified_id_verify_zklogin_id)
-  [Function `check_zklogin_id`](#0x2_zklogin_verified_id_check_zklogin_id)
-  [Function `check_zklogin_id_internal`](#0x2_zklogin_verified_id_check_zklogin_id_internal)


<pre><code><b>use</b> <a href="dependencies/move-stdlib/string.md#0x1_string">0x1::string</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_zklogin_verified_id_VerifiedID"></a>

## Resource `VerifiedID`

Posession of a VerifiedID proves that the user's address was created using zklogin and the given parameters.


<pre><code><b>struct</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">VerifiedID</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>
 The ID of this VerifiedID
</dd>
<dt>
<code>owner: <b>address</b></code>
</dt>
<dd>
 The address this VerifiedID is associated with
</dd>
<dt>
<code>key_claim_name: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a></code>
</dt>
<dd>
 The name of the key claim
</dd>
<dt>
<code>key_claim_value: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a></code>
</dt>
<dd>
 The value of the key claim
</dd>
<dt>
<code>issuer: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a></code>
</dt>
<dd>
 The issuer
</dd>
<dt>
<code>audience: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a></code>
</dt>
<dd>
 The audience (wallet)
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_zklogin_verified_id_EFunctionDisabled"></a>



<pre><code><b>const</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_EFunctionDisabled">EFunctionDisabled</a>: u64 = 0;
</code></pre>



<a name="0x2_zklogin_verified_id_owner"></a>

## Function `owner`

Returns the address associated with the given VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_owner">owner</a>(verified_id: &<a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">zklogin_verified_id::VerifiedID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_owner">owner</a>(verified_id: &<a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">VerifiedID</a>): <b>address</b> {
    verified_id.owner
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_id_key_claim_name"></a>

## Function `key_claim_name`

Returns the name of the key claim associated with the given VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_key_claim_name">key_claim_name</a>(verified_id: &<a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">zklogin_verified_id::VerifiedID</a>): &<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_key_claim_name">key_claim_name</a>(verified_id: &<a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">VerifiedID</a>): &String {
    &verified_id.key_claim_name
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_id_key_claim_value"></a>

## Function `key_claim_value`

Returns the value of the key claim associated with the given VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_key_claim_value">key_claim_value</a>(verified_id: &<a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">zklogin_verified_id::VerifiedID</a>): &<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_key_claim_value">key_claim_value</a>(verified_id: &<a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">VerifiedID</a>): &String {
    &verified_id.key_claim_value
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_id_issuer"></a>

## Function `issuer`

Returns the issuer associated with the given VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_issuer">issuer</a>(verified_id: &<a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">zklogin_verified_id::VerifiedID</a>): &<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_issuer">issuer</a>(verified_id: &<a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">VerifiedID</a>): &String {
    &verified_id.issuer
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_id_audience"></a>

## Function `audience`

Returns the audience (wallet) associated with the given VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_audience">audience</a>(verified_id: &<a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">zklogin_verified_id::VerifiedID</a>): &<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_audience">audience</a>(verified_id: &<a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">VerifiedID</a>): &String {
    &verified_id.audience
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_id_delete"></a>

## Function `delete`

Delete a VerifiedID


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_delete">delete</a>(verified_id: <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">zklogin_verified_id::VerifiedID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_delete">delete</a>(verified_id: <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">VerifiedID</a>) {
    <b>let</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_VerifiedID">VerifiedID</a> { id, owner: _, key_claim_name: _, key_claim_value: _, issuer: _, audience: _ } = verified_id;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_id_verify_zklogin_id"></a>

## Function `verify_zklogin_id`

This function has been disabled.


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_verify_zklogin_id">verify_zklogin_id</a>(_key_claim_name: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, _key_claim_value: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, _issuer: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, _audience: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, _pin_hash: u256, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_verify_zklogin_id">verify_zklogin_id</a>(
    _key_claim_name: String,
    _key_claim_value: String,
    _issuer: String,
    _audience: String,
    _pin_hash: u256,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(<b>false</b>, <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_EFunctionDisabled">EFunctionDisabled</a>);
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_id_check_zklogin_id"></a>

## Function `check_zklogin_id`

This function has been disabled.


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_check_zklogin_id">check_zklogin_id</a>(_address: <b>address</b>, _key_claim_name: &<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, _key_claim_value: &<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, _issuer: &<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, _audience: &<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, _pin_hash: u256): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_check_zklogin_id">check_zklogin_id</a>(
    _address: <b>address</b>,
    _key_claim_name: &String,
    _key_claim_value: &String,
    _issuer: &String,
    _audience: &String,
    _pin_hash: u256
): bool {
    <b>assert</b>!(<b>false</b>, <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_EFunctionDisabled">EFunctionDisabled</a>);
    <b>false</b>
}
</code></pre>



</details>

<a name="0x2_zklogin_verified_id_check_zklogin_id_internal"></a>

## Function `check_zklogin_id_internal`

Returns true if <code><b>address</b></code> was created using zklogin and the given parameters.

Aborts with <code>EInvalidInput</code> if any of <code>kc_name</code>, <code>kc_value</code>, <code>iss</code> and <code>aud</code> is not a properly encoded UTF-8
string or if the inputs are longer than the allowed upper bounds: <code>kc_name</code> must be at most 32 characters,
<code>kc_value</code> must be at most 115 characters and <code>aud</code> must be at most 145 characters.


<pre><code><b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_check_zklogin_id_internal">check_zklogin_id_internal</a>(<b>address</b>: <b>address</b>, key_claim_name: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, key_claim_value: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, issuer: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, audience: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, pin_hash: u256): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="zklogin_verified_id.md#0x2_zklogin_verified_id_check_zklogin_id_internal">check_zklogin_id_internal</a>(
    <b>address</b>: <b>address</b>,
    key_claim_name: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    key_claim_value: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    issuer: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    audience: &<a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    pin_hash: u256
): bool;
</code></pre>



</details>
