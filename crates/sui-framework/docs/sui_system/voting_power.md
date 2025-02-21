---
title: Module `sui_system::voting_power`
---



-  [Struct `VotingPowerInfo`](#sui_system_voting_power_VotingPowerInfo)
-  [Struct `VotingPowerInfoV2`](#sui_system_voting_power_VotingPowerInfoV2)
-  [Constants](#@Constants_0)
-  [Function `set_voting_power`](#sui_system_voting_power_set_voting_power)
-  [Function `init_voting_power_info`](#sui_system_voting_power_init_voting_power_info)
-  [Function `total_stake`](#sui_system_voting_power_total_stake)
-  [Function `insert`](#sui_system_voting_power_insert)
-  [Function `adjust_voting_power`](#sui_system_voting_power_adjust_voting_power)
-  [Function `update_voting_power`](#sui_system_voting_power_update_voting_power)
-  [Function `check_invariants`](#sui_system_voting_power_check_invariants)
-  [Function `total_voting_power`](#sui_system_voting_power_total_voting_power)
-  [Function `quorum_threshold`](#sui_system_voting_power_quorum_threshold)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/u64.md#std_u64">std::u64</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/balance.md#sui_balance">sui::balance</a>;
<b>use</b> <a href="../sui/coin.md#sui_coin">sui::coin</a>;
<b>use</b> <a href="../sui/config.md#sui_config">sui::config</a>;
<b>use</b> <a href="../sui/deny_list.md#sui_deny_list">sui::deny_list</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/sui.md#sui_sui">sui::sui</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
<b>use</b> <a href="../sui_system/staking_pool.md#sui_system_staking_pool">sui_system::staking_pool</a>;
<b>use</b> <a href="../sui_system/validator.md#sui_system_validator">sui_system::validator</a>;
<b>use</b> <a href="../sui_system/validator_cap.md#sui_system_validator_cap">sui_system::validator_cap</a>;
</code></pre>



<a name="sui_system_voting_power_VotingPowerInfo"></a>

## Struct `VotingPowerInfo`

Deprecated. Use VotingPowerInfoV2 instead.


<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfo">VotingPowerInfo</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>validator_index: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_system_voting_power_VotingPowerInfoV2"></a>

## Struct `VotingPowerInfoV2`



<pre><code><b>public</b> <b>struct</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>validator_index: u64</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>stake: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_system_voting_power_EInvalidVotingPower"></a>



<pre><code><b>const</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_EInvalidVotingPower">EInvalidVotingPower</a>: u64 = 4;
</code></pre>



<a name="sui_system_voting_power_ERelativePowerMismatch"></a>



<pre><code><b>const</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_ERelativePowerMismatch">ERelativePowerMismatch</a>: u64 = 2;
</code></pre>



<a name="sui_system_voting_power_ETotalPowerMismatch"></a>



<pre><code><b>const</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_ETotalPowerMismatch">ETotalPowerMismatch</a>: u64 = 1;
</code></pre>



<a name="sui_system_voting_power_EVotingPowerOverThreshold"></a>



<pre><code><b>const</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_EVotingPowerOverThreshold">EVotingPowerOverThreshold</a>: u64 = 3;
</code></pre>



<a name="sui_system_voting_power_MAX_VOTING_POWER"></a>



<pre><code><b>const</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>: u64 = 1000;
</code></pre>



<a name="sui_system_voting_power_QUORUM_THRESHOLD"></a>

Quorum threshold for our fixed voting power--any message signed by this much voting power can be trusted
up to BFT assumptions


<pre><code><b>const</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_QUORUM_THRESHOLD">QUORUM_THRESHOLD</a>: u64 = 6667;
</code></pre>



<a name="sui_system_voting_power_TOTAL_VOTING_POWER"></a>

Set total_voting_power as 10_000 by convention. Individual voting powers can be interpreted
as easily understandable basis points (e.g., voting_power: 100 = 1%, voting_power: 1 = 0.01%) rather than
opaque quantities whose meaning changes from epoch to epoch as the total amount staked shifts.
Fixing the total voting power allows clients to hardcode the quorum threshold and total_voting power rather
than recomputing these.


<pre><code><b>const</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>: u64 = 10000;
</code></pre>



<a name="sui_system_voting_power_set_voting_power"></a>

## Function `set_voting_power`

Set the voting power of all validators.
Each validator's voting power is initialized using their stake. We then attempt to cap their voting power
at <code><a href="../sui_system/voting_power.md#sui_system_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a></code>. If <code><a href="../sui_system/voting_power.md#sui_system_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a></code> is not a feasible cap, we pick the lowest possible cap.


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_set_voting_power">set_voting_power</a>(validators: &<b>mut</b> vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_set_voting_power">set_voting_power</a>(validators: &<b>mut</b> vector&lt;Validator&gt;) {
    // If threshold_pct is too small, it's possible that even when all validators reach the threshold we still don't
    // have 100%. So we bound the threshold_pct to be always enough to find a solution.
    <b>let</b> threshold = <a href="../sui_system/voting_power.md#sui_system_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>.min(
        <a href="../sui_system/voting_power.md#sui_system_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>.max(<a href="../sui_system/voting_power.md#sui_system_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>.divide_and_round_up(validators.length())),
    );
    <b>let</b> (<b>mut</b> info_list, remaining_power) = <a href="../sui_system/voting_power.md#sui_system_voting_power_init_voting_power_info">init_voting_power_info</a>(validators, threshold);
    <a href="../sui_system/voting_power.md#sui_system_voting_power_adjust_voting_power">adjust_voting_power</a>(&<b>mut</b> info_list, threshold, remaining_power);
    <a href="../sui_system/voting_power.md#sui_system_voting_power_update_voting_power">update_voting_power</a>(validators, info_list);
    <a href="../sui_system/voting_power.md#sui_system_voting_power_check_invariants">check_invariants</a>(validators);
}
</code></pre>



</details>

<a name="sui_system_voting_power_init_voting_power_info"></a>

## Function `init_voting_power_info`

Create the initial voting power of each validator, set using their stake, but capped using threshold.
We also perform insertion sort while creating the voting power list, by maintaining the list in
descending order using voting power.
Anything beyond the threshold is added to the remaining_power, which is also returned.


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_init_voting_power_info">init_voting_power_info</a>(validators: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, threshold: u64): (vector&lt;<a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">sui_system::voting_power::VotingPowerInfoV2</a>&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_init_voting_power_info">init_voting_power_info</a>(
    validators: &vector&lt;Validator&gt;,
    threshold: u64,
): (vector&lt;<a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;, u64) {
    <b>let</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a> = <a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a>(validators);
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = validators.length();
    <b>let</b> <b>mut</b> total_power = 0;
    <b>let</b> <b>mut</b> result = vector[];
    <b>while</b> (i &lt; len) {
        <b>let</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> = &validators[i];
        <b>let</b> stake = <a href="../sui_system/validator.md#sui_system_validator">validator</a>.<a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a>();
        <b>let</b> adjusted_stake = stake <b>as</b> u128 * (<a href="../sui_system/voting_power.md#sui_system_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a> <b>as</b> u128) / (<a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a> <b>as</b> u128);
        <b>let</b> <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a> = (adjusted_stake <b>as</b> u64).min(threshold);
        <b>let</b> info = <a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a> {
            validator_index: i,
            <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>,
            stake,
        };
        <a href="../sui_system/voting_power.md#sui_system_voting_power_insert">insert</a>(&<b>mut</b> result, info);
        total_power = total_power + <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>;
        i = i + 1;
    };
    (result, <a href="../sui_system/voting_power.md#sui_system_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a> - total_power)
}
</code></pre>



</details>

<a name="sui_system_voting_power_total_stake"></a>

## Function `total_stake`

Sum up the total stake of all validators.


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a>(validators: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a>(validators: &vector&lt;Validator&gt;): u64 {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = validators.length();
    <b>let</b> <b>mut</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a> =0 ;
    <b>while</b> (i &lt; len) {
        <a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a> = <a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a> + validators[i].<a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a>();
        i = i + 1;
    };
    <a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a>
}
</code></pre>



</details>

<a name="sui_system_voting_power_insert"></a>

## Function `insert`

Insert <code>new_info</code> to <code>info_list</code> as part of insertion sort, such that <code>info_list</code> is always sorted
using stake, in descending order.


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_insert">insert</a>(info_list: &<b>mut</b> vector&lt;<a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">sui_system::voting_power::VotingPowerInfoV2</a>&gt;, new_info: <a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">sui_system::voting_power::VotingPowerInfoV2</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_insert">insert</a>(info_list: &<b>mut</b> vector&lt;<a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;, new_info: <a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>) {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = info_list.length();
    <b>while</b> (i &lt; len && info_list[i].stake &gt; new_info.stake) {
        i = i + 1;
    };
    info_list.<a href="../sui_system/voting_power.md#sui_system_voting_power_insert">insert</a>(new_info, i);
}
</code></pre>



</details>

<a name="sui_system_voting_power_adjust_voting_power"></a>

## Function `adjust_voting_power`

Distribute remaining_power to validators that are not capped at threshold.


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_adjust_voting_power">adjust_voting_power</a>(info_list: &<b>mut</b> vector&lt;<a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">sui_system::voting_power::VotingPowerInfoV2</a>&gt;, threshold: u64, remaining_power: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_adjust_voting_power">adjust_voting_power</a>(info_list: &<b>mut</b> vector&lt;<a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;, threshold: u64, <b>mut</b> remaining_power: u64) {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = info_list.length();
    <b>while</b> (i &lt; len && remaining_power &gt; 0) {
        <b>let</b> v = &<b>mut</b> info_list[i];
        // planned is the amount of extra power we want to distribute to this <a href="../sui_system/validator.md#sui_system_validator">validator</a>.
        <b>let</b> planned = remaining_power.divide_and_round_up(len - i);
        // target is the targeting power this <a href="../sui_system/validator.md#sui_system_validator">validator</a> will reach, capped by threshold.
        <b>let</b> target = threshold.min(v.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a> + planned);
        // actual is the actual amount of power we will be distributing to this <a href="../sui_system/validator.md#sui_system_validator">validator</a>.
        <b>let</b> actual = remaining_power.min(target - v.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>);
        v.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a> = v.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a> + actual;
        <b>assert</b>!(v.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a> &lt;= threshold, <a href="../sui_system/voting_power.md#sui_system_voting_power_EVotingPowerOverThreshold">EVotingPowerOverThreshold</a>);
        remaining_power = remaining_power - actual;
        i = i + 1;
    };
    <b>assert</b>!(remaining_power == 0, <a href="../sui_system/voting_power.md#sui_system_voting_power_ETotalPowerMismatch">ETotalPowerMismatch</a>);
}
</code></pre>



</details>

<a name="sui_system_voting_power_update_voting_power"></a>

## Function `update_voting_power`

Update validators with the decided voting power.


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_update_voting_power">update_voting_power</a>(validators: &<b>mut</b> vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;, info_list: vector&lt;<a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">sui_system::voting_power::VotingPowerInfoV2</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_update_voting_power">update_voting_power</a>(validators: &<b>mut</b> vector&lt;Validator&gt;, <b>mut</b> info_list: vector&lt;<a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;) {
    <b>while</b> (info_list.length() != 0) {
        <b>let</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a> {
            validator_index,
            <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>,
            stake: _,
        } = info_list.pop_back();
        <b>let</b> v = &<b>mut</b> validators[validator_index];
        v.<a href="../sui_system/voting_power.md#sui_system_voting_power_set_voting_power">set_voting_power</a>(<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>);
    };
    info_list.destroy_empty();
}
</code></pre>



</details>

<a name="sui_system_voting_power_check_invariants"></a>

## Function `check_invariants`

Check a few invariants that must hold after setting the voting power.


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_check_invariants">check_invariants</a>(v: &vector&lt;<a href="../sui_system/validator.md#sui_system_validator_Validator">sui_system::validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_check_invariants">check_invariants</a>(v: &vector&lt;Validator&gt;) {
    // First check that the total voting power must be <a href="../sui_system/voting_power.md#sui_system_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>.
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = v.length();
    <b>let</b> <b>mut</b> total = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a> = v[i].<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>();
        <b>assert</b>!(<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a> &gt; 0, <a href="../sui_system/voting_power.md#sui_system_voting_power_EInvalidVotingPower">EInvalidVotingPower</a>);
        total = total + <a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>;
        i = i + 1;
    };
    <b>assert</b>!(total == <a href="../sui_system/voting_power.md#sui_system_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>, <a href="../sui_system/voting_power.md#sui_system_voting_power_ETotalPowerMismatch">ETotalPowerMismatch</a>);
    // Second check that <b>if</b> <a href="../sui_system/validator.md#sui_system_validator">validator</a> A's stake is larger than B's stake, A's voting power must be no less
    // than B's voting power; similarly, <b>if</b> A's stake is less than B's stake, A's voting power must be no larger
    // than B's voting power.
    <b>let</b> <b>mut</b> a = 0;
    <b>while</b> (a &lt; len) {
        <b>let</b> <b>mut</b> b = a + 1;
        <b>while</b> (b &lt; len) {
            <b>let</b> validator_a = &v[a];
            <b>let</b> validator_b = &v[b];
            <b>let</b> stake_a = validator_a.<a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a>();
            <b>let</b> stake_b = validator_b.<a href="../sui_system/voting_power.md#sui_system_voting_power_total_stake">total_stake</a>();
            <b>let</b> power_a = validator_a.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>();
            <b>let</b> power_b = validator_b.<a href="../sui_system/voting_power.md#sui_system_voting_power">voting_power</a>();
            <b>if</b> (stake_a &gt; stake_b) {
                <b>assert</b>!(power_a &gt;= power_b, <a href="../sui_system/voting_power.md#sui_system_voting_power_ERelativePowerMismatch">ERelativePowerMismatch</a>);
            };
            <b>if</b> (stake_a &lt; stake_b) {
                <b>assert</b>!(power_a &lt;= power_b, <a href="../sui_system/voting_power.md#sui_system_voting_power_ERelativePowerMismatch">ERelativePowerMismatch</a>);
            };
            b = b + 1;
        };
        a = a + 1;
    }
}
</code></pre>



</details>

<a name="sui_system_voting_power_total_voting_power"></a>

## Function `total_voting_power`

Return the (constant) total voting power


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_total_voting_power">total_voting_power</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_total_voting_power">total_voting_power</a>(): u64 {
    <a href="../sui_system/voting_power.md#sui_system_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>
}
</code></pre>



</details>

<a name="sui_system_voting_power_quorum_threshold"></a>

## Function `quorum_threshold`

Return the (constant) quorum threshold


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_quorum_threshold">quorum_threshold</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui_system/voting_power.md#sui_system_voting_power_quorum_threshold">quorum_threshold</a>(): u64 {
    <a href="../sui_system/voting_power.md#sui_system_voting_power_QUORUM_THRESHOLD">QUORUM_THRESHOLD</a>
}
</code></pre>



</details>
