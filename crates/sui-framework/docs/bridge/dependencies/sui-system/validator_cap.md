
<a name="0x3_validator_cap"></a>

# Module `0x3::validator_cap`



-  [Resource `UnverifiedValidatorOperationCap`](#0x3_validator_cap_UnverifiedValidatorOperationCap)
-  [Struct `ValidatorOperationCap`](#0x3_validator_cap_ValidatorOperationCap)
-  [Function `unverified_operation_cap_address`](#0x3_validator_cap_unverified_operation_cap_address)
-  [Function `verified_operation_cap_address`](#0x3_validator_cap_verified_operation_cap_address)
-  [Function `new_unverified_validator_operation_cap_and_transfer`](#0x3_validator_cap_new_unverified_validator_operation_cap_and_transfer)
-  [Function `new_from_unverified`](#0x3_validator_cap_new_from_unverified)


<pre><code><b>use</b> <a href="../../dependencies/sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x3_validator_cap_UnverifiedValidatorOperationCap"></a>

## Resource `UnverifiedValidatorOperationCap`



<pre><code><b>struct</b> <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">UnverifiedValidatorOperationCap</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>authorizer_validator_address: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x3_validator_cap_ValidatorOperationCap"></a>

## Struct `ValidatorOperationCap`



<pre><code><b>struct</b> <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_ValidatorOperationCap">ValidatorOperationCap</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>authorizer_validator_address: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x3_validator_cap_unverified_operation_cap_address"></a>

## Function `unverified_operation_cap_address`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_unverified_operation_cap_address">unverified_operation_cap_address</a>(cap: &<a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>): &<b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_unverified_operation_cap_address">unverified_operation_cap_address</a>(cap: &<a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">UnverifiedValidatorOperationCap</a>): &<b>address</b> {
    &cap.authorizer_validator_address
}
</code></pre>



</details>

<a name="0x3_validator_cap_verified_operation_cap_address"></a>

## Function `verified_operation_cap_address`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_verified_operation_cap_address">verified_operation_cap_address</a>(cap: &<a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_ValidatorOperationCap">validator_cap::ValidatorOperationCap</a>): &<b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_verified_operation_cap_address">verified_operation_cap_address</a>(cap: &<a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_ValidatorOperationCap">ValidatorOperationCap</a>): &<b>address</b> {
    &cap.authorizer_validator_address
}
</code></pre>



</details>

<a name="0x3_validator_cap_new_unverified_validator_operation_cap_and_transfer"></a>

## Function `new_unverified_validator_operation_cap_and_transfer`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_new_unverified_validator_operation_cap_and_transfer">new_unverified_validator_operation_cap_and_transfer</a>(validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_new_unverified_validator_operation_cap_and_transfer">new_unverified_validator_operation_cap_and_transfer</a>(
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
): ID {
    // This function needs <b>to</b> be called only by the <a href="../../dependencies/sui-system/validator.md#0x3_validator">validator</a> itself, <b>except</b>
    // 1. in <a href="../../dependencies/sui-system/genesis.md#0x3_genesis">genesis</a> <b>where</b> all valdiators are created by @0x0
    // 2. in tests <b>where</b> @0x0 could be used <b>to</b> simplify the setup
    <b>let</b> sender_address = <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>assert</b>!(sender_address == @0x0 || sender_address == validator_address, 0);

    <b>let</b> operation_cap = <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">UnverifiedValidatorOperationCap</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_new">object::new</a>(ctx),
        authorizer_validator_address: validator_address,
    };
    <b>let</b> operation_cap_id = <a href="../../dependencies/sui-framework/object.md#0x2_object_id">object::id</a>(&operation_cap);
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(operation_cap, validator_address);
    operation_cap_id
}
</code></pre>



</details>

<a name="0x3_validator_cap_new_from_unverified"></a>

## Function `new_from_unverified`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_new_from_unverified">new_from_unverified</a>(cap: &<a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>): <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_ValidatorOperationCap">validator_cap::ValidatorOperationCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_new_from_unverified">new_from_unverified</a>(
    cap: &<a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">UnverifiedValidatorOperationCap</a>,
): <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_ValidatorOperationCap">ValidatorOperationCap</a> {
    <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_ValidatorOperationCap">ValidatorOperationCap</a> {
        authorizer_validator_address: cap.authorizer_validator_address
    }
}
</code></pre>



</details>
