
<a name="0x2_sui_system"></a>

# Module `0x2::sui_system`



-  [Struct `SystemParameters`](#0x2_sui_system_SystemParameters)
-  [Resource `SuiSystemStateInner`](#0x2_sui_system_SuiSystemStateInner)
-  [Resource `SuiSystemState`](#0x2_sui_system_SuiSystemState)
-  [Struct `SystemEpochInfo`](#0x2_sui_system_SystemEpochInfo)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x2_sui_system_create)
-  [Function `request_add_validator`](#0x2_sui_system_request_add_validator)
-  [Function `request_remove_validator`](#0x2_sui_system_request_remove_validator)
-  [Function `request_set_gas_price`](#0x2_sui_system_request_set_gas_price)
-  [Function `request_set_commission_rate`](#0x2_sui_system_request_set_commission_rate)
-  [Function `request_add_delegation`](#0x2_sui_system_request_add_delegation)
-  [Function `request_add_delegation_mul_coin`](#0x2_sui_system_request_add_delegation_mul_coin)
-  [Function `request_add_delegation_with_locked_coin`](#0x2_sui_system_request_add_delegation_with_locked_coin)
-  [Function `request_add_delegation_mul_locked_coin`](#0x2_sui_system_request_add_delegation_mul_locked_coin)
-  [Function `request_withdraw_delegation`](#0x2_sui_system_request_withdraw_delegation)
-  [Function `report_validator`](#0x2_sui_system_report_validator)
-  [Function `undo_report_validator`](#0x2_sui_system_undo_report_validator)
-  [Function `advance_epoch`](#0x2_sui_system_advance_epoch)
-  [Function `advance_epoch_safe_mode`](#0x2_sui_system_advance_epoch_safe_mode)
-  [Function `consensus_commit_prologue`](#0x2_sui_system_consensus_commit_prologue)
-  [Function `load_system_state`](#0x2_sui_system_load_system_state)
-  [Function `load_system_state_mut`](#0x2_sui_system_load_system_state_mut)
-  [Function `epoch`](#0x2_sui_system_epoch)
-  [Function `epoch_start_timestamp_ms`](#0x2_sui_system_epoch_start_timestamp_ms)
-  [Function `validator_stake_amount`](#0x2_sui_system_validator_stake_amount)
-  [Function `validator_staking_pool_id`](#0x2_sui_system_validator_staking_pool_id)
-  [Function `validator_staking_pool_mappings`](#0x2_sui_system_validator_staking_pool_mappings)
-  [Function `get_reporters_of`](#0x2_sui_system_get_reporters_of)
-  [Function `extract_coin_balance`](#0x2_sui_system_extract_coin_balance)
-  [Function `extract_locked_coin_balance`](#0x2_sui_system_extract_locked_coin_balance)
-  [Function `validators`](#0x2_sui_system_validators)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="clock.md#0x2_clock">0x2::clock</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock">0x2::epoch_time_lock</a>;
<b>use</b> <a href="event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="locked_coin.md#0x2_locked_coin">0x2::locked_coin</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="pay.md#0x2_pay">0x2::pay</a>;
<b>use</b> <a href="stake_subsidy.md#0x2_stake_subsidy">0x2::stake_subsidy</a>;
<b>use</b> <a href="staking_pool.md#0x2_staking_pool">0x2::staking_pool</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="validator.md#0x2_validator">0x2::validator</a>;
<b>use</b> <a href="validator_set.md#0x2_validator_set">0x2::validator_set</a>;
<b>use</b> <a href="vec_map.md#0x2_vec_map">0x2::vec_map</a>;
<b>use</b> <a href="vec_set.md#0x2_vec_set">0x2::vec_set</a>;
</code></pre>



<a name="0x2_sui_system_SystemParameters"></a>

## Struct `SystemParameters`

A list of system config parameters.


<pre><code><b>struct</b> <a href="sui_system.md#0x2_sui_system_SystemParameters">SystemParameters</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>min_validator_stake: u64</code>
</dt>
<dd>
 Lower-bound on the amount of stake required to become a validator.
</dd>
<dt>
<code>max_validator_candidate_count: u64</code>
</dt>
<dd>
 Maximum number of validator candidates at any moment.
 We do not allow the number of validators in any epoch to go above this.
</dd>
</dl>


</details>

<a name="0x2_sui_system_SuiSystemStateInner"></a>

## Resource `SuiSystemStateInner`

The top-level object containing all information of the Sui system.


<pre><code><b>struct</b> <a href="sui_system.md#0x2_sui_system_SuiSystemStateInner">SuiSystemStateInner</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>epoch: u64</code>
</dt>
<dd>
 The current epoch ID, starting from 0.
</dd>
<dt>
<code>protocol_version: u64</code>
</dt>
<dd>
 The current protocol version, starting from 1.
</dd>
<dt>
<code>validators: <a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a></code>
</dt>
<dd>
 Contains all information about the validators.
</dd>
<dt>
<code>storage_fund: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>
 The storage fund.
</dd>
<dt>
<code>parameters: <a href="sui_system.md#0x2_sui_system_SystemParameters">sui_system::SystemParameters</a></code>
</dt>
<dd>
 A list of system config parameters.
</dd>
<dt>
<code>reference_gas_price: u64</code>
</dt>
<dd>
 The reference gas price for the current epoch.
</dd>
<dt>
<code>validator_report_records: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;</code>
</dt>
<dd>
 A map storing the records of validator reporting each other during the current epoch.
 There is an entry in the map for each validator that has been reported
 at least once. The entry VecSet contains all the validators that reported
 them. If a validator has never been reported they don't have an entry in this map.
 This map resets every epoch.
</dd>
<dt>
<code><a href="stake_subsidy.md#0x2_stake_subsidy">stake_subsidy</a>: <a href="stake_subsidy.md#0x2_stake_subsidy_StakeSubsidy">stake_subsidy::StakeSubsidy</a></code>
</dt>
<dd>
 Schedule of stake subsidies given out each epoch.
</dd>
<dt>
<code>safe_mode: bool</code>
</dt>
<dd>
 Whether the system is running in a downgraded safe mode due to a non-recoverable bug.
 This is set whenever we failed to execute advance_epoch, and ended up executing advance_epoch_safe_mode.
 It can be reset once we are able to successfully execute advance_epoch.
 TODO: Down the road we may want to save a few states such as pending gas rewards, so that we could
 redistribute them.
</dd>
<dt>
<code>epoch_start_timestamp_ms: u64</code>
</dt>
<dd>
 Unix timestamp of the current epoch start
</dd>
</dl>


</details>

<a name="0x2_sui_system_SuiSystemState"></a>

## Resource `SuiSystemState`



<pre><code><b>struct</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>version: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>system_state: <a href="sui_system.md#0x2_sui_system_SuiSystemStateInner">sui_system::SuiSystemStateInner</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_sui_system_SystemEpochInfo"></a>

## Struct `SystemEpochInfo`

Event containing system-level epoch information, emitted during
the epoch advancement transaction.


<pre><code><b>struct</b> <a href="sui_system.md#0x2_sui_system_SystemEpochInfo">SystemEpochInfo</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>epoch: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>protocol_version: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>reference_gas_price: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>total_stake: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>storage_fund_inflows: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>storage_fund_outflows: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>storage_fund_balance: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>stake_subsidy_amount: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>total_gas_fees: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>total_stake_rewards: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_sui_system_BASIS_POINT_DENOMINATOR"></a>



<pre><code><b>const</b> <a href="sui_system.md#0x2_sui_system_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>: u128 = 10000;
</code></pre>



<a name="0x2_sui_system_EBpsTooLarge"></a>



<pre><code><b>const</b> <a href="sui_system.md#0x2_sui_system_EBpsTooLarge">EBpsTooLarge</a>: u64 = 5;
</code></pre>



<a name="0x2_sui_system_ECannotReportOneself"></a>



<pre><code><b>const</b> <a href="sui_system.md#0x2_sui_system_ECannotReportOneself">ECannotReportOneself</a>: u64 = 3;
</code></pre>



<a name="0x2_sui_system_EEpochNumberMismatch"></a>



<pre><code><b>const</b> <a href="sui_system.md#0x2_sui_system_EEpochNumberMismatch">EEpochNumberMismatch</a>: u64 = 2;
</code></pre>



<a name="0x2_sui_system_ELimitExceeded"></a>



<pre><code><b>const</b> <a href="sui_system.md#0x2_sui_system_ELimitExceeded">ELimitExceeded</a>: u64 = 1;
</code></pre>



<a name="0x2_sui_system_ENotValidator"></a>



<pre><code><b>const</b> <a href="sui_system.md#0x2_sui_system_ENotValidator">ENotValidator</a>: u64 = 0;
</code></pre>



<a name="0x2_sui_system_EReportRecordNotFound"></a>



<pre><code><b>const</b> <a href="sui_system.md#0x2_sui_system_EReportRecordNotFound">EReportRecordNotFound</a>: u64 = 4;
</code></pre>



<a name="0x2_sui_system_EStakedSuiFromWrongEpoch"></a>



<pre><code><b>const</b> <a href="sui_system.md#0x2_sui_system_EStakedSuiFromWrongEpoch">EStakedSuiFromWrongEpoch</a>: u64 = 6;
</code></pre>



<a name="0x2_sui_system_create"></a>

## Function `create`

Create a new SuiSystemState object and make it shared.
This function will be called only once in genesis.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system.md#0x2_sui_system_create">create</a>(validators: <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, stake_subsidy_fund: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, storage_fund: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, max_validator_candidate_count: u64, min_validator_stake: u64, initial_stake_subsidy_amount: u64, protocol_version: u64, epoch_start_timestamp_ms: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system.md#0x2_sui_system_create">create</a>(
    validators: <a href="">vector</a>&lt;Validator&gt;,
    stake_subsidy_fund: Balance&lt;SUI&gt;,
    storage_fund: Balance&lt;SUI&gt;,
    max_validator_candidate_count: u64,
    min_validator_stake: u64,
    initial_stake_subsidy_amount: u64,
    protocol_version: u64,
    epoch_start_timestamp_ms: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> validators = <a href="validator_set.md#0x2_validator_set_new">validator_set::new</a>(validators, ctx);
    <b>let</b> reference_gas_price = <a href="validator_set.md#0x2_validator_set_derive_reference_gas_price">validator_set::derive_reference_gas_price</a>(&validators);
    <b>let</b> system_state = <a href="sui_system.md#0x2_sui_system_SuiSystemStateInner">SuiSystemStateInner</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        epoch: 0,
        protocol_version,
        validators,
        storage_fund,
        parameters: <a href="sui_system.md#0x2_sui_system_SystemParameters">SystemParameters</a> {
            min_validator_stake,
            max_validator_candidate_count,
        },
        reference_gas_price,
        validator_report_records: <a href="vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        <a href="stake_subsidy.md#0x2_stake_subsidy">stake_subsidy</a>: <a href="stake_subsidy.md#0x2_stake_subsidy_create">stake_subsidy::create</a>(stake_subsidy_fund, initial_stake_subsidy_amount),
        safe_mode: <b>false</b>,
        epoch_start_timestamp_ms,
    };
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a> {
        // Use a hardcoded ID.
        id: <a href="object.md#0x2_object_sui_system_state">object::sui_system_state</a>(),
        version: protocol_version,
        system_state,
    };
    // <a href="dynamic_object_field.md#0x2_dynamic_object_field_add">dynamic_object_field::add</a>(&<b>mut</b> self.id, self.version, system_state);
    <a href="transfer.md#0x2_transfer_share_object">transfer::share_object</a>(self);
}
</code></pre>



</details>

<a name="0x2_sui_system_request_add_validator"></a>

## Function `request_add_validator`

Can be called by anyone who wishes to become a validator in the next epoch.
The <code><a href="validator.md#0x2_validator">validator</a></code> object needs to be created before calling this.
The amount of stake in the <code><a href="validator.md#0x2_validator">validator</a></code> object must meet the requirements.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_add_validator">request_add_validator</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, network_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, worker_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, name: <a href="">vector</a>&lt;u8&gt;, description: <a href="">vector</a>&lt;u8&gt;, image_url: <a href="">vector</a>&lt;u8&gt;, project_url: <a href="">vector</a>&lt;u8&gt;, net_address: <a href="">vector</a>&lt;u8&gt;, p2p_address: <a href="">vector</a>&lt;u8&gt;, consensus_address: <a href="">vector</a>&lt;u8&gt;, worker_address: <a href="">vector</a>&lt;u8&gt;, stake: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, gas_price: u64, commission_rate: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_add_validator">request_add_validator</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
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
    consensus_address: <a href="">vector</a>&lt;u8&gt;,
    worker_address: <a href="">vector</a>&lt;u8&gt;,
    stake: Coin&lt;SUI&gt;,
    gas_price: u64,
    commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <b>assert</b>!(
        <a href="validator_set.md#0x2_validator_set_next_epoch_validator_count">validator_set::next_epoch_validator_count</a>(&self.validators) &lt; self.parameters.max_validator_candidate_count,
        <a href="sui_system.md#0x2_sui_system_ELimitExceeded">ELimitExceeded</a>,
    );
    <b>let</b> stake_amount = <a href="coin.md#0x2_coin_value">coin::value</a>(&stake);
    <b>assert</b>!(
        stake_amount &gt;= self.parameters.min_validator_stake,
        <a href="sui_system.md#0x2_sui_system_ELimitExceeded">ELimitExceeded</a>,
    );
    <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator.md#0x2_validator_new">validator::new</a>(
        <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx),
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
        consensus_address,
        worker_address,
        <a href="coin.md#0x2_coin_into_balance">coin::into_balance</a>(stake),
        <a href="_none">option::none</a>(),
        gas_price,
        commission_rate,
        <a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) + 1, // starting next epoch
        ctx
    );

    // TODO: We need <b>to</b> verify the <a href="validator.md#0x2_validator">validator</a> metadata.
    // https://github.com/MystenLabs/<a href="sui.md#0x2_sui">sui</a>/issues/7323

    <a href="validator_set.md#0x2_validator_set_request_add_validator">validator_set::request_add_validator</a>(&<b>mut</b> self.validators, <a href="validator.md#0x2_validator">validator</a>);
}
</code></pre>



</details>

<a name="0x2_sui_system_request_remove_validator"></a>

## Function `request_remove_validator`

A validator can call this function to request a removal in the next epoch.
We use the sender of <code>ctx</code> to look up the validator
(i.e. sender must match the sui_address in the validator).
At the end of the epoch, the <code><a href="validator.md#0x2_validator">validator</a></code> object will be returned to the sui_address
of the validator.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_remove_validator">request_remove_validator</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_remove_validator">request_remove_validator</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="validator_set.md#0x2_validator_set_request_remove_validator">validator_set::request_remove_validator</a>(
        &<b>mut</b> self.validators,
        ctx,
    )
}
</code></pre>



</details>

<a name="0x2_sui_system_request_set_gas_price"></a>

## Function `request_set_gas_price`

A validator can call this entry function to submit a new gas price quote, to be
used for the reference gas price calculation at the end of the epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_set_gas_price">request_set_gas_price</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_gas_price: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_set_gas_price">request_set_gas_price</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    new_gas_price: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="validator_set.md#0x2_validator_set_request_set_gas_price">validator_set::request_set_gas_price</a>(
        &<b>mut</b> self.validators,
        new_gas_price,
        ctx
    )
}
</code></pre>



</details>

<a name="0x2_sui_system_request_set_commission_rate"></a>

## Function `request_set_commission_rate`

A validator can call this entry function to set a new commission rate, updated at the end of the epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_set_commission_rate">request_set_commission_rate</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_commission_rate: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_set_commission_rate">request_set_commission_rate</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    new_commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="validator_set.md#0x2_validator_set_request_set_commission_rate">validator_set::request_set_commission_rate</a>(
        &<b>mut</b> self.validators,
        new_commission_rate,
        ctx
    )
}
</code></pre>



</details>

<a name="0x2_sui_system_request_add_delegation"></a>

## Function `request_add_delegation`

Add delegated stake to a validator's staking pool.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_add_delegation">request_add_delegation</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, delegate_stake: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_add_delegation">request_add_delegation</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    delegate_stake: Coin&lt;SUI&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="validator_set.md#0x2_validator_set_request_add_delegation">validator_set::request_add_delegation</a>(
        &<b>mut</b> self.validators,
        validator_address,
        <a href="coin.md#0x2_coin_into_balance">coin::into_balance</a>(delegate_stake),
        <a href="_none">option::none</a>(),
        ctx,
    );
}
</code></pre>



</details>

<a name="0x2_sui_system_request_add_delegation_mul_coin"></a>

## Function `request_add_delegation_mul_coin`

Add delegated stake to a validator's staking pool using multiple coins.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_add_delegation_mul_coin">request_add_delegation_mul_coin</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, delegate_stakes: <a href="">vector</a>&lt;<a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;&gt;, stake_amount: <a href="_Option">option::Option</a>&lt;u64&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_add_delegation_mul_coin">request_add_delegation_mul_coin</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    delegate_stakes: <a href="">vector</a>&lt;Coin&lt;SUI&gt;&gt;,
    stake_amount: <a href="_Option">option::Option</a>&lt;u64&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <b>let</b> <a href="balance.md#0x2_balance">balance</a> = <a href="sui_system.md#0x2_sui_system_extract_coin_balance">extract_coin_balance</a>(delegate_stakes, stake_amount, ctx);
    <a href="validator_set.md#0x2_validator_set_request_add_delegation">validator_set::request_add_delegation</a>(&<b>mut</b> self.validators, validator_address, <a href="balance.md#0x2_balance">balance</a>, <a href="_none">option::none</a>(), ctx);
}
</code></pre>



</details>

<a name="0x2_sui_system_request_add_delegation_with_locked_coin"></a>

## Function `request_add_delegation_with_locked_coin`

Add delegated stake to a validator's staking pool using a locked SUI coin.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_add_delegation_with_locked_coin">request_add_delegation_with_locked_coin</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, delegate_stake: <a href="locked_coin.md#0x2_locked_coin_LockedCoin">locked_coin::LockedCoin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_add_delegation_with_locked_coin">request_add_delegation_with_locked_coin</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    delegate_stake: LockedCoin&lt;SUI&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <b>let</b> (<a href="balance.md#0x2_balance">balance</a>, lock) = <a href="locked_coin.md#0x2_locked_coin_into_balance">locked_coin::into_balance</a>(delegate_stake);
    <a href="validator_set.md#0x2_validator_set_request_add_delegation">validator_set::request_add_delegation</a>(&<b>mut</b> self.validators, validator_address, <a href="balance.md#0x2_balance">balance</a>, <a href="_some">option::some</a>(lock), ctx);
}
</code></pre>



</details>

<a name="0x2_sui_system_request_add_delegation_mul_locked_coin"></a>

## Function `request_add_delegation_mul_locked_coin`

Add delegated stake to a validator's staking pool using multiple locked SUI coins.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_add_delegation_mul_locked_coin">request_add_delegation_mul_locked_coin</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, delegate_stakes: <a href="">vector</a>&lt;<a href="locked_coin.md#0x2_locked_coin_LockedCoin">locked_coin::LockedCoin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;&gt;, stake_amount: <a href="_Option">option::Option</a>&lt;u64&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_add_delegation_mul_locked_coin">request_add_delegation_mul_locked_coin</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    delegate_stakes: <a href="">vector</a>&lt;LockedCoin&lt;SUI&gt;&gt;,
    stake_amount: <a href="_Option">option::Option</a>&lt;u64&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <b>let</b> (<a href="balance.md#0x2_balance">balance</a>, lock) = <a href="sui_system.md#0x2_sui_system_extract_locked_coin_balance">extract_locked_coin_balance</a>(delegate_stakes, stake_amount, ctx);
    <a href="validator_set.md#0x2_validator_set_request_add_delegation">validator_set::request_add_delegation</a>(
        &<b>mut</b> self.validators,
        validator_address,
        <a href="balance.md#0x2_balance">balance</a>,
        <a href="_some">option::some</a>(lock),
        ctx
    );
}
</code></pre>



</details>

<a name="0x2_sui_system_request_withdraw_delegation"></a>

## Function `request_withdraw_delegation`

Withdraw some portion of a delegation from a validator's staking pool.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_withdraw_delegation">request_withdraw_delegation</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, staked_sui: <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_request_withdraw_delegation">request_withdraw_delegation</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    staked_sui: StakedSui,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <a href="validator_set.md#0x2_validator_set_request_withdraw_delegation">validator_set::request_withdraw_delegation</a>(
        &<b>mut</b> self.validators, staked_sui, ctx,
    );
}
</code></pre>



</details>

<a name="0x2_sui_system_report_validator"></a>

## Function `report_validator`

Report a validator as a bad or non-performant actor in the system.
Succeeds iff both the sender and the input <code>validator_addr</code> are active validators
and they are not the same address. This function is idempotent within an epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_report_validator">report_validator</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, validator_addr: <b>address</b>, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_report_validator">report_validator</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    validator_addr: <b>address</b>,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <b>let</b> sender = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    // Both the reporter and the reported have <b>to</b> be validators.
    <b>assert</b>!(<a href="validator_set.md#0x2_validator_set_is_active_validator">validator_set::is_active_validator</a>(&self.validators, sender), <a href="sui_system.md#0x2_sui_system_ENotValidator">ENotValidator</a>);
    <b>assert</b>!(<a href="validator_set.md#0x2_validator_set_is_active_validator">validator_set::is_active_validator</a>(&self.validators, validator_addr), <a href="sui_system.md#0x2_sui_system_ENotValidator">ENotValidator</a>);
    <b>assert</b>!(sender != validator_addr, <a href="sui_system.md#0x2_sui_system_ECannotReportOneself">ECannotReportOneself</a>);

    <b>if</b> (!<a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&self.validator_report_records, &validator_addr)) {
        <a href="vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> self.validator_report_records, validator_addr, <a href="vec_set.md#0x2_vec_set_singleton">vec_set::singleton</a>(sender));
    } <b>else</b> {
        <b>let</b> reporters = <a href="vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.validator_report_records, &validator_addr);
        <b>if</b> (!<a href="vec_set.md#0x2_vec_set_contains">vec_set::contains</a>(reporters, &sender)) {
            <a href="vec_set.md#0x2_vec_set_insert">vec_set::insert</a>(reporters, sender);
        }
    }
}
</code></pre>



</details>

<a name="0x2_sui_system_undo_report_validator"></a>

## Function `undo_report_validator`

Undo a <code>report_validator</code> action. Aborts if the sender has not reported the
<code>validator_addr</code> within this epoch.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_undo_report_validator">undo_report_validator</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, validator_addr: <b>address</b>, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_undo_report_validator">undo_report_validator</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    validator_addr: <b>address</b>,
    ctx: &TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    <b>let</b> sender = <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);

    <b>assert</b>!(<a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&self.validator_report_records, &validator_addr), <a href="sui_system.md#0x2_sui_system_EReportRecordNotFound">EReportRecordNotFound</a>);
    <b>let</b> reporters = <a href="vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.validator_report_records, &validator_addr);
    <b>assert</b>!(<a href="vec_set.md#0x2_vec_set_contains">vec_set::contains</a>(reporters, &sender), <a href="sui_system.md#0x2_sui_system_EReportRecordNotFound">EReportRecordNotFound</a>);
    <a href="vec_set.md#0x2_vec_set_remove">vec_set::remove</a>(reporters, &sender);
}
</code></pre>



</details>

<a name="0x2_sui_system_advance_epoch"></a>

## Function `advance_epoch`

This function should be called at the end of an epoch, and advances the system to the next epoch.
It does the following things:
1. Add storage charge to the storage fund.
2. Burn the storage rebates from the storage fund. These are already refunded to transaction sender's
gas coins.
3. Distribute computation charge to validator stake and delegation stake.
4. Update all validators.


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_advance_epoch">advance_epoch</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_epoch: u64, next_protocol_version: u64, storage_charge: u64, computation_charge: u64, storage_rebate: u64, storage_fund_reinvest_rate: u64, reward_slashing_rate: u64, epoch_start_timestamp_ms: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_advance_epoch">advance_epoch</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    new_epoch: u64,
    next_protocol_version: u64,
    storage_charge: u64,
    computation_charge: u64,
    storage_rebate: u64,
    storage_fund_reinvest_rate: u64, // share of storage fund's rewards that's reinvested
                                     // into storage fund, in basis point.
    reward_slashing_rate: u64, // how much rewards are slashed <b>to</b> punish a <a href="validator.md#0x2_validator">validator</a>, in bps.
    epoch_start_timestamp_ms: u64, // Timestamp of the epoch start
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    // Validator will make a special system call <b>with</b> sender set <b>as</b> 0x0.
    <b>assert</b>!(<a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, 0);

    self.epoch_start_timestamp_ms = epoch_start_timestamp_ms;

    <b>let</b> bps_denominator_u64 = (<a href="sui_system.md#0x2_sui_system_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a> <b>as</b> u64);
    // Rates can't be higher than 100%.
    <b>assert</b>!(
        storage_fund_reinvest_rate &lt;= bps_denominator_u64
        && reward_slashing_rate &lt;= bps_denominator_u64,
        <a href="sui_system.md#0x2_sui_system_EBpsTooLarge">EBpsTooLarge</a>,
    );

    <b>let</b> total_validators_stake = <a href="validator_set.md#0x2_validator_set_total_stake">validator_set::total_stake</a>(&self.validators);
    <b>let</b> storage_fund_balance = <a href="balance.md#0x2_balance_value">balance::value</a>(&self.storage_fund);
    <b>let</b> total_stake = storage_fund_balance + total_validators_stake;

    <b>let</b> storage_reward = <a href="balance.md#0x2_balance_create_staking_rewards">balance::create_staking_rewards</a>(storage_charge);
    <b>let</b> computation_reward = <a href="balance.md#0x2_balance_create_staking_rewards">balance::create_staking_rewards</a>(computation_charge);

    // Include stake subsidy in the rewards given out <b>to</b> validators and delegators.
    <b>let</b> <a href="stake_subsidy.md#0x2_stake_subsidy">stake_subsidy</a> = <a href="stake_subsidy.md#0x2_stake_subsidy_advance_epoch">stake_subsidy::advance_epoch</a>(&<b>mut</b> self.<a href="stake_subsidy.md#0x2_stake_subsidy">stake_subsidy</a>);
    <b>let</b> stake_subsidy_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&<a href="stake_subsidy.md#0x2_stake_subsidy">stake_subsidy</a>);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> computation_reward, <a href="stake_subsidy.md#0x2_stake_subsidy">stake_subsidy</a>);

    <b>let</b> total_stake_u128 = (total_stake <b>as</b> u128);
    <b>let</b> computation_charge_u128 = (computation_charge <b>as</b> u128);

    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.storage_fund, storage_reward);

    <b>let</b> storage_fund_reward_amount = (storage_fund_balance <b>as</b> u128) * computation_charge_u128 / total_stake_u128;
    <b>let</b> storage_fund_reward = <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> computation_reward, (storage_fund_reward_amount <b>as</b> u64));
    <b>let</b> storage_fund_reinvestment_amount =
        storage_fund_reward_amount * (storage_fund_reinvest_rate <b>as</b> u128) / <a href="sui_system.md#0x2_sui_system_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;
    <b>let</b> storage_fund_reinvestment = <a href="balance.md#0x2_balance_split">balance::split</a>(
        &<b>mut</b> storage_fund_reward,
        (storage_fund_reinvestment_amount <b>as</b> u64),
    );
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.storage_fund, storage_fund_reinvestment);

    self.epoch = self.epoch + 1;
    // Sanity check <b>to</b> make sure we are advancing <b>to</b> the right epoch.
    <b>assert</b>!(new_epoch == self.epoch, 0);
    <b>let</b> total_rewards_amount =
        <a href="balance.md#0x2_balance_value">balance::value</a>(&computation_reward)+ <a href="balance.md#0x2_balance_value">balance::value</a>(&storage_fund_reward);

    <a href="validator_set.md#0x2_validator_set_advance_epoch">validator_set::advance_epoch</a>(
        &<b>mut</b> self.validators,
        &<b>mut</b> computation_reward,
        &<b>mut</b> storage_fund_reward,
        self.validator_report_records,
        reward_slashing_rate,
        ctx,
    );

    self.protocol_version = next_protocol_version;

    // Derive the reference gas price for the new epoch
    self.reference_gas_price = <a href="validator_set.md#0x2_validator_set_derive_reference_gas_price">validator_set::derive_reference_gas_price</a>(&self.validators);
    // Because of precision issues <b>with</b> integer divisions, we expect that there will be some
    // remaining <a href="balance.md#0x2_balance">balance</a> in `storage_fund_reward` and `computation_reward`.
    // All of these go <b>to</b> the storage fund.
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.storage_fund, storage_fund_reward);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> self.storage_fund, computation_reward);

    // Destroy the storage rebate.
    <b>assert</b>!(<a href="balance.md#0x2_balance_value">balance::value</a>(&self.storage_fund) &gt;= storage_rebate, 0);
    <a href="balance.md#0x2_balance_destroy_storage_rebates">balance::destroy_storage_rebates</a>(<a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> self.storage_fund, storage_rebate));

    // Validator reports are only valid for the epoch.
    // TODO: or do we want <b>to</b> make it persistent and validators have <b>to</b> explicitly change their scores?
    self.validator_report_records = <a href="vec_map.md#0x2_vec_map_empty">vec_map::empty</a>();

    <b>let</b> new_total_stake = <a href="validator_set.md#0x2_validator_set_total_stake">validator_set::total_stake</a>(&self.validators);

    <a href="event.md#0x2_event_emit">event::emit</a>(
        <a href="sui_system.md#0x2_sui_system_SystemEpochInfo">SystemEpochInfo</a> {
            epoch: self.epoch,
            protocol_version: self.protocol_version,
            reference_gas_price: self.reference_gas_price,
            total_stake: new_total_stake,
            storage_fund_inflows: storage_charge + (storage_fund_reinvestment_amount <b>as</b> u64),
            storage_fund_outflows: storage_rebate,
            storage_fund_balance: <a href="balance.md#0x2_balance_value">balance::value</a>(&self.storage_fund),
            stake_subsidy_amount,
            total_gas_fees: computation_charge,
            total_stake_rewards: total_rewards_amount,
        }
    );

    self.safe_mode = <b>false</b>;
}
</code></pre>



</details>

<a name="0x2_sui_system_advance_epoch_safe_mode"></a>

## Function `advance_epoch_safe_mode`

An extremely simple version of advance_epoch.
This is called in two situations:
- When the call to advance_epoch failed due to a bug, and we want to be able to keep the
system running and continue making epoch changes.
- When advancing to a new protocol version, we want to be able to change the protocol
version


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_advance_epoch_safe_mode">advance_epoch_safe_mode</a>(wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, new_epoch: u64, next_protocol_version: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_advance_epoch_safe_mode">advance_epoch_safe_mode</a>(
    wrapper: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>,
    new_epoch: u64,
    next_protocol_version: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(wrapper);
    // Validator will make a special system call <b>with</b> sender set <b>as</b> 0x0.
    <b>assert</b>!(<a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, 0);

    self.epoch = new_epoch;
    self.protocol_version = next_protocol_version;
    self.safe_mode = <b>true</b>;
}
</code></pre>



</details>

<a name="0x2_sui_system_consensus_commit_prologue"></a>

## Function `consensus_commit_prologue`



<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_consensus_commit_prologue">consensus_commit_prologue</a>(<a href="clock.md#0x2_clock">clock</a>: &<b>mut</b> <a href="clock.md#0x2_clock_Clock">clock::Clock</a>, timestamp_ms: u64, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="sui_system.md#0x2_sui_system_consensus_commit_prologue">consensus_commit_prologue</a>(
    <a href="clock.md#0x2_clock">clock</a>: &<b>mut</b> Clock,
    timestamp_ms: u64,
    ctx: &TxContext,
) {
    // Validator will make a special system call <b>with</b> sender set <b>as</b> 0x0.
    <b>assert</b>!(<a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx) == @0x0, 0);

    <a href="clock.md#0x2_clock_set_timestamp">clock::set_timestamp</a>(<a href="clock.md#0x2_clock">clock</a>, timestamp_ms);
}
</code></pre>



</details>

<a name="0x2_sui_system_load_system_state"></a>

## Function `load_system_state`



<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_load_system_state">load_system_state</a>(self: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<a href="sui_system.md#0x2_sui_system_SuiSystemStateInner">sui_system::SuiSystemStateInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_load_system_state">load_system_state</a>(self: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>): &<a href="sui_system.md#0x2_sui_system_SuiSystemStateInner">SuiSystemStateInner</a> {
    &self.system_state
    // <a href="dynamic_object_field.md#0x2_dynamic_object_field_borrow">dynamic_object_field::borrow</a>(&self.id, self.version)
}
</code></pre>



</details>

<a name="0x2_sui_system_load_system_state_mut"></a>

## Function `load_system_state_mut`



<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(self: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemStateInner">sui_system::SuiSystemStateInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_load_system_state_mut">load_system_state_mut</a>(self: &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>): &<b>mut</b> <a href="sui_system.md#0x2_sui_system_SuiSystemStateInner">SuiSystemStateInner</a> {
    &<b>mut</b> self.system_state
    // <a href="dynamic_object_field.md#0x2_dynamic_object_field_borrow_mut">dynamic_object_field::borrow_mut</a>(&<b>mut</b> self.id, self.version)
}
</code></pre>



</details>

<a name="0x2_sui_system_epoch"></a>

## Function `epoch`

Return the current epoch number. Useful for applications that need a coarse-grained concept of time,
since epochs are ever-increasing and epoch changes are intended to happen every 24 hours.


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_epoch">epoch</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_epoch">epoch</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>): u64 {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state">load_system_state</a>(wrapper);
    self.epoch
}
</code></pre>



</details>

<a name="0x2_sui_system_epoch_start_timestamp_ms"></a>

## Function `epoch_start_timestamp_ms`

Returns unix timestamp of the start of current epoch


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>): u64 {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state">load_system_state</a>(wrapper);
    self.epoch_start_timestamp_ms
}
</code></pre>



</details>

<a name="0x2_sui_system_validator_stake_amount"></a>

## Function `validator_stake_amount`

Returns the total amount staked with <code>validator_addr</code>.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_validator_stake_amount">validator_stake_amount</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, validator_addr: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_validator_stake_amount">validator_stake_amount</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>, validator_addr: <b>address</b>): u64 {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state">load_system_state</a>(wrapper);
    <a href="validator_set.md#0x2_validator_set_validator_total_stake_amount">validator_set::validator_total_stake_amount</a>(&self.validators, validator_addr)
}
</code></pre>



</details>

<a name="0x2_sui_system_validator_staking_pool_id"></a>

## Function `validator_staking_pool_id`

Returns the staking pool id of a given validator.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_validator_staking_pool_id">validator_staking_pool_id</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, validator_addr: <b>address</b>): <a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_validator_staking_pool_id">validator_staking_pool_id</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>, validator_addr: <b>address</b>): ID {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state">load_system_state</a>(wrapper);
    <a href="validator_set.md#0x2_validator_set_validator_staking_pool_id">validator_set::validator_staking_pool_id</a>(&self.validators, validator_addr)
}
</code></pre>



</details>

<a name="0x2_sui_system_validator_staking_pool_mappings"></a>

## Function `validator_staking_pool_mappings`

Returns reference to the staking pool mappings that map pool ids to active validator addresses


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_validator_staking_pool_mappings">validator_staking_pool_mappings</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<a href="table.md#0x2_table_Table">table::Table</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>, <b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_validator_staking_pool_mappings">validator_staking_pool_mappings</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>): &Table&lt;ID, <b>address</b>&gt; {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state">load_system_state</a>(wrapper);
    <a href="validator_set.md#0x2_validator_set_staking_pool_mappings">validator_set::staking_pool_mappings</a>(&self.validators)
}
</code></pre>



</details>

<a name="0x2_sui_system_get_reporters_of"></a>

## Function `get_reporters_of`

Returns all the validators who have reported <code>addr</code> this epoch.


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_get_reporters_of">get_reporters_of</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>, addr: <b>address</b>): <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_get_reporters_of">get_reporters_of</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>, addr: <b>address</b>): VecSet&lt;<b>address</b>&gt; {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state">load_system_state</a>(wrapper);
    <b>if</b> (<a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&self.validator_report_records, &addr)) {
        *<a href="vec_map.md#0x2_vec_map_get">vec_map::get</a>(&self.validator_report_records, &addr)
    } <b>else</b> {
        <a href="vec_set.md#0x2_vec_set_empty">vec_set::empty</a>()
    }
}
</code></pre>



</details>

<a name="0x2_sui_system_extract_coin_balance"></a>

## Function `extract_coin_balance`

Extract required Balance from vector of Coin<SUI>, transfer the remainder back to sender.


<pre><code><b>fun</b> <a href="sui_system.md#0x2_sui_system_extract_coin_balance">extract_coin_balance</a>(coins: <a href="">vector</a>&lt;<a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;&gt;, amount: <a href="_Option">option::Option</a>&lt;u64&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system.md#0x2_sui_system_extract_coin_balance">extract_coin_balance</a>(coins: <a href="">vector</a>&lt;Coin&lt;SUI&gt;&gt;, amount: <a href="_Option">option::Option</a>&lt;u64&gt;, ctx: &<b>mut</b> TxContext): Balance&lt;SUI&gt; {
    <b>let</b> merged_coin = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> coins);
    <a href="pay.md#0x2_pay_join_vec">pay::join_vec</a>(&<b>mut</b> merged_coin, coins);

    <b>let</b> total_balance = <a href="coin.md#0x2_coin_into_balance">coin::into_balance</a>(merged_coin);
    // <b>return</b> the full amount <b>if</b> amount is not specified
    <b>if</b> (<a href="_is_some">option::is_some</a>(&amount)) {
        <b>let</b> amount = <a href="_destroy_some">option::destroy_some</a>(amount);
        <b>let</b> <a href="balance.md#0x2_balance">balance</a> = <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> total_balance, amount);
        // <a href="transfer.md#0x2_transfer">transfer</a> back the remainder <b>if</b> non zero.
        <b>if</b> (<a href="balance.md#0x2_balance_value">balance::value</a>(&total_balance) &gt; 0) {
            <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(total_balance, ctx), <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx));
        } <b>else</b> {
            <a href="balance.md#0x2_balance_destroy_zero">balance::destroy_zero</a>(total_balance);
        };
        <a href="balance.md#0x2_balance">balance</a>
    } <b>else</b> {
        total_balance
    }
}
</code></pre>



</details>

<a name="0x2_sui_system_extract_locked_coin_balance"></a>

## Function `extract_locked_coin_balance`

Extract required Balance from vector of LockedCoin<SUI>, transfer the remainder back to sender.


<pre><code><b>fun</b> <a href="sui_system.md#0x2_sui_system_extract_locked_coin_balance">extract_locked_coin_balance</a>(coins: <a href="">vector</a>&lt;<a href="locked_coin.md#0x2_locked_coin_LockedCoin">locked_coin::LockedCoin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;&gt;, amount: <a href="_Option">option::Option</a>&lt;u64&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, <a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system.md#0x2_sui_system_extract_locked_coin_balance">extract_locked_coin_balance</a>(
    coins: <a href="">vector</a>&lt;LockedCoin&lt;SUI&gt;&gt;,
    amount: <a href="_Option">option::Option</a>&lt;u64&gt;,
    ctx: &<b>mut</b> TxContext
): (Balance&lt;SUI&gt;, EpochTimeLock) {
    <b>let</b> (total_balance, first_lock) = <a href="locked_coin.md#0x2_locked_coin_into_balance">locked_coin::into_balance</a>(<a href="_pop_back">vector::pop_back</a>(&<b>mut</b> coins));
    <b>let</b> (i, len) = (0, <a href="_length">vector::length</a>(&coins));
    <b>while</b> (i &lt; len) {
        <b>let</b> (<a href="balance.md#0x2_balance">balance</a>, lock) = <a href="locked_coin.md#0x2_locked_coin_into_balance">locked_coin::into_balance</a>(<a href="_pop_back">vector::pop_back</a>(&<b>mut</b> coins));
        // Make sure all time locks are the same
        <b>assert</b>!(<a href="epoch_time_lock.md#0x2_epoch_time_lock_epoch">epoch_time_lock::epoch</a>(&lock) == <a href="epoch_time_lock.md#0x2_epoch_time_lock_epoch">epoch_time_lock::epoch</a>(&first_lock), 0);
        <a href="epoch_time_lock.md#0x2_epoch_time_lock_destroy_unchecked">epoch_time_lock::destroy_unchecked</a>(lock);
        <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> total_balance, <a href="balance.md#0x2_balance">balance</a>);
        i = i + 1
    };
    <a href="_destroy_empty">vector::destroy_empty</a>(coins);

    // <b>return</b> the full amount <b>if</b> amount is not specified
    <b>if</b> (<a href="_is_some">option::is_some</a>(&amount)){
        <b>let</b> amount = <a href="_destroy_some">option::destroy_some</a>(amount);
        <b>let</b> <a href="balance.md#0x2_balance">balance</a> = <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> total_balance, amount);
        <b>if</b> (<a href="balance.md#0x2_balance_value">balance::value</a>(&total_balance) &gt; 0) {
            <a href="locked_coin.md#0x2_locked_coin_new_from_balance">locked_coin::new_from_balance</a>(total_balance, first_lock, <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx), ctx);
        } <b>else</b> {
            <a href="balance.md#0x2_balance_destroy_zero">balance::destroy_zero</a>(total_balance);
        };
        (<a href="balance.md#0x2_balance">balance</a>, first_lock)
    } <b>else</b>{
        (total_balance, first_lock)
    }
}
</code></pre>



</details>

<a name="0x2_sui_system_validators"></a>

## Function `validators`

Return the current validator set


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_validators">validators</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">sui_system::SuiSystemState</a>): &<a href="validator_set.md#0x2_validator_set_ValidatorSet">validator_set::ValidatorSet</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="sui_system.md#0x2_sui_system_validators">validators</a>(wrapper: &<a href="sui_system.md#0x2_sui_system_SuiSystemState">SuiSystemState</a>): &ValidatorSet {
    <b>let</b> self = <a href="sui_system.md#0x2_sui_system_load_system_state">load_system_state</a>(wrapper);
    &self.validators
}
</code></pre>



</details>
