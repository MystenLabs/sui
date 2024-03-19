
<a name="0x3_sui_system"></a>

# Module `0x3::sui_system`



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
-  [Function `request_add_stake_non_entry`](#0x3_sui_system_request_add_stake_non_entry)
-  [Function `request_add_stake_mul_coin`](#0x3_sui_system_request_add_stake_mul_coin)
-  [Function `request_withdraw_stake`](#0x3_sui_system_request_withdraw_stake)
-  [Function `request_withdraw_stake_non_entry`](#0x3_sui_system_request_withdraw_stake_non_entry)
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
-  [Function `pool_exchange_rates`](#0x3_sui_system_pool_exchange_rates)
-  [Function `active_validator_addresses`](#0x3_sui_system_active_validator_addresses)
-  [Function `advance_epoch`](#0x3_sui_system_advance_epoch)
-  [Function `load_system_state`](#0x3_sui_system_load_system_state)
-  [Function `load_system_state_mut`](#0x3_sui_system_load_system_state_mut)
-  [Function `load_inner_maybe_upgrade`](#0x3_sui_system_load_inner_maybe_upgrade)
-  [Function `validator_voting_power`](#0x3_sui_system_validator_voting_power)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../../dependencies/sui-framework/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../../dependencies/sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="../../dependencies/sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../dependencies/sui-framework/sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="../../dependencies/sui-framework/table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../../dependencies/sui-system/stake_subsidy.md#0x3_stake_subsidy">0x3::stake_subsidy</a>;
<b>use</b> <a href="../../dependencies/sui-system/staking_pool.md#0x3_staking_pool">0x3::staking_pool</a>;
<b>use</b> <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner">0x3::sui_system_state_inner</a>;
<b>use</b> <a href="../../dependencies/sui-system/validator.md#0x3_validator">0x3::validator</a>;
<b>use</b> <a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap">0x3::validator_cap</a>;
</code></pre>



<a name="0x3_sui_system_SuiSystemState"></a>

## Resource `SuiSystemState`



<pre><code><b>struct</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a> <b>has</b> key
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
<code>version: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x3_sui_system_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="0x3_sui_system_EWrongInnerVersion"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_EWrongInnerVersion">EWrongInnerVersion</a>: u64 = 1;
</code></pre>



<a name="0x3_sui_system_create"></a>

## Function `create`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_create">create</a>(id: <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>, validators: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, <a href="../../dependencies/sui-system/storage_fund.md#0x3_storage_fund">storage_fund</a>: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, protocol_version: u64, epoch_start_timestamp_ms: u64, parameters: <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">sui_system_state_inner::SystemParameters</a>, <a href="../../dependencies/sui-system/stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>: <a href="../../dependencies/sui-system/stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">stake_subsidy::StakeSubsidy</a>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_create">create</a>(
    id: UID,
    validators: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;,
    <a href="../../dependencies/sui-system/storage_fund.md#0x3_storage_fund">storage_fund</a>: Balance&lt;SUI&gt;,
    protocol_version: u64,
    epoch_start_timestamp_ms: u64,
    parameters: SystemParameters,
    <a href="../../dependencies/sui-system/stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>: StakeSubsidy,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> system_state = <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_create">sui_system_state_inner::create</a>(
        validators,
        <a href="../../dependencies/sui-system/storage_fund.md#0x3_storage_fund">storage_fund</a>,
        protocol_version,
        epoch_start_timestamp_ms,
        parameters,
        <a href="../../dependencies/sui-system/stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>,
        ctx,
    );
    <b>let</b> version = <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_genesis_system_state_version">sui_system_state_inner::genesis_system_state_version</a>();
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a> {
        id,
        version,
    };
    <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> self.id, version, system_state);
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_share_object">transfer::share_object</a>(self);
}
</code></pre>



</details>

<a name="0x3_sui_system_request_add_validator_candidate"></a>

## Function `request_add_validator_candidate`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_add_validator_candidate">request_add_validator_candidate</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, pubkey_bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, network_pubkey_bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, worker_pubkey_bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof_of_possession: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, name: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, description: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, image_url: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, project_url: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, net_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, p2p_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, primary_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, worker_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, gas_price: u64, commission_rate: u64, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_add_validator_candidate">request_add_validator_candidate</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    pubkey_bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    network_pubkey_bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    worker_pubkey_bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    name: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    description: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    image_url: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    project_url: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    net_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    p2p_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    primary_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    worker_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    gas_price: u64,
    commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator_candidate">sui_system_state_inner::request_add_validator_candidate</a>(
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



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_remove_validator_candidate">request_remove_validator_candidate</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_remove_validator_candidate">request_remove_validator_candidate</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator_candidate">sui_system_state_inner::request_remove_validator_candidate</a>(self, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_add_validator"></a>

## Function `request_add_validator`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_add_validator">request_add_validator</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_add_validator">request_add_validator</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator">sui_system_state_inner::request_add_validator</a>(self, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_remove_validator"></a>

## Function `request_remove_validator`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_remove_validator">request_remove_validator</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_remove_validator">request_remove_validator</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator">sui_system_state_inner::request_remove_validator</a>(self, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_set_gas_price"></a>

## Function `request_set_gas_price`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_set_gas_price">request_set_gas_price</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, cap: &<a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, new_gas_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_set_gas_price">request_set_gas_price</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_gas_price">sui_system_state_inner::request_set_gas_price</a>(self, cap, new_gas_price)
}
</code></pre>



</details>

<a name="0x3_sui_system_set_candidate_validator_gas_price"></a>

## Function `set_candidate_validator_gas_price`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_set_candidate_validator_gas_price">set_candidate_validator_gas_price</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, cap: &<a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, new_gas_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_set_candidate_validator_gas_price">set_candidate_validator_gas_price</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_gas_price">sui_system_state_inner::set_candidate_validator_gas_price</a>(self, cap, new_gas_price)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_set_commission_rate"></a>

## Function `request_set_commission_rate`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_set_commission_rate">request_set_commission_rate</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_commission_rate: u64, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_set_commission_rate">request_set_commission_rate</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    new_commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_commission_rate">sui_system_state_inner::request_set_commission_rate</a>(self, new_commission_rate, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_set_candidate_validator_commission_rate"></a>

## Function `set_candidate_validator_commission_rate`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_set_candidate_validator_commission_rate">set_candidate_validator_commission_rate</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_commission_rate: u64, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_set_candidate_validator_commission_rate">set_candidate_validator_commission_rate</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    new_commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_commission_rate">sui_system_state_inner::set_candidate_validator_commission_rate</a>(self, new_commission_rate, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_add_stake"></a>

## Function `request_add_stake`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_add_stake">request_add_stake</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, stake: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_add_stake">request_add_stake</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    stake: Coin&lt;SUI&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> staked_sui = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_add_stake_non_entry">request_add_stake_non_entry</a>(wrapper, stake, validator_address, ctx);
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(staked_sui, <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx));
}
</code></pre>



</details>

<a name="0x3_sui_system_request_add_stake_non_entry"></a>

## Function `request_add_stake_non_entry`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_add_stake_non_entry">request_add_stake_non_entry</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, stake: <a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-system/staking_pool.md#0x3_staking_pool_StakedSui">staking_pool::StakedSui</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_add_stake_non_entry">request_add_stake_non_entry</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    stake: Coin&lt;SUI&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
): StakedSui {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake">sui_system_state_inner::request_add_stake</a>(self, stake, validator_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_request_add_stake_mul_coin"></a>

## Function `request_add_stake_mul_coin`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_add_stake_mul_coin">request_add_stake_mul_coin</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, stakes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;&gt;, stake_amount: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_add_stake_mul_coin">request_add_stake_mul_coin</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    stakes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Coin&lt;SUI&gt;&gt;,
    stake_amount: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <b>let</b> staked_sui = <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake_mul_coin">sui_system_state_inner::request_add_stake_mul_coin</a>(self, stakes, stake_amount, validator_address, ctx);
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(staked_sui, <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx));
}
</code></pre>



</details>

<a name="0x3_sui_system_request_withdraw_stake"></a>

## Function `request_withdraw_stake`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_withdraw_stake">request_withdraw_stake</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, staked_sui: <a href="../../dependencies/sui-system/staking_pool.md#0x3_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_withdraw_stake">request_withdraw_stake</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    staked_sui: StakedSui,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> withdrawn_stake = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_withdraw_stake_non_entry">request_withdraw_stake_non_entry</a>(wrapper, staked_sui, ctx);
    <a href="../../dependencies/sui-framework/transfer.md#0x2_transfer_public_transfer">transfer::public_transfer</a>(<a href="../../dependencies/sui-framework/coin.md#0x2_coin_from_balance">coin::from_balance</a>(withdrawn_stake, ctx), <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx));
}
</code></pre>



</details>

<a name="0x3_sui_system_request_withdraw_stake_non_entry"></a>

## Function `request_withdraw_stake_non_entry`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_withdraw_stake_non_entry">request_withdraw_stake_non_entry</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, staked_sui: <a href="../../dependencies/sui-system/staking_pool.md#0x3_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_request_withdraw_stake_non_entry">request_withdraw_stake_non_entry</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    staked_sui: StakedSui,
    ctx: &<b>mut</b> TxContext,
) : Balance&lt;SUI&gt; {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_request_withdraw_stake">sui_system_state_inner::request_withdraw_stake</a>(self, staked_sui, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_report_validator"></a>

## Function `report_validator`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_report_validator">report_validator</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, cap: &<a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, reportee_addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_report_validator">report_validator</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: <b>address</b>,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator">sui_system_state_inner::report_validator</a>(self, cap, reportee_addr)
}
</code></pre>



</details>

<a name="0x3_sui_system_undo_report_validator"></a>

## Function `undo_report_validator`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_undo_report_validator">undo_report_validator</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, cap: &<a href="../../dependencies/sui-system/validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, reportee_addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_undo_report_validator">undo_report_validator</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: <b>address</b>,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator">sui_system_state_inner::undo_report_validator</a>(self, cap, reportee_addr)
}
</code></pre>



</details>

<a name="0x3_sui_system_rotate_operation_cap"></a>

## Function `rotate_operation_cap`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_rotate_operation_cap">rotate_operation_cap</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_rotate_operation_cap">rotate_operation_cap</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_rotate_operation_cap">sui_system_state_inner::rotate_operation_cap</a>(self, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_name"></a>

## Function `update_validator_name`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_name">update_validator_name</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, name: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_name">update_validator_name</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    name: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_name">sui_system_state_inner::update_validator_name</a>(self, name, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_description"></a>

## Function `update_validator_description`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_description">update_validator_description</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, description: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_description">update_validator_description</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    description: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_description">sui_system_state_inner::update_validator_description</a>(self, description, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_image_url"></a>

## Function `update_validator_image_url`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_image_url">update_validator_image_url</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, image_url: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_image_url">update_validator_image_url</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    image_url: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_image_url">sui_system_state_inner::update_validator_image_url</a>(self, image_url, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_project_url"></a>

## Function `update_validator_project_url`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_project_url">update_validator_project_url</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, project_url: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_project_url">update_validator_project_url</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    project_url: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_project_url">sui_system_state_inner::update_validator_project_url</a>(self, project_url, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_network_address"></a>

## Function `update_validator_next_epoch_network_address`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_network_address">update_validator_next_epoch_network_address</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_network_address">update_validator_next_epoch_network_address</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    network_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_address">sui_system_state_inner::update_validator_next_epoch_network_address</a>(self, network_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_network_address"></a>

## Function `update_candidate_validator_network_address`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_network_address">update_candidate_validator_network_address</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_network_address">update_candidate_validator_network_address</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    network_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_address">sui_system_state_inner::update_candidate_validator_network_address</a>(self, network_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_p2p_address"></a>

## Function `update_validator_next_epoch_p2p_address`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_p2p_address">update_validator_next_epoch_p2p_address</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, p2p_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_p2p_address">update_validator_next_epoch_p2p_address</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    p2p_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_p2p_address">sui_system_state_inner::update_validator_next_epoch_p2p_address</a>(self, p2p_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_p2p_address"></a>

## Function `update_candidate_validator_p2p_address`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_p2p_address">update_candidate_validator_p2p_address</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, p2p_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_p2p_address">update_candidate_validator_p2p_address</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    p2p_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_p2p_address">sui_system_state_inner::update_candidate_validator_p2p_address</a>(self, p2p_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_primary_address"></a>

## Function `update_validator_next_epoch_primary_address`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_primary_address">update_validator_next_epoch_primary_address</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, primary_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_primary_address">update_validator_next_epoch_primary_address</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    primary_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_primary_address">sui_system_state_inner::update_validator_next_epoch_primary_address</a>(self, primary_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_primary_address"></a>

## Function `update_candidate_validator_primary_address`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_primary_address">update_candidate_validator_primary_address</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, primary_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_primary_address">update_candidate_validator_primary_address</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    primary_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_primary_address">sui_system_state_inner::update_candidate_validator_primary_address</a>(self, primary_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_worker_address"></a>

## Function `update_validator_next_epoch_worker_address`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_worker_address">update_validator_next_epoch_worker_address</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_worker_address">update_validator_next_epoch_worker_address</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    worker_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_address">sui_system_state_inner::update_validator_next_epoch_worker_address</a>(self, worker_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_worker_address"></a>

## Function `update_candidate_validator_worker_address`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_worker_address">update_candidate_validator_worker_address</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_worker_address">update_candidate_validator_worker_address</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    worker_address: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_address">sui_system_state_inner::update_candidate_validator_worker_address</a>(self, worker_address, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_protocol_pubkey"></a>

## Function `update_validator_next_epoch_protocol_pubkey`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_protocol_pubkey">update_validator_next_epoch_protocol_pubkey</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, protocol_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof_of_possession: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_protocol_pubkey">update_validator_next_epoch_protocol_pubkey</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    protocol_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey">sui_system_state_inner::update_validator_next_epoch_protocol_pubkey</a>(self, protocol_pubkey, proof_of_possession, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_protocol_pubkey"></a>

## Function `update_candidate_validator_protocol_pubkey`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_protocol_pubkey">update_candidate_validator_protocol_pubkey</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, protocol_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, proof_of_possession: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_protocol_pubkey">update_candidate_validator_protocol_pubkey</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    protocol_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_protocol_pubkey">sui_system_state_inner::update_candidate_validator_protocol_pubkey</a>(self, protocol_pubkey, proof_of_possession, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_worker_pubkey"></a>

## Function `update_validator_next_epoch_worker_pubkey`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_worker_pubkey">update_validator_next_epoch_worker_pubkey</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_worker_pubkey">update_validator_next_epoch_worker_pubkey</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    worker_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_pubkey">sui_system_state_inner::update_validator_next_epoch_worker_pubkey</a>(self, worker_pubkey, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_worker_pubkey"></a>

## Function `update_candidate_validator_worker_pubkey`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_worker_pubkey">update_candidate_validator_worker_pubkey</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_worker_pubkey">update_candidate_validator_worker_pubkey</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    worker_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_pubkey">sui_system_state_inner::update_candidate_validator_worker_pubkey</a>(self, worker_pubkey, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_validator_next_epoch_network_pubkey"></a>

## Function `update_validator_next_epoch_network_pubkey`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_network_pubkey">update_validator_next_epoch_network_pubkey</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_validator_next_epoch_network_pubkey">update_validator_next_epoch_network_pubkey</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    network_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_pubkey">sui_system_state_inner::update_validator_next_epoch_network_pubkey</a>(self, network_pubkey, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_update_candidate_validator_network_pubkey"></a>

## Function `update_candidate_validator_network_pubkey`



<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_network_pubkey">update_candidate_validator_network_pubkey</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;, ctx: &<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_update_candidate_validator_network_pubkey">update_candidate_validator_network_pubkey</a>(
    self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    network_pubkey: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_pubkey">sui_system_state_inner::update_candidate_validator_network_pubkey</a>(self, network_pubkey, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_pool_exchange_rates"></a>

## Function `pool_exchange_rates`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_pool_exchange_rates">pool_exchange_rates</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, pool_id: &<a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>): &<a href="../../dependencies/sui-framework/table.md#0x2_table_Table">table::Table</a>&lt;u64, <a href="../../dependencies/sui-system/staking_pool.md#0x3_staking_pool_PoolTokenExchangeRate">staking_pool::PoolTokenExchangeRate</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_pool_exchange_rates">pool_exchange_rates</a>(
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    pool_id: &ID
): &Table&lt;u64, PoolTokenExchangeRate&gt;  {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_pool_exchange_rates">sui_system_state_inner::pool_exchange_rates</a>(self, pool_id)
}
</code></pre>



</details>

<a name="0x3_sui_system_active_validator_addresses"></a>

## Function `active_validator_addresses`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_active_validator_addresses">active_validator_addresses</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_active_validator_addresses">active_validator_addresses</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<b>address</b>&gt; {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_active_validator_addresses">sui_system_state_inner::active_validator_addresses</a>(self)
}
</code></pre>



</details>

<a name="0x3_sui_system_advance_epoch"></a>

## Function `advance_epoch`



<pre><code><b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_advance_epoch">advance_epoch</a>(storage_reward: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, computation_reward: <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_epoch: u64, next_protocol_version: u64, storage_rebate: u64, non_refundable_storage_fee: u64, storage_fund_reinvest_rate: u64, reward_slashing_rate: u64, epoch_start_timestamp_ms: u64, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../dependencies/sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_advance_epoch">advance_epoch</a>(
    storage_reward: Balance&lt;SUI&gt;,
    computation_reward: Balance&lt;SUI&gt;,
    wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    new_epoch: u64,
    next_protocol_version: u64,
    storage_rebate: u64,
    non_refundable_storage_fee: u64,
    storage_fund_reinvest_rate: u64, // share of storage fund's rewards that's reinvested
                                     // into storage fund, in basis point.
    reward_slashing_rate: u64, // how much rewards are slashed <b>to</b> punish a <a href="../../dependencies/sui-system/validator.md#0x3_validator">validator</a>, in bps.
    epoch_start_timestamp_ms: u64, // Timestamp of the epoch start
    ctx: &<b>mut</b> TxContext,
) : Balance&lt;SUI&gt; {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    // Validator will make a special system call <b>with</b> sender set <b>as</b> 0x0.
    <b>assert</b>!(<a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_ENotSystemAddress">ENotSystemAddress</a>);
    <b>let</b> storage_rebate = <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch">sui_system_state_inner::advance_epoch</a>(
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



<pre><code><b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): &SuiSystemStateInnerV2 {
    <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_inner_maybe_upgrade">load_inner_maybe_upgrade</a>(self)
}
</code></pre>



</details>

<a name="0x3_sui_system_load_system_state_mut"></a>

## Function `load_system_state_mut`



<pre><code><b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<b>mut</b> <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): &<b>mut</b> SuiSystemStateInnerV2 {
    <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_inner_maybe_upgrade">load_inner_maybe_upgrade</a>(self)
}
</code></pre>



</details>

<a name="0x3_sui_system_load_inner_maybe_upgrade"></a>

## Function `load_inner_maybe_upgrade`



<pre><code><b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_inner_maybe_upgrade">load_inner_maybe_upgrade</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<b>mut</b> <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInnerV2">sui_system_state_inner::SuiSystemStateInnerV2</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_inner_maybe_upgrade">load_inner_maybe_upgrade</a>(self: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): &<b>mut</b> SuiSystemStateInnerV2 {
    <b>if</b> (self.version == 1) {
      <b>let</b> v1 = <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field_remove">dynamic_field::remove</a>(&<b>mut</b> self.id, self.version);
      <b>let</b> v2 = <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_v1_to_v2">sui_system_state_inner::v1_to_v2</a>(v1);
      self.version = 2;
      <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> self.id, self.version, v2);
    };

    <b>let</b> inner = <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>(&<b>mut</b> self.id, self.version);
    <b>assert</b>!(<a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_system_state_version">sui_system_state_inner::system_state_version</a>(inner) == self.version, <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_EWrongInnerVersion">EWrongInnerVersion</a>);
    inner
}
</code></pre>



</details>

<a name="0x3_sui_system_validator_voting_power"></a>

## Function `validator_voting_power`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_validator_voting_power">validator_voting_power</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, validator_addr: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_validator_voting_power">validator_voting_power</a>(wrapper: &<b>mut</b> <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>, validator_addr: <b>address</b>): u64 {
    <b>let</b> self = <a href="../../dependencies/sui-system/sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(wrapper);
    <a href="../../dependencies/sui-system/sui_system_state_inner.md#0x3_sui_system_state_inner_validator_voting_power">sui_system_state_inner::validator_voting_power</a>(self, validator_addr)
}
</code></pre>



</details>
