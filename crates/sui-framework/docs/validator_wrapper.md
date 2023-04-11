
<a name="0x3_validator_wrapper"></a>

# Module `0x3::validator_wrapper`



-  [Struct `ValidatorWrapper`](#0x3_validator_wrapper_ValidatorWrapper)
-  [Constants](#@Constants_0)
-  [Function `create_v1`](#0x3_validator_wrapper_create_v1)
-  [Function `load_validator_maybe_upgrade`](#0x3_validator_wrapper_load_validator_maybe_upgrade)
-  [Function `destroy`](#0x3_validator_wrapper_destroy)
-  [Function `upgrade_to_latest`](#0x3_validator_wrapper_upgrade_to_latest)
-  [Function `version`](#0x3_validator_wrapper_version)


<pre><code><b>use</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/versioned.md#0x2_versioned">0x2::versioned</a>;
<b>use</b> <a href="validator.md#0x3_validator">0x3::validator</a>;
</code></pre>



<a name="0x3_validator_wrapper_ValidatorWrapper"></a>

## Struct `ValidatorWrapper`



<pre><code><b>struct</b> <a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">ValidatorWrapper</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>inner: <a href="../../../.././build/Sui/docs/versioned.md#0x2_versioned_Versioned">versioned::Versioned</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x3_validator_wrapper_EInvalidVersion"></a>



<pre><code><b>const</b> <a href="validator_wrapper.md#0x3_validator_wrapper_EInvalidVersion">EInvalidVersion</a>: u64 = 0;
</code></pre>



<a name="0x3_validator_wrapper_create_v1"></a>

## Function `create_v1`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_wrapper.md#0x3_validator_wrapper_create_v1">create_v1</a>(<a href="validator.md#0x3_validator">validator</a>: <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">validator_wrapper::ValidatorWrapper</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_wrapper.md#0x3_validator_wrapper_create_v1">create_v1</a>(<a href="validator.md#0x3_validator">validator</a>: Validator, ctx: &<b>mut</b> TxContext): <a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">ValidatorWrapper</a> {
    <a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">ValidatorWrapper</a> {
        inner: <a href="../../../.././build/Sui/docs/versioned.md#0x2_versioned_create">versioned::create</a>(1, <a href="validator.md#0x3_validator">validator</a>, ctx)
    }
}
</code></pre>



</details>

<a name="0x3_validator_wrapper_load_validator_maybe_upgrade"></a>

## Function `load_validator_maybe_upgrade`

This function should always return the latest supported version.
If the inner version is old, we upgrade it lazily in-place.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_wrapper.md#0x3_validator_wrapper_load_validator_maybe_upgrade">load_validator_maybe_upgrade</a>(self: &<b>mut</b> <a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">validator_wrapper::ValidatorWrapper</a>): &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_wrapper.md#0x3_validator_wrapper_load_validator_maybe_upgrade">load_validator_maybe_upgrade</a>(self: &<b>mut</b> <a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">ValidatorWrapper</a>): &<b>mut</b> Validator {
    <a href="validator_wrapper.md#0x3_validator_wrapper_upgrade_to_latest">upgrade_to_latest</a>(self);
    <a href="../../../.././build/Sui/docs/versioned.md#0x2_versioned_load_value_mut">versioned::load_value_mut</a>(&<b>mut</b> self.inner)
}
</code></pre>



</details>

<a name="0x3_validator_wrapper_destroy"></a>

## Function `destroy`

Destroy the wrapper and retrieve the inner validator object.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_wrapper.md#0x3_validator_wrapper_destroy">destroy</a>(self: <a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">validator_wrapper::ValidatorWrapper</a>): <a href="validator.md#0x3_validator_Validator">validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator_wrapper.md#0x3_validator_wrapper_destroy">destroy</a>(self: <a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">ValidatorWrapper</a>): Validator {
    <a href="validator_wrapper.md#0x3_validator_wrapper_upgrade_to_latest">upgrade_to_latest</a>(&<b>mut</b> self);
    <b>let</b> <a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">ValidatorWrapper</a> { inner } = self;
    <a href="../../../.././build/Sui/docs/versioned.md#0x2_versioned_destroy">versioned::destroy</a>(inner)
}
</code></pre>



</details>

<a name="0x3_validator_wrapper_upgrade_to_latest"></a>

## Function `upgrade_to_latest`



<pre><code><b>fun</b> <a href="validator_wrapper.md#0x3_validator_wrapper_upgrade_to_latest">upgrade_to_latest</a>(self: &<b>mut</b> <a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">validator_wrapper::ValidatorWrapper</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_wrapper.md#0x3_validator_wrapper_upgrade_to_latest">upgrade_to_latest</a>(self: &<b>mut</b> <a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">ValidatorWrapper</a>) {
    <b>let</b> version = <a href="validator_wrapper.md#0x3_validator_wrapper_version">version</a>(self);
    // TODO: When new versions are added, we need <b>to</b> explicitly upgrade here.
    <b>assert</b>!(version == 1, <a href="validator_wrapper.md#0x3_validator_wrapper_EInvalidVersion">EInvalidVersion</a>);
}
</code></pre>



</details>

<a name="0x3_validator_wrapper_version"></a>

## Function `version`



<pre><code><b>fun</b> <a href="validator_wrapper.md#0x3_validator_wrapper_version">version</a>(self: &<a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">validator_wrapper::ValidatorWrapper</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator_wrapper.md#0x3_validator_wrapper_version">version</a>(self: &<a href="validator_wrapper.md#0x3_validator_wrapper_ValidatorWrapper">ValidatorWrapper</a>): u64 {
    <a href="../../../.././build/Sui/docs/versioned.md#0x2_versioned_version">versioned::version</a>(&self.inner)
}
</code></pre>



</details>
