
<a name="0x2_royalty"></a>

# Module `0x2::royalty`

This module implements a set of basic primitives for Kiosk's
Transfer Policies to improve discoverability and serve as a
base for building on top.


-  [Resource `RoyaltyPolicy`](#0x2_royalty_RoyaltyPolicy)
-  [Resource `RoyaltyCollectorCap`](#0x2_royalty_RoyaltyCollectorCap)
-  [Struct `PolicyCreated`](#0x2_royalty_PolicyCreated)
-  [Struct `ZeroPolicyCreated`](#0x2_royalty_ZeroPolicyCreated)
-  [Constants](#@Constants_0)
-  [Function `pay`](#0x2_royalty_pay)
-  [Function `set_zero_policy`](#0x2_royalty_set_zero_policy)
-  [Function `new_royalty_policy`](#0x2_royalty_new_royalty_policy)
-  [Function `set_amount`](#0x2_royalty_set_amount)
-  [Function `set_owner`](#0x2_royalty_set_owner)
-  [Function `withdraw`](#0x2_royalty_withdraw)
-  [Function `destroy_and_withdraw`](#0x2_royalty_destroy_and_withdraw)
-  [Function `amount`](#0x2_royalty_amount)
-  [Function `balance`](#0x2_royalty_balance)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="kiosk.md#0x2_kiosk">0x2::kiosk</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="sui.md#0x2_sui">0x2::sui</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_royalty_RoyaltyPolicy"></a>

## Resource `RoyaltyPolicy`

A transfer policy for a single type <code>T</code> which collects a certain
fee from the <code><a href="kiosk.md#0x2_kiosk">kiosk</a></code> deals and stores them for the policy issuer.


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
<code>amount_bp: u16</code>
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
<dt>
<code>owner: <b>address</b></code>
</dt>
<dd>
 Store creator address for visibility and discoverability purposes
</dd>
</dl>


</details>

<a name="0x2_royalty_RoyaltyCollectorCap"></a>

## Resource `RoyaltyCollectorCap`

A Capability that grants the bearer the ability to change the amount of
royalties collected as well as to withdraw from the <code>policy.<a href="balance.md#0x2_balance">balance</a></code>.


<pre><code><b>struct</b> <a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">RoyaltyCollectorCap</a>&lt;T: store, key&gt; <b>has</b> store, key
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
<code>policy_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>
 Purely cosmetic and discovery field.
 There should be only one Policy for the type <code>T</code> (although it
 is not enforced anywhere by default).
</dd>
</dl>


</details>

<a name="0x2_royalty_PolicyCreated"></a>

## Struct `PolicyCreated`

Event: fired when a new policy has been created for the type <code>T</code>. Meaning
that in most of the cases where a <code><a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a></code> is a shared object, it
can be used in the <code>sui::royalty::pay</code> function.


<pre><code><b>struct</b> <a href="royalty.md#0x2_royalty_PolicyCreated">PolicyCreated</a>&lt;T: store, key&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_royalty_ZeroPolicyCreated"></a>

## Struct `ZeroPolicyCreated`

Event: fired when a free-for-all policy was issued for <code>T</code>. Since the object
is immutably shared, everyone in the ecosystem can discover and use this
object to allow <code>TransferRequest&lt;T&gt;</code>.


<pre><code><b>struct</b> <a href="royalty.md#0x2_royalty_ZeroPolicyCreated">ZeroPolicyCreated</a>&lt;T: store, key&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_royalty_EIncorrectAmount"></a>

For when trying to create a new RoyaltyPolicy with more than 100%.
Or when trying to withdraw more than stored <code><a href="balance.md#0x2_balance">balance</a></code>.


<pre><code><b>const</b> <a href="royalty.md#0x2_royalty_EIncorrectAmount">EIncorrectAmount</a>: u64 = 0;
</code></pre>



<a name="0x2_royalty_MAX_AMOUNT"></a>

Utility constant to calculate the percentage of price to require.


<pre><code><b>const</b> <a href="royalty.md#0x2_royalty_MAX_AMOUNT">MAX_AMOUNT</a>: u16 = 10000;
</code></pre>



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
    <b>let</b> amount = (((paid <b>as</b> u128) * (policy.amount_bp <b>as</b> u128) / (<a href="royalty.md#0x2_royalty_MAX_AMOUNT">MAX_AMOUNT</a> <b>as</b> u128)) <b>as</b> u64);

    <b>let</b> royalty_payment = <a href="balance.md#0x2_balance_split">balance::split</a>(<a href="coin.md#0x2_coin_balance_mut">coin::balance_mut</a>(<a href="coin.md#0x2_coin">coin</a>), amount);
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> policy.<a href="balance.md#0x2_balance">balance</a>, royalty_payment);
}
</code></pre>



</details>

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
    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="royalty.md#0x2_royalty_ZeroPolicyCreated">ZeroPolicyCreated</a>&lt;T&gt; { id: <a href="object.md#0x2_object_id">object::id</a>(&cap) });
    <a href="transfer.md#0x2_transfer_freeze_object">transfer::freeze_object</a>(cap)
}
</code></pre>



</details>

<a name="0x2_royalty_new_royalty_policy"></a>

## Function `new_royalty_policy`

Create new <code><a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a></code> for the <code>T</code> and require an <code>amount</code>
percentage of the trade amount for the transfer to be approved.


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_new_royalty_policy">new_royalty_policy</a>&lt;T: store, key&gt;(cap: <a href="kiosk.md#0x2_kiosk_TransferPolicyCap">kiosk::TransferPolicyCap</a>&lt;T&gt;, amount_bp: u16, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="royalty.md#0x2_royalty_RoyaltyPolicy">royalty::RoyaltyPolicy</a>&lt;T&gt;, <a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">royalty::RoyaltyCollectorCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_new_royalty_policy">new_royalty_policy</a>&lt;T: key + store&gt;(
    cap: TransferPolicyCap&lt;T&gt;,
    amount_bp: u16,
    ctx: &<b>mut</b> TxContext
): (<a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a>&lt;T&gt;, <a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">RoyaltyCollectorCap</a>&lt;T&gt;) {
    <b>assert</b>!(amount_bp &lt;= <a href="royalty.md#0x2_royalty_MAX_AMOUNT">MAX_AMOUNT</a> && amount_bp != 0, <a href="royalty.md#0x2_royalty_EIncorrectAmount">EIncorrectAmount</a>);

    <b>let</b> policy = <a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a> {
        cap, amount_bp,
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        owner: sender(ctx),
        <a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
    };
    <b>let</b> id = <a href="object.md#0x2_object_id">object::id</a>(&policy);
    <b>let</b> cap = <a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">RoyaltyCollectorCap</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        policy_id: id
    };

    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="royalty.md#0x2_royalty_PolicyCreated">PolicyCreated</a>&lt;T&gt; { id });

    (policy, cap)
}
</code></pre>



</details>

<a name="0x2_royalty_set_amount"></a>

## Function `set_amount`

Change the amount in the <code><a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_set_amount">set_amount</a>&lt;T: store, key&gt;(policy: &<b>mut</b> <a href="royalty.md#0x2_royalty_RoyaltyPolicy">royalty::RoyaltyPolicy</a>&lt;T&gt;, _cap: &<a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">royalty::RoyaltyCollectorCap</a>&lt;T&gt;, amount: u16)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_set_amount">set_amount</a>&lt;T: key + store&gt;(
    policy: &<b>mut</b> <a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a>&lt;T&gt;,
    _cap: &<a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">RoyaltyCollectorCap</a>&lt;T&gt;,
    amount: u16,
) {
    <b>assert</b>!(amount &gt; 0 && <a href="royalty.md#0x2_royalty_amount">amount</a> &lt;= <a href="royalty.md#0x2_royalty_MAX_AMOUNT">MAX_AMOUNT</a>, <a href="royalty.md#0x2_royalty_EIncorrectAmount">EIncorrectAmount</a>);
    policy.amount_bp = amount
}
</code></pre>



</details>

<a name="0x2_royalty_set_owner"></a>

## Function `set_owner`

Change the <code>owner</code> field to the tx sender.


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_set_owner">set_owner</a>&lt;T: store, key&gt;(policy: &<b>mut</b> <a href="royalty.md#0x2_royalty_RoyaltyPolicy">royalty::RoyaltyPolicy</a>&lt;T&gt;, _cap: &<a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">royalty::RoyaltyCollectorCap</a>&lt;T&gt;, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_set_owner">set_owner</a>&lt;T: key + store&gt;(
    policy: &<b>mut</b> <a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a>&lt;T&gt;,
    _cap: &<a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">RoyaltyCollectorCap</a>&lt;T&gt;,
    ctx: &TxContext
) {
    policy.owner = sender(ctx)
}
</code></pre>



</details>

<a name="0x2_royalty_withdraw"></a>

## Function `withdraw`

Withdraw <code>amount</code> of SUI from the <code>policy.<a href="balance.md#0x2_balance">balance</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_withdraw">withdraw</a>&lt;T: store, key&gt;(policy: &<b>mut</b> <a href="royalty.md#0x2_royalty_RoyaltyPolicy">royalty::RoyaltyPolicy</a>&lt;T&gt;, _cap: &<a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">royalty::RoyaltyCollectorCap</a>&lt;T&gt;, amount: <a href="_Option">option::Option</a>&lt;u64&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_withdraw">withdraw</a>&lt;T: key + store&gt;(
    policy: &<b>mut</b> <a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a>&lt;T&gt;,
    _cap: &<a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">RoyaltyCollectorCap</a>&lt;T&gt;,
    amount: Option&lt;u64&gt;,
    ctx: &<b>mut</b> TxContext
): Coin&lt;SUI&gt; {
    <b>let</b> available = <a href="balance.md#0x2_balance_value">balance::value</a>(&policy.<a href="balance.md#0x2_balance">balance</a>);
    <b>let</b> amount = <b>if</b> (<a href="_is_some">option::is_some</a>(&amount)) {
        <a href="_destroy_some">option::destroy_some</a>(amount)
    } <b>else</b> {
        available
    };

    <b>assert</b>!(<a href="royalty.md#0x2_royalty_amount">amount</a> &lt;= available, <a href="royalty.md#0x2_royalty_EIncorrectAmount">EIncorrectAmount</a>);
    <a href="coin.md#0x2_coin_take">coin::take</a>(&<b>mut</b> policy.<a href="balance.md#0x2_balance">balance</a>, amount, ctx)
}
</code></pre>



</details>

<a name="0x2_royalty_destroy_and_withdraw"></a>

## Function `destroy_and_withdraw`

Unpack a RoyaltyPolicy object if it is not shared (!!!) and
return the <code>TransferPolicyCap</code> and the remaining balance.


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_destroy_and_withdraw">destroy_and_withdraw</a>&lt;T: store, key&gt;(policy: <a href="royalty.md#0x2_royalty_RoyaltyPolicy">royalty::RoyaltyPolicy</a>&lt;T&gt;, cap: <a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">royalty::RoyaltyCollectorCap</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="kiosk.md#0x2_kiosk_TransferPolicyCap">kiosk::TransferPolicyCap</a>&lt;T&gt;, <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;<a href="sui.md#0x2_sui_SUI">sui::SUI</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_destroy_and_withdraw">destroy_and_withdraw</a>&lt;T: key + store&gt;(
    policy: <a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a>&lt;T&gt;,
    cap: <a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">RoyaltyCollectorCap</a>&lt;T&gt;,
    ctx: &<b>mut</b> TxContext
): (TransferPolicyCap&lt;T&gt;, Coin&lt;SUI&gt;) {
    <b>let</b> <a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a> { id, amount_bp: _, owner: _, cap: transfer_cap, <a href="balance.md#0x2_balance">balance</a> } = policy;
    <b>let</b> <a href="royalty.md#0x2_royalty_RoyaltyCollectorCap">RoyaltyCollectorCap</a> { id: cap_id, policy_id: _ } = cap;

    <a href="object.md#0x2_object_delete">object::delete</a>(cap_id);
    <a href="object.md#0x2_object_delete">object::delete</a>(id);

    (transfer_cap, <a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(<a href="balance.md#0x2_balance">balance</a>, ctx))
}
</code></pre>



</details>

<a name="0x2_royalty_amount"></a>

## Function `amount`

Get the <code>amount</code> field.


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_amount">amount</a>&lt;T: store, key&gt;(self: &<a href="royalty.md#0x2_royalty_RoyaltyPolicy">royalty::RoyaltyPolicy</a>&lt;T&gt;): u16
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="royalty.md#0x2_royalty_amount">amount</a>&lt;T: key + store&gt;(self: &<a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a>&lt;T&gt;): u16 {
    self.amount_bp
}
</code></pre>



</details>

<a name="0x2_royalty_balance"></a>

## Function `balance`

Get the <code><a href="balance.md#0x2_balance">balance</a></code> field.


<pre><code><b>public</b> <b>fun</b> <a href="balance.md#0x2_balance">balance</a>&lt;T: store, key&gt;(self: &<a href="royalty.md#0x2_royalty_RoyaltyPolicy">royalty::RoyaltyPolicy</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="balance.md#0x2_balance">balance</a>&lt;T: key + store&gt;(self: &<a href="royalty.md#0x2_royalty_RoyaltyPolicy">RoyaltyPolicy</a>&lt;T&gt;): u64 {
    <a href="balance.md#0x2_balance_value">balance::value</a>(&self.<a href="balance.md#0x2_balance">balance</a>)
}
</code></pre>



</details>
