
<a name="0x2_voting_power"></a>

# Module `0x2::voting_power`



-  [Constants](#@Constants_0)
-  [Function `update`](#0x2_voting_power_update)
-  [Function `bubble_sort_by_stake`](#0x2_voting_power_bubble_sort_by_stake)
-  [Function `total_stake`](#0x2_voting_power_total_stake)
-  [Function `total_voting_power`](#0x2_voting_power_total_voting_power)
-  [Function `check_intermediate_invariants`](#0x2_voting_power_check_intermediate_invariants)
-  [Function `check_post_invariants`](#0x2_voting_power_check_post_invariants)
-  [Function `check_sorted`](#0x2_voting_power_check_sorted)


<pre><code><b>use</b> <a href="validator.md#0x2_validator">0x2::validator</a>;
</code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x2_voting_power_EInternalInvariantViolation"></a>

We should never observe this, modulo bugs


<pre><code><b>const</b> <a href="voting_power.md#0x2_voting_power_EInternalInvariantViolation">EInternalInvariantViolation</a>: u64 = 0;
</code></pre>



<a name="0x2_voting_power_MAX_VOTING_POWER"></a>

Cap voting power of an individual validator at 10%.


<pre><code><b>const</b> <a href="voting_power.md#0x2_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>: u64 = 1000;
</code></pre>



<a name="0x2_voting_power_QUORUM_THRESHOLD"></a>

Quorum threshold for our fixed voting power--any message signed by this much voting power can be trusted
up to BFT assumotions


<pre><code><b>const</b> <a href="voting_power.md#0x2_voting_power_QUORUM_THRESHOLD">QUORUM_THRESHOLD</a>: u64 = 6667;
</code></pre>



<a name="0x2_voting_power_TOTAL_VOTING_POWER"></a>

Set total_voting_power as 10_000 by convention. Individual voting powers can be interpreted
as easily understandable basis points (e.g., voting_power: 100 = 1%, voting_power: 1 = 0.01%) rather than
opaque quantities whose meaning changes from epoch to epoch as the total amount staked shifts.
Fixing the total voting power allows clients to hardcode the quorum threshold and total_voting power rather
than recomputing these.


<pre><code><b>const</b> <a href="voting_power.md#0x2_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>: u64 = 10000;
</code></pre>



<a name="0x2_voting_power_update"></a>

## Function `update`

Convert each validator's stake to a voting power normalized w.r.t <code><a href="voting_power.md#0x2_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a></code>,
and update <code>active_validators</code> accordingly, and attempt cap each validator's voting power at <code><a href="voting_power.md#0x2_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a></code>.
Capping is handled by redistributing the voting power "taken away from" capped validators *proportionally* among validators with voting
power less than the max.
Similarly, "leftover" voting power due to rounding error is distributed *equally* among validators with voting power less than the max (if possible),
and among all validators if everyone is already at the max.
This function ensures the following invariants:
1. Total voting power of all validators sums to <code><a href="voting_power.md#0x2_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a></code>
2. <code>active_validators</code> is sorted by voting power in descending order
3. <code>active_validators</code> is sorted by stake in descending order
This function attempts to maintain the following invariants whenever possible:
4. Each validator's voting power is <= <code><a href="voting_power.md#0x2_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a></code>
5. If validator A and B have the same stake, they will have the same voting power
Invariant (4) and (5) should almost always hold for Sui in practice due to high validator count, and stakes that aren't exactly
equal, but in theory/in tests these can be violated due to:
- a staking distribution like [1, 1, 1, 1] (will violate (4))
- a staking distribution where at least one validator has <code><a href="voting_power.md#0x2_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a></code>, and there is a remainder after redistributing the
leftovers due to odd numbers. In this case, a single validator will have 1 more than its proportionsal share


<pre><code><b>public</b> <b>fun</b> <b>update</b>(active_validators: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <b>update</b>(active_validators: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;): u64 {
    // sort validators by <a href="stake.md#0x2_stake">stake</a>, in descending order
    <a href="voting_power.md#0x2_voting_power_bubble_sort_by_stake">bubble_sort_by_stake</a>(active_validators);

    <b>let</b> total_stake = <a href="voting_power.md#0x2_voting_power_total_stake">total_stake</a>(active_validators);
    <b>let</b> voting_power_remaining = <a href="voting_power.md#0x2_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>;
    <b>let</b> last_voting_power_remaining = voting_power_remaining;
    <b>let</b> first_non_max_idx = 0;
    <b>let</b> num_validators = <a href="_length">vector::length</a>(active_validators);

    // zero out voting power
    <b>let</b> i = 0;
    <b>while</b> (i &lt; num_validators) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(active_validators, i);
        <a href="validator.md#0x2_validator_set_voting_power">validator::set_voting_power</a>(<a href="validator.md#0x2_validator">validator</a>, 0);
        i = i + 1;
    };

    <b>loop</b> {
        <b>let</b> i = first_non_max_idx;
        // distribute voting power proportional <b>to</b> <a href="stake.md#0x2_stake">stake</a>, but capping at <a href="voting_power.md#0x2_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>
        <b>while</b> (i &lt; num_validators) {
            <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(active_validators, i);
            <b>let</b> validator_stake = <a href="validator.md#0x2_validator_total_stake">validator::total_stake</a>(<a href="validator.md#0x2_validator">validator</a>);
            <b>let</b> prev_voting_power = <a href="validator.md#0x2_validator_voting_power">validator::voting_power</a>(<a href="validator.md#0x2_validator">validator</a>);
            <b>let</b> voting_power_share = (last_voting_power_remaining * validator_stake) / total_stake;
            <b>let</b> new_voting_power = prev_voting_power + voting_power_share;
            <b>let</b> voting_power_distributed = <b>if</b> (new_voting_power &gt;= <a href="voting_power.md#0x2_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>) {
                <a href="validator.md#0x2_validator_set_voting_power">validator::set_voting_power</a>(<a href="validator.md#0x2_validator">validator</a>, <a href="voting_power.md#0x2_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>);
                first_non_max_idx = i + 1;
                voting_power_share - (new_voting_power - <a href="voting_power.md#0x2_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>)
            } <b>else</b> {
                <a href="validator.md#0x2_validator_set_voting_power">validator::set_voting_power</a>(<a href="validator.md#0x2_validator">validator</a>, new_voting_power);
                voting_power_share
            };
            voting_power_remaining = voting_power_remaining - voting_power_distributed;
            i = i + 1
        };
        <a href="voting_power.md#0x2_voting_power_check_intermediate_invariants">check_intermediate_invariants</a>(active_validators, voting_power_remaining, last_voting_power_remaining, first_non_max_idx);
        <b>if</b> (voting_power_remaining == last_voting_power_remaining) { <b>break</b> };
        last_voting_power_remaining = voting_power_remaining
    };

    <b>if</b> (voting_power_remaining == 0) { <b>return</b> <a href="voting_power.md#0x2_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a> };
    // there is a remainder of voting power <b>to</b> be distributed. this can happen for two reasons:
    <b>let</b> i = <b>if</b> (first_non_max_idx == num_validators) {
        // reason 1: all validators have max voting power
        0
    } <b>else</b> {
        // reason 2: there is some leftover rounding error
        <b>assert</b>!(voting_power_remaining &lt; (num_validators - first_non_max_idx), <a href="voting_power.md#0x2_voting_power_EInternalInvariantViolation">EInternalInvariantViolation</a>);
        first_non_max_idx
    };
    <b>let</b> voting_power_share = voting_power_remaining / (num_validators - i);
    <b>let</b> remainder = voting_power_remaining % (num_validators - i);
    <b>while</b> (i &lt; num_validators) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(active_validators, i);
        <b>let</b> prev_voting_power = <a href="validator.md#0x2_validator_voting_power">validator::voting_power</a>(<a href="validator.md#0x2_validator">validator</a>);
        // this may be over the max, but we're ok <b>with</b> that. there's nowhere <b>else</b> <b>to</b>
        // put the excess voting power
        <b>let</b> new_voting_power = prev_voting_power + voting_power_share;
        <a href="validator.md#0x2_validator_set_voting_power">validator::set_voting_power</a>(<a href="validator.md#0x2_validator">validator</a>, new_voting_power);
        <b>if</b> (new_voting_power &gt;= <a href="voting_power.md#0x2_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>) {
            first_non_max_idx = i + 1
        };
        i = i + 1
    };
    // <b>if</b> there's a remainder due <b>to</b> odd numbers, give it <b>to</b> the first non-max <a href="validator.md#0x2_validator">validator</a>, or (<b>if</b> all validators are at max)
    // the first <a href="validator.md#0x2_validator">validator</a>. this preserves the sorting <b>invariant</b>.
    <b>if</b> (remainder != 0) {
        <b>assert</b>!(remainder == 1, <a href="voting_power.md#0x2_voting_power_EInternalInvariantViolation">EInternalInvariantViolation</a>);
        <b>let</b> remainder_recipient_idx = <b>if</b> (first_non_max_idx == num_validators) {
            // all validators have max voting power
            0
        } <b>else</b> {
            first_non_max_idx
        };
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow_mut">vector::borrow_mut</a>(active_validators, remainder_recipient_idx);
        <b>let</b> prev_voting_power = <a href="validator.md#0x2_validator_voting_power">validator::voting_power</a>(<a href="validator.md#0x2_validator">validator</a>);
        <a href="validator.md#0x2_validator_set_voting_power">validator::set_voting_power</a>(<a href="validator.md#0x2_validator">validator</a>, prev_voting_power + remainder)
    };
    <a href="voting_power.md#0x2_voting_power_check_post_invariants">check_post_invariants</a>(active_validators);
    <a href="voting_power.md#0x2_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>
}
</code></pre>



</details>

<a name="0x2_voting_power_bubble_sort_by_stake"></a>

## Function `bubble_sort_by_stake`

Sort <code>v</code> in descending order by stake.


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_bubble_sort_by_stake">bubble_sort_by_stake</a>(v: &<b>mut</b> <a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_bubble_sort_by_stake">bubble_sort_by_stake</a>(v: &<b>mut</b> <a href="">vector</a>&lt;Validator&gt;) {
    <b>let</b> num_validators = <a href="_length">vector::length</a>(v);
    <b>let</b> max_stake = 18_446_744_073_709_551_615;
    <b>loop</b> {
        <b>let</b> i = 0;
        <b>let</b> last_stake = max_stake;
        <b>let</b> changed = <b>false</b>;
        <b>while</b> (i &lt; num_validators) {
            <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow">vector::borrow</a>(v, i);
            <b>let</b> validator_stake = <a href="validator.md#0x2_validator_total_stake">validator::total_stake</a>(<a href="validator.md#0x2_validator">validator</a>);
            <b>if</b> (last_stake &lt; validator_stake) {
                <a href="_swap">vector::swap</a>(v, i - 1, i);
                changed = <b>true</b>
            };
            last_stake = validator_stake;
            i = i + 1
        };
        <b>if</b> (!changed) {
            <b>return</b>
        }
    }
}
</code></pre>



</details>

<a name="0x2_voting_power_total_stake"></a>

## Function `total_stake`

Return the total stake of all validators in <code>v</code>


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x2_voting_power_total_stake">total_stake</a>(v: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x2_voting_power_total_stake">total_stake</a>(v: &<a href="">vector</a>&lt;Validator&gt;): u64 {
    <b>let</b> i = 0;
    <b>let</b> len = <a href="_length">vector::length</a>(v);
    <b>let</b> total_stake = 0;
    <b>while</b> (i &lt; len) {
        total_stake = total_stake + <a href="validator.md#0x2_validator_total_stake">validator::total_stake</a>(<a href="_borrow">vector::borrow</a>(v, i));
        i = i + 1
    };
    total_stake
}
</code></pre>



</details>

<a name="0x2_voting_power_total_voting_power"></a>

## Function `total_voting_power`

Return the total voting power of all validators in <code>v</code>


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x2_voting_power_total_voting_power">total_voting_power</a>(v: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="voting_power.md#0x2_voting_power_total_voting_power">total_voting_power</a>(v: &<a href="">vector</a>&lt;Validator&gt;): u64 {
    <b>let</b> i = 0;
    <b>let</b> len = <a href="_length">vector::length</a>(v);
    <b>let</b> total_voting_power = 0;
    <b>while</b> (i &lt; len) {
        total_voting_power = total_voting_power + <a href="validator.md#0x2_validator_voting_power">validator::voting_power</a>(<a href="_borrow">vector::borrow</a>(v, i));
        i = i + 1
    };
    total_voting_power
}
</code></pre>



</details>

<a name="0x2_voting_power_check_intermediate_invariants"></a>

## Function `check_intermediate_invariants`

Check invariants that should hold on each each iteration of the proportional distribution loop


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_check_intermediate_invariants">check_intermediate_invariants</a>(v: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;, voting_power_remaining: u64, last_voting_power_remaining: u64, first_non_max_idx: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_check_intermediate_invariants">check_intermediate_invariants</a>(
    v: &<a href="">vector</a>&lt;Validator&gt;,
    voting_power_remaining: u64,
    last_voting_power_remaining: u64,
    first_non_max_idx: u64
) {
    // ensure we've conserved voting power
    <b>assert</b>!(<a href="voting_power.md#0x2_voting_power_total_voting_power">total_voting_power</a>(v) + voting_power_remaining == <a href="voting_power.md#0x2_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>, <a href="voting_power.md#0x2_voting_power_EInternalInvariantViolation">EInternalInvariantViolation</a>);
    // ensure we're converging
    <b>assert</b>!(voting_power_remaining &lt;= last_voting_power_remaining, <a href="voting_power.md#0x2_voting_power_EInternalInvariantViolation">EInternalInvariantViolation</a>);
    // check that everything &lt; first_non_max_idx <b>has</b> max voting power,
    // everything &gt;= first_non_max_idx does not have max voting power.
    <b>let</b> i = 0;
    <b>let</b> num_validators = <a href="_length">vector::length</a>(v);
    <b>while</b> (i &lt; num_validators) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow">vector::borrow</a>(v, i);
        <b>let</b> <a href="voting_power.md#0x2_voting_power">voting_power</a> = <a href="validator.md#0x2_validator_voting_power">validator::voting_power</a>(<a href="validator.md#0x2_validator">validator</a>);
        <b>if</b> (i &lt; first_non_max_idx) {
            <b>assert</b>!(<a href="voting_power.md#0x2_voting_power">voting_power</a> &gt;= <a href="voting_power.md#0x2_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>, <a href="voting_power.md#0x2_voting_power_EInternalInvariantViolation">EInternalInvariantViolation</a>);
        } <b>else</b> {
            <b>assert</b>!(<a href="voting_power.md#0x2_voting_power">voting_power</a> &lt; <a href="voting_power.md#0x2_voting_power_MAX_VOTING_POWER">MAX_VOTING_POWER</a>, <a href="voting_power.md#0x2_voting_power_EInternalInvariantViolation">EInternalInvariantViolation</a>);
            // TODO: possible <b>to</b> check that voting power is proportional <b>to</b> <a href="stake.md#0x2_stake">stake</a>?
        };
        i = i + 1
    }
}
</code></pre>



</details>

<a name="0x2_voting_power_check_post_invariants"></a>

## Function `check_post_invariants`

check invariants that should hold after voting power assignment is complete


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_check_post_invariants">check_post_invariants</a>(v: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_check_post_invariants">check_post_invariants</a>(v: &<a href="">vector</a>&lt;Validator&gt;) {
    // 1. Total voting power of all validators sums <b>to</b> `<a href="voting_power.md#0x2_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>`
    <b>assert</b>!(<a href="voting_power.md#0x2_voting_power_total_voting_power">total_voting_power</a>(v) == <a href="voting_power.md#0x2_voting_power_TOTAL_VOTING_POWER">TOTAL_VOTING_POWER</a>, <a href="voting_power.md#0x2_voting_power_EInternalInvariantViolation">EInternalInvariantViolation</a>);
    // 2. `active_validators` is sorted by voting power in descending order
    // 3. `active_validators` is sorted by <a href="stake.md#0x2_stake">stake</a> in descending order
    <a href="voting_power.md#0x2_voting_power_check_sorted">check_sorted</a>(v);
}
</code></pre>



</details>

<a name="0x2_voting_power_check_sorted"></a>

## Function `check_sorted`

Check that <code>v</code> is in descending order by both voting power and stake


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_check_sorted">check_sorted</a>(v: &<a href="">vector</a>&lt;<a href="validator.md#0x2_validator_Validator">validator::Validator</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="voting_power.md#0x2_voting_power_check_sorted">check_sorted</a>(v: &<a href="">vector</a>&lt;Validator&gt;) {
    <b>let</b> num_validators = <a href="_length">vector::length</a>(v);
    <b>let</b> i = 0;
    <b>let</b> u64_max = 18_446_744_073_709_551_615;
    <b>let</b> last_stake = u64_max;
    <b>let</b> last_voting_power = u64_max;
    <b>while</b> (i &lt; num_validators) {
        <b>let</b> <a href="validator.md#0x2_validator">validator</a> = <a href="_borrow">vector::borrow</a>(v, i);
        <b>let</b> <a href="stake.md#0x2_stake">stake</a> = <a href="validator.md#0x2_validator_total_stake">validator::total_stake</a>(<a href="validator.md#0x2_validator">validator</a>);
        <b>let</b> <a href="voting_power.md#0x2_voting_power">voting_power</a> = <a href="validator.md#0x2_validator_voting_power">validator::voting_power</a>(<a href="validator.md#0x2_validator">validator</a>);
        <b>assert</b>!(last_stake &gt;= <a href="stake.md#0x2_stake">stake</a>, <a href="voting_power.md#0x2_voting_power_EInternalInvariantViolation">EInternalInvariantViolation</a>);
        <b>assert</b>!(last_voting_power &gt;= <a href="voting_power.md#0x2_voting_power">voting_power</a>, <a href="voting_power.md#0x2_voting_power_EInternalInvariantViolation">EInternalInvariantViolation</a>);
        last_stake = <a href="stake.md#0x2_stake">stake</a>;
        last_voting_power = <a href="voting_power.md#0x2_voting_power">voting_power</a>;
        i = i + 1
    }
}
</code></pre>



</details>
