---
title: Module `0x3::voting_power`
---



-  [Struct `VotingPowerInfo`](#0x3_voting_power_VotingPowerInfo)
-  [Struct `VotingPowerInfoV2`](#0x3_voting_power_VotingPowerInfoV2)
-  [Constants](#@Constants_0)
-  [Function `set_voting_power`](#0x3_voting_power_set_voting_power)
-  [Function `init_voting_power_info`](#0x3_voting_power_init_voting_power_info)
-  [Function `total_stake`](#0x3_voting_power_total_stake)
-  [Function `insert`](#0x3_voting_power_insert)
-  [Function `adjust_voting_power`](#0x3_voting_power_adjust_voting_power)
-  [Function `update_voting_power`](#0x3_voting_power_update_voting_power)
-  [Function `check_invariants`](#0x3_voting_power_check_invariants)
-  [Function `total_voting_power`](#0x3_voting_power_total_voting_power)
-  [Function `quorum_threshold`](#0x3_voting_power_quorum_threshold)


<pre><code><b>use</b> <a href="../move-stdlib/u64.md#0x1_u64">0x1::u64</a>;
<b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="validator.md#0x3_validator">0x3::validator</a>;
</code></pre>



<a name="0x3_voting_power_VotingPowerInfo"></a>

## Struct `VotingPowerInfo`

Deprecated. Use VotingPowerInfoV2 instead.


<pre><code><b>struct</b> <a href="voting_power.md#0x3_voting_power_VotingPowerInfo">VotingPowerInfo</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>validator_index: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code><a href="voting_power.md#0x3_voting_power">voting_power</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x3_voting_power_VotingPowerInfoV2"></a>

## Struct `VotingPowerInfoV2`



<pre><code><b>struct</b> <a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>validator_index: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code><a href="voting_power.md#0x3_voting_power">voting_power</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>stake: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x3_voting_power_EInvalidVotingPower"></a>



<pre><code><b>const</b> <a href="voting_power.md#0x3_voting_power_EInvalidVotingPower">EInvalidVotingPower</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 4;
</code></pre>



<a name="0x3_voting_power_ERelativePowerMismatch"></a>



<pre><code><b>const</b> <a href="voting_power.md#0x3_voting_power_ERelativePowerMismatch">ERelativePowerMismatch</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0x3_voting_power_ETotalPowerMismatch"></a>



<pre><code><b>const</b> <a href="voting_power.md#0x3_voting_power_ETotalPowerMismatch">ETotalPowerMismatch</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x3_voting_power_EVotingPowerOverThreshold"></a>



<pre><code><b>const</b> <a href="voting_power.md#0x3_voting_power_EVotingPowerOverThreshold">EVotingPowerOverThreshold</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 3;
</code></pre>



<a name="0x3_voting_power_MAX_VOTING_POWER"></a>



<pre><code><b>const</b> <a href="voting_power.md#0x3_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1000;
</code></pre>



<a name="0x3_voting_power_QUORUM_THRESHOLD"></a>

Quorum threshold for our fixed voting power--any message signed by this much voting power can be trusted
up to BFT assumptions


<pre><code><b>const</b> <a href="voting_power.md#0x3_voting_power_QUORUM_THRESHOLD">QUORUM_THRESHOLD</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 6667;
</code></pre>



<a name="0x3_voting_power_TOTAL_VOTING_POWER"></a>

Set total_voting_power as 10_000 by convention. Individual voting powers can be interpreted
as easily understandable basis points (e.g., voting_power: 100 = 1%, voting_power: 1 = 0.01%) rather than
opaque quantities whose meaning changes from epoch to epoch as the total amount staked shifts.
Fixing the total voting power allows clients to hardcode the quorum threshold and total_voting power rather
than recomputing these.


<pre><code><b>const</b> <a href="voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 10000;
</code></pre>



<a name="0x3_voting_power_set_voting_power"></a>

## Function `set_voting_power`

Set the voting power of all validators.
Each validator's voting power is initialized using their stake. We then attempt to cap their voting power
at <code><a href="voting_power.md#0x3_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a></code>. If <code><a href="voting_power.md#0x3_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a></code> is not a feasible cap, we pick the lowest possible cap.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="voting_power.md#0x3_voting_power_set_voting_power">set_voting_power</a>(validators: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="voting_power.md#0x3_voting_power_set_voting_power">set_voting_power</a>(validators: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;) {
    // If threshold_pct is too small, it's possible that even when all validators reach the threshold we still don't
    // have 100%. So we bound the threshold_pct <b>to</b> be always enough <b>to</b> find a solution.
    <b>let</b> threshold = <a href="voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>.<b>min</b>(
        <a href="voting_power.md#0x3_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>.max(<a href="voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>.divide_and_round_up(validators.length())),
    );
    <b>let</b> (<b>mut</b> info_list, remaining_power) = <a href="voting_power.md#0x3_voting_power_init_voting_power_info">init_voting_power_info</a>(validators, threshold);
    <a href="voting_power.md#0x3_voting_power_adjust_voting_power">adjust_voting_power</a>(&<b>mut</b> info_list, threshold, remaining_power);
    <a href="voting_power.md#0x3_voting_power_update_voting_power">update_voting_power</a>(validators, info_list);
    <a href="voting_power.md#0x3_voting_power_check_invariants">check_invariants</a>(validators);
}
</code></pre>



</details>

<a name="0x3_voting_power_init_voting_power_info"></a>

## Function `init_voting_power_info`

Create the initial voting power of each validator, set using their stake, but capped using threshold.
We also perform insertion sort while creating the voting power list, by maintaining the list in
descending order using voting power.
Anything beyond the threshold is added to the remaining_power, which is also returned.


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_init_voting_power_info">init_voting_power_info</a>(validators: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): (<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">voting_power::VotingPowerInfoV2</a>&gt;, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_init_voting_power_info">init_voting_power_info</a>(
    validators: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;,
    threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
): (<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;, <a href="../move-stdlib/u64.md#0x1_u64">u64</a>) {
    <b>let</b> total_stake = <a href="voting_power.md#0x3_voting_power_total_stake">total_stake</a>(validators);
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = validators.length();
    <b>let</b> <b>mut</b> total_power = 0;
    <b>let</b> <b>mut</b> result = <a href="../move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>while</b> (i &lt; len) {
        <b>let</b> <a href="validator.md#0x3_validator">validator</a> = &validators[i];
        <b>let</b> stake = <a href="validator.md#0x3_validator">validator</a>.<a href="voting_power.md#0x3_voting_power_total_stake">total_stake</a>();
        <b>let</b> adjusted_stake = stake <b>as</b> u128 * (<a href="voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a> <b>as</b> u128) / (total_stake <b>as</b> u128);
        <b>let</b> <a href="voting_power.md#0x3_voting_power">voting_power</a> = (adjusted_stake <b>as</b> <a href="../move-stdlib/u64.md#0x1_u64">u64</a>).<b>min</b>(threshold);
        <b>let</b> info = <a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a> {
            validator_index: i,
            <a href="voting_power.md#0x3_voting_power">voting_power</a>,
            stake,
        };
        <a href="voting_power.md#0x3_voting_power_insert">insert</a>(&<b>mut</b> result, info);
        total_power = total_power + <a href="voting_power.md#0x3_voting_power">voting_power</a>;
        i = i + 1;
    };
    (result, <a href="voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a> - total_power)
}
</code></pre>



</details>

<a name="0x3_voting_power_total_stake"></a>

## Function `total_stake`

Sum up the total stake of all validators.


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_total_stake">total_stake</a>(validators: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_total_stake">total_stake</a>(validators: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = validators.length();
    <b>let</b> <b>mut</b> total_stake =0 ;
    <b>while</b> (i &lt; len) {
        total_stake = total_stake + validators[i].<a href="voting_power.md#0x3_voting_power_total_stake">total_stake</a>();
        i = i + 1;
    };
    total_stake
}
</code></pre>



</details>

<a name="0x3_voting_power_insert"></a>

## Function `insert`

Insert <code>new_info</code> to <code>info_list</code> as part of insertion sort, such that <code>info_list</code> is always sorted
using stake, in descending order.


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_insert">insert</a>(info_list: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">voting_power::VotingPowerInfoV2</a>&gt;, new_info: <a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">voting_power::VotingPowerInfoV2</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_insert">insert</a>(info_list: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;, new_info: <a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>) {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = info_list.length();
    <b>while</b> (i &lt; len && info_list[i].stake &gt; new_info.stake) {
        i = i + 1;
    };
    info_list.<a href="voting_power.md#0x3_voting_power_insert">insert</a>(new_info, i);
}
</code></pre>



</details>

<a name="0x3_voting_power_adjust_voting_power"></a>

## Function `adjust_voting_power`

Distribute remaining_power to validators that are not capped at threshold.


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_adjust_voting_power">adjust_voting_power</a>(info_list: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">voting_power::VotingPowerInfoV2</a>&gt;, threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, remaining_power: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_adjust_voting_power">adjust_voting_power</a>(info_list: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;, threshold: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, <b>mut</b> remaining_power: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>) {
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = info_list.length();
    <b>while</b> (i &lt; len && remaining_power &gt; 0) {
        <b>let</b> v = &<b>mut</b> info_list[i];
        // planned is the amount of extra power we want <b>to</b> distribute <b>to</b> this <a href="validator.md#0x3_validator">validator</a>.
        <b>let</b> planned = remaining_power.divide_and_round_up(len - i);
        // target is the targeting power this <a href="validator.md#0x3_validator">validator</a> will reach, capped by threshold.
        <b>let</b> target = threshold.<b>min</b>(v.<a href="voting_power.md#0x3_voting_power">voting_power</a> + planned);
        // actual is the actual amount of power we will be distributing <b>to</b> this <a href="validator.md#0x3_validator">validator</a>.
        <b>let</b> actual = remaining_power.<b>min</b>(target - v.<a href="voting_power.md#0x3_voting_power">voting_power</a>);
        v.<a href="voting_power.md#0x3_voting_power">voting_power</a> = v.<a href="voting_power.md#0x3_voting_power">voting_power</a> + actual;
        <b>assert</b>!(v.<a href="voting_power.md#0x3_voting_power">voting_power</a> &lt;= threshold, <a href="voting_power.md#0x3_voting_power_EVotingPowerOverThreshold">EVotingPowerOverThreshold</a>);
        remaining_power = remaining_power - actual;
        i = i + 1;
    };
    <b>assert</b>!(remaining_power == 0, <a href="voting_power.md#0x3_voting_power_ETotalPowerMismatch">ETotalPowerMismatch</a>);
}
</code></pre>



</details>

<a name="0x3_voting_power_update_voting_power"></a>

## Function `update_voting_power`

Update validators with the decided voting power.


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_update_voting_power">update_voting_power</a>(validators: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, info_list: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">voting_power::VotingPowerInfoV2</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_update_voting_power">update_voting_power</a>(validators: &<b>mut</b> <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;, <b>mut</b> info_list: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;) {
    <b>while</b> (!info_list.is_empty()) {
        <b>let</b> <a href="voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a> {
            validator_index,
            <a href="voting_power.md#0x3_voting_power">voting_power</a>,
            stake: _,
        } = info_list.pop_back();
        <b>let</b> v = &<b>mut</b> validators[validator_index];
        v.<a href="voting_power.md#0x3_voting_power_set_voting_power">set_voting_power</a>(<a href="voting_power.md#0x3_voting_power">voting_power</a>);
    };
    info_list.destroy_empty();
}
</code></pre>



</details>

<a name="0x3_voting_power_check_invariants"></a>

## Function `check_invariants`

Check a few invariants that must hold after setting the voting power.


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_check_invariants">check_invariants</a>(v: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="validator.md#0x3_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x3_voting_power_check_invariants">check_invariants</a>(v: &<a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;) {
    // First check that the total voting power must be <a href="voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>.
    <b>let</b> <b>mut</b> i = 0;
    <b>let</b> len = v.length();
    <b>let</b> <b>mut</b> total = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> <a href="voting_power.md#0x3_voting_power">voting_power</a> = v[i].<a href="voting_power.md#0x3_voting_power">voting_power</a>();
        <b>assert</b>!(<a href="voting_power.md#0x3_voting_power">voting_power</a> &gt; 0, <a href="voting_power.md#0x3_voting_power_EInvalidVotingPower">EInvalidVotingPower</a>);
        total = total + <a href="voting_power.md#0x3_voting_power">voting_power</a>;
        i = i + 1;
    };
    <b>assert</b>!(total == <a href="voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>, <a href="voting_power.md#0x3_voting_power_ETotalPowerMismatch">ETotalPowerMismatch</a>);

    // Second check that <b>if</b> <a href="validator.md#0x3_validator">validator</a> A's stake is larger than B's stake, A's voting power must be no less
    // than B's voting power; similarly, <b>if</b> A's stake is less than B's stake, A's voting power must be no larger
    // than B's voting power.
    <b>let</b> <b>mut</b> a = 0;
    <b>while</b> (a &lt; len) {
        <b>let</b> <b>mut</b> b = a + 1;
        <b>while</b> (b &lt; len) {
            <b>let</b> validator_a = &v[a];
            <b>let</b> validator_b = &v[b];
            <b>let</b> stake_a = validator_a.<a href="voting_power.md#0x3_voting_power_total_stake">total_stake</a>();
            <b>let</b> stake_b = validator_b.<a href="voting_power.md#0x3_voting_power_total_stake">total_stake</a>();
            <b>let</b> power_a = validator_a.<a href="voting_power.md#0x3_voting_power">voting_power</a>();
            <b>let</b> power_b = validator_b.<a href="voting_power.md#0x3_voting_power">voting_power</a>();
            <b>if</b> (stake_a &gt; stake_b) {
                <b>assert</b>!(power_a &gt;= power_b, <a href="voting_power.md#0x3_voting_power_ERelativePowerMismatch">ERelativePowerMismatch</a>);
            };
            <b>if</b> (stake_a &lt; stake_b) {
                <b>assert</b>!(power_a &lt;= power_b, <a href="voting_power.md#0x3_voting_power_ERelativePowerMismatch">ERelativePowerMismatch</a>);
            };
            b = b + 1;
        };
        a = a + 1;
    }
}
</code></pre>



</details>

<a name="0x3_voting_power_total_voting_power"></a>

## Function `total_voting_power`

Return the (constant) total voting power


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x3_voting_power_total_voting_power">total_voting_power</a>(): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x3_voting_power_total_voting_power">total_voting_power</a>(): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <a href="voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>
}
</code></pre>



</details>

<a name="0x3_voting_power_quorum_threshold"></a>

## Function `quorum_threshold`

Return the (constant) quorum threshold


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x3_voting_power_quorum_threshold">quorum_threshold</a>(): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x3_voting_power_quorum_threshold">quorum_threshold</a>(): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <a href="voting_power.md#0x3_voting_power_QUORUM_THRESHOLD">QUORUM_THRESHOLD</a>
}
</code></pre>



</details>
