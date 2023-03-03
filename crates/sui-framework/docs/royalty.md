
<a name="0x2_royalty"></a>

# Module `0x2::royalty`

This module implements a set of basic primitives for Kiosk's
Transfer Policies to improve discoverability and serve as a
base for building on top.


-  [Resource `RoyaltyPolicy`](#0x2_royalty_RoyaltyPolicy)
-  [Constants](#@Constants_0)
-  [Function `set_zero_policy`](#0x2_royalty_set_zero_policy)
-  [Function `new_royalty_policy`](#0x2_royalty_new_royalty_policy)
-  [Function `pay`](#0x2_royalty_pay)


<pre><code><b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="kiosk.md#0x2_kiosk">0x2::kiosk</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_royalty_RoyaltyPolicy"></a>

## Resource `RoyaltyPolicy`

A transfer policy for a single type <code>T</code> which collects a certain
fee from the <code><a href="kiosk.md#0x2_kiosk">kiosk</a></code> deals and stores them for policy issuer.


<pre><code><b>struct</b> <a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a>&lt;T: store, key&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>cap: <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">kiosk::TransferPolicyCap</a>&lt;T&gt;</code>
</dt>
<dd>
 The <code>TransferPolicyCap</code> for the <code>T</code> which is used to call
 the <code><a href="kiosk.md#0x2_kiosk_allow_transfer">kiosk::allow_transfer</a></code> and allow the trade.
</dd>
<dt>
<code>amount: u16</code>
</dt>
<dd>
 Percentage of the trade amount which is required for the
 transfer approval. Denominated in basis points.
 - 10_000 = 100%
 - 100 = 1%
 - 1 = 0.01%
</dd>
<dt>
<code><a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;</code>
</dt>
<dd>
 Accumulated balance - the owner of the Policy can withdraw
 at any time.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_royalty_EIncorrectAmount"></a>

For when trying to create a new RoyaltyPolicy with more than 100%.


<pre><code><b>const</b> <a href="royalty.md#0x2_royalty_EIncorrectAmount">EIncorrectAmount</a>: u64 = 0;
</code></pre>



<a name="0x2_royalty_MAX_AMOUNT"></a>

Utility constant to calculate the percentage of price to require.


<pre><code><b>const</b> <a href="royalty.md#0x2_royalty_MAX_AMOUNT">MAX_AMOUNT</a>: u16 = 10000;
</code></pre>



<a name="0x2_royalty_set_zero_policy"></a>

## Function `set_zero_policy`

A special function used to explicitly indicate that all of the
trades can be performed freely. To achieve that, the <code>TransferPolicyCap</code>
is immutably shared making it available for free use by anyone on the network.


<pre><code><b>public</b> entry <b>fun</b> <a href="royalty.md#0x2_royalty_set_zero_policy">set_zero_policy</a>&lt;T: store, key&gt;(cap: <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">kiosk::TransferPolicyCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_set_zero_policy">set_zero_policy</a>&lt;T: key + store&gt;(cap: TransferPolicyCap&lt;T&gt;) {
    // TODO: emit <a href="event.md#0x2_event">event</a>
    <a href="transfer.md#0x2_transfer_freeze_object">transfer::freeze_object</a>(cap)
}
</code></pre>



</details>

<a name="0x2_royalty_new_royalty_policy"></a>

## Function `new_royalty_policy`

Create new <code><a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a></code> for the <code>T</code> and require an <code>amount</code>
percentage of the trade amount for the transfer to be approved.


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_new_royalty_policy">new_royalty_policy</a>&lt;T: store, key&gt;(cap: <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">kiosk::TransferPolicyCap</a>&lt;T&gt;, amount: u16, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="royalty.md#0x2_royalty_RoyaltyPolicy">royalty::RoyaltyPolicy</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_new_royalty_policy">new_royalty_policy</a>&lt;T: key + store&gt;(
    cap: TransferPolicyCap&lt;T&gt;,
    amount: u16,
    ctx: &<b>mut</b> TxContext
): <a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a>&lt;T&gt; {
    <b>assert</b>!(amount &lt;= <a href="royalty.md#0x2_royalty_MAX_AMOUNT">MAX_AMOUNT</a>, <a href="royalty.md#0x2_royalty_EIncorrectAmount">EIncorrectAmount</a>);

    <b>let</b> id = <a href="object.md#0x2_object_new">object::new</a>(ctx);

    <a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a> {
        id,
        cap,
        amount,
        <a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_zero">balance::zero</a>()
    }
}
</code></pre>



</details>

<a name="0x2_royalty_pay"></a>

## Function `pay`

Perform a Royalty payment and unblock the transfer by consuming
the <code>TransferRequest</code> "hot potato". The <code>T</code> here type-locks the
RoyaltyPolicy and TransferRequest making it impossible to call this
function on a wrong type.


<pre><code><b>public</b> <b>fun</b> <a href="pay.md#0x2_pay">pay</a>&lt;T: store, key&gt;(policy: &<b>mut</b> <a href="royalty.md#0x2_royalty_RoyaltyPolicy">royalty::RoyaltyPolicy</a>&lt;T&gt;, transfer_request: <a href="kiosk.md#0x2_kiosk_TransferRequest">kiosk::TransferRequest</a>&lt;T&gt;, <a href="coin.md#0x2_coin">coin</a>: &<b>mut</b> <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="pay.md#0x2_pay">pay</a>&lt;T: key + store&gt;(
    policy: &<b>mut</b> <a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a>&lt;T&gt;,
    transfer_request: TransferRequest&lt;T&gt;,
    <a href="coin.md#0x2_coin">coin</a>: &<b>mut</b> Coin&lt;SUI&gt;
) {
    <b>let</b> (paid, _from) = <a href="kiosk.md#0x2_kiosk_allow_transfer">kiosk::allow_transfer</a>(&policy.cap, transfer_request);
    <b>let</b> amount = (((paid <b>as</b> u128) * (policy.amount <b>as</b> u128) / (<a href="royalty.md#0x2_royalty_MAX_AMOUNT">MAX_AMOUNT</a> <b>as</b> u128)) <b>as</b> u64);

    <b>let</b> royalty_payment = <a href="balance.md#0x2_balance_split">balance::split</a>(<a href="coin.md#0x2_coin_balance_mut">coin::balance_mut</a>(<a href="coin.md#0x2_coin">coin</a>), amount);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> policy.<a href="balance.md#0x2_balance">balance</a>, royalty_payment);
}
</code></pre>



</details>
