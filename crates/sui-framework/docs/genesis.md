
<a name="0x2_genesis"></a>

# Module `0x2::genesis`



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



<a name="@Constants_0"></a>

## Constants


<a name="0x2_genesis_INIT_STAKE_SUBSIDY_AMOUNT"></a>

Stake subisidy to be given out in the very first epoch. Placeholder value.


<pre><code><b>const</b> <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_AMOUNT">INIT_STAKE_SUBSIDY_AMOUNT</a>: u64 = 1000000;
</code></pre>



<a name="0x2_genesis_INIT_STAKE_SUBSIDY_FUND_BALANCE"></a>

The initial balance of the Subsidy fund in Mist (1 Billion * 10^9)


<pre><code><b>const</b> <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_FUND_BALANCE">INIT_STAKE_SUBSIDY_FUND_BALANCE</a>: u64 = 1000000000000000000;
</code></pre>



<a name="0x2_genesis_create"></a>

## Function `create`

This function will be explicitly called once at genesis.
It will create a singleton SuiSystemState object, which contains
all the information we need in the system.


<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_create">create</a>(initial_sui_custody_account_address: <b>address</b>, initial_validator_stake_mist: u64, governance_start_epoch: u64, validator_pubkeys: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_network_pubkeys: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_worker_pubkeys: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_proof_of_possessions: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_sui_addresses: <a href="">vector</a>&lt;<b>address</b>&gt;, validator_names: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_descriptions: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_image_urls: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_project_urls: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_net_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_p2p_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_primary_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_worker_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_gas_prices: <a href="">vector</a>&lt;u64&gt;, validator_commission_rates: <a href="">vector</a>&lt;u64&gt;, protocol_version: u64, system_state_version: u64, epoch_start_timestamp_ms: u64, epoch_duration_ms: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_create">create</a>(
    initial_sui_custody_account_address: <b>address</b>,
    initial_validator_stake_mist: u64,
    governance_start_epoch: u64,
    validator_pubkeys: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_network_pubkeys: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_worker_pubkeys: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_proof_of_possessions: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_sui_addresses: <a href="">vector</a>&lt;<b>address</b>&gt;,
    validator_names: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_descriptions: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_image_urls: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_project_urls: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_net_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_p2p_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_primary_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_worker_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_gas_prices: <a href="">vector</a>&lt;u64&gt;,
    validator_commission_rates: <a href="">vector</a>&lt;u64&gt;,
    protocol_version: u64,
    system_state_version: u64,
    epoch_start_timestamp_ms: u64,
    epoch_duration_ms: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> sui_supply = <a href="sui.md#0x2_sui_new">sui::new</a>(ctx);
    <b>let</b> subsidy_fund = <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> sui_supply, <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_FUND_BALANCE">INIT_STAKE_SUBSIDY_FUND_BALANCE</a>);
    <b>let</b> storage_fund = <a href="balance.md#0x2_balance_zero">balance::zero</a>();
    <b>let</b> validators = <a href="_empty">vector::empty</a>();
    <b>let</b> count = <a href="_length">vector::length</a>(&validator_pubkeys);
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&validator_sui_addresses) == count
            && <a href="_length">vector::length</a>(&validator_names) == count
            && <a href="_length">vector::length</a>(&validator_descriptions) == count
            && <a href="_length">vector::length</a>(&validator_image_urls) == count
            && <a href="_length">vector::length</a>(&validator_project_urls) == count
            && <a href="_length">vector::length</a>(&validator_net_addresses) == count
            && <a href="_length">vector::length</a>(&validator_p2p_addresses) == count
            && <a href="_length">vector::length</a>(&validator_primary_addresses) == count
            && <a href="_length">vector::length</a>(&validator_worker_addresses) == count
            && <a href="_length">vector::length</a>(&validator_gas_prices) == count
            && <a href="_length">vector::length</a>(&validator_commission_rates) == count,
        1
    );
    <b>let</b> i = 0;
    <b>while</b> (i &lt; count) {
        <b>let</b> sui_address = *<a href="_borrow">vector::borrow</a>(&validator_sui_addresses, i);
        <b>let</b> pubkey = *<a href="_borrow">vector::borrow</a>(&validator_pubkeys, i);
        <b>let</b> network_pubkey = *<a href="_borrow">vector::borrow</a>(&validator_network_pubkeys, i);
        <b>let</b> worker_pubkey = *<a href="_borrow">vector::borrow</a>(&validator_worker_pubkeys, i);
        <b>let</b> proof_of_possession = *<a href="_borrow">vector::borrow</a>(&validator_proof_of_possessions, i);
        <b>let</b> name = *<a href="_borrow">vector::borrow</a>(&validator_names, i);
        <b>let</b> description = *<a href="_borrow">vector::borrow</a>(&validator_descriptions, i);
        <b>let</b> image_url = *<a href="_borrow">vector::borrow</a>(&validator_image_urls, i);
        <b>let</b> project_url = *<a href="_borrow">vector::borrow</a>(&validator_project_urls, i);
        <b>let</b> net_address = *<a href="_borrow">vector::borrow</a>(&validator_net_addresses, i);
        <b>let</b> p2p_address = *<a href="_borrow">vector::borrow</a>(&validator_p2p_addresses, i);
        <b>let</b> primary_address = *<a href="_borrow">vector::borrow</a>(&validator_primary_addresses, i);
        <b>let</b> worker_address = *<a href="_borrow">vector::borrow</a>(&validator_worker_addresses, i);
        <b>let</b> gas_price = *<a href="_borrow">vector::borrow</a>(&validator_gas_prices, i);
        <b>let</b> commission_rate = *<a href="_borrow">vector::borrow</a>(&validator_commission_rates, i);
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> validators, <a href="validator.md#0x2_validator_new">validator::new</a>(
            sui_address,
            pubkey,
            network_pubkey,
            worker_pubkey,
            proof_of_possession,
            name,
            description,
            image_url,
            project_url,
            net_address,
            p2p_address,
            primary_address,
            worker_address,
            // Initialize all validators <b>with</b> uniform stake taken from the subsidy fund.
            <a href="_some">option::some</a>(<a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> subsidy_fund, initial_validator_stake_mist)),
            gas_price,
            commission_rate,
            <b>true</b>, // <a href="validator.md#0x2_validator">validator</a> is active right away
            ctx
        ));
        i = i + 1;
    };

    <a href="sui_system.md#0x2_sui_system_create">sui_system::create</a>(
        validators,
        subsidy_fund,
        storage_fund,
        governance_start_epoch,
        <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_AMOUNT">INIT_STAKE_SUBSIDY_AMOUNT</a>,
        protocol_version,
        system_state_version,
        epoch_start_timestamp_ms,
        epoch_duration_ms,
        ctx,
    );

    <a href="clock.md#0x2_clock_create">clock::create</a>();

    // Transfer the remaining <a href="balance.md#0x2_balance">balance</a> of <a href="sui.md#0x2_sui">sui</a>'s supply <b>to</b> the initial account
    <a href="sui.md#0x2_sui_transfer">sui::transfer</a>(<a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(sui_supply, ctx), initial_sui_custody_account_address);
}
</code></pre>



</details>
