
<a name="0x3_validator"></a>

# Module `0x3::validator`



-  [Struct `ValidatorMetadata`](#0x3_validator_ValidatorMetadata)
-  [Struct `Validator`](#0x3_validator_Validator)
-  [Struct `StakingRequestEvent`](#0x3_validator_StakingRequestEvent)
-  [Struct `UnstakingRequestEvent`](#0x3_validator_UnstakingRequestEvent)
-  [Constants](#@Constants_0)
-  [Function `new_metadata`](#0x3_validator_new_metadata)
-  [Function `new`](#0x3_validator_new)
-  [Function `deactivate`](#0x3_validator_deactivate)
-  [Function `activate`](#0x3_validator_activate)
-  [Function `adjust_stake_and_gas_price`](#0x3_validator_adjust_stake_and_gas_price)
-  [Function `request_add_stake`](#0x3_validator_request_add_stake)
-  [Function `request_add_stake_at_genesis`](#0x3_validator_request_add_stake_at_genesis)
-  [Function `request_withdraw_stake`](#0x3_validator_request_withdraw_stake)
-  [Function `request_set_gas_price`](#0x3_validator_request_set_gas_price)
-  [Function `set_candidate_gas_price`](#0x3_validator_set_candidate_gas_price)
-  [Function `request_set_commission_rate`](#0x3_validator_request_set_commission_rate)
-  [Function `set_candidate_commission_rate`](#0x3_validator_set_candidate_commission_rate)
-  [Function `deposit_stake_rewards`](#0x3_validator_deposit_stake_rewards)
-  [Function `process_pending_stakes_and_withdraws`](#0x3_validator_process_pending_stakes_and_withdraws)
-  [Function `is_preactive`](#0x3_validator_is_preactive)
-  [Function `metadata`](#0x3_validator_metadata)
-  [Function `sui_address`](#0x3_validator_sui_address)
-  [Function `name`](#0x3_validator_name)
-  [Function `description`](#0x3_validator_description)
-  [Function `image_url`](#0x3_validator_image_url)
-  [Function `project_url`](#0x3_validator_project_url)
-  [Function `network_address`](#0x3_validator_network_address)
-  [Function `p2p_address`](#0x3_validator_p2p_address)
-  [Function `primary_address`](#0x3_validator_primary_address)
-  [Function `worker_address`](#0x3_validator_worker_address)
-  [Function `protocol_pubkey_bytes`](#0x3_validator_protocol_pubkey_bytes)
-  [Function `proof_of_possession`](#0x3_validator_proof_of_possession)
-  [Function `network_pubkey_bytes`](#0x3_validator_network_pubkey_bytes)
-  [Function `worker_pubkey_bytes`](#0x3_validator_worker_pubkey_bytes)
-  [Function `next_epoch_network_address`](#0x3_validator_next_epoch_network_address)
-  [Function `next_epoch_p2p_address`](#0x3_validator_next_epoch_p2p_address)
-  [Function `next_epoch_primary_address`](#0x3_validator_next_epoch_primary_address)
-  [Function `next_epoch_worker_address`](#0x3_validator_next_epoch_worker_address)
-  [Function `next_epoch_protocol_pubkey_bytes`](#0x3_validator_next_epoch_protocol_pubkey_bytes)
-  [Function `next_epoch_proof_of_possession`](#0x3_validator_next_epoch_proof_of_possession)
-  [Function `next_epoch_network_pubkey_bytes`](#0x3_validator_next_epoch_network_pubkey_bytes)
-  [Function `next_epoch_worker_pubkey_bytes`](#0x3_validator_next_epoch_worker_pubkey_bytes)
-  [Function `operation_cap_id`](#0x3_validator_operation_cap_id)
-  [Function `next_epoch_gas_price`](#0x3_validator_next_epoch_gas_price)
-  [Function `total_stake_amount`](#0x3_validator_total_stake_amount)
-  [Function `stake_amount`](#0x3_validator_stake_amount)
-  [Function `total_stake`](#0x3_validator_total_stake)
-  [Function `voting_power`](#0x3_validator_voting_power)
-  [Function `set_voting_power`](#0x3_validator_set_voting_power)
-  [Function `pending_stake_amount`](#0x3_validator_pending_stake_amount)
-  [Function `pending_stake_withdraw_amount`](#0x3_validator_pending_stake_withdraw_amount)
-  [Function `gas_price`](#0x3_validator_gas_price)
-  [Function `commission_rate`](#0x3_validator_commission_rate)
-  [Function `pool_token_exchange_rate_at_epoch`](#0x3_validator_pool_token_exchange_rate_at_epoch)
-  [Function `staking_pool_id`](#0x3_validator_staking_pool_id)
-  [Function `is_duplicate`](#0x3_validator_is_duplicate)
-  [Function `is_equal_some_and_value`](#0x3_validator_is_equal_some_and_value)
-  [Function `is_equal_some`](#0x3_validator_is_equal_some)
-  [Function `new_unverified_validator_operation_cap_and_transfer`](#0x3_validator_new_unverified_validator_operation_cap_and_transfer)
-  [Function `update_name`](#0x3_validator_update_name)
-  [Function `update_description`](#0x3_validator_update_description)
-  [Function `update_image_url`](#0x3_validator_update_image_url)
-  [Function `update_project_url`](#0x3_validator_update_project_url)
-  [Function `update_next_epoch_network_address`](#0x3_validator_update_next_epoch_network_address)
-  [Function `update_candidate_network_address`](#0x3_validator_update_candidate_network_address)
-  [Function `update_next_epoch_p2p_address`](#0x3_validator_update_next_epoch_p2p_address)
-  [Function `update_candidate_p2p_address`](#0x3_validator_update_candidate_p2p_address)
-  [Function `update_next_epoch_primary_address`](#0x3_validator_update_next_epoch_primary_address)
-  [Function `update_candidate_primary_address`](#0x3_validator_update_candidate_primary_address)
-  [Function `update_next_epoch_worker_address`](#0x3_validator_update_next_epoch_worker_address)
-  [Function `update_candidate_worker_address`](#0x3_validator_update_candidate_worker_address)
-  [Function `update_next_epoch_protocol_pubkey`](#0x3_validator_update_next_epoch_protocol_pubkey)
-  [Function `update_candidate_protocol_pubkey`](#0x3_validator_update_candidate_protocol_pubkey)
-  [Function `update_next_epoch_network_pubkey`](#0x3_validator_update_next_epoch_network_pubkey)
-  [Function `update_candidate_network_pubkey`](#0x3_validator_update_candidate_network_pubkey)
-  [Function `update_next_epoch_worker_pubkey`](#0x3_validator_update_next_epoch_worker_pubkey)
-  [Function `update_candidate_worker_pubkey`](#0x3_validator_update_candidate_worker_pubkey)
-  [Function `effectuate_staged_metadata`](#0x3_validator_effectuate_staged_metadata)
-  [Function `validate_metadata`](#0x3_validator_validate_metadata)
-  [Function `validate_metadata_bcs`](#0x3_validator_validate_metadata_bcs)
-  [Function `new_from_metadata`](#0x3_validator_new_from_metadata)


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/bcs.md#0x1_bcs">0x1::bcs</a>;
<b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::string</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/bag.md#0x2_bag">0x2::bag</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/url.md#0x2_url">0x2::url</a>;
<b>use</b> <a href="staking_pool.md#0x3_staking_pool">0x3::staking_pool</a>;
<b>use</b> <a href="validator_cap.md#0x3_validator_cap">0x3::validator_cap</a>;
</code></pre>



<a name="0x3_validator_ValidatorMetadata"></a>

## Struct `ValidatorMetadata`



<pre><code><b>struct</b> <a href="validator.md#0x3_validator_ValidatorMetadata">ValidatorMetadata</a> <b>has</b> store
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
<code>protocol_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;</code>
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
<code>image_url: <a href="../../../.././build/Sui/docs/url.md#0x2_url_Url">url::Url</a></code>
</dt>
<dd>

</dd>
<dt>
<code>project_url: <a href="../../../.././build/Sui/docs/url.md#0x2_url_Url">url::Url</a></code>
</dt>
<dd>

</dd>
<dt>
<code>net_address: <a href="_String">string::String</a></code>
</dt>
<dd>
 The network address of the validator (could also contain extra info such as port, DNS and etc.).
</dd>
<dt>
<code>p2p_address: <a href="_String">string::String</a></code>
</dt>
<dd>
 The address of the validator used for p2p activities such as state sync (could also contain extra info such as port, DNS and etc.).
</dd>
<dt>
<code>primary_address: <a href="_String">string::String</a></code>
</dt>
<dd>
 The address of the narwhal primary
</dd>
<dt>
<code>worker_address: <a href="_String">string::String</a></code>
</dt>
<dd>
 The address of the narwhal worker
</dd>
<dt>
<code>next_epoch_protocol_pubkey_bytes: <a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;</code>
</dt>
<dd>
 "next_epoch" metadata only takes effects in the next epoch.
 If none, current value will stay unchanged.
</dd>
<dt>
<code>next_epoch_proof_of_possession: <a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>next_epoch_network_pubkey_bytes: <a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>next_epoch_worker_pubkey_bytes: <a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>next_epoch_net_address: <a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>next_epoch_p2p_address: <a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>next_epoch_primary_address: <a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>next_epoch_worker_address: <a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>extra_fields: <a href="../../../.././build/Sui/docs/bag.md#0x2_bag_Bag">bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="0x3_validator_Validator"></a>

## Struct `Validator`



<pre><code><b>struct</b> <a href="validator.md#0x3_validator_Validator">Validator</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>metadata: <a href="validator.md#0x3_validator_ValidatorMetadata">validator::ValidatorMetadata</a></code>
</dt>
<dd>
 Summary of the validator.
</dd>
<dt>
<code><a href="voting_power.md#0x3_voting_power">voting_power</a>: u64</code>
</dt>
<dd>
 The voting power of this validator, which might be different from its
 stake amount.
</dd>
<dt>
<code>operation_cap_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>
 The ID of this validator's current valid <code>UnverifiedValidatorOperationCap</code>
</dd>
<dt>
<code>gas_price: u64</code>
</dt>
<dd>
 Gas price quote, updated only at end of epoch.
</dd>
<dt>
<code><a href="staking_pool.md#0x3_staking_pool">staking_pool</a>: <a href="staking_pool.md#0x3_staking_pool_StakingPool">staking_pool::StakingPool</a></code>
</dt>
<dd>
 Staking pool for this validator.
</dd>
<dt>
<code>commission_rate: u64</code>
</dt>
<dd>
 Commission rate of the validator, in basis point.
</dd>
<dt>
<code>next_epoch_stake: u64</code>
</dt>
<dd>
 Total amount of stake that would be active in the next epoch.
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
<dt>
<code>extra_fields: <a href="../../../.././build/Sui/docs/bag.md#0x2_bag_Bag">bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="0x3_validator_StakingRequestEvent"></a>

## Struct `StakingRequestEvent`

Event emitted when a new stake request is received.


<pre><code><b>struct</b> <a href="validator.md#0x3_validator_StakingRequestEvent">StakingRequestEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>validator_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>staker_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>epoch: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>amount: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x3_validator_UnstakingRequestEvent"></a>

## Struct `UnstakingRequestEvent`

Event emitted when a new unstake request is received.


<pre><code><b>struct</b> <a href="validator.md#0x3_validator_UnstakingRequestEvent">UnstakingRequestEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pool_id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>validator_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>staker_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>stake_activation_epoch: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>unstaking_epoch: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>principal_amount: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>reward_amount: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x3_validator_ECalledDuringNonGenesis"></a>

Function called during non-genesis times.


<pre><code><b>const</b> <a href="validator.md#0x3_validator_ECalledDuringNonGenesis">ECalledDuringNonGenesis</a>: u64 = 12;
</code></pre>



<a name="0x3_validator_ECommissionRateTooHigh"></a>

Commission rate set by the validator is higher than the threshold


<pre><code><b>const</b> <a href="validator.md#0x3_validator_ECommissionRateTooHigh">ECommissionRateTooHigh</a>: u64 = 8;
</code></pre>



<a name="0x3_validator_EInvalidCap"></a>

Capability code is not valid


<pre><code><b>const</b> <a href="validator.md#0x3_validator_EInvalidCap">EInvalidCap</a>: u64 = 101;
</code></pre>



<a name="0x3_validator_EInvalidProofOfPossession"></a>

Invalid proof_of_possession field in ValidatorMetadata


<pre><code><b>const</b> <a href="validator.md#0x3_validator_EInvalidProofOfPossession">EInvalidProofOfPossession</a>: u64 = 0;
</code></pre>



<a name="0x3_validator_EInvalidStakeAmount"></a>

Stake amount is invalid or wrong.


<pre><code><b>const</b> <a href="validator.md#0x3_validator_EInvalidStakeAmount">EInvalidStakeAmount</a>: u64 = 11;
</code></pre>



<a name="0x3_validator_EMetadataInvalidNetAddr"></a>

Invalid net_address field in ValidatorMetadata


<pre><code><b>const</b> <a href="validator.md#0x3_validator_EMetadataInvalidNetAddr">EMetadataInvalidNetAddr</a>: u64 = 4;
</code></pre>



<a name="0x3_validator_EMetadataInvalidNetPubkey"></a>

Invalid network_pubkey_bytes field in ValidatorMetadata


<pre><code><b>const</b> <a href="validator.md#0x3_validator_EMetadataInvalidNetPubkey">EMetadataInvalidNetPubkey</a>: u64 = 2;
</code></pre>



<a name="0x3_validator_EMetadataInvalidP2pAddr"></a>

Invalid p2p_address field in ValidatorMetadata


<pre><code><b>const</b> <a href="validator.md#0x3_validator_EMetadataInvalidP2pAddr">EMetadataInvalidP2pAddr</a>: u64 = 5;
</code></pre>



<a name="0x3_validator_EMetadataInvalidPrimaryAddr"></a>

Invalid primary_address field in ValidatorMetadata


<pre><code><b>const</b> <a href="validator.md#0x3_validator_EMetadataInvalidPrimaryAddr">EMetadataInvalidPrimaryAddr</a>: u64 = 6;
</code></pre>



<a name="0x3_validator_EMetadataInvalidPubkey"></a>

Invalid pubkey_bytes field in ValidatorMetadata


<pre><code><b>const</b> <a href="validator.md#0x3_validator_EMetadataInvalidPubkey">EMetadataInvalidPubkey</a>: u64 = 1;
</code></pre>



<a name="0x3_validator_EMetadataInvalidWorkerAddr"></a>

Invalidworker_address field in ValidatorMetadata


<pre><code><b>const</b> <a href="validator.md#0x3_validator_EMetadataInvalidWorkerAddr">EMetadataInvalidWorkerAddr</a>: u64 = 7;
</code></pre>



<a name="0x3_validator_EMetadataInvalidWorkerPubkey"></a>

Invalid worker_pubkey_bytes field in ValidatorMetadata


<pre><code><b>const</b> <a href="validator.md#0x3_validator_EMetadataInvalidWorkerPubkey">EMetadataInvalidWorkerPubkey</a>: u64 = 3;
</code></pre>



<a name="0x3_validator_ENewCapNotCreatedByValidatorItself"></a>

New Capability is not created by the validator itself


<pre><code><b>const</b> <a href="validator.md#0x3_validator_ENewCapNotCreatedByValidatorItself">ENewCapNotCreatedByValidatorItself</a>: u64 = 100;
</code></pre>



<a name="0x3_validator_ENotValidatorCandidate"></a>

Intended validator is not a candidate one.


<pre><code><b>const</b> <a href="validator.md#0x3_validator_ENotValidatorCandidate">ENotValidatorCandidate</a>: u64 = 10;
</code></pre>



<a name="0x3_validator_EValidatorMetadataExceedingLengthLimit"></a>

Validator Metadata is too long


<pre><code><b>const</b> <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>: u64 = 9;
</code></pre>



<a name="0x3_validator_MAX_COMMISSION_RATE"></a>



<pre><code><b>const</b> <a href="validator.md#0x3_validator_MAX_COMMISSION_RATE">MAX_COMMISSION_RATE</a>: u64 = 10000;
</code></pre>



<a name="0x3_validator_MAX_VALIDATOR_METADATA_LENGTH"></a>



<pre><code><b>const</b> <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>: u64 = 256;
</code></pre>



<a name="0x3_validator_new_metadata"></a>

## Function `new_metadata`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_new_metadata">new_metadata</a>(sui_address: <b>address</b>, protocol_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, network_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, worker_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, name: <a href="_String">string::String</a>, description: <a href="_String">string::String</a>, image_url: <a href="../../../.././build/Sui/docs/url.md#0x2_url_Url">url::Url</a>, project_url: <a href="../../../.././build/Sui/docs/url.md#0x2_url_Url">url::Url</a>, net_address: <a href="_String">string::String</a>, p2p_address: <a href="_String">string::String</a>, primary_address: <a href="_String">string::String</a>, worker_address: <a href="_String">string::String</a>, extra_fields: <a href="../../../.././build/Sui/docs/bag.md#0x2_bag_Bag">bag::Bag</a>): <a href="validator.md#0x3_validator_ValidatorMetadata">validator::ValidatorMetadata</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_new_metadata">new_metadata</a>(
    sui_address: <b>address</b>,
    protocol_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;,
    network_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;,
    worker_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;,
    proof_of_possession: <a href="">vector</a>&lt;u8&gt;,
    name: String,
    description: String,
    image_url: Url,
    project_url: Url,
    net_address: String,
    p2p_address: String,
    primary_address: String,
    worker_address: String,
    extra_fields: Bag,
): <a href="validator.md#0x3_validator_ValidatorMetadata">ValidatorMetadata</a> {
    <b>let</b> metadata = <a href="validator.md#0x3_validator_ValidatorMetadata">ValidatorMetadata</a> {
        sui_address,
        protocol_pubkey_bytes,
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
        next_epoch_protocol_pubkey_bytes: <a href="_none">option::none</a>(),
        next_epoch_network_pubkey_bytes: <a href="_none">option::none</a>(),
        next_epoch_worker_pubkey_bytes: <a href="_none">option::none</a>(),
        next_epoch_proof_of_possession: <a href="_none">option::none</a>(),
        next_epoch_net_address: <a href="_none">option::none</a>(),
        next_epoch_p2p_address: <a href="_none">option::none</a>(),
        next_epoch_primary_address: <a href="_none">option::none</a>(),
        next_epoch_worker_address: <a href="_none">option::none</a>(),
        extra_fields,
    };
    metadata
}
</code></pre>



</details>

<a name="0x3_validator_new"></a>

## Function `new`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_new">new</a>(sui_address: <b>address</b>, protocol_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, network_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, worker_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;, name: <a href="">vector</a>&lt;u8&gt;, description: <a href="">vector</a>&lt;u8&gt;, image_url: <a href="">vector</a>&lt;u8&gt;, project_url: <a href="">vector</a>&lt;u8&gt;, net_address: <a href="">vector</a>&lt;u8&gt;, p2p_address: <a href="">vector</a>&lt;u8&gt;, primary_address: <a href="">vector</a>&lt;u8&gt;, worker_address: <a href="">vector</a>&lt;u8&gt;, gas_price: u64, commission_rate: u64, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="validator.md#0x3_validator_Validator">validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_new">new</a>(
    sui_address: <b>address</b>,
    protocol_pubkey_bytes: <a href="">vector</a>&lt;u8&gt;,
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
    ctx: &<b>mut</b> TxContext
): <a href="validator.md#0x3_validator_Validator">Validator</a> {
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&net_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>
            && <a href="_length">vector::length</a>(&p2p_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>
            && <a href="_length">vector::length</a>(&primary_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>
            && <a href="_length">vector::length</a>(&worker_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>
            && <a href="_length">vector::length</a>(&name) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>
            && <a href="_length">vector::length</a>(&description) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>
            && <a href="_length">vector::length</a>(&image_url) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>
            && <a href="_length">vector::length</a>(&project_url) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    <b>assert</b>!(<a href="validator.md#0x3_validator_commission_rate">commission_rate</a> &lt;= <a href="validator.md#0x3_validator_MAX_COMMISSION_RATE">MAX_COMMISSION_RATE</a>, <a href="validator.md#0x3_validator_ECommissionRateTooHigh">ECommissionRateTooHigh</a>);

    <b>let</b> metadata = <a href="validator.md#0x3_validator_new_metadata">new_metadata</a>(
        sui_address,
        protocol_pubkey_bytes,
        network_pubkey_bytes,
        worker_pubkey_bytes,
        proof_of_possession,
        <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(name)),
        <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(description)),
        <a href="../../../.././build/Sui/docs/url.md#0x2_url_new_unsafe_from_bytes">url::new_unsafe_from_bytes</a>(image_url),
        <a href="../../../.././build/Sui/docs/url.md#0x2_url_new_unsafe_from_bytes">url::new_unsafe_from_bytes</a>(project_url),
        <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(net_address)),
        <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(p2p_address)),
        <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(primary_address)),
        <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(worker_address)),
        <a href="../../../.././build/Sui/docs/bag.md#0x2_bag_new">bag::new</a>(ctx),
    );

    // Checks that the keys & addresses & PoP are valid.
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&metadata);

    <a href="validator.md#0x3_validator_new_from_metadata">new_from_metadata</a>(
        metadata,
        gas_price,
        commission_rate,
        ctx
    )
}
</code></pre>



</details>

<a name="0x3_validator_deactivate"></a>

## Function `deactivate`

Deactivate this validator's staking pool


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_deactivate">deactivate</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, deactivation_epoch: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_deactivate">deactivate</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, deactivation_epoch: u64) {
    <a href="staking_pool.md#0x3_staking_pool_deactivate_staking_pool">staking_pool::deactivate_staking_pool</a>(&<b>mut</b> self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>, deactivation_epoch)
}
</code></pre>



</details>

<a name="0x3_validator_activate"></a>

## Function `activate`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_activate">activate</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, activation_epoch: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_activate">activate</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, activation_epoch: u64) {
    <a href="staking_pool.md#0x3_staking_pool_activate_staking_pool">staking_pool::activate_staking_pool</a>(&<b>mut</b> self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>, activation_epoch);
}
</code></pre>



</details>

<a name="0x3_validator_adjust_stake_and_gas_price"></a>

## Function `adjust_stake_and_gas_price`

Process pending stake and pending withdraws, and update the gas price.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_adjust_stake_and_gas_price">adjust_stake_and_gas_price</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>) {
    self.gas_price = self.next_epoch_gas_price;
    self.commission_rate = self.next_epoch_commission_rate;
}
</code></pre>



</details>

<a name="0x3_validator_request_add_stake"></a>

## Function `request_add_stake`

Request to add stake to the validator's staking pool, processed at the end of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_request_add_stake">request_add_stake</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, stake: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../../.././build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, staker_address: <b>address</b>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_request_add_stake">request_add_stake</a>(
    self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>,
    stake: Balance&lt;SUI&gt;,
    staker_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> stake_amount = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&stake);
    <b>assert</b>!(stake_amount &gt; 0, <a href="validator.md#0x3_validator_EInvalidStakeAmount">EInvalidStakeAmount</a>);
    <b>let</b> stake_epoch = <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) + 1;
    <a href="staking_pool.md#0x3_staking_pool_request_add_stake">staking_pool::request_add_stake</a>(
        &<b>mut</b> self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>, stake, staker_address, stake_epoch, ctx
    );
    // Process stake right away <b>if</b> staking pool is preactive.
    <b>if</b> (<a href="staking_pool.md#0x3_staking_pool_is_preactive">staking_pool::is_preactive</a>(&self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>)) {
        <a href="staking_pool.md#0x3_staking_pool_process_pending_stake">staking_pool::process_pending_stake</a>(&<b>mut</b> self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>);
    };
    self.next_epoch_stake = self.next_epoch_stake + stake_amount;
    <a href="../../../.././build/Sui/docs/event.md#0x2_event_emit">event::emit</a>(
        <a href="validator.md#0x3_validator_StakingRequestEvent">StakingRequestEvent</a> {
            pool_id: <a href="validator.md#0x3_validator_staking_pool_id">staking_pool_id</a>(self),
            validator_address: self.metadata.sui_address,
            staker_address,
            epoch: <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx),
            amount: stake_amount,
        }
    );
}
</code></pre>



</details>

<a name="0x3_validator_request_add_stake_at_genesis"></a>

## Function `request_add_stake_at_genesis`

Request to add stake to the validator's staking pool at genesis


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_request_add_stake_at_genesis">request_add_stake_at_genesis</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, stake: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../../.././build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, staker_address: <b>address</b>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_request_add_stake_at_genesis">request_add_stake_at_genesis</a>(
    self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>,
    stake: Balance&lt;SUI&gt;,
    staker_address: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(<a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx) == 0, <a href="validator.md#0x3_validator_ECalledDuringNonGenesis">ECalledDuringNonGenesis</a>);
    <b>let</b> stake_amount = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&stake);
    <b>assert</b>!(stake_amount &gt; 0, <a href="validator.md#0x3_validator_EInvalidStakeAmount">EInvalidStakeAmount</a>);

    <a href="staking_pool.md#0x3_staking_pool_request_add_stake">staking_pool::request_add_stake</a>(
        &<b>mut</b> self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>,
        stake,
        staker_address,
        0, // epoch 0 -- <a href="genesis.md#0x3_genesis">genesis</a>
        ctx
    );

    // Process stake right away
    <a href="staking_pool.md#0x3_staking_pool_process_pending_stake">staking_pool::process_pending_stake</a>(&<b>mut</b> self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>);
    self.next_epoch_stake = self.next_epoch_stake + stake_amount;
}
</code></pre>



</details>

<a name="0x3_validator_request_withdraw_stake"></a>

## Function `request_withdraw_stake`

Request to withdraw stake from the validator's staking pool, processed at the end of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_request_withdraw_stake">request_withdraw_stake</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, staked_sui: <a href="staking_pool.md#0x3_staking_pool_StakedSui">staking_pool::StakedSui</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_request_withdraw_stake">request_withdraw_stake</a>(
    self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>,
    staked_sui: StakedSui,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> principal_amount = <a href="staking_pool.md#0x3_staking_pool_staked_sui_amount">staking_pool::staked_sui_amount</a>(&staked_sui);
    <b>let</b> stake_activation_epoch = <a href="staking_pool.md#0x3_staking_pool_stake_activation_epoch">staking_pool::stake_activation_epoch</a>(&staked_sui);
    <b>let</b> withdraw_amount = <a href="staking_pool.md#0x3_staking_pool_request_withdraw_stake">staking_pool::request_withdraw_stake</a>(
            &<b>mut</b> self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>, staked_sui, ctx);
    <b>let</b> reward_amount = withdraw_amount - principal_amount;
    self.next_epoch_stake = self.next_epoch_stake - withdraw_amount;
    <a href="../../../.././build/Sui/docs/event.md#0x2_event_emit">event::emit</a>(
        <a href="validator.md#0x3_validator_UnstakingRequestEvent">UnstakingRequestEvent</a> {
            pool_id: <a href="validator.md#0x3_validator_staking_pool_id">staking_pool_id</a>(self),
            validator_address: self.metadata.sui_address,
            staker_address: <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx),
            stake_activation_epoch,
            unstaking_epoch: <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_epoch">tx_context::epoch</a>(ctx),
            principal_amount,
            reward_amount,
        }
    )
}
</code></pre>



</details>

<a name="0x3_validator_request_set_gas_price"></a>

## Function `request_set_gas_price`

Request to set new gas price for the next epoch.
Need to present a <code>ValidatorOperationCap</code>.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_request_set_gas_price">request_set_gas_price</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, verified_cap: <a href="validator_cap.md#0x3_validator_cap_ValidatorOperationCap">validator_cap::ValidatorOperationCap</a>, new_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_request_set_gas_price">request_set_gas_price</a>(
    self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>,
    verified_cap: ValidatorOperationCap,
    new_price: u64,
) {
    <b>let</b> validator_address = *<a href="validator_cap.md#0x3_validator_cap_verified_operation_cap_address">validator_cap::verified_operation_cap_address</a>(&verified_cap);
    <b>assert</b>!(validator_address == self.metadata.sui_address, <a href="validator.md#0x3_validator_EInvalidCap">EInvalidCap</a>);
    self.next_epoch_gas_price = new_price;
}
</code></pre>



</details>

<a name="0x3_validator_set_candidate_gas_price"></a>

## Function `set_candidate_gas_price`

Set new gas price for the candidate validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_set_candidate_gas_price">set_candidate_gas_price</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, verified_cap: <a href="validator_cap.md#0x3_validator_cap_ValidatorOperationCap">validator_cap::ValidatorOperationCap</a>, new_price: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_set_candidate_gas_price">set_candidate_gas_price</a>(
    self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>,
    verified_cap: ValidatorOperationCap,
    new_price: u64
) {
    <b>assert</b>!(<a href="validator.md#0x3_validator_is_preactive">is_preactive</a>(self), <a href="validator.md#0x3_validator_ENotValidatorCandidate">ENotValidatorCandidate</a>);
    <b>let</b> validator_address = *<a href="validator_cap.md#0x3_validator_cap_verified_operation_cap_address">validator_cap::verified_operation_cap_address</a>(&verified_cap);
    <b>assert</b>!(validator_address == self.metadata.sui_address, <a href="validator.md#0x3_validator_EInvalidCap">EInvalidCap</a>);
    self.next_epoch_gas_price = new_price;
    self.gas_price = new_price;
}
</code></pre>



</details>

<a name="0x3_validator_request_set_commission_rate"></a>

## Function `request_set_commission_rate`

Request to set new commission rate for the next epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_request_set_commission_rate">request_set_commission_rate</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, new_commission_rate: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_request_set_commission_rate">request_set_commission_rate</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, new_commission_rate: u64) {
    <b>assert</b>!(new_commission_rate &lt;= <a href="validator.md#0x3_validator_MAX_COMMISSION_RATE">MAX_COMMISSION_RATE</a>, <a href="validator.md#0x3_validator_ECommissionRateTooHigh">ECommissionRateTooHigh</a>);
    self.next_epoch_commission_rate = new_commission_rate;
}
</code></pre>



</details>

<a name="0x3_validator_set_candidate_commission_rate"></a>

## Function `set_candidate_commission_rate`

Set new commission rate for the candidate validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_set_candidate_commission_rate">set_candidate_commission_rate</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, new_commission_rate: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_set_candidate_commission_rate">set_candidate_commission_rate</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, new_commission_rate: u64) {
    <b>assert</b>!(<a href="validator.md#0x3_validator_is_preactive">is_preactive</a>(self), <a href="validator.md#0x3_validator_ENotValidatorCandidate">ENotValidatorCandidate</a>);
    <b>assert</b>!(new_commission_rate &lt;= <a href="validator.md#0x3_validator_MAX_COMMISSION_RATE">MAX_COMMISSION_RATE</a>, <a href="validator.md#0x3_validator_ECommissionRateTooHigh">ECommissionRateTooHigh</a>);
    self.commission_rate = new_commission_rate;
}
</code></pre>



</details>

<a name="0x3_validator_deposit_stake_rewards"></a>

## Function `deposit_stake_rewards`

Deposit stakes rewards into the validator's staking pool, called at the end of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_deposit_stake_rewards">deposit_stake_rewards</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, reward: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../../../.././build/Sui/docs/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_deposit_stake_rewards">deposit_stake_rewards</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, reward: Balance&lt;SUI&gt;) {
    self.next_epoch_stake = self.next_epoch_stake + <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&reward);
    <a href="staking_pool.md#0x3_staking_pool_deposit_rewards">staking_pool::deposit_rewards</a>(&<b>mut</b> self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>, reward);
}
</code></pre>



</details>

<a name="0x3_validator_process_pending_stakes_and_withdraws"></a>

## Function `process_pending_stakes_and_withdraws`

Process pending stakes and withdraws, called at the end of the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_process_pending_stakes_and_withdraws">process_pending_stakes_and_withdraws</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_process_pending_stakes_and_withdraws">process_pending_stakes_and_withdraws</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, ctx: &<b>mut</b> TxContext) {
    <a href="staking_pool.md#0x3_staking_pool_process_pending_stakes_and_withdraws">staking_pool::process_pending_stakes_and_withdraws</a>(&<b>mut</b> self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>, ctx);
    <b>assert</b>!(<a href="validator.md#0x3_validator_stake_amount">stake_amount</a>(self) == self.next_epoch_stake, <a href="validator.md#0x3_validator_EInvalidStakeAmount">EInvalidStakeAmount</a>);
}
</code></pre>



</details>

<a name="0x3_validator_is_preactive"></a>

## Function `is_preactive`

Returns true if the validator is preactive.


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_is_preactive">is_preactive</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_is_preactive">is_preactive</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): bool {
    <a href="staking_pool.md#0x3_staking_pool_is_preactive">staking_pool::is_preactive</a>(&self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>)
}
</code></pre>



</details>

<a name="0x3_validator_metadata"></a>

## Function `metadata`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_metadata">metadata</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="validator.md#0x3_validator_ValidatorMetadata">validator::ValidatorMetadata</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_metadata">metadata</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &<a href="validator.md#0x3_validator_ValidatorMetadata">ValidatorMetadata</a> {
    &self.metadata
}
</code></pre>



</details>

<a name="0x3_validator_sui_address"></a>

## Function `sui_address`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_sui_address">sui_address</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_sui_address">sui_address</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): <b>address</b> {
    self.metadata.sui_address
}
</code></pre>



</details>

<a name="0x3_validator_name"></a>

## Function `name`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_name">name</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_name">name</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &String {
    &self.metadata.name
}
</code></pre>



</details>

<a name="0x3_validator_description"></a>

## Function `description`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_description">description</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_description">description</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &String {
    &self.metadata.description
}
</code></pre>



</details>

<a name="0x3_validator_image_url"></a>

## Function `image_url`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_image_url">image_url</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="../../../.././build/Sui/docs/url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_image_url">image_url</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &Url {
    &self.metadata.image_url
}
</code></pre>



</details>

<a name="0x3_validator_project_url"></a>

## Function `project_url`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_project_url">project_url</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="../../../.././build/Sui/docs/url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_project_url">project_url</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &Url {
    &self.metadata.project_url
}
</code></pre>



</details>

<a name="0x3_validator_network_address"></a>

## Function `network_address`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_network_address">network_address</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_network_address">network_address</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &String {
    &self.metadata.net_address
}
</code></pre>



</details>

<a name="0x3_validator_p2p_address"></a>

## Function `p2p_address`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_p2p_address">p2p_address</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_p2p_address">p2p_address</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &String {
    &self.metadata.p2p_address
}
</code></pre>



</details>

<a name="0x3_validator_primary_address"></a>

## Function `primary_address`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_primary_address">primary_address</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_primary_address">primary_address</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &String {
    &self.metadata.primary_address
}
</code></pre>



</details>

<a name="0x3_validator_worker_address"></a>

## Function `worker_address`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_worker_address">worker_address</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_worker_address">worker_address</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &String {
    &self.metadata.worker_address
}
</code></pre>



</details>

<a name="0x3_validator_protocol_pubkey_bytes"></a>

## Function `protocol_pubkey_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_protocol_pubkey_bytes">protocol_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_protocol_pubkey_bytes">protocol_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &<a href="">vector</a>&lt;u8&gt; {
    &self.metadata.protocol_pubkey_bytes
}
</code></pre>



</details>

<a name="0x3_validator_proof_of_possession"></a>

## Function `proof_of_possession`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_proof_of_possession">proof_of_possession</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_proof_of_possession">proof_of_possession</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &<a href="">vector</a>&lt;u8&gt; {
    &self.metadata.proof_of_possession
}
</code></pre>



</details>

<a name="0x3_validator_network_pubkey_bytes"></a>

## Function `network_pubkey_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_network_pubkey_bytes">network_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_network_pubkey_bytes">network_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &<a href="">vector</a>&lt;u8&gt; {
    &self.metadata.network_pubkey_bytes
}
</code></pre>



</details>

<a name="0x3_validator_worker_pubkey_bytes"></a>

## Function `worker_pubkey_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_worker_pubkey_bytes">worker_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_worker_pubkey_bytes">worker_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &<a href="">vector</a>&lt;u8&gt; {
    &self.metadata.worker_pubkey_bytes
}
</code></pre>



</details>

<a name="0x3_validator_next_epoch_network_address"></a>

## Function `next_epoch_network_address`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_network_address">next_epoch_network_address</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_network_address">next_epoch_network_address</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &Option&lt;String&gt; {
    &self.metadata.next_epoch_net_address
}
</code></pre>



</details>

<a name="0x3_validator_next_epoch_p2p_address"></a>

## Function `next_epoch_p2p_address`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_p2p_address">next_epoch_p2p_address</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_p2p_address">next_epoch_p2p_address</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &Option&lt;String&gt; {
    &self.metadata.next_epoch_p2p_address
}
</code></pre>



</details>

<a name="0x3_validator_next_epoch_primary_address"></a>

## Function `next_epoch_primary_address`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_primary_address">next_epoch_primary_address</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_primary_address">next_epoch_primary_address</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &Option&lt;String&gt; {
    &self.metadata.next_epoch_primary_address
}
</code></pre>



</details>

<a name="0x3_validator_next_epoch_worker_address"></a>

## Function `next_epoch_worker_address`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_worker_address">next_epoch_worker_address</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_worker_address">next_epoch_worker_address</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &Option&lt;String&gt; {
    &self.metadata.next_epoch_worker_address
}
</code></pre>



</details>

<a name="0x3_validator_next_epoch_protocol_pubkey_bytes"></a>

## Function `next_epoch_protocol_pubkey_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_protocol_pubkey_bytes">next_epoch_protocol_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_protocol_pubkey_bytes">next_epoch_protocol_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &Option&lt;<a href="">vector</a>&lt;u8&gt;&gt; {
    &self.metadata.next_epoch_protocol_pubkey_bytes
}
</code></pre>



</details>

<a name="0x3_validator_next_epoch_proof_of_possession"></a>

## Function `next_epoch_proof_of_possession`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_proof_of_possession">next_epoch_proof_of_possession</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_proof_of_possession">next_epoch_proof_of_possession</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &Option&lt;<a href="">vector</a>&lt;u8&gt;&gt; {
    &self.metadata.next_epoch_proof_of_possession
}
</code></pre>



</details>

<a name="0x3_validator_next_epoch_network_pubkey_bytes"></a>

## Function `next_epoch_network_pubkey_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_network_pubkey_bytes">next_epoch_network_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_network_pubkey_bytes">next_epoch_network_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &Option&lt;<a href="">vector</a>&lt;u8&gt;&gt; {
    &self.metadata.next_epoch_network_pubkey_bytes
}
</code></pre>



</details>

<a name="0x3_validator_next_epoch_worker_pubkey_bytes"></a>

## Function `next_epoch_worker_pubkey_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_worker_pubkey_bytes">next_epoch_worker_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;u8&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_worker_pubkey_bytes">next_epoch_worker_pubkey_bytes</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &Option&lt;<a href="">vector</a>&lt;u8&gt;&gt; {
    &self.metadata.next_epoch_worker_pubkey_bytes
}
</code></pre>



</details>

<a name="0x3_validator_operation_cap_id"></a>

## Function `operation_cap_id`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_operation_cap_id">operation_cap_id</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): &<a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_operation_cap_id">operation_cap_id</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): &ID {
    &self.operation_cap_id
}
</code></pre>



</details>

<a name="0x3_validator_next_epoch_gas_price"></a>

## Function `next_epoch_gas_price`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_gas_price">next_epoch_gas_price</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_next_epoch_gas_price">next_epoch_gas_price</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): u64 {
    self.next_epoch_gas_price
}
</code></pre>



</details>

<a name="0x3_validator_total_stake_amount"></a>

## Function `total_stake_amount`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_total_stake_amount">total_stake_amount</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_total_stake_amount">total_stake_amount</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): u64 {
    <b>spec</b> {
        // TODO: this should be provable rather than assumed
        <b>assume</b> self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>.sui_balance &lt;= MAX_U64;
    };
    <a href="staking_pool.md#0x3_staking_pool_sui_balance">staking_pool::sui_balance</a>(&self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>)
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>aborts_if</b> <b>false</b>;
</code></pre>



</details>

<a name="0x3_validator_stake_amount"></a>

## Function `stake_amount`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_stake_amount">stake_amount</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_stake_amount">stake_amount</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): u64 {
    <a href="staking_pool.md#0x3_staking_pool_sui_balance">staking_pool::sui_balance</a>(&self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>)
}
</code></pre>



</details>

<a name="0x3_validator_total_stake"></a>

## Function `total_stake`

Return the total amount staked with this validator


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_total_stake">total_stake</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_total_stake">total_stake</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): u64 {
    <a href="validator.md#0x3_validator_stake_amount">stake_amount</a>(self)
}
</code></pre>



</details>

<a name="0x3_validator_voting_power"></a>

## Function `voting_power`

Return the voting power of this validator.


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x3_voting_power">voting_power</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x3_voting_power">voting_power</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): u64 {
    self.<a href="voting_power.md#0x3_voting_power">voting_power</a>
}
</code></pre>



</details>

<a name="0x3_validator_set_voting_power"></a>

## Function `set_voting_power`

Set the voting power of this validator, called only from validator_set.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_set_voting_power">set_voting_power</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, new_voting_power: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_set_voting_power">set_voting_power</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, new_voting_power: u64) {
    self.<a href="voting_power.md#0x3_voting_power">voting_power</a> = new_voting_power;
}
</code></pre>



</details>

<a name="0x3_validator_pending_stake_amount"></a>

## Function `pending_stake_amount`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_pending_stake_amount">pending_stake_amount</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_pending_stake_amount">pending_stake_amount</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): u64 {
    <a href="staking_pool.md#0x3_staking_pool_pending_stake_amount">staking_pool::pending_stake_amount</a>(&self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>)
}
</code></pre>



</details>

<a name="0x3_validator_pending_stake_withdraw_amount"></a>

## Function `pending_stake_withdraw_amount`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_pending_stake_withdraw_amount">pending_stake_withdraw_amount</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_pending_stake_withdraw_amount">pending_stake_withdraw_amount</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): u64 {
    <a href="staking_pool.md#0x3_staking_pool_pending_stake_withdraw_amount">staking_pool::pending_stake_withdraw_amount</a>(&self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>)
}
</code></pre>



</details>

<a name="0x3_validator_gas_price"></a>

## Function `gas_price`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_gas_price">gas_price</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_gas_price">gas_price</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): u64 {
    self.gas_price
}
</code></pre>



</details>

<a name="0x3_validator_commission_rate"></a>

## Function `commission_rate`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_commission_rate">commission_rate</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_commission_rate">commission_rate</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): u64 {
    self.commission_rate
}
</code></pre>



</details>

<a name="0x3_validator_pool_token_exchange_rate_at_epoch"></a>

## Function `pool_token_exchange_rate_at_epoch`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>, epoch: u64): <a href="staking_pool.md#0x3_staking_pool_PoolTokenExchangeRate">staking_pool::PoolTokenExchangeRate</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_pool_token_exchange_rate_at_epoch">pool_token_exchange_rate_at_epoch</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>, epoch: u64): PoolTokenExchangeRate {
    <a href="staking_pool.md#0x3_staking_pool_pool_token_exchange_rate_at_epoch">staking_pool::pool_token_exchange_rate_at_epoch</a>(&self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>, epoch)
}
</code></pre>



</details>

<a name="0x3_validator_staking_pool_id"></a>

## Function `staking_pool_id`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_staking_pool_id">staking_pool_id</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_staking_pool_id">staking_pool_id</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>): ID {
    <a href="../../../.././build/Sui/docs/object.md#0x2_object_id">object::id</a>(&self.<a href="staking_pool.md#0x3_staking_pool">staking_pool</a>)
}
</code></pre>



</details>

<a name="0x3_validator_is_duplicate"></a>

## Function `is_duplicate`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_is_duplicate">is_duplicate</a>(self: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>, other: &<a href="validator.md#0x3_validator_Validator">validator::Validator</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_is_duplicate">is_duplicate</a>(self: &<a href="validator.md#0x3_validator_Validator">Validator</a>, other: &<a href="validator.md#0x3_validator_Validator">Validator</a>): bool {
     self.metadata.sui_address == other.metadata.sui_address
        || self.metadata.name == other.metadata.name
        || self.metadata.net_address == other.metadata.net_address
        || self.metadata.p2p_address == other.metadata.p2p_address
        || self.metadata.protocol_pubkey_bytes == other.metadata.protocol_pubkey_bytes
        || self.metadata.network_pubkey_bytes == other.metadata.network_pubkey_bytes
        || self.metadata.network_pubkey_bytes == other.metadata.worker_pubkey_bytes
        || self.metadata.worker_pubkey_bytes == other.metadata.worker_pubkey_bytes
        || self.metadata.worker_pubkey_bytes == other.metadata.network_pubkey_bytes
        // All next epoch parameters.
        || <a href="validator.md#0x3_validator_is_equal_some">is_equal_some</a>(&self.metadata.next_epoch_net_address, &other.metadata.next_epoch_net_address)
        || <a href="validator.md#0x3_validator_is_equal_some">is_equal_some</a>(&self.metadata.next_epoch_p2p_address, &other.metadata.next_epoch_p2p_address)
        || <a href="validator.md#0x3_validator_is_equal_some">is_equal_some</a>(&self.metadata.next_epoch_protocol_pubkey_bytes, &other.metadata.next_epoch_protocol_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some">is_equal_some</a>(&self.metadata.next_epoch_network_pubkey_bytes, &other.metadata.next_epoch_network_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some">is_equal_some</a>(&self.metadata.next_epoch_network_pubkey_bytes, &other.metadata.next_epoch_worker_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some">is_equal_some</a>(&self.metadata.next_epoch_worker_pubkey_bytes, &other.metadata.next_epoch_worker_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some">is_equal_some</a>(&self.metadata.next_epoch_worker_pubkey_bytes, &other.metadata.next_epoch_network_pubkey_bytes)
        // My next epoch parameters <b>with</b> other current epoch parameters.
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&self.metadata.next_epoch_net_address, &other.metadata.net_address)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&self.metadata.next_epoch_p2p_address, &other.metadata.p2p_address)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&self.metadata.next_epoch_protocol_pubkey_bytes, &other.metadata.protocol_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&self.metadata.next_epoch_network_pubkey_bytes, &other.metadata.network_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&self.metadata.next_epoch_network_pubkey_bytes, &other.metadata.worker_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&self.metadata.next_epoch_worker_pubkey_bytes, &other.metadata.worker_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&self.metadata.next_epoch_worker_pubkey_bytes, &other.metadata.network_pubkey_bytes)
        // Other next epoch parameters <b>with</b> my current epoch parameters.
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&other.metadata.next_epoch_net_address, &self.metadata.net_address)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&other.metadata.next_epoch_p2p_address, &self.metadata.p2p_address)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&other.metadata.next_epoch_protocol_pubkey_bytes, &self.metadata.protocol_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&other.metadata.next_epoch_network_pubkey_bytes, &self.metadata.network_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&other.metadata.next_epoch_network_pubkey_bytes, &self.metadata.worker_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&other.metadata.next_epoch_worker_pubkey_bytes, &self.metadata.worker_pubkey_bytes)
        || <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>(&other.metadata.next_epoch_worker_pubkey_bytes, &self.metadata.network_pubkey_bytes)
}
</code></pre>



</details>

<a name="0x3_validator_is_equal_some_and_value"></a>

## Function `is_equal_some_and_value`



<pre><code><b>fun</b> <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>&lt;T&gt;(a: &<a href="_Option">option::Option</a>&lt;T&gt;, b: &T): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator.md#0x3_validator_is_equal_some_and_value">is_equal_some_and_value</a>&lt;T&gt;(a: &Option&lt;T&gt;, b: &T): bool {
    <b>if</b> (<a href="_is_none">option::is_none</a>(a)) {
        <b>false</b>
    } <b>else</b> {
        <a href="_borrow">option::borrow</a>(a) == b
    }
}
</code></pre>



</details>

<a name="0x3_validator_is_equal_some"></a>

## Function `is_equal_some`



<pre><code><b>fun</b> <a href="validator.md#0x3_validator_is_equal_some">is_equal_some</a>&lt;T&gt;(a: &<a href="_Option">option::Option</a>&lt;T&gt;, b: &<a href="_Option">option::Option</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator.md#0x3_validator_is_equal_some">is_equal_some</a>&lt;T&gt;(a: &Option&lt;T&gt;, b: &Option&lt;T&gt;): bool {
    <b>if</b> (<a href="_is_none">option::is_none</a>(a) || <a href="_is_none">option::is_none</a>(b)) {
        <b>false</b>
    } <b>else</b> {
        <a href="_borrow">option::borrow</a>(a) == <a href="_borrow">option::borrow</a>(b)
    }
}
</code></pre>



</details>

<a name="0x3_validator_new_unverified_validator_operation_cap_and_transfer"></a>

## Function `new_unverified_validator_operation_cap_and_transfer`

Create a new <code>UnverifiedValidatorOperationCap</code>, transfer to the validator,
and registers it, thus revoking the previous cap's permission.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_new_unverified_validator_operation_cap_and_transfer">new_unverified_validator_operation_cap_and_transfer</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_new_unverified_validator_operation_cap_and_transfer">new_unverified_validator_operation_cap_and_transfer</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, ctx: &<b>mut</b> TxContext) {
    <b>let</b> <b>address</b> = <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx);
    <b>assert</b>!(<b>address</b> == self.metadata.sui_address, <a href="validator.md#0x3_validator_ENewCapNotCreatedByValidatorItself">ENewCapNotCreatedByValidatorItself</a>);
    <b>let</b> new_id = <a href="validator_cap.md#0x3_validator_cap_new_unverified_validator_operation_cap_and_transfer">validator_cap::new_unverified_validator_operation_cap_and_transfer</a>(<b>address</b>, ctx);
    self.operation_cap_id = new_id;
}
</code></pre>



</details>

<a name="0x3_validator_update_name"></a>

## Function `update_name`

Update name of the validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_name">update_name</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, name: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_name">update_name</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, name: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&name) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    self.metadata.name = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(name));
}
</code></pre>



</details>

<a name="0x3_validator_update_description"></a>

## Function `update_description`

Update description of the validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_description">update_description</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, description: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_description">update_description</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, description: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&description) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    self.metadata.description = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(description));
}
</code></pre>



</details>

<a name="0x3_validator_update_image_url"></a>

## Function `update_image_url`

Update image url of the validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_image_url">update_image_url</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, image_url: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_image_url">update_image_url</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, image_url: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&image_url) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    self.metadata.image_url = <a href="../../../.././build/Sui/docs/url.md#0x2_url_new_unsafe_from_bytes">url::new_unsafe_from_bytes</a>(image_url);
}
</code></pre>



</details>

<a name="0x3_validator_update_project_url"></a>

## Function `update_project_url`

Update project url of the validator.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_project_url">update_project_url</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, project_url: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_project_url">update_project_url</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, project_url: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&project_url) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    self.metadata.project_url = <a href="../../../.././build/Sui/docs/url.md#0x2_url_new_unsafe_from_bytes">url::new_unsafe_from_bytes</a>(project_url);
}
</code></pre>



</details>

<a name="0x3_validator_update_next_epoch_network_address"></a>

## Function `update_next_epoch_network_address`

Update network address of this validator, taking effects from next epoch


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_network_address">update_next_epoch_network_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, net_address: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_network_address">update_next_epoch_network_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, net_address: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&net_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    <b>let</b> net_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(net_address));
    self.metadata.next_epoch_net_address = <a href="_some">option::some</a>(net_address);
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_candidate_network_address"></a>

## Function `update_candidate_network_address`

Update network address of this candidate validator


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_network_address">update_candidate_network_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, net_address: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_network_address">update_candidate_network_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, net_address: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(<a href="validator.md#0x3_validator_is_preactive">is_preactive</a>(self), <a href="validator.md#0x3_validator_ENotValidatorCandidate">ENotValidatorCandidate</a>);
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&net_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    <b>let</b> net_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(net_address));
    self.metadata.net_address = net_address;
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_next_epoch_p2p_address"></a>

## Function `update_next_epoch_p2p_address`

Update p2p address of this validator, taking effects from next epoch


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_p2p_address">update_next_epoch_p2p_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, p2p_address: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_p2p_address">update_next_epoch_p2p_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, p2p_address: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&p2p_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    <b>let</b> p2p_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(p2p_address));
    self.metadata.next_epoch_p2p_address = <a href="_some">option::some</a>(p2p_address);
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_candidate_p2p_address"></a>

## Function `update_candidate_p2p_address`

Update p2p address of this candidate validator


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_p2p_address">update_candidate_p2p_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, p2p_address: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_p2p_address">update_candidate_p2p_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, p2p_address: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(<a href="validator.md#0x3_validator_is_preactive">is_preactive</a>(self), <a href="validator.md#0x3_validator_ENotValidatorCandidate">ENotValidatorCandidate</a>);
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&p2p_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    <b>let</b> p2p_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(p2p_address));
    self.metadata.p2p_address = p2p_address;
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_next_epoch_primary_address"></a>

## Function `update_next_epoch_primary_address`

Update primary address of this validator, taking effects from next epoch


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_primary_address">update_next_epoch_primary_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, primary_address: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_primary_address">update_next_epoch_primary_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, primary_address: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&primary_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    <b>let</b> primary_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(primary_address));
    self.metadata.next_epoch_primary_address = <a href="_some">option::some</a>(primary_address);
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_candidate_primary_address"></a>

## Function `update_candidate_primary_address`

Update primary address of this candidate validator


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_primary_address">update_candidate_primary_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, primary_address: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_primary_address">update_candidate_primary_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, primary_address: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(<a href="validator.md#0x3_validator_is_preactive">is_preactive</a>(self), <a href="validator.md#0x3_validator_ENotValidatorCandidate">ENotValidatorCandidate</a>);
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&primary_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    <b>let</b> primary_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(primary_address));
    self.metadata.primary_address = primary_address;
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_next_epoch_worker_address"></a>

## Function `update_next_epoch_worker_address`

Update worker address of this validator, taking effects from next epoch


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_worker_address">update_next_epoch_worker_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, worker_address: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_worker_address">update_next_epoch_worker_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, worker_address: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&worker_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    <b>let</b> worker_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(worker_address));
    self.metadata.next_epoch_worker_address = <a href="_some">option::some</a>(worker_address);
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_candidate_worker_address"></a>

## Function `update_candidate_worker_address`

Update worker address of this candidate validator


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_worker_address">update_candidate_worker_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, worker_address: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_worker_address">update_candidate_worker_address</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, worker_address: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(<a href="validator.md#0x3_validator_is_preactive">is_preactive</a>(self), <a href="validator.md#0x3_validator_ENotValidatorCandidate">ENotValidatorCandidate</a>);
    <b>assert</b>!(
        <a href="_length">vector::length</a>(&worker_address) &lt;= <a href="validator.md#0x3_validator_MAX_VALIDATOR_METADATA_LENGTH">MAX_VALIDATOR_METADATA_LENGTH</a>,
        <a href="validator.md#0x3_validator_EValidatorMetadataExceedingLengthLimit">EValidatorMetadataExceedingLengthLimit</a>
    );
    <b>let</b> worker_address = <a href="_from_ascii">string::from_ascii</a>(<a href="_string">ascii::string</a>(worker_address));
    self.metadata.worker_address = worker_address;
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_next_epoch_protocol_pubkey"></a>

## Function `update_next_epoch_protocol_pubkey`

Update protocol public key of this validator, taking effects from next epoch


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_protocol_pubkey">update_next_epoch_protocol_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, protocol_pubkey: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_protocol_pubkey">update_next_epoch_protocol_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, protocol_pubkey: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;) {
    self.metadata.next_epoch_protocol_pubkey_bytes = <a href="_some">option::some</a>(protocol_pubkey);
    self.metadata.next_epoch_proof_of_possession = <a href="_some">option::some</a>(proof_of_possession);
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_candidate_protocol_pubkey"></a>

## Function `update_candidate_protocol_pubkey`

Update protocol public key of this candidate validator


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_protocol_pubkey">update_candidate_protocol_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, protocol_pubkey: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_protocol_pubkey">update_candidate_protocol_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, protocol_pubkey: <a href="">vector</a>&lt;u8&gt;, proof_of_possession: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(<a href="validator.md#0x3_validator_is_preactive">is_preactive</a>(self), <a href="validator.md#0x3_validator_ENotValidatorCandidate">ENotValidatorCandidate</a>);
    self.metadata.protocol_pubkey_bytes = protocol_pubkey;
    self.metadata.proof_of_possession = proof_of_possession;
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_next_epoch_network_pubkey"></a>

## Function `update_next_epoch_network_pubkey`

Update network public key of this validator, taking effects from next epoch


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_network_pubkey">update_next_epoch_network_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, network_pubkey: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_network_pubkey">update_next_epoch_network_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, network_pubkey: <a href="">vector</a>&lt;u8&gt;) {
    self.metadata.next_epoch_network_pubkey_bytes = <a href="_some">option::some</a>(network_pubkey);
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_candidate_network_pubkey"></a>

## Function `update_candidate_network_pubkey`

Update network public key of this candidate validator


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_network_pubkey">update_candidate_network_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, network_pubkey: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_network_pubkey">update_candidate_network_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, network_pubkey: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(<a href="validator.md#0x3_validator_is_preactive">is_preactive</a>(self), <a href="validator.md#0x3_validator_ENotValidatorCandidate">ENotValidatorCandidate</a>);
    self.metadata.network_pubkey_bytes = network_pubkey;
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_next_epoch_worker_pubkey"></a>

## Function `update_next_epoch_worker_pubkey`

Update Narwhal worker public key of this validator, taking effects from next epoch


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_worker_pubkey">update_next_epoch_worker_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, worker_pubkey: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_next_epoch_worker_pubkey">update_next_epoch_worker_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, worker_pubkey: <a href="">vector</a>&lt;u8&gt;) {
    self.metadata.next_epoch_worker_pubkey_bytes = <a href="_some">option::some</a>(worker_pubkey);
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_update_candidate_worker_pubkey"></a>

## Function `update_candidate_worker_pubkey`

Update Narwhal worker public key of this candidate validator


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_worker_pubkey">update_candidate_worker_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>, worker_pubkey: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_update_candidate_worker_pubkey">update_candidate_worker_pubkey</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>, worker_pubkey: <a href="">vector</a>&lt;u8&gt;) {
    <b>assert</b>!(<a href="validator.md#0x3_validator_is_preactive">is_preactive</a>(self), <a href="validator.md#0x3_validator_ENotValidatorCandidate">ENotValidatorCandidate</a>);
    self.metadata.worker_pubkey_bytes = worker_pubkey;
    <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(&self.metadata);
}
</code></pre>



</details>

<a name="0x3_validator_effectuate_staged_metadata"></a>

## Function `effectuate_staged_metadata`

Effectutate all staged next epoch metadata for this validator.
NOTE: this function SHOULD ONLY be called by validator_set when
advancing an epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_effectuate_staged_metadata">effectuate_staged_metadata</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">validator::Validator</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="validator.md#0x3_validator_effectuate_staged_metadata">effectuate_staged_metadata</a>(self: &<b>mut</b> <a href="validator.md#0x3_validator_Validator">Validator</a>) {
    <b>if</b> (<a href="_is_some">option::is_some</a>(<a href="validator.md#0x3_validator_next_epoch_network_address">next_epoch_network_address</a>(self))) {
        self.metadata.net_address = <a href="_extract">option::extract</a>(&<b>mut</b> self.metadata.next_epoch_net_address);
        self.metadata.next_epoch_net_address = <a href="_none">option::none</a>();
    };

    <b>if</b> (<a href="_is_some">option::is_some</a>(<a href="validator.md#0x3_validator_next_epoch_p2p_address">next_epoch_p2p_address</a>(self))) {
        self.metadata.p2p_address = <a href="_extract">option::extract</a>(&<b>mut</b> self.metadata.next_epoch_p2p_address);
        self.metadata.next_epoch_p2p_address = <a href="_none">option::none</a>();
    };

    <b>if</b> (<a href="_is_some">option::is_some</a>(<a href="validator.md#0x3_validator_next_epoch_primary_address">next_epoch_primary_address</a>(self))) {
        self.metadata.primary_address = <a href="_extract">option::extract</a>(&<b>mut</b> self.metadata.next_epoch_primary_address);
        self.metadata.next_epoch_primary_address = <a href="_none">option::none</a>();
    };

    <b>if</b> (<a href="_is_some">option::is_some</a>(<a href="validator.md#0x3_validator_next_epoch_worker_address">next_epoch_worker_address</a>(self))) {
        self.metadata.worker_address = <a href="_extract">option::extract</a>(&<b>mut</b> self.metadata.next_epoch_worker_address);
        self.metadata.next_epoch_worker_address = <a href="_none">option::none</a>();
    };

    <b>if</b> (<a href="_is_some">option::is_some</a>(<a href="validator.md#0x3_validator_next_epoch_protocol_pubkey_bytes">next_epoch_protocol_pubkey_bytes</a>(self))) {
        self.metadata.protocol_pubkey_bytes = <a href="_extract">option::extract</a>(&<b>mut</b> self.metadata.next_epoch_protocol_pubkey_bytes);
        self.metadata.next_epoch_protocol_pubkey_bytes = <a href="_none">option::none</a>();
        self.metadata.proof_of_possession = <a href="_extract">option::extract</a>(&<b>mut</b> self.metadata.next_epoch_proof_of_possession);
        self.metadata.next_epoch_proof_of_possession = <a href="_none">option::none</a>();
    };

    <b>if</b> (<a href="_is_some">option::is_some</a>(<a href="validator.md#0x3_validator_next_epoch_network_pubkey_bytes">next_epoch_network_pubkey_bytes</a>(self))) {
        self.metadata.network_pubkey_bytes = <a href="_extract">option::extract</a>(&<b>mut</b> self.metadata.next_epoch_network_pubkey_bytes);
        self.metadata.next_epoch_network_pubkey_bytes = <a href="_none">option::none</a>();
    };

    <b>if</b> (<a href="_is_some">option::is_some</a>(<a href="validator.md#0x3_validator_next_epoch_worker_pubkey_bytes">next_epoch_worker_pubkey_bytes</a>(self))) {
        self.metadata.worker_pubkey_bytes = <a href="_extract">option::extract</a>(&<b>mut</b> self.metadata.next_epoch_worker_pubkey_bytes);
        self.metadata.next_epoch_worker_pubkey_bytes = <a href="_none">option::none</a>();
    };
}
</code></pre>



</details>

<a name="0x3_validator_validate_metadata"></a>

## Function `validate_metadata`

Aborts if validator metadata is valid


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(metadata: &<a href="validator.md#0x3_validator_ValidatorMetadata">validator::ValidatorMetadata</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_validate_metadata">validate_metadata</a>(metadata: &<a href="validator.md#0x3_validator_ValidatorMetadata">ValidatorMetadata</a>) {
    <a href="validator.md#0x3_validator_validate_metadata_bcs">validate_metadata_bcs</a>(<a href="../../../.././build/Sui/docs/bcs.md#0x1_bcs_to_bytes">bcs::to_bytes</a>(metadata));
}
</code></pre>



</details>

<a name="0x3_validator_validate_metadata_bcs"></a>

## Function `validate_metadata_bcs`



<pre><code><b>public</b> <b>fun</b> <a href="validator.md#0x3_validator_validate_metadata_bcs">validate_metadata_bcs</a>(metadata: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="validator.md#0x3_validator_validate_metadata_bcs">validate_metadata_bcs</a>(metadata: <a href="">vector</a>&lt;u8&gt;);
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>pragma</b> opaque;
<b>aborts_if</b> [abstract] <b>true</b>;
</code></pre>



</details>

<a name="0x3_validator_new_from_metadata"></a>

## Function `new_from_metadata`

Create a new validator from the given <code><a href="validator.md#0x3_validator_ValidatorMetadata">ValidatorMetadata</a></code>, called by both <code>new</code> and <code>new_for_testing</code>.


<pre><code><b>fun</b> <a href="validator.md#0x3_validator_new_from_metadata">new_from_metadata</a>(metadata: <a href="validator.md#0x3_validator_ValidatorMetadata">validator::ValidatorMetadata</a>, gas_price: u64, commission_rate: u64, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="validator.md#0x3_validator_Validator">validator::Validator</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="validator.md#0x3_validator_new_from_metadata">new_from_metadata</a>(
    metadata: <a href="validator.md#0x3_validator_ValidatorMetadata">ValidatorMetadata</a>,
    gas_price: u64,
    commission_rate: u64,
    ctx: &<b>mut</b> TxContext
): <a href="validator.md#0x3_validator_Validator">Validator</a> {
    <b>let</b> sui_address = metadata.sui_address;

    <b>let</b> <a href="staking_pool.md#0x3_staking_pool">staking_pool</a> = <a href="staking_pool.md#0x3_staking_pool_new">staking_pool::new</a>(ctx);

    <b>let</b> operation_cap_id = <a href="validator_cap.md#0x3_validator_cap_new_unverified_validator_operation_cap_and_transfer">validator_cap::new_unverified_validator_operation_cap_and_transfer</a>(sui_address, ctx);
    <a href="validator.md#0x3_validator_Validator">Validator</a> {
        metadata,
        // Initialize the voting power <b>to</b> be 0.
        // At the epoch change <b>where</b> this <a href="validator.md#0x3_validator">validator</a> is actually added <b>to</b> the
        // active <a href="validator.md#0x3_validator">validator</a> set, the voting power will be updated accordingly.
        <a href="voting_power.md#0x3_voting_power">voting_power</a>: 0,
        operation_cap_id,
        gas_price,
        <a href="staking_pool.md#0x3_staking_pool">staking_pool</a>,
        commission_rate,
        next_epoch_stake: 0,
        next_epoch_gas_price: gas_price,
        next_epoch_commission_rate: commission_rate,
        extra_fields: <a href="../../../.././build/Sui/docs/bag.md#0x2_bag_new">bag::new</a>(ctx),
    }
}
</code></pre>



</details>
