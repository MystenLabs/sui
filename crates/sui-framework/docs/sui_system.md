
<a name="0x3_sui_system"></a>

# Module `0x3::sui_system`

Sui System State Type Upgrade Guide
<code><a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a></code> is a thin wrapper around <code>SuiSystemStateInner</code> that provides a versioned interface.
The <code><a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a></code> object has a fixed ID 0x5, and the <code>SuiSystemStateInner</code> object is stored as a dynamic field.
There are a few different ways to upgrade the <code>SuiSystemStateInner</code> type:

The simplest and one that doesn't involve a real upgrade is to just add dynamic fields to the <code>extra_fields</code> field
of <code>SuiSystemStateInner</code> or any of its sub type. This is useful when we are in a rush, or making a small change,
or still experimenting a new field.

To properly upgrade the <code>SuiSystemStateInner</code> type, we need to ship a new framework that does the following:
1. Define a new <code>SuiSystemStateInner</code>type (e.g. <code>SuiSystemStateInnerV2</code>).
2. Define a data migration function that migrates the old <code>SuiSystemStateInner</code> to the new one (i.e. SuiSystemStateInnerV2).
3. Replace all uses of <code>SuiSystemStateInner</code> with <code>SuiSystemStateInnerV2</code> in both sui_system.move and sui_system_state_inner.move,
with the exception of the <code><a href="sui_system_state_inner.md#0x3_sui_system_state_inner_create">sui_system_state_inner::create</a></code> function, which should always return the genesis type.
4. Inside <code>load_inner_maybe_upgrade</code> function, check the current version in the wrapper, and if it's not the latest version,
call the data migration function to upgrade the inner object. Make sure to also update the version in the wrapper.
A detailed example can be found in sui/tests/framework_upgrades/mock_sui_systems/shallow_upgrade.
Along with the Move change, we also need to update the Rust code to support the new type. This includes:
1. Define a new <code>SuiSystemStateInner</code> struct type that matches the new Move type, and implement the SuiSystemStateTrait.
2. Update the <code><a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a></code> struct to include the new version as a new enum variant.
3. Update the <code>get_sui_system_state</code> function to handle the new version.
To test that the upgrade will be successful, we need to modify <code>sui_system_state_production_upgrade_test</code> test in
protocol_version_tests and trigger a real upgrade using the new framework. We will need to keep this directory as old version,
put the new framework in a new directory, and run the test to exercise the upgrade.

To upgrade Validator type, besides everything above, we also need to:
1. Define a new Validator type (e.g. ValidatorV2).
2. Define a data migration function that migrates the old Validator to the new one (i.e. ValidatorV2).
3. Replace all uses of Validator with ValidatorV2 except the genesis creation function.
4. In validator_wrapper::upgrade_to_latest, check the current version in the wrapper, and if it's not the latest version,
call the data migration function to upgrade it.
In Rust, we also need to add a new case in <code>get_validator_from_table</code>.
Note that it is possible to upgrade SuiSystemStateInner without upgrading Validator, but not the other way around.
And when we only upgrade SuiSystemStateInner, the version of Validator in the wrapper will not be updated, and hence may become
inconsistent with the version of SuiSystemStateInner. This is fine as long as we don't use the Validator version to determine
the SuiSystemStateInner version, or vice versa.


-  [Resource `SuiSystemState`](#0x3_sui_system_SuiSystemState)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x3_sui_system_create)
-  [Function `request_add_validator_candidate`](#0x3_sui_system_request_add_validator_candidate)
-  [Function `request_remove_validator_candidate`](#0x3_sui_system_request_remove_validator_candidate)
-  [Function `request_add_validator`](#0x3_sui_system_request_add_validator)
-  [Function `request_remove_validator`](#0x3_sui_system_request_remove_validator)
-  [Function `request_set_gas_price`](#0x3_sui_system_request_set_gas_price)
-  [Function `set_candidate_validator_gas_price`](#0x3_sui_system_set_candidate_validator_gas_price)
-  [Function `request_set_commission_rate`](#0x3_sui_system_request_set_commission_rate)
-  [Function `set_candidate_validator_commission_rate`](#0x3_sui_system_set_candidate_validator_commission_rate)
-  [Function `request_add_stake`](#0x3_sui_system_request_add_stake)
-  [Function `request_add_stake_mul_coin`](#0x3_sui_system_request_add_stake_mul_coin)
-  [Function `request_withdraw_stake`](#0x3_sui_system_request_withdraw_stake)
-  [Function `report_validator`](#0x3_sui_system_report_validator)
-  [Function `undo_report_validator`](#0x3_sui_system_undo_report_validator)
-  [Function `rotate_operation_cap`](#0x3_sui_system_rotate_operation_cap)
-  [Function `update_validator_name`](#0x3_sui_system_update_validator_name)
-  [Function `update_validator_description`](#0x3_sui_system_update_validator_description)
-  [Function `update_validator_image_url`](#0x3_sui_system_update_validator_image_url)
-  [Function `update_validator_project_url`](#0x3_sui_system_update_validator_project_url)
-  [Function `update_validator_next_epoch_network_address`](#0x3_sui_system_update_validator_next_epoch_network_address)
-  [Function `update_candidate_validator_network_address`](#0x3_sui_system_update_candidate_validator_network_address)
-  [Function `update_validator_next_epoch_p2p_address`](#0x3_sui_system_update_validator_next_epoch_p2p_address)
-  [Function `update_candidate_validator_p2p_address`](#0x3_sui_system_update_candidate_validator_p2p_address)
-  [Function `update_validator_next_epoch_primary_address`](#0x3_sui_system_update_validator_next_epoch_primary_address)
-  [Function `update_candidate_validator_primary_address`](#0x3_sui_system_update_candidate_validator_primary_address)
-  [Function `update_validator_next_epoch_worker_address`](#0x3_sui_system_update_validator_next_epoch_worker_address)
-  [Function `update_candidate_validator_worker_address`](#0x3_sui_system_update_candidate_validator_worker_address)
-  [Function `update_validator_next_epoch_protocol_pubkey`](#0x3_sui_system_update_validator_next_epoch_protocol_pubkey)
-  [Function `update_candidate_validator_protocol_pubkey`](#0x3_sui_system_update_candidate_validator_protocol_pubkey)
-  [Function `update_validator_next_epoch_worker_pubkey`](#0x3_sui_system_update_validator_next_epoch_worker_pubkey)
-  [Function `update_candidate_validator_worker_pubkey`](#0x3_sui_system_update_candidate_validator_worker_pubkey)
-  [Function `update_validator_next_epoch_network_pubkey`](#0x3_sui_system_update_validator_next_epoch_network_pubkey)
-  [Function `update_candidate_validator_network_pubkey`](#0x3_sui_system_update_candidate_validator_network_pubkey)
-  [Function `advance_epoch`](#0x3_sui_system_advance_epoch)
-  [Function `load_system_state`](#0x3_sui_system_load_system_state)
-  [Function `load_system_state_mut`](#0x3_sui_system_load_system_state_mut)
-  [Function `load_inner_maybe_upgrade`](#0x3_sui_system_load_inner_maybe_upgrade)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="../../../build/Sui/docs/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../../../build/Sui/docs/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../../../build/Sui/docs/dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="../../../build/Sui/docs/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../../build/Sui/docs/sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="../../../build/Sui/docs/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="stake_subsidy.md#0x3_stake_subsidy">0x3::stake_subsidy</a>;
<b>use</b> <a href="staking_pool.md#0x3_staking_pool">0x3::staking_pool</a>;
<b>use</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner">0x3::sui_system_state_inner</a>;
<b>use</b> <a href="validator.md#0x3_validator">0x3::validator</a>;
<b>use</b> <a href="validator_cap.md#0x3_validator_cap">0x3::validator_cap</a>;
</code></pre>



<a name="0x3_sui_system_SuiSystemState"></a>

## Resource `SuiSystemState`



<pre><code><b>struct</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../../build/Sui/docs/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>version: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x3_sui_system_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="sui_system.md#0x3_sui_system_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="0x3_sui_system_EWrongInnerVersion"></a>



<pre><code><b>const</b> <a href="sui_system.md#0x3_sui_system_EWrongInnerVersion">EWrongInnerVersion</a>: u64 = 1;
</code></pre>



<a name="0x3_sui_system_create"></a>

## Function `create`

Create a new SuiSystemState object and make it shared.
This function will be called only once in genesis.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system.md#0x3_sui_system_create">create</a>(id: <a href="../../../build/Sui/docs/object.md#0x2_object_UID">object::UID</a>, validators: <a href="">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, <a href="storage_fund.md#0x3_storage_fund">storage_fund</a>: <a href="../../../build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../../build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, protocol_version: u64, epoch_start_timestamp_ms: u64, parameters: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">sui_system_state_inner::SystemParameters</a>, <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>: <a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">stake_subsidy::StakeSubsidy</a>, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system.md#0x3_sui_system_create">create</a>(
    id: UID,
    validators: <a href="">vector</a>&lt;Validator&gt;,
    <a href="storage_fund.md#0x3_storage_fund">storage_fund</a>: Balance&lt;SUI&gt;,
    protocol_version: u64,
    epoch_start_timestamp_ms: u64,
    parameters: SystemParameters,
    <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>: StakeSubsidy,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> system_state = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_create">sui_system_state_inner::create</a>(
        validators,
        <a href="storage_fund.md#0x3_storage_fund">storage_fund</a>,
        protocol_version,
        epoch_start_timestamp_ms,
        parameters,
        <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>,
        ctx,
    );
    <b>let</b> version = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_genesis_system_state_version">sui_system_state_inner::genesis_system_state_version</a>();
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a> {
        id,
        version,
    };
    <a href="../../../build/Sui/docs/dynamic_field.md#0x2_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> self.id, version, system_state);
    <a href="../../../build/Sui/docs/transfer.md#0x2_transfer_share_object">transfer::share_object</a>(self);
}
</code></pre>



</details>

<a name="0x3_sui_system_request_add_validator_candidate"></a>

## Function `request_add_validator_candidate`

Can be called by anyone who wishes to become a validator candidate and starts accuring delegated
stakes in their staking pool. Once they have at least <code>MIN_VALIDATOR_JOINING_STAKE</code> amount of stake they
can call <code>request_add_validator</code> to officially become an active validator at the next epoch.
Aborts if the caller is already a pending or active validator, or a validator candidate.
Note: <code>proof_of_possession</code> MUST be a valid signature using sui_address and protocol_pubkey_bytes.
To produce a valid PoP, run [fn test_proof_of_possession].


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_validator_candidate">request_add_validator_candidate</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, network_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, worker_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, name: <a href="">vector</a>&lt;u8&gt;, description: <a href="">vector</a>&lt;u8&gt;, image_url: <a href="">vector</a>&lt;u8&gt;, project_url: <a href="">vector</a>&lt;u8&gt;, net_address: <a href="">vector</a>&lt;u8&gt;, p2p_address: <a href="">vector</a>&lt;u8&gt;, primary_address: <a href="">vector</a>&lt;u8&gt;, worker_address: <a href="">vector</a>&lt;u8&gt;, gas_price: u64, commission_rate: u64, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_validator_candidate">request_add_validator_candidate</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    pubkey_bytes: <a href="">vector</a>&lt;u8&gt;,
    network_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;,
    worker_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="">vector</a>&lt;u8&gt;,
    name: <a href="">vector</a>&lt;u8&gt;,
    description: <a href="">vector</a>&lt;u8&gt;,
    image_url: <a href="">vector</a>&lt;u8&gt;,
    project_url: <a href="">vector</a>&lt;u8&gt;,
    net_address: <a href="">vector</a>&lt;u8&gt;,
    p2p_address: <a href="">vector</a>&lt;u8&gt;,
    primary_address: <a href="">vector</a>&lt;u8&gt;,
    worker_address: <a href="">vector</a>&lt;u8&gt;,
    gas_price: u64,
    commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator_candidate">sui_system_state_inner::request_add_validator_candidate</a>(
        self,
        pubkey_bytes,
        network_pubkey_bytes,
        worker_pubkey_bytes,
        proof_of_possession,
        name,
        description,
        image_url,
        project_url,
        net_address,
        p2p_address,
        primary_address,
        worker_address,
        gas_price,
        commission_rate,
        ctx,
    )
}
</code></pre>



</details>

<a name="0x3_sui_system_request_remove_validator_candidate"></a>

## Function `request_remove_validator_candidate`

Called by a validator candidate to remove themselves from the candidacy. After this call
their staking pool becomes deactivate.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_remove_validator_candidate">request_remove_validator_candidate</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_remove_validator_candidate">request_remove_validator_candidate</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator_candidate">sui_system_state_inner::request_remove_validator_candidate</a>(self, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_add_validator"></a>

## Function `request_add_validator`

Called by a validator candidate to add themselves to the active validator set beginning next epoch.
Aborts if the validator is a duplicate with one of the pending or active validators, or if the amount of
stake the validator has doesn't meet the min threshold, or if the number of new validators for the next
epoch has already reached the maximum.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_validator">request_add_validator</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_validator">request_add_validator</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator">sui_system_state_inner::request_add_validator</a>(self, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_remove_validator"></a>

## Function `request_remove_validator`

A validator can call this function to request a removal in the next epoch.
We use the sender of <code>ctx</code> to look up the validator
(i.e. sender must match the sui_address in the validator).
At the end of the epoch, the <code><a href="validator.md#0x3_validator">validator</a></code> object will be returned to the sui_address
of the validator.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_remove_validator">request_remove_validator</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_remove_validator">request_remove_validator</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator">sui_system_state_inner::request_remove_validator</a>(self, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_set_gas_price"></a>

## Function `request_set_gas_price`

A validator can call this entry function to submit a new gas price quote, to be
used for the reference gas price calculation at the end of the epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_set_gas_price">request_set_gas_price</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, new_gas_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_set_gas_price">request_set_gas_price</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_gas_price">sui_system_state_inner::request_set_gas_price</a>(self, cap, new_gas_price)
}
</code></pre>



</details>

<a name="0x3_sui_system_set_candidate_validator_gas_price"></a>

## Function `set_candidate_validator_gas_price`

This entry function is used to set new gas price for candidate validators


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_set_candidate_validator_gas_price">set_candidate_validator_gas_price</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, new_gas_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_set_candidate_validator_gas_price">set_candidate_validator_gas_price</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_gas_price">sui_system_state_inner::set_candidate_validator_gas_price</a>(self, cap, new_gas_price)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_set_commission_rate"></a>

## Function `request_set_commission_rate`

A validator can call this entry function to set a new commission rate, updated at the end of
the epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_set_commission_rate">request_set_commission_rate</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_commission_rate: u64, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_set_commission_rate">request_set_commission_rate</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    new_commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_commission_rate">sui_system_state_inner::request_set_commission_rate</a>(self, new_commission_rate, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_set_candidate_validator_commission_rate"></a>

## Function `set_candidate_validator_commission_rate`

This entry function is used to set new commission rate for candidate validators


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_set_candidate_validator_commission_rate">set_candidate_validator_commission_rate</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_commission_rate: u64, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_set_candidate_validator_commission_rate">set_candidate_validator_commission_rate</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    new_commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_commission_rate">sui_system_state_inner::set_candidate_validator_commission_rate</a>(self, new_commission_rate, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_add_stake"></a>

## Function `request_add_stake`

Add stake to a validator's staking pool.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_stake">request_add_stake</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, stake: <a href="../../../build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../../../build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_stake">request_add_stake</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    stake: Coin&lt;SUI&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake">sui_system_state_inner::request_add_stake</a>(self, stake, validator_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_add_stake_mul_coin"></a>

## Function `request_add_stake_mul_coin`

Add stake to a validator's staking pool using multiple coins.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_stake_mul_coin">request_add_stake_mul_coin</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, stakes: <a href="">vector</a>&lt;<a href="../../../build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../../../build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;&gt;, stake_amount: <a href="_Option">option::Option</a>&lt;u64&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_stake_mul_coin">request_add_stake_mul_coin</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    stakes: <a href="">vector</a>&lt;Coin&lt;SUI&gt;&gt;,
    stake_amount: <a href="_Option">option::Option</a>&lt;u64&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake_mul_coin">sui_system_state_inner::request_add_stake_mul_coin</a>(self, stakes, stake_amount, validator_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_withdraw_stake"></a>

## Function `request_withdraw_stake`

Withdraw some portion of a stake from a validator's staking pool.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_withdraw_stake">request_withdraw_stake</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, staked_sui: <a href="staking_pool.md#0x3_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_withdraw_stake">request_withdraw_stake</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    staked_sui: StakedSui,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_withdraw_stake">sui_system_state_inner::request_withdraw_stake</a>(self, staked_sui, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_report_validator"></a>

## Function `report_validator`

Report a validator as a bad or non-performant actor in the system.
Succeeds if all the following are satisfied:
1. both the reporter in <code>cap</code> and the input <code>reportee_addr</code> are active validators.
2. reporter and reportee not the same address.
3. the cap object is still valid.
This function is idempotent.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_report_validator">report_validator</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, reportee_addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_report_validator">report_validator</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: <b>address</b>,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator">sui_system_state_inner::report_validator</a>(self, cap, reportee_addr)
}
</code></pre>



</details>

<a name="0x3_sui_system_undo_report_validator"></a>

## Function `undo_report_validator`

Undo a <code>report_validator</code> action. Aborts if
1. the reportee is not a currently active validator or
2. the sender has not previously reported the <code>reportee_addr</code>, or
3. the cap is not valid


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_undo_report_validator">undo_report_validator</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, reportee_addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_undo_report_validator">undo_report_validator</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: <b>address</b>,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator">sui_system_state_inner::undo_report_validator</a>(self, cap, reportee_addr)
}
</code></pre>



</details>

<a name="0x3_sui_system_rotate_operation_cap"></a>

## Function `rotate_operation_cap`

Create a new <code>UnverifiedValidatorOperationCap</code>, transfer it to the
validator and registers it. The original object is thus revoked.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_rotate_operation_cap">rotate_operation_cap</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_rotate_operation_cap">rotate_operation_cap</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_rotate_operation_cap">sui_system_state_inner::rotate_operation_cap</a>(self, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_name"></a>

## Function `update_validator_name`

Update a validator's name.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_name">update_validator_name</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, name: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_name">update_validator_name</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    name: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_name">sui_system_state_inner::update_validator_name</a>(self, name, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_description"></a>

## Function `update_validator_description`

Update a validator's description


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_description">update_validator_description</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, description: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_description">update_validator_description</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    description: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_description">sui_system_state_inner::update_validator_description</a>(self, description, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_image_url"></a>

## Function `update_validator_image_url`

Update a validator's image url


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_image_url">update_validator_image_url</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, image_url: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_image_url">update_validator_image_url</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    image_url: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_image_url">sui_system_state_inner::update_validator_image_url</a>(self, image_url, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_project_url"></a>

## Function `update_validator_project_url`

Update a validator's project url


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_project_url">update_validator_project_url</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, project_url: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_project_url">update_validator_project_url</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    project_url: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_project_url">sui_system_state_inner::update_validator_project_url</a>(self, project_url, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_network_address"></a>

## Function `update_validator_next_epoch_network_address`

Update a validator's network address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_network_address">update_validator_next_epoch_network_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_network_address">update_validator_next_epoch_network_address</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    network_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_address">sui_system_state_inner::update_validator_next_epoch_network_address</a>(self, network_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_network_address"></a>

## Function `update_candidate_validator_network_address`

Update candidate validator's network address.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_network_address">update_candidate_validator_network_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_network_address">update_candidate_validator_network_address</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    network_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_address">sui_system_state_inner::update_candidate_validator_network_address</a>(self, network_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_p2p_address"></a>

## Function `update_validator_next_epoch_p2p_address`

Update a validator's p2p address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_p2p_address">update_validator_next_epoch_p2p_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, p2p_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_p2p_address">update_validator_next_epoch_p2p_address</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    p2p_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_p2p_address">sui_system_state_inner::update_validator_next_epoch_p2p_address</a>(self, p2p_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_p2p_address"></a>

## Function `update_candidate_validator_p2p_address`

Update candidate validator's p2p address.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_p2p_address">update_candidate_validator_p2p_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, p2p_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_p2p_address">update_candidate_validator_p2p_address</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    p2p_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_p2p_address">sui_system_state_inner::update_candidate_validator_p2p_address</a>(self, p2p_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_primary_address"></a>

## Function `update_validator_next_epoch_primary_address`

Update a validator's narwhal primary address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_primary_address">update_validator_next_epoch_primary_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, primary_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_primary_address">update_validator_next_epoch_primary_address</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    primary_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_primary_address">sui_system_state_inner::update_validator_next_epoch_primary_address</a>(self, primary_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_primary_address"></a>

## Function `update_candidate_validator_primary_address`

Update candidate validator's narwhal primary address.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_primary_address">update_candidate_validator_primary_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, primary_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_primary_address">update_candidate_validator_primary_address</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    primary_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_primary_address">sui_system_state_inner::update_candidate_validator_primary_address</a>(self, primary_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_worker_address"></a>

## Function `update_validator_next_epoch_worker_address`

Update a validator's narwhal worker address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_worker_address">update_validator_next_epoch_worker_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_worker_address">update_validator_next_epoch_worker_address</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    worker_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_address">sui_system_state_inner::update_validator_next_epoch_worker_address</a>(self, worker_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_worker_address"></a>

## Function `update_candidate_validator_worker_address`

Update candidate validator's narwhal worker address.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_worker_address">update_candidate_validator_worker_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_worker_address">update_candidate_validator_worker_address</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    worker_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_address">sui_system_state_inner::update_candidate_validator_worker_address</a>(self, worker_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_protocol_pubkey"></a>

## Function `update_validator_next_epoch_protocol_pubkey`

Update a validator's public key of protocol key and proof of possession.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_protocol_pubkey">update_validator_next_epoch_protocol_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, protocol_pubkey: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_protocol_pubkey">update_validator_next_epoch_protocol_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    protocol_pubkey: <a href="">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey">sui_system_state_inner::update_validator_next_epoch_protocol_pubkey</a>(self, protocol_pubkey, proof_of_possession, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_protocol_pubkey"></a>

## Function `update_candidate_validator_protocol_pubkey`

Update candidate validator's public key of protocol key and proof of possession.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_protocol_pubkey">update_candidate_validator_protocol_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, protocol_pubkey: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_protocol_pubkey">update_candidate_validator_protocol_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    protocol_pubkey: <a href="">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_protocol_pubkey">sui_system_state_inner::update_candidate_validator_protocol_pubkey</a>(self, protocol_pubkey, proof_of_possession, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_worker_pubkey"></a>

## Function `update_validator_next_epoch_worker_pubkey`

Update a validator's public key of worker key.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_worker_pubkey">update_validator_next_epoch_worker_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_worker_pubkey">update_validator_next_epoch_worker_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    worker_pubkey: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_pubkey">sui_system_state_inner::update_validator_next_epoch_worker_pubkey</a>(self, worker_pubkey, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_worker_pubkey"></a>

## Function `update_candidate_validator_worker_pubkey`

Update candidate validator's public key of worker key.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_worker_pubkey">update_candidate_validator_worker_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_worker_pubkey">update_candidate_validator_worker_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    worker_pubkey: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_pubkey">sui_system_state_inner::update_candidate_validator_worker_pubkey</a>(self, worker_pubkey, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_network_pubkey"></a>

## Function `update_validator_next_epoch_network_pubkey`

Update a validator's public key of network key.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_network_pubkey">update_validator_next_epoch_network_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_network_pubkey">update_validator_next_epoch_network_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    network_pubkey: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_pubkey">sui_system_state_inner::update_validator_next_epoch_network_pubkey</a>(self, network_pubkey, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_network_pubkey"></a>

## Function `update_candidate_validator_network_pubkey`

Update candidate validator's public key of network key.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_network_pubkey">update_candidate_validator_network_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_network_pubkey">update_candidate_validator_network_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    network_pubkey: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_pubkey">sui_system_state_inner::update_candidate_validator_network_pubkey</a>(self, network_pubkey, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_advance_epoch"></a>

## Function `advance_epoch`

This function should be called at the end of an epoch, and advances the system to the next epoch.
It does the following things:
1. Add storage charge to the storage fund.
2. Burn the storage rebates from the storage fund. These are already refunded to transaction sender's
gas coins.
3. Distribute computation charge to validator stake.
4. Update all validators.


<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_advance_epoch">advance_epoch</a>(storage_reward: <a href="../../../build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../../build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, computation_reward: <a href="../../../build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../../build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_epoch: u64, next_protocol_version: u64, storage_rebate: u64, non_refundable_storage_fee: u64, storage_fund_reinvest_rate: u64, reward_slashing_rate: u64, epoch_start_timestamp_ms: u64, ctx: &<b>mut</b> <a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../../build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../../build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_advance_epoch">advance_epoch</a>(
    storage_reward: Balance&lt;SUI&gt;,
    computation_reward: Balance&lt;SUI&gt;,
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    new_epoch: u64,
    next_protocol_version: u64,
    storage_rebate: u64,
    non_refundable_storage_fee: u64,
    storage_fund_reinvest_rate: u64, // share of storage fund's rewards that's reinvested
                                     // into storage fund, in basis point.
    reward_slashing_rate: u64, // how much rewards are slashed <b>to</b> punish a <a href="validator.md#0x3_validator">validator</a>, in bps.
    epoch_start_timestamp_ms: u64, // Timestamp of the epoch start
    ctx: &<b>mut</b> TxContext,
) : Balance&lt;SUI&gt; {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    // Validator will make a special system call <b>with</b> sender set <b>as</b> 0x0.
    <b>assert</b>!(<a href="../../../build/Sui/docs/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="sui_system.md#0x3_sui_system_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> storage_rebate = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch">sui_system_state_inner::advance_epoch</a>(
        self,
        new_epoch,
        next_protocol_version,
        storage_reward,
        computation_reward,
        storage_rebate,
        non_refundable_storage_fee,
        storage_fund_reinvest_rate,
        reward_slashing_rate,
        epoch_start_timestamp_ms,
        ctx,
    );

    storage_rebate
}
</code></pre>



</details>

<a name="0x3_sui_system_load_system_state"></a>

## Function `load_system_state`



<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): &SuiSystemStateInnerV2 {
    <a href="sui_system.md#0x3_sui_system_load_inner_maybe_upgrade">load_inner_maybe_upgrade</a>(self)
}
</code></pre>



</details>

<a name="0x3_sui_system_load_system_state_mut"></a>

## Function `load_system_state_mut`



<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): &<b>mut</b> SuiSystemStateInnerV2 {
    <a href="sui_system.md#0x3_sui_system_load_inner_maybe_upgrade">load_inner_maybe_upgrade</a>(self)
}
</code></pre>



</details>

<a name="0x3_sui_system_load_inner_maybe_upgrade"></a>

## Function `load_inner_maybe_upgrade`



<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_load_inner_maybe_upgrade">load_inner_maybe_upgrade</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_load_inner_maybe_upgrade">load_inner_maybe_upgrade</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): &<b>mut</b> SuiSystemStateInnerV2 {
    <b>if</b> (self.version == 1) {
      <b>let</b> v1 = <a href="../../../build/Sui/docs/dynamic_field.md#0x2_dynamic_field_remove">dynamic_field::remove</a>(&<b>mut</b> self.id, self.version);
      <b>let</b> v2 = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_v1_to_v2">sui_system_state_inner::v1_to_v2</a>(v1);
      self.version = 2;
      <a href="../../../build/Sui/docs/dynamic_field.md#0x2_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> self.id, self.version, v2);
    };

    <b>let</b> inner = <a href="../../../build/Sui/docs/dynamic_field.md#0x2_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>(&<b>mut</b> self.id, self.version);
    <b>assert</b>!(<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_system_state_version">sui_system_state_inner::system_state_version</a>(inner) == self.version, <a href="sui_system.md#0x3_sui_system_EWrongInnerVersion">EWrongInnerVersion</a>);
    inner
}
</code></pre>



</details>
