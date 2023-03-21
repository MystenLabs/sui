
<a name="0x3_sui_system"></a>

# Module `0x3::sui_system`



-  [Resource `SuiSystemState`](#0x3_sui_system_SuiSystemState)
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
-  [Function `advance_epoch_safe_mode`](#0x3_sui_system_advance_epoch_safe_mode)
-  [Function `epoch`](#0x3_sui_system_epoch)
-  [Function `epoch_start_timestamp_ms`](#0x3_sui_system_epoch_start_timestamp_ms)
-  [Function `validator_stake_amount`](#0x3_sui_system_validator_stake_amount)
-  [Function `validator_staking_pool_id`](#0x3_sui_system_validator_staking_pool_id)
-  [Function `validator_staking_pool_mappings`](#0x3_sui_system_validator_staking_pool_mappings)
-  [Function `get_reporters_of`](#0x3_sui_system_get_reporters_of)
-  [Function `load_system_state`](#0x3_sui_system_load_system_state)
-  [Function `load_system_state_mut`](#0x3_sui_system_load_system_state_mut)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x2::balance</a>;
<b>use</b> <a href="">0x2::coin</a>;
<b>use</b> <a href="">0x2::dynamic_field</a>;
<b>use</b> <a href="">0x2::object</a>;
<b>use</b> <a href="">0x2::sui</a>;
<b>use</b> <a href="">0x2::table</a>;
<b>use</b> <a href="">0x2::transfer</a>;
<b>use</b> <a href="">0x2::tx_context</a>;
<b>use</b> <a href="">0x2::vec_set</a>;
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
<code>id: <a href="_UID">object::UID</a></code>
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

<a name="0x3_sui_system_create"></a>

## Function `create`

Create a new SuiSystemState object and make it shared.
This function will be called only once in genesis.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system.md#0x3_sui_system_create">create</a>(id: <a href="_UID">object::UID</a>, validators: <a href="">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, stake_subsidy_fund: <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, storage_fund: <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, protocol_version: u64, governance_start_epoch: u64, epoch_start_timestamp_ms: u64, epoch_duration_ms: u64, initial_stake_subsidy_distribution_amount: u64, stake_subsidy_period_length: u64, stake_subsidy_decrease_rate: u16, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system.md#0x3_sui_system_create">create</a>(
    id: UID,
    validators: <a href="">vector</a>&lt;Validator&gt;,
    stake_subsidy_fund: Balance&lt;SUI&gt;,
    storage_fund: Balance&lt;SUI&gt;,
    protocol_version: u64,
    governance_start_epoch: u64,
    epoch_start_timestamp_ms: u64,
    epoch_duration_ms: u64,
    initial_stake_subsidy_distribution_amount: u64,
    stake_subsidy_period_length: u64,
    stake_subsidy_decrease_rate: u16,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> system_state = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_create">sui_system_state_inner::create</a>(
        validators,
        stake_subsidy_fund,
        storage_fund,
        protocol_version,
        governance_start_epoch,
        epoch_start_timestamp_ms,
        epoch_duration_ms,
        initial_stake_subsidy_distribution_amount,
        stake_subsidy_period_length,
        stake_subsidy_decrease_rate,
        ctx,
    );
    <b>let</b> version = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_system_state_version">sui_system_state_inner::system_state_version</a>(&system_state);
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a> {
        id,
        version,
    };
    <a href="_add">dynamic_field::add</a>(&<b>mut</b> self.id, version, system_state);
    <a href="_share_object">transfer::share_object</a>(self);
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_validator_candidate">request_add_validator_candidate</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, network_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, worker_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, name: <a href="">vector</a>&lt;u8&gt;, description: <a href="">vector</a>&lt;u8&gt;, image_url: <a href="">vector</a>&lt;u8&gt;, project_url: <a href="">vector</a>&lt;u8&gt;, net_address: <a href="">vector</a>&lt;u8&gt;, p2p_address: <a href="">vector</a>&lt;u8&gt;, primary_address: <a href="">vector</a>&lt;u8&gt;, worker_address: <a href="">vector</a>&lt;u8&gt;, gas_price: u64, commission_rate: u64, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_remove_validator_candidate">request_remove_validator_candidate</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_validator">request_add_validator</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_remove_validator">request_remove_validator</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_set_commission_rate">request_set_commission_rate</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_commission_rate: u64, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_set_candidate_validator_commission_rate">set_candidate_validator_commission_rate</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_commission_rate: u64, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_stake">request_add_stake</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, stake: <a href="_Coin">coin::Coin</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_add_stake_mul_coin">request_add_stake_mul_coin</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, stakes: <a href="">vector</a>&lt;<a href="_Coin">coin::Coin</a>&lt;<a href="_SUI">sui::SUI</a>&gt;&gt;, stake_amount: <a href="_Option">option::Option</a>&lt;u64&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_request_withdraw_stake">request_withdraw_stake</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, staked_sui: <a href="staking_pool.md#0x3_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_rotate_operation_cap">rotate_operation_cap</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_name">update_validator_name</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, name: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_description">update_validator_description</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, description: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_image_url">update_validator_image_url</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, image_url: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_project_url">update_validator_project_url</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, project_url: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_network_address">update_validator_next_epoch_network_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_network_address">update_candidate_validator_network_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_p2p_address">update_validator_next_epoch_p2p_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, p2p_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_p2p_address">update_candidate_validator_p2p_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, p2p_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_primary_address">update_validator_next_epoch_primary_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, primary_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_primary_address">update_candidate_validator_primary_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, primary_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_worker_address">update_validator_next_epoch_worker_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_worker_address">update_candidate_validator_worker_address</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_protocol_pubkey">update_validator_next_epoch_protocol_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, protocol_pubkey: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_protocol_pubkey">update_candidate_validator_protocol_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, protocol_pubkey: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_worker_pubkey">update_validator_next_epoch_worker_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_worker_pubkey">update_candidate_validator_worker_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, worker_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_validator_next_epoch_network_pubkey">update_validator_next_epoch_network_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x3_sui_system_update_candidate_validator_network_pubkey">update_candidate_validator_network_pubkey</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, network_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
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


<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_advance_epoch">advance_epoch</a>(storage_reward: <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, computation_reward: <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_epoch: u64, next_protocol_version: u64, storage_rebate: u64, storage_fund_reinvest_rate: u64, reward_slashing_rate: u64, epoch_start_timestamp_ms: u64, new_system_state_version: u64, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>): <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;
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
    storage_fund_reinvest_rate: u64, // share of storage fund's rewards that's reinvested
                                     // into storage fund, in basis point.
    reward_slashing_rate: u64, // how much rewards are slashed <b>to</b> punish a <a href="validator.md#0x3_validator">validator</a>, in bps.
    epoch_start_timestamp_ms: u64, // Timestamp of the epoch start
    new_system_state_version: u64,
    ctx: &<b>mut</b> TxContext,
) : Balance&lt;SUI&gt; {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    // Validator will make a special system call <b>with</b> sender set <b>as</b> 0x0.
    <b>assert</b>!(<a href="_sender">tx_context::sender</a>(ctx) == @0x0, 0);
    <b>let</b> old_protocol_version = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_protocol_version">sui_system_state_inner::protocol_version</a>(self);
    <b>let</b> storage_rebate = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch">sui_system_state_inner::advance_epoch</a>(
        self,
        new_epoch,
        next_protocol_version,
        storage_reward,
        computation_reward,
        storage_rebate,
        storage_fund_reinvest_rate,
        reward_slashing_rate,
        epoch_start_timestamp_ms,
        ctx,
    );

    <b>if</b> (new_system_state_version != wrapper.version) {
        // If we are upgrading the system state, we need <b>to</b> make sure that the protocol version
        // is also upgraded.
        <b>assert</b>!(old_protocol_version != next_protocol_version, 0);
        <b>let</b> cur_state: SuiSystemStateInner = <a href="_remove">dynamic_field::remove</a>(&<b>mut</b> wrapper.id, wrapper.version);
        <b>let</b> new_state = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_upgrade_system_state">sui_system_state_inner::upgrade_system_state</a>(cur_state, new_system_state_version, ctx);
        wrapper.version = new_system_state_version;
        <a href="_add">dynamic_field::add</a>(&<b>mut</b> wrapper.id, wrapper.version, new_state);
    };
    storage_rebate
}
</code></pre>



</details>

<a name="0x3_sui_system_advance_epoch_safe_mode"></a>

## Function `advance_epoch_safe_mode`

An extremely simple version of advance_epoch.
This is called in two situations:
- When the call to advance_epoch failed due to a bug, and we want to be able to keep the
system running and continue making epoch changes.
- When advancing to a new protocol version, we want to be able to change the protocol
version


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system.md#0x3_sui_system_advance_epoch_safe_mode">advance_epoch_safe_mode</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_epoch: u64, next_protocol_version: u64, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system.md#0x3_sui_system_advance_epoch_safe_mode">advance_epoch_safe_mode</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>,
    new_epoch: u64,
    next_protocol_version: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    // Validator will make a special system call <b>with</b> sender set <b>as</b> 0x0.
    <b>assert</b>!(<a href="_sender">tx_context::sender</a>(ctx) == @0x0, 0);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch_safe_mode">sui_system_state_inner::advance_epoch_safe_mode</a>(self, new_epoch, next_protocol_version, ctx)
}
</code></pre>



</details>

<a name="0x3_sui_system_epoch"></a>

## Function `epoch`

Return the current epoch number. Useful for applications that need a coarse-grained concept of time,
since epochs are ever-increasing and epoch changes are intended to happen every 24 hours.


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_epoch">epoch</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_epoch">epoch</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): u64 {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch">sui_system_state_inner::epoch</a>(self)
}
</code></pre>



</details>

<a name="0x3_sui_system_epoch_start_timestamp_ms"></a>

## Function `epoch_start_timestamp_ms`

Returns unix timestamp of the start of current epoch


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): u64 {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch_start_timestamp_ms">sui_system_state_inner::epoch_start_timestamp_ms</a>(self)
}
</code></pre>



</details>

<a name="0x3_sui_system_validator_stake_amount"></a>

## Function `validator_stake_amount`

Returns the total amount staked with <code>validator_addr</code>.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_validator_stake_amount">validator_stake_amount</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, validator_addr: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_validator_stake_amount">validator_stake_amount</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>, validator_addr: <b>address</b>): u64 {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_stake_amount">sui_system_state_inner::validator_stake_amount</a>(self, validator_addr)
}
</code></pre>



</details>

<a name="0x3_sui_system_validator_staking_pool_id"></a>

## Function `validator_staking_pool_id`

Returns the staking pool id of a given validator.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_validator_staking_pool_id">validator_staking_pool_id</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, validator_addr: <b>address</b>): <a href="_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_validator_staking_pool_id">validator_staking_pool_id</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>, validator_addr: <b>address</b>): ID {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_staking_pool_id">sui_system_state_inner::validator_staking_pool_id</a>(self, validator_addr)
}
</code></pre>



</details>

<a name="0x3_sui_system_validator_staking_pool_mappings"></a>

## Function `validator_staking_pool_mappings`

Returns reference to the staking pool mappings that map pool ids to active validator addresses


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_validator_staking_pool_mappings">validator_staking_pool_mappings</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<a href="_Table">table::Table</a>&lt;<a href="_ID">object::ID</a>, <b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_validator_staking_pool_mappings">validator_staking_pool_mappings</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): &Table&lt;ID, <b>address</b>&gt; {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_staking_pool_mappings">sui_system_state_inner::validator_staking_pool_mappings</a>(self)
}
</code></pre>



</details>

<a name="0x3_sui_system_get_reporters_of"></a>

## Function `get_reporters_of`

Returns all the validators who are currently reporting <code>addr</code>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_get_reporters_of">get_reporters_of</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, addr: <b>address</b>): <a href="_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x3_sui_system_get_reporters_of">get_reporters_of</a>(wrapper: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>, addr: <b>address</b>): VecSet&lt;<b>address</b>&gt; {
    <b>let</b> self = <a href="sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(wrapper);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_get_reporters_of">sui_system_state_inner::get_reporters_of</a>(self, addr)
}
</code></pre>



</details>

<a name="0x3_sui_system_load_system_state"></a>

## Function `load_system_state`



<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(self: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_load_system_state">load_system_state</a>(self: &<a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): &SuiSystemStateInner {
    <b>let</b> version = self.version;
    <b>let</b> inner: &SuiSystemStateInner = <a href="_borrow">dynamic_field::borrow</a>(&self.id, version);
    <b>assert</b>!(<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_system_state_version">sui_system_state_inner::system_state_version</a>(inner) == version, 0);
    inner
}
</code></pre>



</details>

<a name="0x3_sui_system_load_system_state_mut"></a>

## Function `load_system_state_mut`



<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system.md#0x3_sui_system_load_system_state_mut">load_system_state_mut</a>(self: &<b>mut</b> <a href="sui_system.md#0x3_sui_system_SuiSystemState">SuiSystemState</a>): &<b>mut</b> SuiSystemStateInner {
    <b>let</b> version = self.version;
    <b>let</b> inner: &<b>mut</b> SuiSystemStateInner = <a href="_borrow_mut">dynamic_field::borrow_mut</a>(&<b>mut</b> self.id, version);
    <b>assert</b>!(<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_system_state_version">sui_system_state_inner::system_state_version</a>(inner) == version, 0);
    inner
}
</code></pre>



</details>
