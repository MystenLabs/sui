
<a name="0x3_genesis"></a>

# Module `0x3::genesis`



-  [Struct `GenesisValidatorMetadata`](#0x3_genesis_GenesisValidatorMetadata)
-  [Struct `GenesisChainParameters`](#0x3_genesis_GenesisChainParameters)
-  [Struct `TokenDistributionSchedule`](#0x3_genesis_TokenDistributionSchedule)
-  [Struct `TokenAllocation`](#0x3_genesis_TokenAllocation)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x3_genesis_create)
-  [Function `allocate_tokens`](#0x3_genesis_allocate_tokens)
-  [Function `activate_validators`](#0x3_genesis_activate_validators)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::vector</a>;
<b>use</b> <a href="">0x2::balance</a>;
<b>use</b> <a href="">0x2::coin</a>;
<b>use</b> <a href="">0x2::object</a>;
<b>use</b> <a href="">0x2::sui</a>;
<b>use</b> <a href="">0x2::tx_context</a>;
<b>use</b> <a href="sui_system.md#0x3_sui_system">0x3::sui_system</a>;
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
<code>name: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>description: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>image_url: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>project_url: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>sui_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>gas_price: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>commission_rate: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>protocol_public_key: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>proof_of_possession: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>network_public_key: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>worker_public_key: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>network_address: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>p2p_address: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>primary_address: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>worker_address: <a href="">vector</a>&lt;u8&gt;</code>
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
<code>protocol_version: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>stake_subsidy_start_epoch: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>chain_start_timestamp_ms: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>epoch_duration_ms: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>stake_subsidy_initial_distribution_amount: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>stake_subsidy_period_length: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>stake_subsidy_decrease_rate: u16</code>
</dt>
<dd>

</dd>
<dt>
<code>max_validator_count: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>min_validator_joining_stake: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>validator_low_stake_threshold: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>validator_very_low_stake_threshold: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>validator_low_stake_grace_period: u64</code>
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
<code>stake_subsidy_fund_mist: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>allocations: <a href="">vector</a>&lt;<a href="genesis.md#0x3_genesis_TokenAllocation">genesis::TokenAllocation</a>&gt;</code>
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
<code>amount_mist: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>staked_with_validator: <a href="_Option">option::Option</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>
 Indicates if this allocation should be staked at genesis and with which validator
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x3_genesis_INIT_STAKE_SUBSIDY_AMOUNT"></a>

Stake subisidy to be given out in the very first epoch in Mist (1 million * 10^9).


<pre><code><b>const</b> <a href="genesis.md#0x3_genesis_INIT_STAKE_SUBSIDY_AMOUNT">INIT_STAKE_SUBSIDY_AMOUNT</a>: u64 = 1000000000000000;
</code></pre>



<a name="0x3_genesis_create"></a>

## Function `create`

This function will be explicitly called once at genesis.
It will create a singleton SuiSystemState object, which contains
all the information we need in the system.


<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_create">create</a>(sui_system_state_id: <a href="_UID">object::UID</a>, sui_supply: <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, genesis_chain_parameters: <a href="genesis.md#0x3_genesis_GenesisChainParameters">genesis::GenesisChainParameters</a>, genesis_validators: <a href="">vector</a>&lt;<a href="genesis.md#0x3_genesis_GenesisValidatorMetadata">genesis::GenesisValidatorMetadata</a>&gt;, token_distribution_schedule: <a href="genesis.md#0x3_genesis_TokenDistributionSchedule">genesis::TokenDistributionSchedule</a>, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_create">create</a>(
    sui_system_state_id: UID,
    sui_supply: Balance&lt;SUI&gt;,
    genesis_chain_parameters: <a href="genesis.md#0x3_genesis_GenesisChainParameters">GenesisChainParameters</a>,
    genesis_validators: <a href="">vector</a>&lt;<a href="genesis.md#0x3_genesis_GenesisValidatorMetadata">GenesisValidatorMetadata</a>&gt;,
    token_distribution_schedule: <a href="genesis.md#0x3_genesis_TokenDistributionSchedule">TokenDistributionSchedule</a>,
    ctx: &<b>mut</b> TxContext,
) {
    // Ensure this is only called at <a href="genesis.md#0x3_genesis">genesis</a>
    <b>assert</b>!(<a href="_epoch">tx_context::epoch</a>(ctx) == 0, 0);

    <b>let</b> <a href="genesis.md#0x3_genesis_TokenDistributionSchedule">TokenDistributionSchedule</a> {
        stake_subsidy_fund_mist,
        allocations,
    } = token_distribution_schedule;

    <b>let</b> subsidy_fund = <a href="_split">balance::split</a>(
        &<b>mut</b> sui_supply,
        stake_subsidy_fund_mist,
    );
    <b>let</b> storage_fund = <a href="_zero">balance::zero</a>();

    // Create all the `Validator` structs
    <b>let</b> validators = <a href="_empty">vector::empty</a>();
    <b>let</b> count = <a href="_length">vector::length</a>(&genesis_validators);
    <b>let</b> i = 0;
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
        } = *<a href="_borrow">vector::borrow</a>(&genesis_validators, i);

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
            2,
        );

        <a href="_push_back">vector::push_back</a>(&<b>mut</b> validators, <a href="validator.md#0x3_validator">validator</a>);

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

    <a href="sui_system.md#0x3_sui_system_create">sui_system::create</a>(
        sui_system_state_id,
        validators,
        subsidy_fund,
        storage_fund,
        genesis_chain_parameters.protocol_version,
        genesis_chain_parameters.stake_subsidy_start_epoch,
        genesis_chain_parameters.chain_start_timestamp_ms,
        genesis_chain_parameters.epoch_duration_ms,
        genesis_chain_parameters.stake_subsidy_initial_distribution_amount,
        genesis_chain_parameters.stake_subsidy_period_length,
        genesis_chain_parameters.stake_subsidy_decrease_rate,
        genesis_chain_parameters.max_validator_count,
        genesis_chain_parameters.min_validator_joining_stake,
        genesis_chain_parameters.validator_low_stake_threshold,
        genesis_chain_parameters.validator_very_low_stake_threshold,
        genesis_chain_parameters.validator_low_stake_grace_period,
        ctx,
    );
}
</code></pre>



</details>

<a name="0x3_genesis_allocate_tokens"></a>

## Function `allocate_tokens`



<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_allocate_tokens">allocate_tokens</a>(sui_supply: <a href="_Balance">balance::Balance</a>&lt;<a href="_SUI">sui::SUI</a>&gt;, allocations: <a href="">vector</a>&lt;<a href="genesis.md#0x3_genesis_TokenAllocation">genesis::TokenAllocation</a>&gt;, validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, ctx: &<b>mut</b> <a href="_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_allocate_tokens">allocate_tokens</a>(
    sui_supply: Balance&lt;SUI&gt;,
    allocations: <a href="">vector</a>&lt;<a href="genesis.md#0x3_genesis_TokenAllocation">TokenAllocation</a>&gt;,
    validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;,
    ctx: &<b>mut</b> TxContext,
) {

    <b>while</b> (!<a href="_is_empty">vector::is_empty</a>(&allocations)) {
        <b>let</b> <a href="genesis.md#0x3_genesis_TokenAllocation">TokenAllocation</a> {
            recipient_address,
            amount_mist,
            staked_with_validator,
        } = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> allocations);

        <b>let</b> allocation_balance = <a href="_split">balance::split</a>(&<b>mut</b> sui_supply, amount_mist);

        <b>if</b> (<a href="_is_some">option::is_some</a>(&staked_with_validator)) {
            <b>let</b> validator_address = <a href="_destroy_some">option::destroy_some</a>(staked_with_validator);
            <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="validator_set.md#0x3_validator_set_get_validator_mut">validator_set::get_validator_mut</a>(validators, validator_address);
            <a href="validator.md#0x3_validator_request_add_stake_at_genesis">validator::request_add_stake_at_genesis</a>(
                <a href="validator.md#0x3_validator">validator</a>,
                allocation_balance,
                recipient_address,
                ctx
            );
        } <b>else</b> {
            <a href="_transfer">sui::transfer</a>(
                <a href="_from_balance">coin::from_balance</a>(allocation_balance, ctx),
                recipient_address,
            );
        };
    };
    <a href="_destroy_empty">vector::destroy_empty</a>(allocations);

    // Provided allocations must fully allocate the sui_supply and there
    // should be none left at this point.
    <a href="_destroy_zero">balance::destroy_zero</a>(sui_supply);
}
</code></pre>



</details>

<a name="0x3_genesis_activate_validators"></a>

## Function `activate_validators`



<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_activate_validators">activate_validators</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x3_genesis_activate_validators">activate_validators</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;) {
    // Activate all <a href="genesis.md#0x3_genesis">genesis</a> validators
    <b>let</b> count = <a href="_length">vector::length</a>(validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; count) {
        <b>let</b> <a href="validator.md#0x3_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(validators, i);
        <a href="validator.md#0x3_validator_activate">validator::activate</a>(<a href="validator.md#0x3_validator">validator</a>, 0);

        i = i + 1;
    };

}
</code></pre>



</details>
