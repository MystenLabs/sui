
<a name="0x2_genesis"></a>

# Module `0x2::genesis`



-  [Constants](#@Constants_0)
-  [Function `create`](#0x2_genesis_create)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="clock.md#0x2_clock">0x2::clock</a>;
<b>use</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock">0x2::epoch_time_lock</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="sui_system.md#0x2_sui_system">0x2::sui_system</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="validator.md#0x2_validator">0x2::validator</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_genesis_INIT_MAX_VALIDATOR_COUNT"></a>

Initial value of the upper-bound on the number of validators.


<pre><code><b>const</b> <a href="genesis.md#0x2_genesis_INIT_MAX_VALIDATOR_COUNT">INIT_MAX_VALIDATOR_COUNT</a>: u64 = 100;
</code></pre>



<a name="0x2_genesis_INIT_MIN_VALIDATOR_STAKE"></a>

Initial value of the lower-bound on the amount of stake required to become a validator.
TODO: testnet only. Needs to be changed.


<pre><code><b>const</b> <a href="genesis.md#0x2_genesis_INIT_MIN_VALIDATOR_STAKE">INIT_MIN_VALIDATOR_STAKE</a>: u64 = 1;
</code></pre>



<a name="0x2_genesis_INIT_STAKE_SUBSIDY_AMOUNT"></a>

Stake subisidy to be given out in the very first epoch. Placeholder value.


<pre><code><b>const</b> <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_AMOUNT">INIT_STAKE_SUBSIDY_AMOUNT</a>: u64 = 1000000;
</code></pre>



<a name="0x2_genesis_INIT_STORAGE_FUND"></a>

The initial amount of SUI locked in the storage fund.


<pre><code><b>const</b> <a href="genesis.md#0x2_genesis_INIT_STORAGE_FUND">INIT_STORAGE_FUND</a>: u64 = 1;
</code></pre>



<a name="0x2_genesis_create"></a>

## Function `create`

This function will be explicitly called once at genesis.
It will create a singleton SuiSystemState object, which contains
all the information we need in the system.


<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_create">create</a>(validator_pubkeys: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_network_pubkeys: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_worker_pubkeys: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_proof_of_possessions: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_sui_addresses: <a href="">vector</a>&lt;<b>address</b>&gt;, validator_names: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_descriptions: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_image_urls: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_project_urls: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_net_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_consensus_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_worker_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;, validator_stakes: <a href="">vector</a>&lt;u64&gt;, validator_gas_prices: <a href="">vector</a>&lt;u64&gt;, validator_commission_rates: <a href="">vector</a>&lt;u64&gt;, protocol_version: u64, epoch_start_timestamp_ms: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="genesis.md#0x2_genesis_create">create</a>(
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
    validator_consensus_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_worker_addresses: <a href="">vector</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;,
    validator_stakes: <a href="">vector</a>&lt;u64&gt;,
    validator_gas_prices: <a href="">vector</a>&lt;u64&gt;,
    validator_commission_rates: <a href="">vector</a>&lt;u64&gt;,
    protocol_version: u64,
    epoch_start_timestamp_ms: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> sui_supply = <a href="sui.md#0x2_sui_new">sui::new</a>(ctx);
    <b>let</b> storage_fund = <a href="balance.md#0x2_balance_increase_supply">balance::increase_supply</a>(&<b>mut</b> sui_supply, <a href="genesis.md#0x2_genesis_INIT_STORAGE_FUND">INIT_STORAGE_FUND</a>);
    <b>let</b> validators = <a href="_empty">vector::empty</a>();
    <b>let</b> count = <a href="_length">vector::length</a>(&validator_pubkeys);
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&validator_sui_addresses) == count
            && <a href="_length">vector::length</a>(&validator_stakes) == count
            && <a href="_length">vector::length</a>(&validator_names) == count
            && <a href="_length">vector::length</a>(&validator_descriptions) == count
            && <a href="_length">vector::length</a>(&validator_image_urls) == count
            && <a href="_length">vector::length</a>(&validator_project_urls) == count
            && <a href="_length">vector::length</a>(&validator_net_addresses) == count
            && <a href="_length">vector::length</a>(&validator_consensus_addresses) == count
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
        <b>let</b> consensus_address = *<a href="_borrow">vector::borrow</a>(&validator_consensus_addresses, i);
        <b>let</b> worker_address = *<a href="_borrow">vector::borrow</a>(&validator_worker_addresses, i);
        <b>let</b> <a href="stake.md#0x2_stake">stake</a> = *<a href="_borrow">vector::borrow</a>(&validator_stakes, i);
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
            consensus_address,
            worker_address,
            <a href="balance.md#0x2_balance_increase_supply">balance::increase_supply</a>(&<b>mut</b> sui_supply, <a href="stake.md#0x2_stake">stake</a>),
            <a href="_none">option::none</a>(),
            gas_price,
            commission_rate,
            ctx
        ));
        i = i + 1;
    };

    <a href="sui_system.md#0x2_sui_system_create">sui_system::create</a>(
        validators,
        sui_supply,
        storage_fund,
        <a href="genesis.md#0x2_genesis_INIT_MAX_VALIDATOR_COUNT">INIT_MAX_VALIDATOR_COUNT</a>,
        <a href="genesis.md#0x2_genesis_INIT_MIN_VALIDATOR_STAKE">INIT_MIN_VALIDATOR_STAKE</a>,
        <a href="genesis.md#0x2_genesis_INIT_STAKE_SUBSIDY_AMOUNT">INIT_STAKE_SUBSIDY_AMOUNT</a>,
        protocol_version,
        epoch_start_timestamp_ms,
    );

    <a href="clock.md#0x2_clock_create">clock::create</a>();
}
</code></pre>



</details>
