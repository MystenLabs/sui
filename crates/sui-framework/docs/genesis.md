
<a name="0x2_genesis"></a>

# Module `0x2::genesis`



-  [Struct `GenesisValidatorMetadata`](#0x2_genesis_GenesisValidatorMetadata)
-  [Struct `GenesisChainParameters`](#0x2_genesis_GenesisChainParameters)
-  [Struct `TokenDistributionSchedule`](#0x2_genesis_TokenDistributionSchedule)
-  [Struct `TokenAllocation`](#0x2_genesis_TokenAllocation)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x2_genesis_create)
-  [Function `allocate_tokens`](#0x2_genesis_allocate_tokens)
-  [Function `activate_validators`](#0x2_genesis_activate_validators)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="clock.md#0x2_clock">0x2::clock</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="sui_system.md#0x2_sui_system">0x2::sui_system</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="validator.md#0x2_validator">0x2::validator</a>;
<b>use</b> <a href="validator_set.md#0x2_validator_set">0x2::validator_set</a>;
</code></pre>



<a name="0x2_genesis_GenesisValidatorMetadata"></a>

## Struct `GenesisValidatorMetadata`



<pre><code><b>struct</b> <a href="genesis.md#0x2_genesis_GenesisValidatorMetadata">GenesisValidatorMetadata</a> <b>has</b> <b>copy</b>, drop
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

<a name="0x2_genesis_GenesisChainParameters"></a>

## Struct `GenesisChainParameters`



<pre><code><b>struct</b> <a href="genesis.md#0x2_genesis_GenesisChainParameters">GenesisChainParameters</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>governance_start_epoch: u64</code>
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
</dl>


</details>

<a name="0x2_genesis_TokenDistributionSchedule"></a>

## Struct `TokenDistributionSchedule`



<pre><code><b>struct</b> <a href="genesis.md#0x2_genesis_TokenDistributionSchedule">TokenDistributionSchedule</a> <b>has</b> <b>copy</b>, drop
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
<code>allocations: <a href="">vector</a>&lt;<a href="genesis.md#0x2_genesis_TokenAllocation">genesis::TokenAllocation</a>&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_genesis_TokenAllocation"></a>

## Struct `TokenAllocation`



<pre><code><b>struct</b> <a href="genesis.md#0x2_genesis_TokenAllocation">TokenAllocation</a> <b>has</b> <b>copy</b>, drop
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


<a name="0x2_genesis_INIT_STAKE_SUBSIDY_AMOUNT"></a>

Stake subisidy to be given out in the very first epoch in Mist (1 million * 10^9).


<pre><code><b>const</b> <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_AMOUNT">INIT_STAKE_SUBSIDY_AMOUNT</a>: u64 = 1000000000000000;
</code></pre>



<a name="0x2_genesis_create"></a>

## Function `create`

This function will be explicitly called once at genesis.
It will create a singleton SuiSystemState object, which contains
all the information we need in the system.


<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_create">create</a>(genesis_chain_parameters: <a href="genesis.md#0x2_genesis_GenesisChainParameters">genesis::GenesisChainParameters</a>, genesis_validators: <a href="">vector</a>&lt;<a href="genesis.md#0x2_genesis_GenesisValidatorMetadata">genesis::GenesisValidatorMetadata</a>&gt;, token_distribution_schedule: <a href="genesis.md#0x2_genesis_TokenDistributionSchedule">genesis::TokenDistributionSchedule</a>, protocol_version: u64, system_state_version: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_create">create</a>(
    genesis_chain_parameters: <a href="genesis.md#0x2_genesis_GenesisChainParameters">GenesisChainParameters</a>,
    genesis_validators: <a href="">vector</a>&lt;<a href="genesis.md#0x2_genesis_GenesisValidatorMetadata">GenesisValidatorMetadata</a>&gt;,
    token_distribution_schedule: <a href="genesis.md#0x2_genesis_TokenDistributionSchedule">TokenDistributionSchedule</a>,
    protocol_version: u64,
    system_state_version: u64,
    ctx: &<b>mut</b> TxContext,
) {
    // Ensure this is only called at <a href="genesis.md#0x2_genesis">genesis</a>
    <b>assert</b>!(<a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) == 0, 0);

    <b>let</b> sui_supply = <a href="sui.md#0x2_sui_new">sui::new</a>(ctx);
    <b>let</b> subsidy_fund = <a href="balance.md#0x2_balance_split">balance::split</a>(
        &<b>mut</b> sui_supply,
        token_distribution_schedule.stake_subsidy_fund_mist
    );
    <b>let</b> storage_fund = <a href="balance.md#0x2_balance_zero">balance::zero</a>();

    // Create all the `Validator` structs
    <b>let</b> validators = <a href="_empty">vector::empty</a>();
    <b>let</b> count = <a href="_length">vector::length</a>(&genesis_validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; count) {
        <b>let</b> <a href="genesis.md#0x2_genesis_GenesisValidatorMetadata">GenesisValidatorMetadata</a> {
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

        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator.md#0x2_validator_new">validator::new</a>(
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

        <a href="_push_back">vector::push_back</a>(&<b>mut</b> validators, <a href="validator.md#0x2_validator">validator</a>);

        i = i + 1;
    };

    // Allocate tokens and staking operations
    <a href="genesis.md#0x2_genesis_allocate_tokens">allocate_tokens</a>(
        sui_supply,
        token_distribution_schedule.allocations,
        &<b>mut</b> validators,
        ctx
    );

    // Activate all validators
    <a href="genesis.md#0x2_genesis_activate_validators">activate_validators</a>(&<b>mut</b> validators);

    <a href="sui_system.md#0x2_sui_system_create">sui_system::create</a>(
        validators,
        subsidy_fund,
        storage_fund,
        genesis_chain_parameters.governance_start_epoch,
        <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_AMOUNT">INIT_STAKE_SUBSIDY_AMOUNT</a>,
        protocol_version,
        system_state_version,
        genesis_chain_parameters.chain_start_timestamp_ms,
        genesis_chain_parameters.epoch_duration_ms,
        ctx,
    );

    <a href="clock.md#0x2_clock_create">clock::create</a>();
}
</code></pre>



</details>

<a name="0x2_genesis_allocate_tokens"></a>

## Function `allocate_tokens`



<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_allocate_tokens">allocate_tokens</a>(sui_supply: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, allocations: <a href="">vector</a>&lt;<a href="genesis.md#0x2_genesis_TokenAllocation">genesis::TokenAllocation</a>&gt;, validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_allocate_tokens">allocate_tokens</a>(
    sui_supply: Balance&lt;SUI&gt;,
    allocations: <a href="">vector</a>&lt;<a href="genesis.md#0x2_genesis_TokenAllocation">TokenAllocation</a>&gt;,
    validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> count = <a href="_length">vector::length</a>(&allocations);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; count) {
        <b>let</b> allocation = *<a href="_borrow">vector::borrow</a>(&allocations, i);

        <b>let</b> allocation_balance = <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> sui_supply, allocation.amount_mist);

        <b>if</b> (<a href="_is_some">option::is_some</a>(&allocation.staked_with_validator)) {
            <b>let</b> validator_address = <a href="_destroy_some">option::destroy_some</a>(allocation.staked_with_validator);
            <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="validator_set.md#0x2_validator_set_get_validator_mut">validator_set::get_validator_mut</a>(validators, validator_address);
            <a href="validator.md#0x2_validator_request_add_stake_at_genesis">validator::request_add_stake_at_genesis</a>(
                <a href="validator.md#0x2_validator">validator</a>,
                allocation_balance,
                allocation.recipient_address,
                ctx
            );
        } <b>else</b> {
            <a href="sui.md#0x2_sui_transfer">sui::transfer</a>(
                <a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(allocation_balance, ctx),
                allocation.recipient_address,
            );
        };

        i = i + 1;
    };

    // Provided allocations must fully allocate the sui_supply and there
    // should be none left at this point.
    <a href="balance.md#0x2_balance_destroy_zero">balance::destroy_zero</a>(sui_supply);
}
</code></pre>



</details>

<a name="0x2_genesis_activate_validators"></a>

## Function `activate_validators`



<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_activate_validators">activate_validators</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_activate_validators">activate_validators</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;) {
    // Activate all <a href="genesis.md#0x2_genesis">genesis</a> validators
    <b>let</b> count = <a href="_length">vector::length</a>(validators);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; count) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(validators, i);
        <a href="validator.md#0x2_validator_activate">validator::activate</a>(<a href="validator.md#0x2_validator">validator</a>, 0);

        i = i + 1;
    };

}
</code></pre>



</details>
