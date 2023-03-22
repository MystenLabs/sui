
<a name="0x3_sui_system_state_inner"></a>

# Module `0x3::sui_system_state_inner`



-  [Struct `SystemParameters`](#0x3_sui_system_state_inner_SystemParameters)
-  [Struct `SuiSystemStateInner`](#0x3_sui_system_state_inner_SuiSystemStateInner)
-  [Struct `SystemEpochInfoEvent`](#0x3_sui_system_state_inner_SystemEpochInfoEvent)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x3_sui_system_state_inner_create)
-  [Function `request_add_validator_candidate`](#0x3_sui_system_state_inner_request_add_validator_candidate)
-  [Function `request_remove_validator_candidate`](#0x3_sui_system_state_inner_request_remove_validator_candidate)
-  [Function `request_add_validator`](#0x3_sui_system_state_inner_request_add_validator)
-  [Function `request_remove_validator`](#0x3_sui_system_state_inner_request_remove_validator)
-  [Function `request_set_gas_price`](#0x3_sui_system_state_inner_request_set_gas_price)
-  [Function `set_candidate_validator_gas_price`](#0x3_sui_system_state_inner_set_candidate_validator_gas_price)
-  [Function `request_set_commission_rate`](#0x3_sui_system_state_inner_request_set_commission_rate)
-  [Function `set_candidate_validator_commission_rate`](#0x3_sui_system_state_inner_set_candidate_validator_commission_rate)
-  [Function `request_add_stake`](#0x3_sui_system_state_inner_request_add_stake)
-  [Function `request_add_stake_mul_coin`](#0x3_sui_system_state_inner_request_add_stake_mul_coin)
-  [Function `request_withdraw_stake`](#0x3_sui_system_state_inner_request_withdraw_stake)
-  [Function `report_validator`](#0x3_sui_system_state_inner_report_validator)
-  [Function `undo_report_validator`](#0x3_sui_system_state_inner_undo_report_validator)
-  [Function `report_validator_impl`](#0x3_sui_system_state_inner_report_validator_impl)
-  [Function `undo_report_validator_impl`](#0x3_sui_system_state_inner_undo_report_validator_impl)
-  [Function `rotate_operation_cap`](#0x3_sui_system_state_inner_rotate_operation_cap)
-  [Function `update_validator_name`](#0x3_sui_system_state_inner_update_validator_name)
-  [Function `update_validator_description`](#0x3_sui_system_state_inner_update_validator_description)
-  [Function `update_validator_image_url`](#0x3_sui_system_state_inner_update_validator_image_url)
-  [Function `update_validator_project_url`](#0x3_sui_system_state_inner_update_validator_project_url)
-  [Function `update_validator_next_epoch_network_address`](#0x3_sui_system_state_inner_update_validator_next_epoch_network_address)
-  [Function `update_candidate_validator_network_address`](#0x3_sui_system_state_inner_update_candidate_validator_network_address)
-  [Function `update_validator_next_epoch_p2p_address`](#0x3_sui_system_state_inner_update_validator_next_epoch_p2p_address)
-  [Function `update_candidate_validator_p2p_address`](#0x3_sui_system_state_inner_update_candidate_validator_p2p_address)
-  [Function `update_validator_next_epoch_primary_address`](#0x3_sui_system_state_inner_update_validator_next_epoch_primary_address)
-  [Function `update_candidate_validator_primary_address`](#0x3_sui_system_state_inner_update_candidate_validator_primary_address)
-  [Function `update_validator_next_epoch_worker_address`](#0x3_sui_system_state_inner_update_validator_next_epoch_worker_address)
-  [Function `update_candidate_validator_worker_address`](#0x3_sui_system_state_inner_update_candidate_validator_worker_address)
-  [Function `update_validator_next_epoch_protocol_pubkey`](#0x3_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey)
-  [Function `update_candidate_validator_protocol_pubkey`](#0x3_sui_system_state_inner_update_candidate_validator_protocol_pubkey)
-  [Function `update_validator_next_epoch_worker_pubkey`](#0x3_sui_system_state_inner_update_validator_next_epoch_worker_pubkey)
-  [Function `update_candidate_validator_worker_pubkey`](#0x3_sui_system_state_inner_update_candidate_validator_worker_pubkey)
-  [Function `update_validator_next_epoch_network_pubkey`](#0x3_sui_system_state_inner_update_validator_next_epoch_network_pubkey)
-  [Function `update_candidate_validator_network_pubkey`](#0x3_sui_system_state_inner_update_candidate_validator_network_pubkey)
-  [Function `advance_epoch`](#0x3_sui_system_state_inner_advance_epoch)
-  [Function `advance_epoch_safe_mode`](#0x3_sui_system_state_inner_advance_epoch_safe_mode)
-  [Function `epoch`](#0x3_sui_system_state_inner_epoch)
-  [Function `protocol_version`](#0x3_sui_system_state_inner_protocol_version)
-  [Function `system_state_version`](#0x3_sui_system_state_inner_system_state_version)
-  [Function `epoch_start_timestamp_ms`](#0x3_sui_system_state_inner_epoch_start_timestamp_ms)
-  [Function `validator_stake_amount`](#0x3_sui_system_state_inner_validator_stake_amount)
-  [Function `validator_staking_pool_id`](#0x3_sui_system_state_inner_validator_staking_pool_id)
-  [Function `validator_staking_pool_mappings`](#0x3_sui_system_state_inner_validator_staking_pool_mappings)
-  [Function `get_reporters_of`](#0x3_sui_system_state_inner_get_reporters_of)
-  [Function `upgrade_system_state`](#0x3_sui_system_state_inner_upgrade_system_state)
-  [Function `extract_coin_balance`](#0x3_sui_system_state_inner_extract_coin_balance)
-  [Module Specification](#@Module_Specification_1)


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
<b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::string</a>;
<b>use</b> <a href="">0x2::bag</a>;
<b>use</b> <a href="">0x2::balance</a>;
<b>use</b> <a href="">0x2::coin</a>;
<b>use</b> <a href="">0x2::event</a>;
<b>use</b> <a href="">0x2::object</a>;
<b>use</b> <a href="">0x2::pay</a>;
<b>use</b> <a href="">0x2::sui</a>;
<b>use</b> <a href="">0x2::table</a>;
<b>use</b> <a href="">0x2::transfer</a>;
<b>use</b> <a href="">0x2::tx_context</a>;
<b>use</b> <a href="">0x2::url</a>;
<b>use</b> <a href="">0x2::vec_map</a>;
<b>use</b> <a href="">0x2::vec_set</a>;
<b>use</b> <a href="stake_subsidy.md#0x3_stake_subsidy">0x3::stake_subsidy</a>;
<b>use</b> <a href="staking_pool.md#0x3_staking_pool">0x3::staking_pool</a>;
<b>use</b> <a href="validator.md#0x3_validator">0x3::validator</a>;
<b>use</b> <a href="validator_cap.md#0x3_validator_cap">0x3::validator_cap</a>;
<b>use</b> <a href="validator_set.md#0x3_validator_set">0x3::validator_set</a>;
</code></pre>



<a name="0x3_sui_system_state_inner_SystemParameters"></a>

## Struct `SystemParameters`

A list of system config parameters.


<pre><code><b>struct</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">SystemParameters</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>governance_start_epoch: u64</code>
</dt>
<dd>
 The starting epoch in which various on-chain governance features take effect:
 - stake subsidies are paid out
</dd>
<dt>
<code>epoch_duration_ms: u64</code>
</dt>
<dd>
 The duration of an epoch, in milliseconds.
</dd>
<dt>
<code>extra_fields: <a href="_Bag">bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="0x3_sui_system_state_inner_SuiSystemStateInner"></a>

## Struct `SuiSystemStateInner`

The top-level object containing all information of the Sui system.


<pre><code><b>struct</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
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
<code>system_state_version: u64</code>
</dt>
<dd>
 The current version of the system state data structure type.
 This is always the same as SuiSystemState.version. Keeping a copy here so that
 we know what version it is by inspecting SuiSystemStateInner as well.
</dd>
<dt>
<code>validators: <a href="validator_set.md#0x3_validator_set_ValidatorSet">validator_set::ValidatorSet</a></code>
</dt>
<dd>
 Contains all information about the validators.
</dd>
<dt>
<code>storage_fund: <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>
 The storage fund.
</dd>
<dt>
<code>parameters: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">sui_system_state_inner::SystemParameters</a></code>
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
<code>validator_report_records: <a href="_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;</code>
</dt>
<dd>
 A map storing the records of validator reporting each other.
 There is an entry in the map for each validator that has been reported
 at least once. The entry VecSet contains all the validators that reported
 them. If a validator has never been reported they don't have an entry in this map.
 This map persists across epoch: a peer continues being in a reported state until the
 reporter doesn't explicitly remove their report.
 Note that in case we want to support validator address change in future,
 the reports should be based on validator ids
</dd>
<dt>
<code><a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>: <a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">stake_subsidy::StakeSubsidy</a></code>
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
 MUSTFIX: We need to save pending gas rewards, so that we could redistribute them.
</dd>
<dt>
<code>epoch_start_timestamp_ms: u64</code>
</dt>
<dd>
 Unix timestamp of the current epoch start
</dd>
<dt>
<code>extra_fields: <a href="_Bag">bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="0x3_sui_system_state_inner_SystemEpochInfoEvent"></a>

## Struct `SystemEpochInfoEvent`

Event containing system-level epoch information, emitted during
the epoch advancement transaction.


<pre><code><b>struct</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemEpochInfoEvent">SystemEpochInfoEvent</a> <b>has</b> <b>copy</b>, drop
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
<code>storage_fund_reinvestment: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>storage_charge: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>storage_rebate: u64</code>
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
<code>total_stake_rewards_distributed: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>leftover_storage_fund_inflow: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x3_sui_system_state_inner_ACTIVE_OR_PENDING_VALIDATOR"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ACTIVE_OR_PENDING_VALIDATOR">ACTIVE_OR_PENDING_VALIDATOR</a>: u8 = 2;
</code></pre>



<a name="0x3_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>: u8 = 1;
</code></pre>



<a name="0x3_sui_system_state_inner_ANY_VALIDATOR"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ANY_VALIDATOR">ANY_VALIDATOR</a>: u8 = 3;
</code></pre>



<a name="0x3_sui_system_state_inner_BASIS_POINT_DENOMINATOR"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>: u128 = 10000;
</code></pre>



<a name="0x3_sui_system_state_inner_EBpsTooLarge"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EBpsTooLarge">EBpsTooLarge</a>: u64 = 5;
</code></pre>



<a name="0x3_sui_system_state_inner_ECannotReportOneself"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ECannotReportOneself">ECannotReportOneself</a>: u64 = 3;
</code></pre>



<a name="0x3_sui_system_state_inner_EEpochNumberMismatch"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EEpochNumberMismatch">EEpochNumberMismatch</a>: u64 = 2;
</code></pre>



<a name="0x3_sui_system_state_inner_ELimitExceeded"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ELimitExceeded">ELimitExceeded</a>: u64 = 1;
</code></pre>



<a name="0x3_sui_system_state_inner_ENotValidator"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ENotValidator">ENotValidator</a>: u64 = 0;
</code></pre>



<a name="0x3_sui_system_state_inner_EReportRecordNotFound"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EReportRecordNotFound">EReportRecordNotFound</a>: u64 = 4;
</code></pre>



<a name="0x3_sui_system_state_inner_EStakedSuiFromWrongEpoch"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EStakedSuiFromWrongEpoch">EStakedSuiFromWrongEpoch</a>: u64 = 6;
</code></pre>



<a name="0x3_sui_system_state_inner_MAX_VALIDATOR_COUNT"></a>

Maximum number of active validators at any moment.
We do not allow the number of validators in any epoch to go above this.


<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_MAX_VALIDATOR_COUNT">MAX_VALIDATOR_COUNT</a>: u64 = 150;
</code></pre>



<a name="0x3_sui_system_state_inner_MIN_VALIDATOR_JOINING_STAKE"></a>

Lower-bound on the amount of stake required to become a validator.


<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_MIN_VALIDATOR_JOINING_STAKE">MIN_VALIDATOR_JOINING_STAKE</a>: u64 = 30000000000000000;
</code></pre>



<a name="0x3_sui_system_state_inner_VALIDATOR_LOW_STAKE_GRACE_PERIOD"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_VALIDATOR_LOW_STAKE_GRACE_PERIOD">VALIDATOR_LOW_STAKE_GRACE_PERIOD</a>: u64 = 7;
</code></pre>



<a name="0x3_sui_system_state_inner_VALIDATOR_LOW_STAKE_THRESHOLD"></a>

Validators with stake amount below <code><a href="sui_system_state_inner.md#0x3_sui_system_state_inner_VALIDATOR_LOW_STAKE_THRESHOLD">VALIDATOR_LOW_STAKE_THRESHOLD</a></code> are considered to
have low stake and will be escorted out of the validator set after being below this
threshold for more than <code><a href="sui_system_state_inner.md#0x3_sui_system_state_inner_VALIDATOR_LOW_STAKE_GRACE_PERIOD">VALIDATOR_LOW_STAKE_GRACE_PERIOD</a></code> number of epochs.
And validators with stake below <code><a href="sui_system_state_inner.md#0x3_sui_system_state_inner_VALIDATOR_VERY_LOW_STAKE_THRESHOLD">VALIDATOR_VERY_LOW_STAKE_THRESHOLD</a></code> will be removed
immediately at epoch change, no grace period.


<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_VALIDATOR_LOW_STAKE_THRESHOLD">VALIDATOR_LOW_STAKE_THRESHOLD</a>: u64 = 25000000000000000;
</code></pre>



<a name="0x3_sui_system_state_inner_VALIDATOR_VERY_LOW_STAKE_THRESHOLD"></a>



<pre><code><b>const</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_VALIDATOR_VERY_LOW_STAKE_THRESHOLD">VALIDATOR_VERY_LOW_STAKE_THRESHOLD</a>: u64 = 20000000000000000;
</code></pre>



<a name="0x3_sui_system_state_inner_create"></a>

## Function `create`

Create a new SuiSystemState object and make it shared.
This function will be called only once in genesis.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_create">create</a>(validators: <a href="">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, stake_subsidy_fund: <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, storage_fund: <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, protocol_version: u64, system_state_version: u64, governance_start_epoch: u64, epoch_start_timestamp_ms: u64, epoch_duration_ms: u64, initial_stake_subsidy_distribution_amount: u64, stake_subsidy_period_length: u64, stake_subsidy_decrease_rate: u16, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>): <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_create">create</a>(
    validators: <a href="">vector</a>&lt;Validator&gt;,
    stake_subsidy_fund: Balance&lt;SUI&gt;,
    storage_fund: Balance&lt;SUI&gt;,
    protocol_version: u64,
    system_state_version: u64,
    governance_start_epoch: u64,
    epoch_start_timestamp_ms: u64,
    epoch_duration_ms: u64,
    initial_stake_subsidy_distribution_amount: u64,
    stake_subsidy_period_length: u64,
    stake_subsidy_decrease_rate: u16,
    ctx: &<b>mut</b> TxContext,
): <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> {
    <b>let</b> validators = <a href="validator_set.md#0x3_validator_set_new">validator_set::new</a>(validators, ctx);
    <b>let</b> reference_gas_price = <a href="validator_set.md#0x3_validator_set_derive_reference_gas_price">validator_set::derive_reference_gas_price</a>(&validators);
    <b>let</b> system_state = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> {
        epoch: 0,
        protocol_version,
        system_state_version,
        validators,
        storage_fund,
        parameters: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemParameters">SystemParameters</a> {
            governance_start_epoch,
            epoch_duration_ms,
            extra_fields: <a href="_new">bag::new</a>(ctx),
        },
        reference_gas_price,
        validator_report_records: <a href="_empty">vec_map::empty</a>(),
        <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>: <a href="stake_subsidy.md#0x3_stake_subsidy_create">stake_subsidy::create</a>(
            stake_subsidy_fund,
            initial_stake_subsidy_distribution_amount,
            stake_subsidy_period_length,
            stake_subsidy_decrease_rate,
            ctx
        ),
        safe_mode: <b>false</b>,
        epoch_start_timestamp_ms,
        extra_fields: <a href="_new">bag::new</a>(ctx),
    };
    system_state
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_add_validator_candidate"></a>

## Function `request_add_validator_candidate`

Can be called by anyone who wishes to become a validator candidate and starts accuring delegated
stakes in their staking pool. Once they have at least <code><a href="sui_system_state_inner.md#0x3_sui_system_state_inner_MIN_VALIDATOR_JOINING_STAKE">MIN_VALIDATOR_JOINING_STAKE</a></code> amount of stake they
can call <code>request_add_validator</code> to officially become an active validator at the next epoch.
Aborts if the caller is already a pending or active validator, or a validator candidate.
Note: <code>proof_of_possession</code> MUST be a valid signature using sui_address and protocol_pubkey_bytes.
To produce a valid PoP, run [fn test_proof_of_possession].


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator_candidate">request_add_validator_candidate</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, network_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, worker_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, name: <a href="">vector</a>&lt;u8&gt;, description: <a href="">vector</a>&lt;u8&gt;, image_url: <a href="">vector</a>&lt;u8&gt;, project_url: <a href="">vector</a>&lt;u8&gt;, net_address: <a href="">vector</a>&lt;u8&gt;, p2p_address: <a href="">vector</a>&lt;u8&gt;, primary_address: <a href="">vector</a>&lt;u8&gt;, worker_address: <a href="">vector</a>&lt;u8&gt;, gas_price: u64, commission_rate: u64, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator_candidate">request_add_validator_candidate</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
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
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator.md#0x3_validator_new">validator::new</a>(
        <a href="_sender">tx_context::sender</a>(ctx),
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
        ctx
    );

    <a href="validator_set.md#0x3_validator_set_request_add_validator_candidate">validator_set::request_add_validator_candidate</a>(&<b>mut</b> self.validators, <a href="validator.md#0x3_validator">validator</a>, ctx);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_remove_validator_candidate"></a>

## Function `request_remove_validator_candidate`

Called by a validator candidate to remove themselves from the candidacy. After this call
their staking pool becomes deactivate.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator_candidate">request_remove_validator_candidate</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator_candidate">request_remove_validator_candidate</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="validator_set.md#0x3_validator_set_request_remove_validator_candidate">validator_set::request_remove_validator_candidate</a>(&<b>mut</b> self.validators, ctx);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_add_validator"></a>

## Function `request_add_validator`

Called by a validator candidate to add themselves to the active validator set beginning next epoch.
Aborts if the validator is a duplicate with one of the pending or active validators, or if the amount of
stake the validator has doesn't meet the min threshold, or if the number of new validators for the next
epoch has already reached the maximum.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator">request_add_validator</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_validator">request_add_validator</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(
        <a href="validator_set.md#0x3_validator_set_next_epoch_validator_count">validator_set::next_epoch_validator_count</a>(&self.validators) &lt; <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_MAX_VALIDATOR_COUNT">MAX_VALIDATOR_COUNT</a>,
        <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ELimitExceeded">ELimitExceeded</a>,
    );

    <a href="validator_set.md#0x3_validator_set_request_add_validator">validator_set::request_add_validator</a>(&<b>mut</b> self.validators, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_MIN_VALIDATOR_JOINING_STAKE">MIN_VALIDATOR_JOINING_STAKE</a>, ctx);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_remove_validator"></a>

## Function `request_remove_validator`

A validator can call this function to request a removal in the next epoch.
We use the sender of <code>ctx</code> to look up the validator
(i.e. sender must match the sui_address in the validator).
At the end of the epoch, the <code><a href="validator.md#0x3_validator">validator</a></code> object will be returned to the sui_address
of the validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator">request_remove_validator</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_remove_validator">request_remove_validator</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="validator_set.md#0x3_validator_set_request_remove_validator">validator_set::request_remove_validator</a>(
        &<b>mut</b> self.validators,
        ctx,
    )
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_set_gas_price"></a>

## Function `request_set_gas_price`

A validator can call this function to submit a new gas price quote, to be
used for the reference gas price calculation at the end of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_gas_price">request_set_gas_price</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, new_gas_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_gas_price">request_set_gas_price</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    // Verify the represented <b>address</b> is an active or pending <a href="validator.md#0x3_validator">validator</a>, and the capability is still valid.
    <b>let</b> verified_cap = <a href="validator_set.md#0x3_validator_set_verify_cap">validator_set::verify_cap</a>(&<b>mut</b> self.validators, cap, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ACTIVE_OR_PENDING_VALIDATOR">ACTIVE_OR_PENDING_VALIDATOR</a>);
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_verified_cap">validator_set::get_validator_mut_with_verified_cap</a>(&<b>mut</b> self.validators, &verified_cap, <b>false</b> /* include_candidate */);

    <a href="validator.md#0x3_validator_request_set_gas_price">validator::request_set_gas_price</a>(<a href="validator.md#0x3_validator">validator</a>, verified_cap, new_gas_price);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_set_candidate_validator_gas_price"></a>

## Function `set_candidate_validator_gas_price`

This function is used to set new gas price for candidate validators


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_gas_price">set_candidate_validator_gas_price</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, new_gas_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_gas_price">set_candidate_validator_gas_price</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    cap: &UnverifiedValidatorOperationCap,
    new_gas_price: u64,
) {
    // Verify the represented <b>address</b> is an active or pending <a href="validator.md#0x3_validator">validator</a>, and the capability is still valid.
    <b>let</b> verified_cap = <a href="validator_set.md#0x3_validator_set_verify_cap">validator_set::verify_cap</a>(&<b>mut</b> self.validators, cap, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ANY_VALIDATOR">ANY_VALIDATOR</a>);
    <b>let</b> candidate = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_verified_cap">validator_set::get_validator_mut_with_verified_cap</a>(&<b>mut</b> self.validators, &verified_cap, <b>true</b> /* include_candidate */);
    <a href="validator.md#0x3_validator_set_candidate_gas_price">validator::set_candidate_gas_price</a>(candidate, verified_cap, new_gas_price)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_set_commission_rate"></a>

## Function `request_set_commission_rate`

A validator can call this function to set a new commission rate, updated at the end of
the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_commission_rate">request_set_commission_rate</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, new_commission_rate: u64, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_set_commission_rate">request_set_commission_rate</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    new_commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="validator_set.md#0x3_validator_set_request_set_commission_rate">validator_set::request_set_commission_rate</a>(
        &<b>mut</b> self.validators,
        new_commission_rate,
        ctx
    )
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_set_candidate_validator_commission_rate"></a>

## Function `set_candidate_validator_commission_rate`

This function is used to set new commission rate for candidate validators


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_commission_rate">set_candidate_validator_commission_rate</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, new_commission_rate: u64, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_set_candidate_validator_commission_rate">set_candidate_validator_commission_rate</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    new_commission_rate: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> candidate = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_set_candidate_commission_rate">validator::set_candidate_commission_rate</a>(candidate, new_commission_rate)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_add_stake"></a>

## Function `request_add_stake`

Add stake to a validator's staking pool.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake">request_add_stake</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, stake: <a href="_Coin">coin::Coin</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake">request_add_stake</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    stake: Coin&lt;SUI&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="validator_set.md#0x3_validator_set_request_add_stake">validator_set::request_add_stake</a>(
        &<b>mut</b> self.validators,
        validator_address,
        <a href="_into_balance">coin::into_balance</a>(stake),
        ctx,
    );
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_add_stake_mul_coin"></a>

## Function `request_add_stake_mul_coin`

Add stake to a validator's staking pool using multiple coins.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake_mul_coin">request_add_stake_mul_coin</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, stakes: <a href="">vector</a>&lt;<a href="_Coin">coin::Coin</a>&lt;<a href="_SUI">sui::SUI</a>&gt;&gt;, stake_amount: <a href="_Option">option::Option</a>&lt;u64&gt;, validator_address: <b>address</b>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_add_stake_mul_coin">request_add_stake_mul_coin</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    stakes: <a href="">vector</a>&lt;Coin&lt;SUI&gt;&gt;,
    stake_amount: <a href="_Option">option::Option</a>&lt;u64&gt;,
    validator_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> <a href="">balance</a> = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_extract_coin_balance">extract_coin_balance</a>(stakes, stake_amount, ctx);
    <a href="validator_set.md#0x3_validator_set_request_add_stake">validator_set::request_add_stake</a>(&<b>mut</b> self.validators, validator_address, <a href="">balance</a>, ctx);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_request_withdraw_stake"></a>

## Function `request_withdraw_stake`

Withdraw some portion of a stake from a validator's staking pool.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_withdraw_stake">request_withdraw_stake</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, staked_sui: <a href="staking_pool.md#0x3_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_request_withdraw_stake">request_withdraw_stake</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    staked_sui: StakedSui,
    ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(stake_activation_epoch(&staked_sui) &lt;= <a href="_epoch">tx_context::epoch</a>(ctx), 0);
    <a href="validator_set.md#0x3_validator_set_request_withdraw_stake">validator_set::request_withdraw_stake</a>(
        &<b>mut</b> self.validators, staked_sui, ctx,
    );
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_report_validator"></a>

## Function `report_validator`

Report a validator as a bad or non-performant actor in the system.
Succeeds if all the following are satisfied:
1. both the reporter in <code>cap</code> and the input <code>reportee_addr</code> are active validators.
2. reporter and reportee not the same address.
3. the cap object is still valid.
This function is idempotent.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator">report_validator</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, reportee_addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator">report_validator</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: <b>address</b>,
) {
    // Reportee needs <b>to</b> be an active <a href="validator.md#0x3_validator">validator</a>
    <b>assert</b>!(<a href="validator_set.md#0x3_validator_set_is_active_validator_by_sui_address">validator_set::is_active_validator_by_sui_address</a>(&self.validators, reportee_addr), <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ENotValidator">ENotValidator</a>);
    // Verify the represented reporter <b>address</b> is an active <a href="validator.md#0x3_validator">validator</a>, and the capability is still valid.
    <b>let</b> verified_cap = <a href="validator_set.md#0x3_validator_set_verify_cap">validator_set::verify_cap</a>(&<b>mut</b> self.validators, cap, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator_impl">report_validator_impl</a>(verified_cap, reportee_addr, &<b>mut</b> self.validator_report_records);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_undo_report_validator"></a>

## Function `undo_report_validator`

Undo a <code>report_validator</code> action. Aborts if
1. the reportee is not a currently active validator or
2. the sender has not previously reported the <code>reportee_addr</code>, or
3. the cap is not valid


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator">undo_report_validator</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, cap: &<a href="validator_cap.md#0x3_validator_cap_UnverifiedValidatorOperationCap">validator_cap::UnverifiedValidatorOperationCap</a>, reportee_addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator">undo_report_validator</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    cap: &UnverifiedValidatorOperationCap,
    reportee_addr: <b>address</b>,
) {
    <b>let</b> verified_cap = <a href="validator_set.md#0x3_validator_set_verify_cap">validator_set::verify_cap</a>(&<b>mut</b> self.validators, cap, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ACTIVE_VALIDATOR_ONLY">ACTIVE_VALIDATOR_ONLY</a>);
    <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator_impl">undo_report_validator_impl</a>(verified_cap, reportee_addr, &<b>mut</b> self.validator_report_records);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_report_validator_impl"></a>

## Function `report_validator_impl`



<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator_impl">report_validator_impl</a>(verified_cap: <a href="validator_cap.md#0x3_validator_cap_ValidatorOperationCap">validator_cap::ValidatorOperationCap</a>, reportee_addr: <b>address</b>, validator_report_records: &<b>mut</b> <a href="_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_report_validator_impl">report_validator_impl</a>(
    verified_cap: ValidatorOperationCap,
    reportee_addr: <b>address</b>,
    validator_report_records: &<b>mut</b> VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
) {
    <b>let</b> reporter_address = *<a href="validator_cap.md#0x3_validator_cap_verified_operation_cap_address">validator_cap::verified_operation_cap_address</a>(&verified_cap);
    <b>assert</b>!(reporter_address != reportee_addr, <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_ECannotReportOneself">ECannotReportOneself</a>);
    <b>if</b> (!<a href="_contains">vec_map::contains</a>(validator_report_records, &reportee_addr)) {
        <a href="_insert">vec_map::insert</a>(validator_report_records, reportee_addr, <a href="_singleton">vec_set::singleton</a>(reporter_address));
    } <b>else</b> {
        <b>let</b> reporters = <a href="_get_mut">vec_map::get_mut</a>(validator_report_records, &reportee_addr);
        <b>if</b> (!<a href="_contains">vec_set::contains</a>(reporters, &reporter_address)) {
            <a href="_insert">vec_set::insert</a>(reporters, reporter_address);
        }
    }
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_undo_report_validator_impl"></a>

## Function `undo_report_validator_impl`



<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator_impl">undo_report_validator_impl</a>(verified_cap: <a href="validator_cap.md#0x3_validator_cap_ValidatorOperationCap">validator_cap::ValidatorOperationCap</a>, reportee_addr: <b>address</b>, validator_report_records: &<b>mut</b> <a href="_VecMap">vec_map::VecMap</a>&lt;<b>address</b>, <a href="_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_undo_report_validator_impl">undo_report_validator_impl</a>(
    verified_cap: ValidatorOperationCap,
    reportee_addr: <b>address</b>,
    validator_report_records: &<b>mut</b> VecMap&lt;<b>address</b>, VecSet&lt;<b>address</b>&gt;&gt;,
) {
    <b>assert</b>!(<a href="_contains">vec_map::contains</a>(validator_report_records, &reportee_addr), <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EReportRecordNotFound">EReportRecordNotFound</a>);
    <b>let</b> reporters = <a href="_get_mut">vec_map::get_mut</a>(validator_report_records, &reportee_addr);

    <b>let</b> reporter_addr = *<a href="validator_cap.md#0x3_validator_cap_verified_operation_cap_address">validator_cap::verified_operation_cap_address</a>(&verified_cap);
    <b>assert</b>!(<a href="_contains">vec_set::contains</a>(reporters, &reporter_addr), <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EReportRecordNotFound">EReportRecordNotFound</a>);

    <a href="_remove">vec_set::remove</a>(reporters, &reporter_addr);
    <b>if</b> (<a href="_is_empty">vec_set::is_empty</a>(reporters)) {
        <a href="_remove">vec_map::remove</a>(validator_report_records, &reportee_addr);
    }
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_rotate_operation_cap"></a>

## Function `rotate_operation_cap`

Create a new <code>UnverifiedValidatorOperationCap</code>, transfer it to the
validator and registers it. The original object is thus revoked.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_rotate_operation_cap">rotate_operation_cap</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_rotate_operation_cap">rotate_operation_cap</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_new_unverified_validator_operation_cap_and_transfer">validator::new_unverified_validator_operation_cap_and_transfer</a>(<a href="validator.md#0x3_validator">validator</a>, ctx);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_name"></a>

## Function `update_validator_name`

Update a validator's name.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_name">update_validator_name</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, name: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_name">update_validator_name</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    name: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_update_name">validator::update_name</a>(<a href="validator.md#0x3_validator">validator</a>, <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(name)));
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_description"></a>

## Function `update_validator_description`

Update a validator's description


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_description">update_validator_description</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, description: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_description">update_validator_description</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    description: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_update_description">validator::update_description</a>(<a href="validator.md#0x3_validator">validator</a>, <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(description)));
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_image_url"></a>

## Function `update_validator_image_url`

Update a validator's image url


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_image_url">update_validator_image_url</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, image_url: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_image_url">update_validator_image_url</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    image_url: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_update_image_url">validator::update_image_url</a>(<a href="validator.md#0x3_validator">validator</a>, <a href="_new_unsafe_from_bytes">url::new_unsafe_from_bytes</a>(image_url));
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_project_url"></a>

## Function `update_validator_project_url`

Update a validator's project url


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_project_url">update_validator_project_url</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, project_url: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_project_url">update_validator_project_url</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    project_url: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_update_project_url">validator::update_project_url</a>(<a href="validator.md#0x3_validator">validator</a>, <a href="_new_unsafe_from_bytes">url::new_unsafe_from_bytes</a>(project_url));
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_network_address"></a>

## Function `update_validator_next_epoch_network_address`

Update a validator's network address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_address">update_validator_next_epoch_network_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, network_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_address">update_validator_next_epoch_network_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    network_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx">validator_set::get_validator_mut_with_ctx</a>(&<b>mut</b> self.validators, ctx);
    <b>let</b> network_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(network_address));
    <a href="validator.md#0x3_validator_update_next_epoch_network_address">validator::update_next_epoch_network_address</a>(<a href="validator.md#0x3_validator">validator</a>, network_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_network_address"></a>

## Function `update_candidate_validator_network_address`

Update candidate validator's network address.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_address">update_candidate_validator_network_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, network_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_address">update_candidate_validator_network_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    network_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <b>let</b> network_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(network_address));
    <a href="validator.md#0x3_validator_update_candidate_network_address">validator::update_candidate_network_address</a>(candidate, network_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_p2p_address"></a>

## Function `update_validator_next_epoch_p2p_address`

Update a validator's p2p address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_p2p_address">update_validator_next_epoch_p2p_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, p2p_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_p2p_address">update_validator_next_epoch_p2p_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    p2p_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx">validator_set::get_validator_mut_with_ctx</a>(&<b>mut</b> self.validators, ctx);
    <b>let</b> p2p_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(p2p_address));
    <a href="validator.md#0x3_validator_update_next_epoch_p2p_address">validator::update_next_epoch_p2p_address</a>(<a href="validator.md#0x3_validator">validator</a>, p2p_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_p2p_address"></a>

## Function `update_candidate_validator_p2p_address`

Update candidate validator's p2p address.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_p2p_address">update_candidate_validator_p2p_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, p2p_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_p2p_address">update_candidate_validator_p2p_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    p2p_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <b>let</b> p2p_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(p2p_address));
    <a href="validator.md#0x3_validator_update_candidate_p2p_address">validator::update_candidate_p2p_address</a>(candidate, p2p_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_primary_address"></a>

## Function `update_validator_next_epoch_primary_address`

Update a validator's narwhal primary address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_primary_address">update_validator_next_epoch_primary_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, primary_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_primary_address">update_validator_next_epoch_primary_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    primary_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx">validator_set::get_validator_mut_with_ctx</a>(&<b>mut</b> self.validators, ctx);
    <b>let</b> primary_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(primary_address));
    <a href="validator.md#0x3_validator_update_next_epoch_primary_address">validator::update_next_epoch_primary_address</a>(<a href="validator.md#0x3_validator">validator</a>, primary_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_primary_address"></a>

## Function `update_candidate_validator_primary_address`

Update candidate validator's narwhal primary address.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_primary_address">update_candidate_validator_primary_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, primary_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_primary_address">update_candidate_validator_primary_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    primary_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <b>let</b> primary_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(primary_address));
    <a href="validator.md#0x3_validator_update_candidate_primary_address">validator::update_candidate_primary_address</a>(candidate, primary_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_worker_address"></a>

## Function `update_validator_next_epoch_worker_address`

Update a validator's narwhal worker address.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_address">update_validator_next_epoch_worker_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, worker_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_address">update_validator_next_epoch_worker_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    worker_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx">validator_set::get_validator_mut_with_ctx</a>(&<b>mut</b> self.validators, ctx);
    <b>let</b> worker_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(worker_address));
    <a href="validator.md#0x3_validator_update_next_epoch_worker_address">validator::update_next_epoch_worker_address</a>(<a href="validator.md#0x3_validator">validator</a>, worker_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_worker_address"></a>

## Function `update_candidate_validator_worker_address`

Update candidate validator's narwhal worker address.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_address">update_candidate_validator_worker_address</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, worker_address: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_address">update_candidate_validator_worker_address</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    worker_address: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <b>let</b> worker_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(worker_address));
    <a href="validator.md#0x3_validator_update_candidate_worker_address">validator::update_candidate_worker_address</a>(candidate, worker_address);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey"></a>

## Function `update_validator_next_epoch_protocol_pubkey`

Update a validator's public key of protocol key and proof of possession.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey">update_validator_next_epoch_protocol_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, protocol_pubkey: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_protocol_pubkey">update_validator_next_epoch_protocol_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    protocol_pubkey: <a href="">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx">validator_set::get_validator_mut_with_ctx</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_update_next_epoch_protocol_pubkey">validator::update_next_epoch_protocol_pubkey</a>(<a href="validator.md#0x3_validator">validator</a>, protocol_pubkey, proof_of_possession);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_protocol_pubkey"></a>

## Function `update_candidate_validator_protocol_pubkey`

Update candidate validator's public key of protocol key and proof of possession.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_protocol_pubkey">update_candidate_validator_protocol_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, protocol_pubkey: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_protocol_pubkey">update_candidate_validator_protocol_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    protocol_pubkey: <a href="">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_update_candidate_protocol_pubkey">validator::update_candidate_protocol_pubkey</a>(candidate, protocol_pubkey, proof_of_possession);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_worker_pubkey"></a>

## Function `update_validator_next_epoch_worker_pubkey`

Update a validator's public key of worker key.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_pubkey">update_validator_next_epoch_worker_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, worker_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_worker_pubkey">update_validator_next_epoch_worker_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    worker_pubkey: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx">validator_set::get_validator_mut_with_ctx</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_update_next_epoch_worker_pubkey">validator::update_next_epoch_worker_pubkey</a>(<a href="validator.md#0x3_validator">validator</a>, worker_pubkey);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_worker_pubkey"></a>

## Function `update_candidate_validator_worker_pubkey`

Update candidate validator's public key of worker key.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_pubkey">update_candidate_validator_worker_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, worker_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_worker_pubkey">update_candidate_validator_worker_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    worker_pubkey: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_update_candidate_worker_pubkey">validator::update_candidate_worker_pubkey</a>(candidate, worker_pubkey);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_validator_next_epoch_network_pubkey"></a>

## Function `update_validator_next_epoch_network_pubkey`

Update a validator's public key of network key.
The change will only take effects starting from the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_pubkey">update_validator_next_epoch_network_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, network_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_validator_next_epoch_network_pubkey">update_validator_next_epoch_network_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    network_pubkey: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx">validator_set::get_validator_mut_with_ctx</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_update_next_epoch_network_pubkey">validator::update_next_epoch_network_pubkey</a>(<a href="validator.md#0x3_validator">validator</a>, network_pubkey);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_update_candidate_validator_network_pubkey"></a>

## Function `update_candidate_validator_network_pubkey`

Update candidate validator's public key of network key.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_pubkey">update_candidate_validator_network_pubkey</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, network_pubkey: <a href="">vector</a>&lt;u8&gt;, ctx: &<a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_update_candidate_validator_network_pubkey">update_candidate_validator_network_pubkey</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    network_pubkey: <a href="">vector</a>&lt;u8&gt;,
    ctx: &TxContext,
) {
    <b>let</b> candidate = <a href="validator_set.md#0x3_validator_set_get_validator_mut_with_ctx_including_candidates">validator_set::get_validator_mut_with_ctx_including_candidates</a>(&<b>mut</b> self.validators, ctx);
    <a href="validator.md#0x3_validator_update_candidate_network_pubkey">validator::update_candidate_network_pubkey</a>(candidate, network_pubkey);
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_advance_epoch"></a>

## Function `advance_epoch`

This function should be called at the end of an epoch, and advances the system to the next epoch.
It does the following things:
1. Add storage charge to the storage fund.
2. Burn the storage rebates from the storage fund. These are already refunded to transaction sender's
gas coins.
3. Distribute computation charge to validator stake.
4. Update all validators.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch">advance_epoch</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, new_epoch: u64, next_protocol_version: u64, storage_reward: <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, computation_reward: <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, storage_rebate_amount: u64, storage_fund_reinvest_rate: u64, reward_slashing_rate: u64, epoch_start_timestamp_ms: u64, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>): <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch">advance_epoch</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    new_epoch: u64,
    next_protocol_version: u64,
    storage_reward: Balance&lt;SUI&gt;,
    computation_reward: Balance&lt;SUI&gt;,
    storage_rebate_amount: u64,
    storage_fund_reinvest_rate: u64, // share of storage fund's rewards that's reinvested
                                     // into storage fund, in basis point.
    reward_slashing_rate: u64, // how much rewards are slashed <b>to</b> punish a <a href="validator.md#0x3_validator">validator</a>, in bps.
    epoch_start_timestamp_ms: u64, // Timestamp of the epoch start
    ctx: &<b>mut</b> TxContext,
) : Balance&lt;SUI&gt; {
    self.epoch_start_timestamp_ms = epoch_start_timestamp_ms;

    <b>let</b> bps_denominator_u64 = (<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a> <b>as</b> u64);
    // Rates can't be higher than 100%.
    <b>assert</b>!(
        storage_fund_reinvest_rate &lt;= bps_denominator_u64
        && reward_slashing_rate &lt;= bps_denominator_u64,
        <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_EBpsTooLarge">EBpsTooLarge</a>,
    );

    <b>let</b> total_validators_stake = <a href="validator_set.md#0x3_validator_set_total_stake">validator_set::total_stake</a>(&self.validators);
    <b>let</b> storage_fund_balance = <a href="_value">balance::value</a>(&self.storage_fund);
    <b>let</b> total_stake = storage_fund_balance + total_validators_stake;

    <b>let</b> storage_charge = <a href="_value">balance::value</a>(&storage_reward);
    <b>let</b> computation_charge = <a href="_value">balance::value</a>(&computation_reward);

    // Include stake subsidy in the rewards given out <b>to</b> validators and stakers.
    // Delay distributing any stake subsidies until after `governance_start_epoch`.
    <b>let</b> <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a> = <b>if</b> (<a href="_epoch">tx_context::epoch</a>(ctx) &gt;= self.parameters.governance_start_epoch) {
        <a href="stake_subsidy.md#0x3_stake_subsidy_advance_epoch">stake_subsidy::advance_epoch</a>(&<b>mut</b> self.<a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>)
    } <b>else</b> {
        <a href="_zero">balance::zero</a>()
    };

    <b>let</b> stake_subsidy_amount = <a href="_value">balance::value</a>(&<a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>);
    <a href="_join">balance::join</a>(&<b>mut</b> computation_reward, <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>);

    <b>let</b> total_stake_u128 = (total_stake <b>as</b> u128);
    <b>let</b> computation_charge_u128 = (computation_charge <b>as</b> u128);

    <a href="_join">balance::join</a>(&<b>mut</b> self.storage_fund, storage_reward);

    <b>let</b> storage_fund_reward_amount = (storage_fund_balance <b>as</b> u128) * computation_charge_u128 / total_stake_u128;
    <b>let</b> storage_fund_reward = <a href="_split">balance::split</a>(&<b>mut</b> computation_reward, (storage_fund_reward_amount <b>as</b> u64));
    <b>let</b> storage_fund_reinvestment_amount =
        storage_fund_reward_amount * (storage_fund_reinvest_rate <b>as</b> u128) / <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;
    <b>let</b> storage_fund_reinvestment = <a href="_split">balance::split</a>(
        &<b>mut</b> storage_fund_reward,
        (storage_fund_reinvestment_amount <b>as</b> u64),
    );
    <a href="_join">balance::join</a>(&<b>mut</b> self.storage_fund, storage_fund_reinvestment);

    self.epoch = self.epoch + 1;
    // Sanity check <b>to</b> make sure we are advancing <b>to</b> the right epoch.
    <b>assert</b>!(new_epoch == self.epoch, 0);

    <b>let</b> computation_reward_amount_before_distribution = <a href="_value">balance::value</a>(&computation_reward);
    <b>let</b> storage_fund_reward_amount_before_distribution = <a href="_value">balance::value</a>(&storage_fund_reward);

    <a href="validator_set.md#0x3_validator_set_advance_epoch">validator_set::advance_epoch</a>(
        &<b>mut</b> self.validators,
        &<b>mut</b> computation_reward,
        &<b>mut</b> storage_fund_reward,
        &<b>mut</b> self.validator_report_records,
        reward_slashing_rate,
        <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_VALIDATOR_LOW_STAKE_THRESHOLD">VALIDATOR_LOW_STAKE_THRESHOLD</a>,
        <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_VALIDATOR_VERY_LOW_STAKE_THRESHOLD">VALIDATOR_VERY_LOW_STAKE_THRESHOLD</a>,
        <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_VALIDATOR_LOW_STAKE_GRACE_PERIOD">VALIDATOR_LOW_STAKE_GRACE_PERIOD</a>,
        self.parameters.governance_start_epoch,
        ctx,
    );

    <b>let</b> computation_reward_amount_after_distribution = <a href="_value">balance::value</a>(&computation_reward);
    <b>let</b> storage_fund_reward_amount_after_distribution = <a href="_value">balance::value</a>(&storage_fund_reward);
    <b>let</b> computation_reward_distributed = computation_reward_amount_before_distribution - computation_reward_amount_after_distribution;
    <b>let</b> storage_fund_reward_distributed = storage_fund_reward_amount_before_distribution - storage_fund_reward_amount_after_distribution;

    self.protocol_version = next_protocol_version;

    // Derive the reference gas price for the new epoch
    self.reference_gas_price = <a href="validator_set.md#0x3_validator_set_derive_reference_gas_price">validator_set::derive_reference_gas_price</a>(&self.validators);
    // Because of precision issues <b>with</b> integer divisions, we expect that there will be some
    // remaining <a href="">balance</a> in `storage_fund_reward` and `computation_reward`.
    // All of these go <b>to</b> the storage fund.
    <b>let</b> leftover_storage_fund_inflow = <a href="_value">balance::value</a>(&storage_fund_reward) + <a href="_value">balance::value</a>(&computation_reward);
    <a href="_join">balance::join</a>(&<b>mut</b> self.storage_fund, storage_fund_reward);
    <a href="_join">balance::join</a>(&<b>mut</b> self.storage_fund, computation_reward);

    // Destroy the storage rebate.
    <b>assert</b>!(<a href="_value">balance::value</a>(&self.storage_fund) &gt;= storage_rebate_amount, 0);
    <b>let</b> storage_rebate = <a href="_split">balance::split</a>(&<b>mut</b> self.storage_fund, storage_rebate_amount);

    <b>let</b> new_total_stake = <a href="validator_set.md#0x3_validator_set_total_stake">validator_set::total_stake</a>(&self.validators);

    <a href="_emit">event::emit</a>(
        <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SystemEpochInfoEvent">SystemEpochInfoEvent</a> {
            epoch: self.epoch,
            protocol_version: self.protocol_version,
            reference_gas_price: self.reference_gas_price,
            total_stake: new_total_stake,
            storage_charge,
            storage_fund_reinvestment: (storage_fund_reinvestment_amount <b>as</b> u64),
            storage_rebate: storage_rebate_amount,
            storage_fund_balance: <a href="_value">balance::value</a>(&self.storage_fund),
            stake_subsidy_amount,
            total_gas_fees: computation_charge,
            total_stake_rewards_distributed: computation_reward_distributed + storage_fund_reward_distributed,
            leftover_storage_fund_inflow,
        }
    );
    self.safe_mode = <b>false</b>;
    storage_rebate
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_advance_epoch_safe_mode"></a>

## Function `advance_epoch_safe_mode`

An extremely simple version of advance_epoch.
This is called in two situations:
- When the call to advance_epoch failed due to a bug, and we want to be able to keep the
system running and continue making epoch changes.
- When advancing to a new protocol version, we want to be able to change the protocol
version


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch_safe_mode">advance_epoch_safe_mode</a>(self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, new_epoch: u64, next_protocol_version: u64, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_advance_epoch_safe_mode">advance_epoch_safe_mode</a>(
    self: &<b>mut</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    new_epoch: u64,
    next_protocol_version: u64,
    ctx: &<b>mut</b> TxContext,
) {
    // Validator will make a special system call <b>with</b> sender set <b>as</b> 0x0.
    <b>assert</b>!(<a href="_sender">tx_context::sender</a>(ctx) == @0x0, 0);

    self.epoch = new_epoch;
    self.protocol_version = next_protocol_version;
    self.safe_mode = <b>true</b>;
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_epoch"></a>

## Function `epoch`

Return the current epoch number. Useful for applications that need a coarse-grained concept of time,
since epochs are ever-increasing and epoch changes are intended to happen every 24 hours.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch">epoch</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch">epoch</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>): u64 {
    self.epoch
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_protocol_version"></a>

## Function `protocol_version`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_protocol_version">protocol_version</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_protocol_version">protocol_version</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>): u64 {
    self.protocol_version
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_system_state_version"></a>

## Function `system_state_version`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_system_state_version">system_state_version</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_system_state_version">system_state_version</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>): u64 {
    self.system_state_version
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_epoch_start_timestamp_ms"></a>

## Function `epoch_start_timestamp_ms`

Returns unix timestamp of the start of current epoch


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_epoch_start_timestamp_ms">epoch_start_timestamp_ms</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>): u64 {
    self.epoch_start_timestamp_ms
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_validator_stake_amount"></a>

## Function `validator_stake_amount`

Returns the total amount staked with <code>validator_addr</code>.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_stake_amount">validator_stake_amount</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, validator_addr: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_stake_amount">validator_stake_amount</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>, validator_addr: <b>address</b>): u64 {
    <a href="validator_set.md#0x3_validator_set_validator_total_stake_amount">validator_set::validator_total_stake_amount</a>(&self.validators, validator_addr)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_validator_staking_pool_id"></a>

## Function `validator_staking_pool_id`

Returns the staking pool id of a given validator.
Aborts if <code>validator_addr</code> is not an active validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_staking_pool_id">validator_staking_pool_id</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, validator_addr: <b>address</b>): <a href="_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_staking_pool_id">validator_staking_pool_id</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>, validator_addr: <b>address</b>): ID {

    <a href="validator_set.md#0x3_validator_set_validator_staking_pool_id">validator_set::validator_staking_pool_id</a>(&self.validators, validator_addr)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_validator_staking_pool_mappings"></a>

## Function `validator_staking_pool_mappings`

Returns reference to the staking pool mappings that map pool ids to active validator addresses


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_staking_pool_mappings">validator_staking_pool_mappings</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>): &<a href="_Table">table::Table</a>&lt;<a href="_ID">object::ID</a>, <b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_validator_staking_pool_mappings">validator_staking_pool_mappings</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>): &Table&lt;ID, <b>address</b>&gt; {

    <a href="validator_set.md#0x3_validator_set_staking_pool_mappings">validator_set::staking_pool_mappings</a>(&self.validators)
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_get_reporters_of"></a>

## Function `get_reporters_of`

Returns all the validators who are currently reporting <code>addr</code>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_get_reporters_of">get_reporters_of</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, addr: <b>address</b>): <a href="_VecSet">vec_set::VecSet</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_get_reporters_of">get_reporters_of</a>(self: &<a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>, addr: <b>address</b>): VecSet&lt;<b>address</b>&gt; {

    <b>if</b> (<a href="_contains">vec_map::contains</a>(&self.validator_report_records, &addr)) {
        *<a href="_get">vec_map::get</a>(&self.validator_report_records, &addr)
    } <b>else</b> {
        <a href="_empty">vec_set::empty</a>()
    }
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_upgrade_system_state"></a>

## Function `upgrade_system_state`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_upgrade_system_state">upgrade_system_state</a>(self: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>, new_system_state_version: u64, _ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>): <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">sui_system_state_inner::SuiSystemStateInner</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_upgrade_system_state">upgrade_system_state</a>(
    self: <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a>,
    new_system_state_version: u64,
    _ctx: &<b>mut</b> TxContext,
): <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_SuiSystemStateInner">SuiSystemStateInner</a> {
    // Whenever we upgrade the system state version, we will have <b>to</b> first
    // ship a framework upgrade that introduces a new system state type, and make this
    // function generate such type from the <b>old</b> state.
    self.system_state_version = new_system_state_version;
    self
}
</code></pre>



</details>

<a name="0x3_sui_system_state_inner_extract_coin_balance"></a>

## Function `extract_coin_balance`

Extract required Balance from vector of Coin<SUI>, transfer the remainder back to sender.


<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_extract_coin_balance">extract_coin_balance</a>(coins: <a href="">vector</a>&lt;<a href="_Coin">coin::Coin</a>&lt;<a href="_SUI">sui::SUI</a>&gt;&gt;, amount: <a href="_Option">option::Option</a>&lt;u64&gt;, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>): <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_extract_coin_balance">extract_coin_balance</a>(coins: <a href="">vector</a>&lt;Coin&lt;SUI&gt;&gt;, amount: <a href="_Option">option::Option</a>&lt;u64&gt;, ctx: &<b>mut</b> TxContext): Balance&lt;SUI&gt; {
    <b>let</b> merged_coin = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> coins);
    <a href="_join_vec">pay::join_vec</a>(&<b>mut</b> merged_coin, coins);

    <b>let</b> total_balance = <a href="_into_balance">coin::into_balance</a>(merged_coin);
    // <b>return</b> the full amount <b>if</b> amount is not specified
    <b>if</b> (<a href="_is_some">option::is_some</a>(&amount)) {
        <b>let</b> amount = <a href="_destroy_some">option::destroy_some</a>(amount);
        <b>let</b> <a href="">balance</a> = <a href="_split">balance::split</a>(&<b>mut</b> total_balance, amount);
        // <a href="">transfer</a> back the remainder <b>if</b> non zero.
        <b>if</b> (<a href="_value">balance::value</a>(&total_balance) &gt; 0) {
            <a href="_public_transfer">transfer::public_transfer</a>(<a href="_from_balance">coin::from_balance</a>(total_balance, ctx), <a href="_sender">tx_context::sender</a>(ctx));
        } <b>else</b> {
            <a href="_destroy_zero">balance::destroy_zero</a>(total_balance);
        };
        <a href="">balance</a>
    } <b>else</b> {
        total_balance
    }
}
</code></pre>



</details>

<a name="@Module_Specification_1"></a>

## Module Specification



<pre><code><b>pragma</b> verify = <b>false</b>;
</code></pre>
