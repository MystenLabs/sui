
<a name="0x2_genesis"></a>

# Module `0x2::genesis`



-  [Struct `GenesisValidatorMetadata`](#0x2_genesis_GenesisValidatorMetadata)
-  [Struct `GenesisChainParameters`](#0x2_genesis_GenesisChainParameters)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x2_genesis_create)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="clock.md#0x2_clock">0x2::clock</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="sui_system.md#0x2_sui_system">0x2::sui_system</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="validator.md#0x2_validator">0x2::validator</a>;
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
<code>initial_sui_custody_account_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>initial_validator_stake_mist: u64</code>
</dt>
<dd>

</dd>
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

<a name="@Constants_0"></a>

## Constants


<a name="0x2_genesis_INIT_STAKE_SUBSIDY_AMOUNT"></a>

Stake subisidy to be given out in the very first epoch in Mist (1 million * 10^9).


<pre><code><b>const</b> <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_AMOUNT">INIT_STAKE_SUBSIDY_AMOUNT</a>: u64 = 1000000000000000;
</code></pre>



<a name="0x2_genesis_INIT_STAKE_SUBSIDY_FUND_BALANCE"></a>

The initial balance of the Subsidy fund in Mist (1 Billion * 10^9)


<pre><code><b>const</b> <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_FUND_BALANCE">INIT_STAKE_SUBSIDY_FUND_BALANCE</a>: u64 = 1000000000000000000;
</code></pre>



<a name="0x2_genesis_INIT_STAKE_SUBSIDY_FUND_BALANCE_TEST_ONLY"></a>



<pre><code><b>const</b> <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_FUND_BALANCE_TEST_ONLY">INIT_STAKE_SUBSIDY_FUND_BALANCE_TEST_ONLY</a>: u64 = 100000000000000000;
</code></pre>



<a name="0x2_genesis_create"></a>

## Function `create`

This function will be explicitly called once at genesis.
It will create a singleton SuiSystemState object, which contains
all the information we need in the system.


<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_create">create</a>(genesis_chain_parameters: <a href="genesis.md#0x2_genesis_GenesisChainParameters">genesis::GenesisChainParameters</a>, genesis_validators: <a href="">vector</a>&lt;<a href="genesis.md#0x2_genesis_GenesisValidatorMetadata">genesis::GenesisValidatorMetadata</a>&gt;, protocol_version: u64, system_state_version: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_create">create</a>(
    genesis_chain_parameters: <a href="genesis.md#0x2_genesis_GenesisChainParameters">GenesisChainParameters</a>,
    genesis_validators: <a href="">vector</a>&lt;<a href="genesis.md#0x2_genesis_GenesisValidatorMetadata">GenesisValidatorMetadata</a>&gt;,
    protocol_version: u64,
    system_state_version: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> sui_supply = <a href="sui.md#0x2_sui_new">sui::new</a>(ctx);
    <b>let</b> subsidy_fund = <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> sui_supply, <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_FUND_BALANCE_TEST_ONLY">INIT_STAKE_SUBSIDY_FUND_BALANCE_TEST_ONLY</a>);
    <b>let</b> storage_fund = <a href="balance.md#0x2_balance_zero">balance::zero</a>();
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
            // Initialize all validators <b>with</b> uniform stake taken from the subsidy fund.
            // TODO: change this back <b>to</b> take from subsidy fund instead.
            <a href="_some">option::some</a>(<a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> sui_supply, genesis_chain_parameters.initial_validator_stake_mist)),
            gas_price,
            commission_rate,
            ctx
        );

        <a href="validator.md#0x2_validator_activate">validator::activate</a>(&<b>mut</b> <a href="validator.md#0x2_validator">validator</a>, 0);

        <a href="_push_back">vector::push_back</a>(&<b>mut</b> validators, <a href="validator.md#0x2_validator">validator</a>);

        i = i + 1;
    };

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

    // Transfer the remaining <a href="balance.md#0x2_balance">balance</a> of <a href="sui.md#0x2_sui">sui</a>'s supply <b>to</b> the initial account
    <a href="sui.md#0x2_sui_transfer">sui::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(sui_supply, ctx), genesis_chain_parameters.initial_sui_custody_account_address);
}
</code></pre>



</details>
