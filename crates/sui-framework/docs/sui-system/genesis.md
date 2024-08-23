---
title: Module `0x3::genesis`
---



-  [Struct `GenesisValidatorMetadata`](#0x3_genesis_GenesisValidatorMetadata)
-  [Struct `GenesisChainParameters`](#0x3_genesis_GenesisChainParameters)
-  [Struct `TokenDistributionSchedule`](#0x3_genesis_TokenDistributionSchedule)
-  [Struct `TokenAllocation`](#0x3_genesis_TokenAllocation)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x3_genesis_create)
-  [Function `allocate_tokens`](#0x3_genesis_allocate_tokens)
-  [Function `activate_validators`](#0x3_genesis_activate_validators)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="../sui-framework/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../sui-framework/sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="stake_subsidy.md#0x3_stake_subsidy">0x3::stake_subsidy</a>;
<b>use</b> <a href="sui_system.md#0x3_sui_system">0x3::sui_system</a>;
<b>use</b> <a href="sui_system_state_inner.md#0x3_sui_system_state_inner">0x3::sui_system_state_inner</a>;
<b>use</b> <a href="validator.md#0x3_validator">0x3::validator</a>;
<b>use</b> <a href="validator_set.md#0x3_validator_set">0x3::validator_set</a>;
</code></pre>



<a name="0x3_genesis_GenesisValidatorMetadata"></a>

## Struct `GenesisValidatorMetadata`



<pre><code><b>struct</b> <a href="genesis.md#0x3_genesis_GenesisValidatorMetadata">GenesisValidatorMetadata</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>name: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>description: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>image_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>project_url: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>sui_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>gas_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>commission_rate: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>protocol_public_key: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>proof_of_possession: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>network_public_key: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>worker_public_key: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>network_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>p2p_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>primary_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>worker_address: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x3_genesis_GenesisChainParameters"></a>

## Struct `GenesisChainParameters`



<pre><code><b>struct</b> <a href="genesis.md#0x3_genesis_GenesisChainParameters">GenesisChainParameters</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>protocol_version: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>chain_start_timestamp_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>epoch_duration_ms: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>stake_subsidy_start_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>stake_subsidy_initial_distribution_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>stake_subsidy_period_length: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>stake_subsidy_decrease_rate: u16</code>
</dt>
<dd>

</dd>
<dt>
<code>max_validator_count: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>min_validator_joining_stake: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>validator_low_stake_threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>validator_very_low_stake_threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>validator_low_stake_grace_period: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x3_genesis_TokenDistributionSchedule"></a>

## Struct `TokenDistributionSchedule`



<pre><code><b>struct</b> <a href="genesis.md#0x3_genesis_TokenDistributionSchedule">TokenDistributionSchedule</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>stake_subsidy_fund_mist: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>allocations: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="genesis.md#0x3_genesis_TokenAllocation">genesis::TokenAllocation</a>&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x3_genesis_TokenAllocation"></a>

## Struct `TokenAllocation`



<pre><code><b>struct</b> <a href="genesis.md#0x3_genesis_TokenAllocation">TokenAllocation</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>recipient_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>amount_mist: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>staked_with_validator: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>
 Indicates if this allocation should be staked at genesis and with which validator
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x3_genesis_EDuplicateValidator"></a>

The <code>create</code> function was called with duplicate validators.


<pre><code><b>const</b> <a href="genesis.md#0x3_genesis_EDuplicateValidator">EDuplicateValidator</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x3_genesis_ENotCalledAtGenesis"></a>

The <code>create</code> function was called at a non-genesis epoch.


<pre><code><b>const</b> <a href="genesis.md#0x3_genesis_ENotCalledAtGenesis">ENotCalledAtGenesis</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x3_genesis_create"></a>

## Function `create`

This function will be explicitly called once at genesis.
It will create a singleton SuiSystemState object, which contains
all the information we need in the system.


<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_create">create</a>(sui_system_state_id: <a href="../sui-framework/object.md#0x2_object_UID">object::UID</a>, sui_supply: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, genesis_chain_parameters: <a href="genesis.md#0x3_genesis_GenesisChainParameters">genesis::GenesisChainParameters</a>, genesis_validators: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="genesis.md#0x3_genesis_GenesisValidatorMetadata">genesis::GenesisValidatorMetadata</a>&gt;, token_distribution_schedule: <a href="genesis.md#0x3_genesis_TokenDistributionSchedule">genesis::TokenDistributionSchedule</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_create">create</a>(
    sui_system_state_id: UID,
    <b>mut</b> sui_supply: Balance&lt;SUI&gt;,
    genesis_chain_parameters: <a href="genesis.md#0x3_genesis_GenesisChainParameters">GenesisChainParameters</a>,
    genesis_validators: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="genesis.md#0x3_genesis_GenesisValidatorMetadata">GenesisValidatorMetadata</a>&gt;,
    token_distribution_schedule: <a href="genesis.md#0x3_genesis_TokenDistributionSchedule">TokenDistributionSchedule</a>,
    ctx: &<b>mut</b> TxContext,
) {
    // Ensure this is only called at <a href="genesis.md#0x3_genesis">genesis</a>
    <b>assert</b>!(ctx.epoch() == 0, <a href="genesis.md#0x3_genesis_ENotCalledAtGenesis">ENotCalledAtGenesis</a>);

    <b>let</b> <a href="genesis.md#0x3_genesis_TokenDistributionSchedule">TokenDistributionSchedule</a> {
        stake_subsidy_fund_mist,
        allocations,
    } = token_distribution_schedule;

    <b>let</b> subsidy_fund = sui_supply.split(stake_subsidy_fund_mist);
    <b>let</b> <a href="storage_fund.md#0x3_storage_fund">storage_fund</a> = <a href="../sui-framework/balance.md#0x2_balance_zero">balance::zero</a>();

    // Create all the `Validator` structs
    <b>let</b> <b>mut</b> validators = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>let</b> count = genesis_validators.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; count) {
        <b>let</b> <a href="genesis.md#0x3_genesis_GenesisValidatorMetadata">GenesisValidatorMetadata</a> {
            name,
            description,
            image_url,
            project_url,
            sui_address,
            gas_price,
            commission_rate,
            protocol_public_key,
            proof_of_possession,
            network_public_key,
            worker_public_key,
            network_address,
            p2p_address,
            primary_address,
            worker_address,
        } = genesis_validators[i];

        <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator.md#0x3_validator_new">validator::new</a>(
            sui_address,
            protocol_public_key,
            network_public_key,
            worker_public_key,
            proof_of_possession,
            name,
            description,
            image_url,
            project_url,
            network_address,
            p2p_address,
            primary_address,
            worker_address,
            gas_price,
            commission_rate,
            ctx
        );

        // Ensure that each <a href="validator.md#0x3_validator">validator</a> is unique
        <b>assert</b>!(
            !<a href="validator_set.md#0x3_validator_set_is_duplicate_validator">validator_set::is_duplicate_validator</a>(&validators, &<a href="validator.md#0x3_validator">validator</a>),
            <a href="genesis.md#0x3_genesis_EDuplicateValidator">EDuplicateValidator</a>,
        );

        validators.push_back(<a href="validator.md#0x3_validator">validator</a>);

        i = i + 1;
    };

    // Allocate tokens and staking operations
    <a href="genesis.md#0x3_genesis_allocate_tokens">allocate_tokens</a>(
        sui_supply,
        allocations,
        &<b>mut</b> validators,
        ctx
    );

    // Activate all validators
    <a href="genesis.md#0x3_genesis_activate_validators">activate_validators</a>(&<b>mut</b> validators);

    <b>let</b> system_parameters = <a href="sui_system_state_inner.md#0x3_sui_system_state_inner_create_system_parameters">sui_system_state_inner::create_system_parameters</a>(
        genesis_chain_parameters.epoch_duration_ms,
        genesis_chain_parameters.stake_subsidy_start_epoch,

        // Validator committee parameters
        genesis_chain_parameters.max_validator_count,
        genesis_chain_parameters.min_validator_joining_stake,
        genesis_chain_parameters.validator_low_stake_threshold,
        genesis_chain_parameters.validator_very_low_stake_threshold,
        genesis_chain_parameters.validator_low_stake_grace_period,

        ctx,
    );

    <b>let</b> <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a> = <a href="stake_subsidy.md#0x3_stake_subsidy_create">stake_subsidy::create</a>(
        subsidy_fund,
        genesis_chain_parameters.stake_subsidy_initial_distribution_amount,
        genesis_chain_parameters.stake_subsidy_period_length,
        genesis_chain_parameters.stake_subsidy_decrease_rate,
        ctx,
    );

    <a href="sui_system.md#0x3_sui_system_create">sui_system::create</a>(
        sui_system_state_id,
        validators,
        <a href="storage_fund.md#0x3_storage_fund">storage_fund</a>,
        genesis_chain_parameters.protocol_version,
        genesis_chain_parameters.chain_start_timestamp_ms,
        system_parameters,
        <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>,
        ctx,
    );
}
</code></pre>



</details>

<a name="0x3_genesis_allocate_tokens"></a>

## Function `allocate_tokens`



<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_allocate_tokens">allocate_tokens</a>(sui_supply: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, allocations: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="genesis.md#0x3_genesis_TokenAllocation">genesis::TokenAllocation</a>&gt;, validators: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_allocate_tokens">allocate_tokens</a>(
    <b>mut</b> sui_supply: Balance&lt;SUI&gt;,
    <b>mut</b> allocations: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="genesis.md#0x3_genesis_TokenAllocation">TokenAllocation</a>&gt;,
    validators: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;,
    ctx: &<b>mut</b> TxContext,
) {

    <b>while</b> (!allocations.is_empty()) {
        <b>let</b> <a href="genesis.md#0x3_genesis_TokenAllocation">TokenAllocation</a> {
            recipient_address,
            amount_mist,
            staked_with_validator,
        } = allocations.pop_back();

        <b>let</b> allocation_balance = sui_supply.split(amount_mist);

        <b>if</b> (staked_with_validator.is_some()) {
            <b>let</b> validator_address = staked_with_validator.destroy_some();
            <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut">validator_set::get_validator_mut</a>(validators, validator_address);
            <a href="validator.md#0x3_validator">validator</a>.request_add_stake_at_genesis(
                allocation_balance,
                recipient_address,
                ctx
            );
        } <b>else</b> {
            <a href="../sui-framework/sui.md#0x2_sui_transfer">sui::transfer</a>(
                allocation_balance.into_coin(ctx),
                recipient_address,
            );
        };
    };
    allocations.destroy_empty();

    // Provided allocations must fully allocate the sui_supply and there
    // should be none left at this point.
    sui_supply.destroy_zero();
}
</code></pre>



</details>

<a name="0x3_genesis_activate_validators"></a>

## Function `activate_validators`



<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_activate_validators">activate_validators</a>(validators: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_activate_validators">activate_validators</a>(validators: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;) {
    // Activate all <a href="genesis.md#0x3_genesis">genesis</a> validators
    <b>let</b> count = validators.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; count) {
        <b>let</b> <a href="validator.md#0x3_validator">validator</a> = &<b>mut</b> validators[i];
        <a href="validator.md#0x3_validator">validator</a>.activate(0);

        i = i + 1;
    };

}
</code></pre>



</details>
