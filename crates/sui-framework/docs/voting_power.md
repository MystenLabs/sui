
<a name="0x2_voting_power"></a>

# Module `0x2::voting_power`



-  [Struct `VotingPowerInfo`](#0x2_voting_power_VotingPowerInfo)
-  [Function `set_voting_power`](#0x2_voting_power_set_voting_power)
-  [Function `init_voting_power_info`](#0x2_voting_power_init_voting_power_info)
-  [Function `bubble_sort`](#0x2_voting_power_bubble_sort)
-  [Function `adjust_voting_power`](#0x2_voting_power_adjust_voting_power)
-  [Function `update_voting_power`](#0x2_voting_power_update_voting_power)


<pre><code><b>use</b> <a href="">0x1::vector</a>;
<b>use</b> <a href="math.md#0x2_math">0x2::math</a>;
<b>use</b> <a href="validator.md#0x2_validator">0x2::validator</a>;
</code></pre>



<a name="0x2_voting_power_VotingPowerInfo"></a>

## Struct `VotingPowerInfo`



<pre><code><b>struct</b> <a href="voting_power.md#0x2_voting_power_VotingPowerInfo">VotingPowerInfo</a>
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
<code><a href="voting_power.md#0x2_voting_power">voting_power</a>: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_voting_power_set_voting_power"></a>

## Function `set_voting_power`

Set the voting power of all validators. The total stake of all validators is provided in <code>total_stake</code>.
threshold_pct is a percentage threshold of max voting power that we want to cap on. If threshold_pct is 10,
then we want to cap the voting power at 10%.


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x2_voting_power_set_voting_power">set_voting_power</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, total_stake: u64, threshold_pct: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x2_voting_power_set_voting_power">set_voting_power</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;, total_stake: u64, threshold_pct: u64) {
    // Make sure that it's actually feasible <b>to</b> cap their voting power at this percentage.
    <b>assert</b>!(<a href="_length">vector::length</a>(validators) * threshold_pct &gt;= 100, 0);
    // Plus 1 <b>to</b> make sure that we don't end up <b>with</b> an impossible task due <b>to</b> rounding the threshold down.
    <b>let</b> threshold = total_stake * threshold_pct / 100 + 1;
    <b>let</b> (info_list, remaining_power) = <a href="voting_power.md#0x2_voting_power_init_voting_power_info">init_voting_power_info</a>(validators, threshold);
    <a href="voting_power.md#0x2_voting_power_bubble_sort">bubble_sort</a>(&<b>mut</b> info_list);
    <a href="voting_power.md#0x2_voting_power_adjust_voting_power">adjust_voting_power</a>(&<b>mut</b> info_list, threshold, remaining_power);
    <a href="voting_power.md#0x2_voting_power_update_voting_power">update_voting_power</a>(validators, info_list);
}
</code></pre>



</details>

<a name="0x2_voting_power_init_voting_power_info"></a>

## Function `init_voting_power_info`

Create the initial voting power of each validator, set using their stake, but capped using threshold.
Anything beyond the threshold is added to the remaining_power, which is also returned.


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_init_voting_power_info">init_voting_power_info</a>(validators: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, threshold: u64): (<a href="">vector</a>&lt;<a href="voting_power.md#0x2_voting_power_VotingPowerInfo">voting_power::VotingPowerInfo</a>&gt;, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_init_voting_power_info">init_voting_power_info</a>(validators: &<a href="">vector</a>&lt;Validator&gt;, threshold: u64): (<a href="">vector</a>&lt;<a href="voting_power.md#0x2_voting_power_VotingPowerInfo">VotingPowerInfo</a>&gt;, u64) {
    <b>let</b> i = 0;
    <b>let</b> len = <a href="_length">vector::length</a>(validators);
    <b>let</b> remaining_power = 0;
    <b>let</b> result = <a href="">vector</a>[];
    <b>while</b> (i &lt; len) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow">vector::borrow</a>(validators, i);
        <b>let</b> <a href="stake.md#0x2_stake">stake</a> = <a href="validator.md#0x2_validator_total_stake">validator::total_stake</a>(<a href="validator.md#0x2_validator">validator</a>);
        <b>let</b> <a href="voting_power.md#0x2_voting_power">voting_power</a> = <a href="math.md#0x2_math_min">math::min</a>(<a href="stake.md#0x2_stake">stake</a>, threshold);
        <b>let</b> info = <a href="voting_power.md#0x2_voting_power_VotingPowerInfo">VotingPowerInfo</a> {
            validator_index: i,
            <a href="voting_power.md#0x2_voting_power">voting_power</a>,
        };
        <a href="_push_back">vector::push_back</a>(&<b>mut</b> result, info);
        remaining_power = remaining_power + <a href="stake.md#0x2_stake">stake</a> - <a href="voting_power.md#0x2_voting_power">voting_power</a>;
        i = i + 1;
    };
    (result, remaining_power)
}
</code></pre>



</details>

<a name="0x2_voting_power_bubble_sort"></a>

## Function `bubble_sort`

Sort the voting power info list, using the voting power, in descending order.


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_bubble_sort">bubble_sort</a>(info_list: &<b>mut</b> <a href="">vector</a>&lt;<a href="voting_power.md#0x2_voting_power_VotingPowerInfo">voting_power::VotingPowerInfo</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_bubble_sort">bubble_sort</a>(info_list: &<b>mut</b> <a href="">vector</a>&lt;<a href="voting_power.md#0x2_voting_power_VotingPowerInfo">VotingPowerInfo</a>&gt;) {
    <b>let</b> len = <a href="_length">vector::length</a>(info_list);
    <b>let</b> changed = <b>true</b>;
    <b>while</b> (changed) {
        changed = <b>false</b>;
        <b>let</b> i = 0;
        <b>while</b> (i + 1 &lt; len) {
            <b>if</b> (<a href="_borrow">vector::borrow</a>(info_list, i).<a href="voting_power.md#0x2_voting_power">voting_power</a> &lt; <a href="_borrow">vector::borrow</a>(info_list, i + 1).<a href="voting_power.md#0x2_voting_power">voting_power</a>) {
                changed = <b>true</b>;
                <a href="_swap">vector::swap</a>(info_list, i, i + 1);
            };
            i = i + 1;
        }
    }
}
</code></pre>



</details>

<a name="0x2_voting_power_adjust_voting_power"></a>

## Function `adjust_voting_power`

Distribute remaining_power to validators that are not capped at threshold.


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_adjust_voting_power">adjust_voting_power</a>(info_list: &<b>mut</b> <a href="">vector</a>&lt;<a href="voting_power.md#0x2_voting_power_VotingPowerInfo">voting_power::VotingPowerInfo</a>&gt;, threshold: u64, remaining_power: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_adjust_voting_power">adjust_voting_power</a>(info_list: &<b>mut</b> <a href="">vector</a>&lt;<a href="voting_power.md#0x2_voting_power_VotingPowerInfo">VotingPowerInfo</a>&gt;, threshold: u64, remaining_power: u64) {
    <b>let</b> i = 0;
    <b>let</b> len = <a href="_length">vector::length</a>(info_list);
    <b>while</b> (i &lt; len && remaining_power &gt; 0) {
        <b>let</b> v = <a href="_borrow_mut">vector::borrow_mut</a>(info_list, i);
        <b>let</b> planned = remaining_power / (len - i) + 1;
        <b>let</b> target = <a href="math.md#0x2_math_min">math::min</a>(threshold, v.<a href="voting_power.md#0x2_voting_power">voting_power</a> + planned);
        <b>let</b> actural = <a href="math.md#0x2_math_min">math::min</a>(remaining_power, target - v.<a href="voting_power.md#0x2_voting_power">voting_power</a>);
        v.<a href="voting_power.md#0x2_voting_power">voting_power</a> = v.<a href="voting_power.md#0x2_voting_power">voting_power</a> + actural;
        remaining_power = remaining_power - actural;
        <b>assert</b>!(v.<a href="voting_power.md#0x2_voting_power">voting_power</a> &lt;= threshold, 0);
        i = i + 1;
    };
    <b>assert</b>!(remaining_power == 0, 0);
}
</code></pre>



</details>

<a name="0x2_voting_power_update_voting_power"></a>

## Function `update_voting_power`

Update validators with the decided voting power.


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_update_voting_power">update_voting_power</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, info_list: <a href="">vector</a>&lt;<a href="voting_power.md#0x2_voting_power_VotingPowerInfo">voting_power::VotingPowerInfo</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_update_voting_power">update_voting_power</a>(validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;, info_list: <a href="">vector</a>&lt;<a href="voting_power.md#0x2_voting_power_VotingPowerInfo">VotingPowerInfo</a>&gt;) {
    <b>while</b> (!<a href="_is_empty">vector::is_empty</a>(&info_list)) {
        <b>let</b> <a href="voting_power.md#0x2_voting_power_VotingPowerInfo">VotingPowerInfo</a> {
            validator_index,
            <a href="voting_power.md#0x2_voting_power">voting_power</a>,
        } = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> info_list);
        <b>let</b> v = <a href="_borrow_mut">vector::borrow_mut</a>(validators, validator_index);
        <a href="validator.md#0x2_validator_set_voting_power">validator::set_voting_power</a>(v, <a href="voting_power.md#0x2_voting_power">voting_power</a>);
    };
    <a href="_destroy_empty">vector::destroy_empty</a>(info_list);
}
</code></pre>



</details>
