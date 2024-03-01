
<a name="0x3_voting_power"></a>

# Module `0x3::voting_power`



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


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
<b>use</b> <a href="../../dependencies/sui-framework/math.md#0x2_math">0x2::math</a>;
<b>use</b> <a href="../../dependencies/sui-system/validator.md#0x3_validator">0x3::validator</a>;
</code></pre>



<a name="0x3_voting_power_VotingPowerInfo"></a>

## Struct `VotingPowerInfo`



<pre><code><b>struct</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfo">VotingPowerInfo</a> <b>has</b> drop
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
<code><a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a>: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x3_voting_power_VotingPowerInfoV2"></a>

## Struct `VotingPowerInfoV2`



<pre><code><b>struct</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a> <b>has</b> drop
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
<code><a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a>: u64</code>
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


<a name="0x3_voting_power_EInvalidVotingPower"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_EInvalidVotingPower">EInvalidVotingPower</a>: u64 = 4;
</code></pre>



<a name="0x3_voting_power_ERelativePowerMismatch"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_ERelativePowerMismatch">ERelativePowerMismatch</a>: u64 = 2;
</code></pre>



<a name="0x3_voting_power_ETotalPowerMismatch"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_ETotalPowerMismatch">ETotalPowerMismatch</a>: u64 = 1;
</code></pre>



<a name="0x3_voting_power_EVotingPowerOverThreshold"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_EVotingPowerOverThreshold">EVotingPowerOverThreshold</a>: u64 = 3;
</code></pre>



<a name="0x3_voting_power_MAX_VOTING_POWER"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>: u64 = 1000;
</code></pre>



<a name="0x3_voting_power_QUORUM_THRESHOLD"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_QUORUM_THRESHOLD">QUORUM_THRESHOLD</a>: u64 = 6667;
</code></pre>



<a name="0x3_voting_power_TOTAL_VOTING_POWER"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>: u64 = 10000;
</code></pre>



<a name="0x3_voting_power_set_voting_power"></a>

## Function `set_voting_power`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_set_voting_power">set_voting_power</a>(validators: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/validator.md#0x3_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_set_voting_power">set_voting_power</a>(validators: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;) {
    // If threshold_pct is too small, it's possible that even when all validators reach the threshold we still don't
    // have 100%. So we bound the threshold_pct <b>to</b> be always enough <b>to</b> find a solution.
    <b>let</b> threshold = <a href="../../dependencies/sui-framework/math.md#0x2_math_min">math::min</a>(
        <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>,
        <a href="../../dependencies/sui-framework/math.md#0x2_math_max">math::max</a>(<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>, divide_and_round_up(<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>, <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(validators))),
    );
    <b>let</b> (info_list, remaining_power) = <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_init_voting_power_info">init_voting_power_info</a>(validators, threshold);
    <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_adjust_voting_power">adjust_voting_power</a>(&<b>mut</b> info_list, threshold, remaining_power);
    <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_update_voting_power">update_voting_power</a>(validators, info_list);
    <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_check_invariants">check_invariants</a>(validators);
}
</code></pre>



</details>

<a name="0x3_voting_power_init_voting_power_info"></a>

## Function `init_voting_power_info`



<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_init_voting_power_info">init_voting_power_info</a>(validators: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, threshold: u64): (<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">voting_power::VotingPowerInfoV2</a>&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_init_voting_power_info">init_voting_power_info</a>(
    validators: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;,
    threshold: u64,
): (<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;, u64) {
    <b>let</b> total_stake = <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_total_stake">total_stake</a>(validators);
    <b>let</b> i = 0;
    <b>let</b> len = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(validators);
    <b>let</b> total_power = 0;
    <b>let</b> result = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>[];
    <b>while</b> (i &lt; len) {
        <b>let</b> <a href="../../dependencies/sui-system/validator.md#0x3_validator">validator</a> = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(validators, i);
        <b>let</b> stake = <a href="../../dependencies/sui-system/validator.md#0x3_validator_total_stake">validator::total_stake</a>(<a href="../../dependencies/sui-system/validator.md#0x3_validator">validator</a>);
        <b>let</b> adjusted_stake = (stake <b>as</b> u128) * (<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a> <b>as</b> u128) / (total_stake <b>as</b> u128);
        <b>let</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a> = <a href="../../dependencies/sui-framework/math.md#0x2_math_min">math::min</a>((adjusted_stake <b>as</b> u64), threshold);
        <b>let</b> info = <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a> {
            validator_index: i,
            <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a>,
            stake,
        };
        <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_insert">insert</a>(&<b>mut</b> result, info);
        total_power = total_power + <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a>;
        i = i + 1;
    };
    (result, <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a> - total_power)
}
</code></pre>



</details>

<a name="0x3_voting_power_total_stake"></a>

## Function `total_stake`



<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_total_stake">total_stake</a>(validators: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/validator.md#0x3_validator_Validator">validator::Validator</a>&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_total_stake">total_stake</a>(validators: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;): u64 {
    <b>let</b> i = 0;
    <b>let</b> len = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(validators);
    <b>let</b> total_stake =0 ;
    <b>while</b> (i &lt; len) {
        total_stake = total_stake + <a href="../../dependencies/sui-system/validator.md#0x3_validator_total_stake">validator::total_stake</a>(<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(validators, i));
        i = i + 1;
    };
    total_stake
}
</code></pre>



</details>

<a name="0x3_voting_power_insert"></a>

## Function `insert`



<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_insert">insert</a>(info_list: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">voting_power::VotingPowerInfoV2</a>&gt;, new_info: <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">voting_power::VotingPowerInfoV2</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_insert">insert</a>(info_list: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;, new_info: <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>) {
    <b>let</b> i = 0;
    <b>let</b> len = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(info_list);
    <b>while</b> (i &lt; len && <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(info_list, i).stake &gt; new_info.stake) {
        i = i + 1;
    };
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_insert">vector::insert</a>(info_list, new_info, i);
}
</code></pre>



</details>

<a name="0x3_voting_power_adjust_voting_power"></a>

## Function `adjust_voting_power`



<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_adjust_voting_power">adjust_voting_power</a>(info_list: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">voting_power::VotingPowerInfoV2</a>&gt;, threshold: u64, remaining_power: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_adjust_voting_power">adjust_voting_power</a>(info_list: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;, threshold: u64, remaining_power: u64) {
    <b>let</b> i = 0;
    <b>let</b> len = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(info_list);
    <b>while</b> (i &lt; len && remaining_power &gt; 0) {
        <b>let</b> v = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow_mut">vector::borrow_mut</a>(info_list, i);
        // planned is the amount of extra power we want <b>to</b> distribute <b>to</b> this <a href="../../dependencies/sui-system/validator.md#0x3_validator">validator</a>.
        <b>let</b> planned = divide_and_round_up(remaining_power, len - i);
        // target is the targeting power this <a href="../../dependencies/sui-system/validator.md#0x3_validator">validator</a> will reach, capped by threshold.
        <b>let</b> target = <a href="../../dependencies/sui-framework/math.md#0x2_math_min">math::min</a>(threshold, v.<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a> + planned);
        // actual is the actual amount of power we will be distributing <b>to</b> this <a href="../../dependencies/sui-system/validator.md#0x3_validator">validator</a>.
        <b>let</b> actual = <a href="../../dependencies/sui-framework/math.md#0x2_math_min">math::min</a>(remaining_power, target - v.<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a>);
        v.<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a> = v.<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a> + actual;
        <b>assert</b>!(v.<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a> &lt;= threshold, <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_EVotingPowerOverThreshold">EVotingPowerOverThreshold</a>);
        remaining_power = remaining_power - actual;
        i = i + 1;
    };
    <b>assert</b>!(remaining_power == 0, <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_ETotalPowerMismatch">ETotalPowerMismatch</a>);
}
</code></pre>



</details>

<a name="0x3_voting_power_update_voting_power"></a>

## Function `update_voting_power`



<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_update_voting_power">update_voting_power</a>(validators: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/validator.md#0x3_validator_Validator">validator::Validator</a>&gt;, info_list: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">voting_power::VotingPowerInfoV2</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_update_voting_power">update_voting_power</a>(validators: &<b>mut</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;, info_list: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a>&gt;) {
    <b>while</b> (!<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&info_list)) {
        <b>let</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_VotingPowerInfoV2">VotingPowerInfoV2</a> {
            validator_index,
            <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a>,
            stake: _,
        } = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> info_list);
        <b>let</b> v = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow_mut">vector::borrow_mut</a>(validators, validator_index);
        <a href="../../dependencies/sui-system/validator.md#0x3_validator_set_voting_power">validator::set_voting_power</a>(v, <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a>);
    };
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_destroy_empty">vector::destroy_empty</a>(info_list);
}
</code></pre>



</details>

<a name="0x3_voting_power_check_invariants"></a>

## Function `check_invariants`



<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_check_invariants">check_invariants</a>(v: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;<a href="../../dependencies/sui-system/validator.md#0x3_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_check_invariants">check_invariants</a>(v: &<a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Validator&gt;) {
    // First check that the total voting power must be <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>.
    <b>let</b> i = 0;
    <b>let</b> len = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(v);
    <b>let</b> total = 0;
    <b>while</b> (i &lt; len) {
        <b>let</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a> = <a href="../../dependencies/sui-system/validator.md#0x3_validator_voting_power">validator::voting_power</a>(<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(v, i));
        <b>assert</b>!(<a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a> &gt; 0, <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_EInvalidVotingPower">EInvalidVotingPower</a>);
        total = total + <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power">voting_power</a>;
        i = i + 1;
    };
    <b>assert</b>!(total == <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>, <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_ETotalPowerMismatch">ETotalPowerMismatch</a>);

    // Second check that <b>if</b> <a href="../../dependencies/sui-system/validator.md#0x3_validator">validator</a> A's stake is larger than B's stake, A's voting power must be no less
    // than B's voting power; similarly, <b>if</b> A's stake is less than B's stake, A's voting power must be no larger
    // than B's voting power.
    <b>let</b> a = 0;
    <b>while</b> (a &lt; len) {
        <b>let</b> b = a + 1;
        <b>while</b> (b &lt; len) {
            <b>let</b> validator_a = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(v, a);
            <b>let</b> validator_b = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(v, b);
            <b>let</b> stake_a = <a href="../../dependencies/sui-system/validator.md#0x3_validator_total_stake">validator::total_stake</a>(validator_a);
            <b>let</b> stake_b = <a href="../../dependencies/sui-system/validator.md#0x3_validator_total_stake">validator::total_stake</a>(validator_b);
            <b>let</b> power_a = <a href="../../dependencies/sui-system/validator.md#0x3_validator_voting_power">validator::voting_power</a>(validator_a);
            <b>let</b> power_b = <a href="../../dependencies/sui-system/validator.md#0x3_validator_voting_power">validator::voting_power</a>(validator_b);
            <b>if</b> (stake_a &gt; stake_b) {
                <b>assert</b>!(power_a &gt;= power_b, <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_ERelativePowerMismatch">ERelativePowerMismatch</a>);
            };
            <b>if</b> (stake_a &lt; stake_b) {
                <b>assert</b>!(power_a &lt;= power_b, <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_ERelativePowerMismatch">ERelativePowerMismatch</a>);
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



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_total_voting_power">total_voting_power</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_total_voting_power">total_voting_power</a>(): u64 {
    <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>
}
</code></pre>



</details>

<a name="0x3_voting_power_quorum_threshold"></a>

## Function `quorum_threshold`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_quorum_threshold">quorum_threshold</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_quorum_threshold">quorum_threshold</a>(): u64 {
    <a href="../../dependencies/sui-system/voting_power.md#0x3_voting_power_QUORUM_THRESHOLD">QUORUM_THRESHOLD</a>
}
</code></pre>



</details>
