---
title: Module `0x3::stake_subsidy`
---



-  [Struct `StakeSubsidy`](#0x3_stake_subsidy_StakeSubsidy)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x3_stake_subsidy_create)
-  [Function `advance_epoch`](#0x3_stake_subsidy_advance_epoch)
-  [Function `current_epoch_subsidy_amount`](#0x3_stake_subsidy_current_epoch_subsidy_amount)


<pre><code><b>use</b> <a href="../move-stdlib/u64.md#0x1_u64">0x1::u64</a>;
<b>use</b> <a href="../sui-framework/bag.md#0x2_bag">0x2::bag</a>;
<b>use</b> <a href="../sui-framework/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../sui-framework/sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x3_stake_subsidy_StakeSubsidy"></a>

## Struct `StakeSubsidy`



<pre><code><b>struct</b> <a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">StakeSubsidy</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui-framework/balance.md#0x2_balance">balance</a>: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>
 Balance of SUI set aside for stake subsidies that will be drawn down over time.
</dd>
<dt>
<code>distribution_counter: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Count of the number of times stake subsidies have been distributed.
</dd>
<dt>
<code>current_distribution_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 The amount of stake subsidy to be drawn down per distribution.
 This amount decays and decreases over time.
</dd>
<dt>
<code>stake_subsidy_period_length: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>
 Number of distributions to occur before the distribution amount decays.
</dd>
<dt>
<code>stake_subsidy_decrease_rate: u16</code>
</dt>
<dd>
 The rate at which the distribution amount decays at the end of each
 period. Expressed in basis points.
</dd>
<dt>
<code>extra_fields: <a href="../sui-framework/bag.md#0x2_bag_Bag">bag::Bag</a></code>
</dt>
<dd>
 Any extra fields that's not defined statically.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x3_stake_subsidy_BASIS_POINT_DENOMINATOR"></a>



<pre><code><b>const</b> <a href="stake_subsidy.md#0x3_stake_subsidy_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>: u128 = 10000;
</code></pre>



<a name="0x3_stake_subsidy_ESubsidyDecreaseRateTooLarge"></a>



<pre><code><b>const</b> <a href="stake_subsidy.md#0x3_stake_subsidy_ESubsidyDecreaseRateTooLarge">ESubsidyDecreaseRateTooLarge</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x3_stake_subsidy_create"></a>

## Function `create`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="stake_subsidy.md#0x3_stake_subsidy_create">create</a>(<a href="../sui-framework/balance.md#0x2_balance">balance</a>: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, initial_distribution_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, stake_subsidy_period_length: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, stake_subsidy_decrease_rate: u16, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">stake_subsidy::StakeSubsidy</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="stake_subsidy.md#0x3_stake_subsidy_create">create</a>(
    <a href="../sui-framework/balance.md#0x2_balance">balance</a>: Balance&lt;SUI&gt;,
    initial_distribution_amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    stake_subsidy_period_length: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    stake_subsidy_decrease_rate: u16,
    ctx: &<b>mut</b> TxContext,
): <a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">StakeSubsidy</a> {
    // Rate can't be higher than 100%.
    <b>assert</b>!(
        stake_subsidy_decrease_rate &lt;= <a href="stake_subsidy.md#0x3_stake_subsidy_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a> <b>as</b> u16,
        <a href="stake_subsidy.md#0x3_stake_subsidy_ESubsidyDecreaseRateTooLarge">ESubsidyDecreaseRateTooLarge</a>,
    );

    <a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">StakeSubsidy</a> {
        <a href="../sui-framework/balance.md#0x2_balance">balance</a>,
        distribution_counter: 0,
        current_distribution_amount: initial_distribution_amount,
        stake_subsidy_period_length,
        stake_subsidy_decrease_rate,
        extra_fields: <a href="../sui-framework/bag.md#0x2_bag_new">bag::new</a>(ctx),
    }
}
</code></pre>



</details>

<a name="0x3_stake_subsidy_advance_epoch"></a>

## Function `advance_epoch`

Advance the epoch counter and draw down the subsidy for the epoch.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="stake_subsidy.md#0x3_stake_subsidy_advance_epoch">advance_epoch</a>(self: &<b>mut</b> <a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">stake_subsidy::StakeSubsidy</a>): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="../sui-framework/sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="stake_subsidy.md#0x3_stake_subsidy_advance_epoch">advance_epoch</a>(self: &<b>mut</b> <a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">StakeSubsidy</a>): Balance&lt;SUI&gt; {
    // Take the minimum of the reward amount and the remaining <a href="../sui-framework/balance.md#0x2_balance">balance</a> in
    // order <b>to</b> ensure we don't overdraft the remaining stake subsidy
    // <a href="../sui-framework/balance.md#0x2_balance">balance</a>
    <b>let</b> to_withdraw = self.current_distribution_amount.<b>min</b>(self.<a href="../sui-framework/balance.md#0x2_balance">balance</a>.value());

    // Drawn down the subsidy for this epoch.
    <b>let</b> <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a> = self.<a href="../sui-framework/balance.md#0x2_balance">balance</a>.split(to_withdraw);

    self.distribution_counter = self.distribution_counter + 1;

    // Decrease the subsidy amount only when the current period ends.
    <b>if</b> (self.distribution_counter % self.stake_subsidy_period_length == 0) {
        <b>let</b> decrease_amount = self.current_distribution_amount <b>as</b> u128
            * (self.stake_subsidy_decrease_rate <b>as</b> u128) / <a href="stake_subsidy.md#0x3_stake_subsidy_BASIS_POINT_DENOMINATOR">BASIS_POINT_DENOMINATOR</a>;
        self.current_distribution_amount = self.current_distribution_amount - (decrease_amount <b>as</b> <a href="../move-stdlib/u64.md#0x1_u64">u64</a>)
    };

    <a href="stake_subsidy.md#0x3_stake_subsidy">stake_subsidy</a>
}
</code></pre>



</details>

<a name="0x3_stake_subsidy_current_epoch_subsidy_amount"></a>

## Function `current_epoch_subsidy_amount`

Returns the amount of stake subsidy to be added at the end of the current epoch.


<pre><code><b>public</b> <b>fun</b> <a href="stake_subsidy.md#0x3_stake_subsidy_current_epoch_subsidy_amount">current_epoch_subsidy_amount</a>(self: &<a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">stake_subsidy::StakeSubsidy</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="stake_subsidy.md#0x3_stake_subsidy_current_epoch_subsidy_amount">current_epoch_subsidy_amount</a>(self: &<a href="stake_subsidy.md#0x3_stake_subsidy_StakeSubsidy">StakeSubsidy</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    self.current_distribution_amount.<b>min</b>(self.<a href="../sui-framework/balance.md#0x2_balance">balance</a>.value())
}
</code></pre>



</details>
