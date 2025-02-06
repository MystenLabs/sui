---
title: Module `sui::nitro_attestation`
---



-  [Struct `PCREntry`](#sui_nitro_attestation_PCREntry)
-  [Struct `NitroAttestationDocument`](#sui_nitro_attestation_NitroAttestationDocument)
-  [Constants](#@Constants_0)
-  [Function `load_nitro_attestation`](#sui_nitro_attestation_load_nitro_attestation)
-  [Function `module_id`](#sui_nitro_attestation_module_id)
-  [Function `timestamp`](#sui_nitro_attestation_timestamp)
-  [Function `digest`](#sui_nitro_attestation_digest)
-  [Function `pcrs`](#sui_nitro_attestation_pcrs)
-  [Function `public_key`](#sui_nitro_attestation_public_key)
-  [Function `user_data`](#sui_nitro_attestation_user_data)
-  [Function `nonce`](#sui_nitro_attestation_nonce)
-  [Function `index`](#sui_nitro_attestation_index)
-  [Function `value`](#sui_nitro_attestation_value)
-  [Function `load_nitro_attestation_internal`](#sui_nitro_attestation_load_nitro_attestation_internal)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/clock.md#sui_clock">sui::clock</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="sui_nitro_attestation_PCREntry"></a>

## Struct `PCREntry`

Represents a PCR entry with an index and value.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_PCREntry">PCREntry</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/nitro_attestation.md#sui_nitro_attestation_index">index</a>: u8</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui/nitro_attestation.md#sui_nitro_attestation_value">value</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_nitro_attestation_NitroAttestationDocument"></a>

## Struct `NitroAttestationDocument`

Nitro Attestation Document defined for AWS.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">NitroAttestationDocument</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/nitro_attestation.md#sui_nitro_attestation_module_id">module_id</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
 Issuing Nitro hypervisor module ID.
</dd>
<dt>
<code><a href="../sui/nitro_attestation.md#sui_nitro_attestation_timestamp">timestamp</a>: u64</code>
</dt>
<dd>
 UTC time when document was created, in milliseconds since UNIX epoch.
</dd>
<dt>
<code><a href="../sui/nitro_attestation.md#sui_nitro_attestation_digest">digest</a>: vector&lt;u8&gt;</code>
</dt>
<dd>
 The digest function used for calculating the register values.
</dd>
<dt>
<code><a href="../sui/nitro_attestation.md#sui_nitro_attestation_pcrs">pcrs</a>: vector&lt;<a href="../sui/nitro_attestation.md#sui_nitro_attestation_PCREntry">sui::nitro_attestation::PCREntry</a>&gt;</code>
</dt>
<dd>
 A list of PCREntry containing the index and the PCR bytes.
 <https://docs.aws.amazon.com/enclaves/latest/user/set-up-attestation.html#where>.
</dd>
<dt>
<code><a href="../sui/nitro_attestation.md#sui_nitro_attestation_public_key">public_key</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;vector&lt;u8&gt;&gt;</code>
</dt>
<dd>
 An optional DER-encoded key the attestation, consumer can use to encrypt data with.
</dd>
<dt>
<code><a href="../sui/nitro_attestation.md#sui_nitro_attestation_user_data">user_data</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;vector&lt;u8&gt;&gt;</code>
</dt>
<dd>
 Additional signed user data, defined by protocol.
</dd>
<dt>
<code><a href="../sui/nitro_attestation.md#sui_nitro_attestation_nonce">nonce</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;vector&lt;u8&gt;&gt;</code>
</dt>
<dd>
 An optional cryptographic nonce provided by the attestation consumer as a proof of
 authenticity.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_nitro_attestation_EInvalidPCRsError"></a>

Error that the PCRs are invalid.


<pre><code><b>const</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_EInvalidPCRsError">EInvalidPCRsError</a>: u64 = 3;
</code></pre>



<a name="sui_nitro_attestation_ENotSupportedError"></a>

Error that the feature is not available on this network.


<pre><code><b>const</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_ENotSupportedError">ENotSupportedError</a>: u64 = 0;
</code></pre>



<a name="sui_nitro_attestation_EParseError"></a>

Error that the attestation input failed to be parsed.


<pre><code><b>const</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_EParseError">EParseError</a>: u64 = 1;
</code></pre>



<a name="sui_nitro_attestation_EVerifyError"></a>

Error that the attestation failed to be verified.


<pre><code><b>const</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_EVerifyError">EVerifyError</a>: u64 = 2;
</code></pre>



<a name="sui_nitro_attestation_load_nitro_attestation"></a>

## Function `load_nitro_attestation`

@param attestation: attesttaion documents bytes data.
@param clock: the clock object.

Returns the parsed NitroAttestationDocument after verifying the attestation,
may abort with errors described above.


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_load_nitro_attestation">load_nitro_attestation</a>(attestation: vector&lt;u8&gt;, <a href="../sui/clock.md#sui_clock">clock</a>: &<a href="../sui/clock.md#sui_clock_Clock">sui::clock::Clock</a>): <a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">sui::nitro_attestation::NitroAttestationDocument</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>entry</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_load_nitro_attestation">load_nitro_attestation</a>(
    attestation: vector&lt;u8&gt;,
    <a href="../sui/clock.md#sui_clock">clock</a>: &Clock
): <a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">NitroAttestationDocument</a> {
    <a href="../sui/nitro_attestation.md#sui_nitro_attestation_load_nitro_attestation_internal">load_nitro_attestation_internal</a>(&attestation, <a href="../sui/clock.md#sui_clock_timestamp_ms">clock::timestamp_ms</a>(<a href="../sui/clock.md#sui_clock">clock</a>))
}
</code></pre>



</details>

<a name="sui_nitro_attestation_module_id"></a>

## Function `module_id`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_module_id">module_id</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">sui::nitro_attestation::NitroAttestationDocument</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_module_id">module_id</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">NitroAttestationDocument</a>): &vector&lt;u8&gt; {
    &attestation.<a href="../sui/nitro_attestation.md#sui_nitro_attestation_module_id">module_id</a>
}
</code></pre>



</details>

<a name="sui_nitro_attestation_timestamp"></a>

## Function `timestamp`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_timestamp">timestamp</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">sui::nitro_attestation::NitroAttestationDocument</a>): &u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_timestamp">timestamp</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">NitroAttestationDocument</a>): &u64 {
    &attestation.<a href="../sui/nitro_attestation.md#sui_nitro_attestation_timestamp">timestamp</a>
}
</code></pre>



</details>

<a name="sui_nitro_attestation_digest"></a>

## Function `digest`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_digest">digest</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">sui::nitro_attestation::NitroAttestationDocument</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_digest">digest</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">NitroAttestationDocument</a>): &vector&lt;u8&gt; {
    &attestation.<a href="../sui/nitro_attestation.md#sui_nitro_attestation_digest">digest</a>
}
</code></pre>



</details>

<a name="sui_nitro_attestation_pcrs"></a>

## Function `pcrs`

Returns a list of mapping PCREntry containg the index and the PCR bytes.
Currently AWS supports PCR0, PCR1, PCR2, PCR3, PCR4, PCR8.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_pcrs">pcrs</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">sui::nitro_attestation::NitroAttestationDocument</a>): &vector&lt;<a href="../sui/nitro_attestation.md#sui_nitro_attestation_PCREntry">sui::nitro_attestation::PCREntry</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_pcrs">pcrs</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">NitroAttestationDocument</a>): &vector&lt;<a href="../sui/nitro_attestation.md#sui_nitro_attestation_PCREntry">PCREntry</a>&gt; {
    &attestation.<a href="../sui/nitro_attestation.md#sui_nitro_attestation_pcrs">pcrs</a>
}
</code></pre>



</details>

<a name="sui_nitro_attestation_public_key"></a>

## Function `public_key`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_public_key">public_key</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">sui::nitro_attestation::NitroAttestationDocument</a>): &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;vector&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_public_key">public_key</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">NitroAttestationDocument</a>): &Option&lt;vector&lt;u8&gt;&gt; {
    &attestation.<a href="../sui/nitro_attestation.md#sui_nitro_attestation_public_key">public_key</a>
}
</code></pre>



</details>

<a name="sui_nitro_attestation_user_data"></a>

## Function `user_data`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_user_data">user_data</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">sui::nitro_attestation::NitroAttestationDocument</a>): &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;vector&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_user_data">user_data</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">NitroAttestationDocument</a>): &Option&lt;vector&lt;u8&gt;&gt; {
    &attestation.<a href="../sui/nitro_attestation.md#sui_nitro_attestation_user_data">user_data</a>
}
</code></pre>



</details>

<a name="sui_nitro_attestation_nonce"></a>

## Function `nonce`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_nonce">nonce</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">sui::nitro_attestation::NitroAttestationDocument</a>): &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;vector&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_nonce">nonce</a>(attestation: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">NitroAttestationDocument</a>): &Option&lt;vector&lt;u8&gt;&gt; {
    &attestation.<a href="../sui/nitro_attestation.md#sui_nitro_attestation_nonce">nonce</a>
}
</code></pre>



</details>

<a name="sui_nitro_attestation_index"></a>

## Function `index`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_index">index</a>(<b>entry</b>: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_PCREntry">sui::nitro_attestation::PCREntry</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_index">index</a>(<b>entry</b>: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_PCREntry">PCREntry</a>): u8 {
    <b>entry</b>.<a href="../sui/nitro_attestation.md#sui_nitro_attestation_index">index</a>
}
</code></pre>



</details>

<a name="sui_nitro_attestation_value"></a>

## Function `value`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_value">value</a>(<b>entry</b>: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_PCREntry">sui::nitro_attestation::PCREntry</a>): &vector&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_value">value</a>(<b>entry</b>: &<a href="../sui/nitro_attestation.md#sui_nitro_attestation_PCREntry">PCREntry</a>): &vector&lt;u8&gt; {
    &<b>entry</b>.<a href="../sui/nitro_attestation.md#sui_nitro_attestation_value">value</a>
}
</code></pre>



</details>

<a name="sui_nitro_attestation_load_nitro_attestation_internal"></a>

## Function `load_nitro_attestation_internal`

Internal native function


<pre><code><b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_load_nitro_attestation_internal">load_nitro_attestation_internal</a>(attestation: &vector&lt;u8&gt;, current_timestamp: u64): <a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">sui::nitro_attestation::NitroAttestationDocument</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/nitro_attestation.md#sui_nitro_attestation_load_nitro_attestation_internal">load_nitro_attestation_internal</a>(
    attestation: &vector&lt;u8&gt;,
    current_timestamp: u64,
): <a href="../sui/nitro_attestation.md#sui_nitro_attestation_NitroAttestationDocument">NitroAttestationDocument</a>;
</code></pre>



</details>
