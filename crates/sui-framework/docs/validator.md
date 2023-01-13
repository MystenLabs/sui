
<a name="0x2_validator"></a>

# Module `0x2::validator`



-  [Struct `ValidatorMetadata`](#0x2_validator_ValidatorMetadata)
-  [Struct `Validator`](#0x2_validator_Validator)
-  [Constants](#@Constants_0)
-  [Function `verify_proof_of_possession`](#0x2_validator_verify_proof_of_possession)
-  [Function `new`](#0x2_validator_new)
-  [Function `destroy`](#0x2_validator_destroy)
-  [Function `request_add_stake`](#0x2_validator_request_add_stake)
-  [Function `request_withdraw_stake`](#0x2_validator_request_withdraw_stake)
-  [Function `adjust_stake_and_gas_price`](#0x2_validator_adjust_stake_and_gas_price)
-  [Function `request_add_delegation`](#0x2_validator_request_add_delegation)
-  [Function `request_withdraw_delegation`](#0x2_validator_request_withdraw_delegation)
-  [Function `decrease_next_epoch_delegation`](#0x2_validator_decrease_next_epoch_delegation)
-  [Function `request_set_gas_price`](#0x2_validator_request_set_gas_price)
-  [Function `request_set_commission_rate`](#0x2_validator_request_set_commission_rate)
-  [Function `deposit_delegation_rewards`](#0x2_validator_deposit_delegation_rewards)
-  [Function `process_pending_delegations_and_withdraws`](#0x2_validator_process_pending_delegations_and_withdraws)
-  [Function `get_staking_pool_mut_ref`](#0x2_validator_get_staking_pool_mut_ref)
-  [Function `metadata`](#0x2_validator_metadata)
-  [Function `sui_address`](#0x2_validator_sui_address)
-  [Function `stake_amount`](#0x2_validator_stake_amount)
-  [Function `delegate_amount`](#0x2_validator_delegate_amount)
-  [Function `total_stake`](#0x2_validator_total_stake)
-  [Function `voting_power`](#0x2_validator_voting_power)
-  [Function `set_voting_power`](#0x2_validator_set_voting_power)
-  [Function `pending_stake_amount`](#0x2_validator_pending_stake_amount)
-  [Function `pending_withdraw`](#0x2_validator_pending_withdraw)
-  [Function `gas_price`](#0x2_validator_gas_price)
-  [Function `commission_rate`](#0x2_validator_commission_rate)
-  [Function `pool_token_exchange_rate`](#0x2_validator_pool_token_exchange_rate)
-  [Function `is_duplicate`](#0x2_validator_is_duplicate)


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
<b>use</b> <a href="">0x1::bcs</a>;
<b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::string</a>;
<b>use</b> <a href="">0x1::vector</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="bls12381.md#0x2_bls12381">0x2::bls12381</a>;
<b>use</b> <a href="epoch_time_lock.md#0x2_epoch_time_lock">0x2::epoch_time_lock</a>;
<b>use</b> <a href="stake.md#0x2_stake">0x2::stake</a>;
<b>use</b> <a href="staking_pool.md#0x2_staking_pool">0x2::staking_pool</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="url.md#0x2_url">0x2::url</a>;
</code></pre>



<a name="0x2_validator_ValidatorMetadata"></a>

## Struct `ValidatorMetadata`



<pre><code><b>struct</b> <a href="validator.md#0x2_validator_ValidatorMetadata">ValidatorMetadata</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>sui_address: <b>address</b></code>
</dt>
<dd>
 The Sui Address of the validator. This is the sender that created the Validator object,
 and also the address to send validator/coins to during withdraws.
</dd>
<dt>
<code>pubkey_bytes: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The public key bytes corresponding to the private key that the validator
 holds to sign transactions. For now, this is the same as AuthorityName.
</dd>
<dt>
<code>network_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The public key bytes corresponding to the private key that the validator
 uses to establish TLS connections
</dd>
<dt>
<code>worker_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The public key bytes correstponding to the Narwhal Worker
</dd>
<dt>
<code>proof_of_possession: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 This is a proof that the validator has ownership of the private key
</dd>
<dt>
<code>name: <a href="_String">string::String</a></code>
</dt>
<dd>
 A unique human-readable name of this validator.
</dd>
<dt>
<code>description: <a href="_String">string::String</a></code>
</dt>
<dd>

</dd>
<dt>
<code>image_url: <a href="url.md#0x2_url_Url">url::Url</a></code>
</dt>
<dd>

</dd>
<dt>
<code>project_url: <a href="url.md#0x2_url_Url">url::Url</a></code>
</dt>
<dd>

</dd>
<dt>
<code>net_address: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The network address of the validator (could also contain extra info such as port, DNS and etc.).
</dd>
<dt>
<code>consensus_address: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The address of the narwhal primary
</dd>
<dt>
<code>worker_address: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 The address of the narwhal worker
</dd>
<dt>
<code>next_epoch_stake: u64</code>
</dt>
<dd>
 Total amount of validator stake that would be active in the next epoch.
</dd>
<dt>
<code>next_epoch_delegation: u64</code>
</dt>
<dd>
 Total amount of delegated stake that would be active in the next epoch.
</dd>
<dt>
<code>next_epoch_gas_price: u64</code>
</dt>
<dd>
 This validator's gas price quote for the next epoch.
</dd>
<dt>
<code>next_epoch_commission_rate: u64</code>
</dt>
<dd>
 The commission rate of the validator starting the next epoch, in basis point.
</dd>
</dl>


</details>

<a name="0x2_validator_Validator"></a>

## Struct `Validator`



<pre><code><b>struct</b> <a href="validator.md#0x2_validator_Validator">Validator</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>metadata: <a href="validator.md#0x2_validator_ValidatorMetadata">validator::ValidatorMetadata</a></code>
</dt>
<dd>
 Summary of the validator.
</dd>
<dt>
<code>voting_power: u64</code>
</dt>
<dd>
 The voting power of this validator, which might be different from its
 stake amount.
</dd>
<dt>
<code>stake_amount: u64</code>
</dt>
<dd>
 The current active stake amount. This will not change during an epoch. It can only
 be updated at the end of epoch.
</dd>
<dt>
<code>pending_stake: u64</code>
</dt>
<dd>
 Pending stake deposit amount, processed at end of epoch.
</dd>
<dt>
<code>pending_withdraw: u64</code>
</dt>
<dd>
 Pending withdraw amount, processed at end of epoch.
</dd>
<dt>
<code>gas_price: u64</code>
</dt>
<dd>
 Gas price quote, updated only at end of epoch.
</dd>
<dt>
<code>delegation_staking_pool: <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a></code>
</dt>
<dd>
 Staking pool for the stakes delegated to this validator.
</dd>
<dt>
<code>commission_rate: u64</code>
</dt>
<dd>
 Commission rate of the validator, in basis point.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_validator_PROOF_OF_POSSESSION_DOMAIN"></a>



<pre><code><b>const</b> <a href="validator.md#0x2_validator_PROOF_OF_POSSESSION_DOMAIN">PROOF_OF_POSSESSION_DOMAIN</a>: <a href="">vector</a>&lt;u8&gt; = [107, 111, 115, 107];
</code></pre>



<a name="0x2_validator_verify_proof_of_possession"></a>

## Function `verify_proof_of_possession`



<pre><code><b>fun</b> <a href="validator.md#0x2_validator_verify_proof_of_possession">verify_proof_of_possession</a>(proof_of_possession: <a href="">vector</a>&lt;u8&gt;, sui_address: <b>address</b>, pubkey_bytes: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator.md#0x2_validator_verify_proof_of_possession">verify_proof_of_possession</a>(
    proof_of_possession: <a href="">vector</a>&lt;u8&gt;,
    sui_address: <b>address</b>,
    pubkey_bytes: <a href="">vector</a>&lt;u8&gt;
) {
    // The proof of possession is the signature over ValidatorPK || AccountAddress.
    // This proves that the account <b>address</b> is owned by the holder of ValidatorPK, and <b>ensures</b>
    // that PK <b>exists</b>.
    <b>let</b> signed_bytes = pubkey_bytes;
    <b>let</b> address_bytes = <a href="_to_bytes">bcs::to_bytes</a>(&sui_address);
    <a href="_append">vector::append</a>(&<b>mut</b> signed_bytes, address_bytes);
    <b>assert</b>!(
        bls12381_min_sig_verify_with_domain(&proof_of_possession, &pubkey_bytes, signed_bytes, <a href="validator.md#0x2_validator_PROOF_OF_POSSESSION_DOMAIN">PROOF_OF_POSSESSION_DOMAIN</a>) == <b>true</b>,
        0
    );
}
</code></pre>



</details>

<a name="0x2_validator_new"></a>

## Function `new`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_new">new</a>(sui_address: <b>address</b>, pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, network_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, worker_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, name: <a href="">vector</a>&lt;u8&gt;, description: <a href="">vector</a>&lt;u8&gt;, image_url: <a href="">vector</a>&lt;u8&gt;, project_url: <a href="">vector</a>&lt;u8&gt;, net_address: <a href="">vector</a>&lt;u8&gt;, consensus_address: <a href="">vector</a>&lt;u8&gt;, worker_address: <a href="">vector</a>&lt;u8&gt;, <a href="stake.md#0x2_stake">stake</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, coin_locked_until_epoch: <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;, gas_price: u64, commission_rate: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="validator.md#0x2_validator_Validator">validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_new">new</a>(
    sui_address: <b>address</b>,
    pubkey_bytes: <a href="">vector</a>&lt;u8&gt;,
    network_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;,
    worker_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="">vector</a>&lt;u8&gt;,
    name: <a href="">vector</a>&lt;u8&gt;,
    description: <a href="">vector</a>&lt;u8&gt;,
    image_url: <a href="">vector</a>&lt;u8&gt;,
    project_url: <a href="">vector</a>&lt;u8&gt;,
    net_address: <a href="">vector</a>&lt;u8&gt;,
    consensus_address: <a href="">vector</a>&lt;u8&gt;,
    worker_address: <a href="">vector</a>&lt;u8&gt;,
    <a href="stake.md#0x2_stake">stake</a>: Balance&lt;SUI&gt;,
    coin_locked_until_epoch: Option&lt;EpochTimeLock&gt;,
    gas_price: u64,
    commission_rate: u64,
    ctx: &<b>mut</b> TxContext
): <a href="validator.md#0x2_validator_Validator">Validator</a> {
    <b>assert</b>!(
        // TODO: These constants are arbitrary, will adjust once we know more.
        <a href="_length">vector::length</a>(&net_address) &lt;= 128
            && <a href="_length">vector::length</a>(&name) &lt;= 128
            && <a href="_length">vector::length</a>(&description) &lt;= 150
            && <a href="_length">vector::length</a>(&pubkey_bytes) &lt;= 128,
        0
    );
    <a href="validator.md#0x2_validator_verify_proof_of_possession">verify_proof_of_possession</a>(
        proof_of_possession,
        sui_address,
        pubkey_bytes
    );
    <b>let</b> stake_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&<a href="stake.md#0x2_stake">stake</a>);
    <a href="stake.md#0x2_stake_create">stake::create</a>(<a href="stake.md#0x2_stake">stake</a>, sui_address, coin_locked_until_epoch, ctx);
    <a href="validator.md#0x2_validator_Validator">Validator</a> {
        metadata: <a href="validator.md#0x2_validator_ValidatorMetadata">ValidatorMetadata</a> {
            sui_address,
            pubkey_bytes,
            network_pubkey_bytes,
            worker_pubkey_bytes,
            proof_of_possession,
            name: <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(name)),
            description: <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(description)),
            image_url: <a href="url.md#0x2_url_new_unsafe_from_bytes">url::new_unsafe_from_bytes</a>(image_url),
            project_url: <a href="url.md#0x2_url_new_unsafe_from_bytes">url::new_unsafe_from_bytes</a>(project_url),
            net_address,
            consensus_address,
            worker_address,
            next_epoch_stake: stake_amount,
            next_epoch_delegation: 0,
            next_epoch_gas_price: gas_price,
            next_epoch_commission_rate: commission_rate,
        },
        // Initialize the voting power <b>to</b> be the same <b>as</b> the <a href="stake.md#0x2_stake">stake</a> amount.
        // At the epoch change <b>where</b> this <a href="validator.md#0x2_validator">validator</a> is actually added <b>to</b> the
        // active <a href="validator.md#0x2_validator">validator</a> set, the voting power will be updated accordingly.
        voting_power: stake_amount,
        stake_amount,
        pending_stake: 0,
        pending_withdraw: 0,
        gas_price,
        delegation_staking_pool: <a href="staking_pool.md#0x2_staking_pool_new">staking_pool::new</a>(sui_address, <a href="tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) + 1, ctx),
        commission_rate,
    }
}
</code></pre>



</details>

<a name="0x2_validator_destroy"></a>

## Function `destroy`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_destroy">destroy</a>(self: <a href="validator.md#0x2_validator_Validator">validator::Validator</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_destroy">destroy</a>(self: <a href="validator.md#0x2_validator_Validator">Validator</a>, ctx: &<b>mut</b> TxContext) {
    <b>let</b> <a href="validator.md#0x2_validator_Validator">Validator</a> {
        metadata: _,
        voting_power: _,
        stake_amount: _,
        pending_stake: _,
        pending_withdraw: _,
        gas_price: _,
        delegation_staking_pool,
        commission_rate: _,
    } = self;
    <a href="staking_pool.md#0x2_staking_pool_deactivate_staking_pool">staking_pool::deactivate_staking_pool</a>(delegation_staking_pool, ctx);
}
</code></pre>



</details>

<a name="0x2_validator_request_add_stake"></a>

## Function `request_add_stake`

Add stake to an active validator. The new stake is added to the pending_stake field,
which will be processed at the end of epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_add_stake">request_add_stake</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>, new_stake: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, coin_locked_until_epoch: <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_add_stake">request_add_stake</a>(
    self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>,
    new_stake: Balance&lt;SUI&gt;,
    coin_locked_until_epoch: Option&lt;EpochTimeLock&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> new_stake_value = <a href="balance.md#0x2_balance_value">balance::value</a>(&new_stake);
    self.pending_stake = self.pending_stake + new_stake_value;
    self.metadata.next_epoch_stake = self.metadata.next_epoch_stake + new_stake_value;
    <a href="stake.md#0x2_stake_create">stake::create</a>(new_stake, self.metadata.sui_address, coin_locked_until_epoch, ctx);
}
</code></pre>



</details>

<a name="0x2_validator_request_withdraw_stake"></a>

## Function `request_withdraw_stake`

Withdraw stake from an active validator. Since it's active, we need
to add it to the pending withdraw amount and process it at the end
of epoch. We also need to make sure there is sufficient amount to withdraw such that the validator's
stake still satisfy the minimum requirement.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_withdraw_stake">request_withdraw_stake</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>, <a href="stake.md#0x2_stake">stake</a>: &<b>mut</b> <a href="stake.md#0x2_stake_Stake">stake::Stake</a>, withdraw_amount: u64, min_validator_stake: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_withdraw_stake">request_withdraw_stake</a>(
    self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>,
    <a href="stake.md#0x2_stake">stake</a>: &<b>mut</b> Stake,
    withdraw_amount: u64,
    min_validator_stake: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(self.metadata.next_epoch_stake &gt;= withdraw_amount + min_validator_stake, 0);
    self.pending_withdraw = self.pending_withdraw + withdraw_amount;
    self.metadata.next_epoch_stake = self.metadata.next_epoch_stake - withdraw_amount;
    <a href="stake.md#0x2_stake_withdraw_stake">stake::withdraw_stake</a>(<a href="stake.md#0x2_stake">stake</a>, withdraw_amount, ctx);
}
</code></pre>



</details>

<a name="0x2_validator_adjust_stake_and_gas_price"></a>

## Function `adjust_stake_and_gas_price`

Process pending stake and pending withdraws, and update the gas price.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>) {
    self.stake_amount = self.stake_amount + self.pending_stake - self.pending_withdraw;
    self.pending_stake = 0;
    self.pending_withdraw = 0;
    self.gas_price = self.metadata.next_epoch_gas_price;
    self.commission_rate = self.metadata.next_epoch_commission_rate;
    <b>assert</b>!(self.stake_amount == self.metadata.next_epoch_stake, 0);
}
</code></pre>



</details>

<a name="0x2_validator_request_add_delegation"></a>

## Function `request_add_delegation`

Request to add delegation to the validator's staking pool, processed at the end of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_add_delegation">request_add_delegation</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>, delegated_stake: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, locking_period: <a href="_Option">option::Option</a>&lt;<a href="epoch_time_lock.md#0x2_epoch_time_lock_EpochTimeLock">epoch_time_lock::EpochTimeLock</a>&gt;, delegator: <b>address</b>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_add_delegation">request_add_delegation</a>(
    self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>,
    delegated_stake: Balance&lt;SUI&gt;,
    locking_period: Option&lt;EpochTimeLock&gt;,
    delegator: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> delegate_amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&delegated_stake);
    <b>assert</b>!(delegate_amount &gt; 0, 0);
    <a href="staking_pool.md#0x2_staking_pool_request_add_delegation">staking_pool::request_add_delegation</a>(&<b>mut</b> self.delegation_staking_pool, delegated_stake, locking_period, delegator, ctx);
    self.metadata.next_epoch_delegation = self.metadata.next_epoch_delegation + delegate_amount;
}
</code></pre>



</details>

<a name="0x2_validator_request_withdraw_delegation"></a>

## Function `request_withdraw_delegation`

Request to withdraw delegation from the validator's staking pool, processed at the end of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_withdraw_delegation">request_withdraw_delegation</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>, delegation: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_Delegation">staking_pool::Delegation</a>, staked_sui: &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakedSui">staking_pool::StakedSui</a>, principal_withdraw_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_withdraw_delegation">request_withdraw_delegation</a>(
    self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>,
    delegation: &<b>mut</b> Delegation,
    staked_sui: &<b>mut</b> StakedSui,
    principal_withdraw_amount: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <a href="staking_pool.md#0x2_staking_pool_request_withdraw_delegation">staking_pool::request_withdraw_delegation</a>(
            &<b>mut</b> self.delegation_staking_pool, delegation, staked_sui, principal_withdraw_amount, ctx);
    <a href="validator.md#0x2_validator_decrease_next_epoch_delegation">decrease_next_epoch_delegation</a>(self, principal_withdraw_amount);
}
</code></pre>



</details>

<a name="0x2_validator_decrease_next_epoch_delegation"></a>

## Function `decrease_next_epoch_delegation`

Decrement the delegation amount for next epoch. Also called by <code><a href="validator_set.md#0x2_validator_set">validator_set</a></code> when handling delegation switches.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_decrease_next_epoch_delegation">decrease_next_epoch_delegation</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>, amount: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_decrease_next_epoch_delegation">decrease_next_epoch_delegation</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>, amount: u64) {
    self.metadata.next_epoch_delegation = self.metadata.next_epoch_delegation - amount;
}
</code></pre>



</details>

<a name="0x2_validator_request_set_gas_price"></a>

## Function `request_set_gas_price`

Request to set new gas price for the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_set_gas_price">request_set_gas_price</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>, new_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_set_gas_price">request_set_gas_price</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>, new_price: u64) {
    self.metadata.next_epoch_gas_price = new_price;
}
</code></pre>



</details>

<a name="0x2_validator_request_set_commission_rate"></a>

## Function `request_set_commission_rate`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_set_commission_rate">request_set_commission_rate</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>, new_commission_rate: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_request_set_commission_rate">request_set_commission_rate</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>, new_commission_rate: u64) {
    self.metadata.next_epoch_commission_rate = new_commission_rate;
}
</code></pre>



</details>

<a name="0x2_validator_deposit_delegation_rewards"></a>

## Function `deposit_delegation_rewards`

Deposit delegations rewards into the validator's staking pool, called at the end of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_deposit_delegation_rewards">deposit_delegation_rewards</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>, reward: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_deposit_delegation_rewards">deposit_delegation_rewards</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>, reward: Balance&lt;SUI&gt;) {
    self.metadata.next_epoch_delegation = self.metadata.next_epoch_delegation + <a href="balance.md#0x2_balance_value">balance::value</a>(&reward);
    <a href="staking_pool.md#0x2_staking_pool_deposit_rewards">staking_pool::deposit_rewards</a>(&<b>mut</b> self.delegation_staking_pool, reward);
}
</code></pre>



</details>

<a name="0x2_validator_process_pending_delegations_and_withdraws"></a>

## Function `process_pending_delegations_and_withdraws`

Process pending delegations and withdraws, called at the end of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_process_pending_delegations_and_withdraws">process_pending_delegations_and_withdraws</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_process_pending_delegations_and_withdraws">process_pending_delegations_and_withdraws</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>, ctx: &<b>mut</b> TxContext) {
    <a href="staking_pool.md#0x2_staking_pool_process_pending_delegations">staking_pool::process_pending_delegations</a>(&<b>mut</b> self.delegation_staking_pool, ctx);
    <b>let</b> reward_withdraw_amount = <a href="staking_pool.md#0x2_staking_pool_process_pending_delegation_withdraws">staking_pool::process_pending_delegation_withdraws</a>(
        &<b>mut</b> self.delegation_staking_pool, ctx);
    self.metadata.next_epoch_delegation = self.metadata.next_epoch_delegation - reward_withdraw_amount;
    <b>assert</b>!(<a href="validator.md#0x2_validator_delegate_amount">delegate_amount</a>(self) == self.metadata.next_epoch_delegation, 0);
}
</code></pre>



</details>

<a name="0x2_validator_get_staking_pool_mut_ref"></a>

## Function `get_staking_pool_mut_ref`

Called by <code><a href="validator_set.md#0x2_validator_set">validator_set</a></code> for handling delegation switches.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_get_staking_pool_mut_ref">get_staking_pool_mut_ref</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>): &<b>mut</b> <a href="staking_pool.md#0x2_staking_pool_StakingPool">staking_pool::StakingPool</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_get_staking_pool_mut_ref">get_staking_pool_mut_ref</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>) : &<b>mut</b> StakingPool {
    &<b>mut</b> self.delegation_staking_pool
}
</code></pre>



</details>

<a name="0x2_validator_metadata"></a>

## Function `metadata`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_metadata">metadata</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): &<a href="validator.md#0x2_validator_ValidatorMetadata">validator::ValidatorMetadata</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_metadata">metadata</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>): &<a href="validator.md#0x2_validator_ValidatorMetadata">ValidatorMetadata</a> {
    &self.metadata
}
</code></pre>



</details>

<a name="0x2_validator_sui_address"></a>

## Function `sui_address`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_sui_address">sui_address</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_sui_address">sui_address</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>): <b>address</b> {
    self.metadata.sui_address
}
</code></pre>



</details>

<a name="0x2_validator_stake_amount"></a>

## Function `stake_amount`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_stake_amount">stake_amount</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_stake_amount">stake_amount</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>): u64 {
    self.stake_amount
}
</code></pre>



</details>

<a name="0x2_validator_delegate_amount"></a>

## Function `delegate_amount`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_delegate_amount">delegate_amount</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_delegate_amount">delegate_amount</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>): u64 {
    <a href="staking_pool.md#0x2_staking_pool_sui_balance">staking_pool::sui_balance</a>(&self.delegation_staking_pool)
}
</code></pre>



</details>

<a name="0x2_validator_total_stake"></a>

## Function `total_stake`

Return the total amount staked with this validator, including both validator stake and deledgated stake


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_total_stake">total_stake</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_total_stake">total_stake</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>): u64 {
    <a href="validator.md#0x2_validator_stake_amount">stake_amount</a>(self) + <a href="validator.md#0x2_validator_delegate_amount">delegate_amount</a>(self)
}
</code></pre>



</details>

<a name="0x2_validator_voting_power"></a>

## Function `voting_power`

Return the voting power of this validator.


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_voting_power">voting_power</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_voting_power">voting_power</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>): u64 {
    self.voting_power
}
</code></pre>



</details>

<a name="0x2_validator_set_voting_power"></a>

## Function `set_voting_power`

Set the voting power of this validator, called only from validator_set.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_set_voting_power">set_voting_power</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">validator::Validator</a>, new_voting_power: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x2_validator_set_voting_power">set_voting_power</a>(self: &<b>mut</b> <a href="validator.md#0x2_validator_Validator">Validator</a>, new_voting_power: u64) {
    self.voting_power = new_voting_power;
}
</code></pre>



</details>

<a name="0x2_validator_pending_stake_amount"></a>

## Function `pending_stake_amount`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_pending_stake_amount">pending_stake_amount</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_pending_stake_amount">pending_stake_amount</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>): u64 {
    self.pending_stake
}
</code></pre>



</details>

<a name="0x2_validator_pending_withdraw"></a>

## Function `pending_withdraw`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_pending_withdraw">pending_withdraw</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_pending_withdraw">pending_withdraw</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>): u64 {
    self.pending_withdraw
}
</code></pre>



</details>

<a name="0x2_validator_gas_price"></a>

## Function `gas_price`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_gas_price">gas_price</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_gas_price">gas_price</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>): u64 {
    self.gas_price
}
</code></pre>



</details>

<a name="0x2_validator_commission_rate"></a>

## Function `commission_rate`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_commission_rate">commission_rate</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_commission_rate">commission_rate</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>): u64 {
    self.commission_rate
}
</code></pre>



</details>

<a name="0x2_validator_pool_token_exchange_rate"></a>

## Function `pool_token_exchange_rate`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_pool_token_exchange_rate">pool_token_exchange_rate</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): <a href="staking_pool.md#0x2_staking_pool_PoolTokenExchangeRate">staking_pool::PoolTokenExchangeRate</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_pool_token_exchange_rate">pool_token_exchange_rate</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>): PoolTokenExchangeRate {
    <a href="staking_pool.md#0x2_staking_pool_pool_token_exchange_rate">staking_pool::pool_token_exchange_rate</a>(&self.delegation_staking_pool)
}
</code></pre>



</details>

<a name="0x2_validator_is_duplicate"></a>

## Function `is_duplicate`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_is_duplicate">is_duplicate</a>(self: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>, other: &<a href="validator.md#0x2_validator_Validator">validator::Validator</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x2_validator_is_duplicate">is_duplicate</a>(self: &<a href="validator.md#0x2_validator_Validator">Validator</a>, other: &<a href="validator.md#0x2_validator_Validator">Validator</a>): bool {
     self.metadata.sui_address == other.metadata.sui_address
        || self.metadata.name == other.metadata.name
        || self.metadata.net_address == other.metadata.net_address
        || self.metadata.pubkey_bytes == other.metadata.pubkey_bytes
}
</code></pre>



</details>
