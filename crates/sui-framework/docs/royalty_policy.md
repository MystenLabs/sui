
<a name="0x2_royalty_policy"></a>

# Module `0x2::royalty_policy`

A <code>TransferPolicy</code> Rule which implements percentage-based royalty fee.


-  [Struct `Rule`](#0x2_royalty_policy_Rule)
-  [Struct `Config`](#0x2_royalty_policy_Config)
-  [Constants](#@Constants_0)
-  [Function `set`](#0x2_royalty_policy_set)
-  [Function `pay`](#0x2_royalty_policy_pay)


<pre><code><b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="transfer_policy.md#0x2_transfer_policy">0x2::transfer_policy</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_royalty_policy_Rule"></a>

## Struct `Rule`

The "Rule" witness to authorize the policy.


<pre><code><b>struct</b> <a href="royalty_policy.md#0x2_royalty_policy_Rule">Rule</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>dummy_field: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_royalty_policy_Config"></a>

## Struct `Config`

Configuration for the Rule.


<pre><code><b>struct</b> <a href="royalty_policy.md#0x2_royalty_policy_Config">Config</a> <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>amount_bp: u16</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_royalty_policy_EIncorrectArgument"></a>

The <code>amount_bp</code> passed is more than 100%.


<pre><code><b>const</b> <a href="royalty_policy.md#0x2_royalty_policy_EIncorrectArgument">EIncorrectArgument</a>: u64 = 0;
</code></pre>



<a name="0x2_royalty_policy_EInsufficientAmount"></a>

The <code>Coin</code> used for payment is not enough to cover the fee.


<pre><code><b>const</b> <a href="royalty_policy.md#0x2_royalty_policy_EInsufficientAmount">EInsufficientAmount</a>: u64 = 1;
</code></pre>



<a name="0x2_royalty_policy_MAX_BPS"></a>

Max value for the <code>amount_bp</code>.


<pre><code><b>const</b> <a href="royalty_policy.md#0x2_royalty_policy_MAX_BPS">MAX_BPS</a>: u16 = 10000;
</code></pre>



<a name="0x2_royalty_policy_set"></a>

## Function `set`

Creator action: Set the Royalty policy for the <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="royalty_policy.md#0x2_royalty_policy_set">set</a>&lt;T: store, key&gt;(policy: &<b>mut</b> <a href="transfer_policy.md#0x2_transfer_policy_TransferPolicy">transfer_policy::TransferPolicy</a>&lt;T&gt;, cap: &<a href="transfer_policy.md#0x2_transfer_policy_TransferPolicyCap">transfer_policy::TransferPolicyCap</a>&lt;T&gt;, amount_bp: u16)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="royalty_policy.md#0x2_royalty_policy_set">set</a>&lt;T: key + store&gt;(
    policy: &<b>mut</b> TransferPolicy&lt;T&gt;,
    cap: &TransferPolicyCap&lt;T&gt;,
    amount_bp: u16
) {
    <b>assert</b>!(amount_bp &lt; <a href="royalty_policy.md#0x2_royalty_policy_MAX_BPS">MAX_BPS</a>, <a href="royalty_policy.md#0x2_royalty_policy_EIncorrectArgument">EIncorrectArgument</a>);
    policy::add_rule(<a href="royalty_policy.md#0x2_royalty_policy_Rule">Rule</a> {}, policy, cap, <a href="royalty_policy.md#0x2_royalty_policy_Config">Config</a> { amount_bp })
}
</code></pre>



</details>

<a name="0x2_royalty_policy_pay"></a>

## Function `pay`

Buyer action: Pay the royalty fee for the transfer.


<pre><code><b>public</b> <b>fun</b> <a href="pay.md#0x2_pay">pay</a>&lt;T: store, key&gt;(policy: &<b>mut</b> <a href="transfer_policy.md#0x2_transfer_policy_TransferPolicy">transfer_policy::TransferPolicy</a>&lt;T&gt;, request: &<b>mut</b> <a href="transfer_policy.md#0x2_transfer_policy_TransferRequest">transfer_policy::TransferRequest</a>&lt;T&gt;, payment: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="pay.md#0x2_pay">pay</a>&lt;T: key + store&gt;(
    policy: &<b>mut</b> TransferPolicy&lt;T&gt;,
    request: &<b>mut</b> TransferRequest&lt;T&gt;,
    payment: &<b>mut</b> Coin&lt;SUI&gt;,
    ctx: &<b>mut</b> TxContext
) {
    <b>let</b> config: &<a href="royalty_policy.md#0x2_royalty_policy_Config">Config</a> = policy::get_rule(<a href="royalty_policy.md#0x2_royalty_policy_Rule">Rule</a> {}, policy);
    <b>let</b> paid = policy::paid(request);
    <b>let</b> amount = (((paid <b>as</b> u128) * (config.amount_bp <b>as</b> u128) / 10_000) <b>as</b> u64);

    <b>assert</b>!(<a href="coin.md#0x2_coin_value">coin::value</a>(payment) &gt;= amount, <a href="royalty_policy.md#0x2_royalty_policy_EInsufficientAmount">EInsufficientAmount</a>);

    <b>let</b> fee = <a href="coin.md#0x2_coin_split">coin::split</a>(payment, amount, ctx);
    policy::add_to_balance(<a href="royalty_policy.md#0x2_royalty_policy_Rule">Rule</a> {}, policy, fee);
    policy::add_receipt(<a href="royalty_policy.md#0x2_royalty_policy_Rule">Rule</a> {}, request)
}
</code></pre>



</details>
