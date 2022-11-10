
<a name="0x2_safe"></a>

# Module `0x2::safe`

The Safe standard is a minimalistic shared wrapper around a coin. It provides a way for users to provide third-party dApps with
the capability to transfer coins away from their wallets, if they are provided with the correct permission.


-  [Resource `Safe`](#0x2_safe_Safe)
-  [Resource `OwnerCapability`](#0x2_safe_OwnerCapability)
-  [Resource `TransferCapability`](#0x2_safe_TransferCapability)
-  [Constants](#@Constants_0)
-  [Function `check_capability_validity`](#0x2_safe_check_capability_validity)
-  [Function `check_owner_capability_validity`](#0x2_safe_check_owner_capability_validity)
-  [Function `create_capability_`](#0x2_safe_create_capability_)
-  [Function `balance`](#0x2_safe_balance)
-  [Function `create_`](#0x2_safe_create_)
-  [Function `create`](#0x2_safe_create)
-  [Function `create_empty`](#0x2_safe_create_empty)
-  [Function `deposit_`](#0x2_safe_deposit_)
-  [Function `deposit`](#0x2_safe_deposit)
-  [Function `withdraw_`](#0x2_safe_withdraw_)
-  [Function `withdraw`](#0x2_safe_withdraw)
-  [Function `debit`](#0x2_safe_debit)
-  [Function `revoke_transfer_capability`](#0x2_safe_revoke_transfer_capability)
-  [Function `self_revoke_transfer_capability`](#0x2_safe_self_revoke_transfer_capability)
-  [Function `create_transfer_capability`](#0x2_safe_create_transfer_capability)


<pre><code><b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="vec_set.md#0x2_vec_set">0x2::vec_set</a>;
</code></pre>



<a name="0x2_safe_Safe"></a>

## Resource `Safe`

Allows any holder of a capability to transfer a fixed amount of assets from the safe.
Useful in situations like an NFT marketplace where you wish to buy the NFTs at a specific price.

@ownership: Shared



<pre><code><b>struct</b> <a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt; <b>has</b> key
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
<code><a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>allowed_safes: <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_safe_OwnerCapability"></a>

## Resource `OwnerCapability`



<pre><code><b>struct</b> <a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a>&lt;T&gt; <b>has</b> store, key
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
<code>safe_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_safe_TransferCapability"></a>

## Resource `TransferCapability`


Allows the owner of the capability to take <code>amount</code> of coins from the box.

@ownership: Owned


<pre><code><b>struct</b> <a href="safe.md#0x2_safe_TransferCapability">TransferCapability</a>&lt;T&gt; <b>has</b> store, key
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
<code>safe_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
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

<a name="@Constants_0"></a>

## Constants


<a name="0x2_safe_INVALID_OWNER_CAPABILITY"></a>



<pre><code><b>const</b> <a href="safe.md#0x2_safe_INVALID_OWNER_CAPABILITY">INVALID_OWNER_CAPABILITY</a>: u64 = 1;
</code></pre>



<a name="0x2_safe_INVALID_TRANSFER_CAPABILITY"></a>



<pre><code><b>const</b> <a href="safe.md#0x2_safe_INVALID_TRANSFER_CAPABILITY">INVALID_TRANSFER_CAPABILITY</a>: u64 = 0;
</code></pre>



<a name="0x2_safe_MAX_CAPABILITY_ISSUABLE"></a>



<pre><code><b>const</b> <a href="safe.md#0x2_safe_MAX_CAPABILITY_ISSUABLE">MAX_CAPABILITY_ISSUABLE</a>: u64 = 1000;
</code></pre>



<a name="0x2_safe_OVERDRAWN"></a>



<pre><code><b>const</b> <a href="safe.md#0x2_safe_OVERDRAWN">OVERDRAWN</a>: u64 = 3;
</code></pre>



<a name="0x2_safe_TRANSFER_CAPABILITY_REVOKED"></a>



<pre><code><b>const</b> <a href="safe.md#0x2_safe_TRANSFER_CAPABILITY_REVOKED">TRANSFER_CAPABILITY_REVOKED</a>: u64 = 2;
</code></pre>



<a name="0x2_safe_check_capability_validity"></a>

## Function `check_capability_validity`

HELPER FUNCTIONS
Check that the capability has not yet been revoked by the owner.


<pre><code><b>fun</b> <a href="safe.md#0x2_safe_check_capability_validity">check_capability_validity</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_TransferCapability">safe::TransferCapability</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="safe.md#0x2_safe_check_capability_validity">check_capability_validity</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_TransferCapability">TransferCapability</a>&lt;T&gt;) {
    // Check that the ids match
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(<a href="safe.md#0x2_safe">safe</a>) == capability.safe_id, <a href="safe.md#0x2_safe_INVALID_TRANSFER_CAPABILITY">INVALID_TRANSFER_CAPABILITY</a>);
    // Check that it <b>has</b> not been cancelled
    <b>assert</b>!(<a href="vec_set.md#0x2_vec_set_contains">vec_set::contains</a>(&<a href="safe.md#0x2_safe">safe</a>.allowed_safes, &<a href="object.md#0x2_object_id">object::id</a>(capability)), <a href="safe.md#0x2_safe_TRANSFER_CAPABILITY_REVOKED">TRANSFER_CAPABILITY_REVOKED</a>);
}
</code></pre>



</details>

<a name="0x2_safe_check_owner_capability_validity"></a>

## Function `check_owner_capability_validity`



<pre><code><b>fun</b> <a href="safe.md#0x2_safe_check_owner_capability_validity">check_owner_capability_validity</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_OwnerCapability">safe::OwnerCapability</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="safe.md#0x2_safe_check_owner_capability_validity">check_owner_capability_validity</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a>&lt;T&gt;) {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(<a href="safe.md#0x2_safe">safe</a>) == capability.safe_id, <a href="safe.md#0x2_safe_INVALID_OWNER_CAPABILITY">INVALID_OWNER_CAPABILITY</a>);
}
</code></pre>



</details>

<a name="0x2_safe_create_capability_"></a>

## Function `create_capability_`

Helper function to create a capability.


<pre><code><b>fun</b> <a href="safe.md#0x2_safe_create_capability_">create_capability_</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;, withdraw_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="safe.md#0x2_safe_TransferCapability">safe::TransferCapability</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="safe.md#0x2_safe_create_capability_">create_capability_</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;, withdraw_amount: u64, ctx: &<b>mut</b> TxContext): <a href="safe.md#0x2_safe_TransferCapability">TransferCapability</a>&lt;T&gt; {
    <b>let</b> cap_id = <a href="object.md#0x2_object_new">object::new</a>(ctx);
    <a href="vec_set.md#0x2_vec_set_insert">vec_set::insert</a>(&<b>mut</b> <a href="safe.md#0x2_safe">safe</a>.allowed_safes, <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&cap_id));

    <b>let</b> capability = <a href="safe.md#0x2_safe_TransferCapability">TransferCapability</a> {
        id: cap_id,
        safe_id: <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&<a href="safe.md#0x2_safe">safe</a>.id),
        amount: withdraw_amount,
    };

    capability
}
</code></pre>



</details>

<a name="0x2_safe_balance"></a>

## Function `balance`

PUBLIC FUNCTIONS


<pre><code><b>public</b> <b>fun</b> <a href="balance.md#0x2_balance">balance</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;): &<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="balance.md#0x2_balance">balance</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;): &Balance&lt;T&gt; {
    &<a href="safe.md#0x2_safe">safe</a>.<a href="balance.md#0x2_balance">balance</a>
}
</code></pre>



</details>

<a name="0x2_safe_create_"></a>

## Function `create_`

Wrap a coin around a safe.
a trusted party (or smart contract) to transfer the object out.


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_create_">create_</a>&lt;T&gt;(<a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="safe.md#0x2_safe_OwnerCapability">safe::OwnerCapability</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_create_">create_</a>&lt;T&gt;(<a href="balance.md#0x2_balance">balance</a>: Balance&lt;T&gt;, ctx: &<b>mut</b> TxContext): <a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a>&lt;T&gt; {
    <b>let</b> <a href="safe.md#0x2_safe">safe</a> = <a href="safe.md#0x2_safe_Safe">Safe</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        <a href="balance.md#0x2_balance">balance</a>,
        allowed_safes: <a href="vec_set.md#0x2_vec_set_empty">vec_set::empty</a>(),
    };
    <b>let</b> cap = <a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        safe_id: <a href="object.md#0x2_object_id">object::id</a>(&<a href="safe.md#0x2_safe">safe</a>),
    };
    <a href="transfer.md#0x2_transfer_share_object">transfer::share_object</a>(<a href="safe.md#0x2_safe">safe</a>);
    cap
}
</code></pre>



</details>

<a name="0x2_safe_create"></a>

## Function `create`



<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_create">create</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_create">create</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: Coin&lt;T&gt;, ctx: &<b>mut</b> TxContext) {
    <b>let</b> <a href="balance.md#0x2_balance">balance</a> = <a href="coin.md#0x2_coin_into_balance">coin::into_balance</a>(<a href="coin.md#0x2_coin">coin</a>);
    <b>let</b> cap = <a href="safe.md#0x2_safe_create_">create_</a>&lt;T&gt;(<a href="balance.md#0x2_balance">balance</a>, ctx);
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(cap, sender(ctx));
}
</code></pre>



</details>

<a name="0x2_safe_create_empty"></a>

## Function `create_empty`



<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_create_empty">create_empty</a>&lt;T&gt;(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_create_empty">create_empty</a>&lt;T&gt;(ctx: &<b>mut</b> TxContext) {
    <b>let</b> empty_balance = <a href="balance.md#0x2_balance_zero">balance::zero</a>&lt;T&gt;();
    <b>let</b> cap = <a href="safe.md#0x2_safe_create_">create_</a>(empty_balance, ctx);
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(cap, sender(ctx));
}
</code></pre>



</details>

<a name="0x2_safe_deposit_"></a>

## Function `deposit_`

Deposit funds to the safe


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_deposit_">deposit_</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;, <a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_deposit_">deposit_</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;, <a href="balance.md#0x2_balance">balance</a>: Balance&lt;T&gt;) {
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> <a href="safe.md#0x2_safe">safe</a>.<a href="balance.md#0x2_balance">balance</a>, <a href="balance.md#0x2_balance">balance</a>);
}
</code></pre>



</details>

<a name="0x2_safe_deposit"></a>

## Function `deposit`

Deposit funds to the safe


<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_deposit">deposit</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;, <a href="coin.md#0x2_coin">coin</a>: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_deposit">deposit</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;, <a href="coin.md#0x2_coin">coin</a>: Coin&lt;T&gt;) {
    <b>let</b> <a href="balance.md#0x2_balance">balance</a> = <a href="coin.md#0x2_coin_into_balance">coin::into_balance</a>(<a href="coin.md#0x2_coin">coin</a>);
    <a href="safe.md#0x2_safe_deposit_">deposit_</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>, <a href="balance.md#0x2_balance">balance</a>);
}
</code></pre>



</details>

<a name="0x2_safe_withdraw_"></a>

## Function `withdraw_`

Withdraw coins from the safe as a <code><a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a></code> holder


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_withdraw_">withdraw_</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_OwnerCapability">safe::OwnerCapability</a>&lt;T&gt;, withdraw_amount: u64): <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_withdraw_">withdraw_</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a>&lt;T&gt;, withdraw_amount: u64): Balance&lt;T&gt; {
    // Ensures that only the owner can withdraw from the <a href="safe.md#0x2_safe">safe</a>.
    <a href="safe.md#0x2_safe_check_owner_capability_validity">check_owner_capability_validity</a>(<a href="safe.md#0x2_safe">safe</a>, capability);
    <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> <a href="safe.md#0x2_safe">safe</a>.<a href="balance.md#0x2_balance">balance</a>, withdraw_amount)
}
</code></pre>



</details>

<a name="0x2_safe_withdraw"></a>

## Function `withdraw`

Withdraw coins from the safe as a <code><a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a></code> holder


<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_withdraw">withdraw</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_OwnerCapability">safe::OwnerCapability</a>&lt;T&gt;, withdraw_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_withdraw">withdraw</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a>&lt;T&gt;, withdraw_amount: u64, ctx: &<b>mut</b> TxContext) {
    <b>let</b> <a href="balance.md#0x2_balance">balance</a> = <a href="safe.md#0x2_safe_withdraw_">withdraw_</a>(<a href="safe.md#0x2_safe">safe</a>, capability, withdraw_amount);
    <b>let</b> <a href="coin.md#0x2_coin">coin</a> = <a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(<a href="balance.md#0x2_balance">balance</a>, ctx);
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="coin.md#0x2_coin">coin</a>, sender(ctx));
}
</code></pre>



</details>

<a name="0x2_safe_debit"></a>

## Function `debit`

Withdraw coins from the safe as a <code><a href="safe.md#0x2_safe_TransferCapability">TransferCapability</a></code> holder.


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_debit">debit</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;, capability: &<b>mut</b> <a href="safe.md#0x2_safe_TransferCapability">safe::TransferCapability</a>&lt;T&gt;, withdraw_amount: u64): <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_debit">debit</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;, capability: &<b>mut</b> <a href="safe.md#0x2_safe_TransferCapability">TransferCapability</a>&lt;T&gt;, withdraw_amount: u64): Balance&lt;T&gt; {
    // Check the validity of the capability
    <a href="safe.md#0x2_safe_check_capability_validity">check_capability_validity</a>(<a href="safe.md#0x2_safe">safe</a>, capability);

    // Withdraw funds
    <b>assert</b>!(capability.amount &gt;= withdraw_amount, <a href="safe.md#0x2_safe_OVERDRAWN">OVERDRAWN</a>);
    capability.amount = capability.amount - withdraw_amount;
    <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> <a href="safe.md#0x2_safe">safe</a>.<a href="balance.md#0x2_balance">balance</a>, withdraw_amount)
}
</code></pre>



</details>

<a name="0x2_safe_revoke_transfer_capability"></a>

## Function `revoke_transfer_capability`

Revoke a <code><a href="safe.md#0x2_safe_TransferCapability">TransferCapability</a></code> as an <code><a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a></code> holder


<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_revoke_transfer_capability">revoke_transfer_capability</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_OwnerCapability">safe::OwnerCapability</a>&lt;T&gt;, capability_id: <a href="object.md#0x2_object_ID">object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_revoke_transfer_capability">revoke_transfer_capability</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a>&lt;T&gt;, capability_id: ID) {
    // Ensures that only the owner can withdraw from the <a href="safe.md#0x2_safe">safe</a>.
    <a href="safe.md#0x2_safe_check_owner_capability_validity">check_owner_capability_validity</a>(<a href="safe.md#0x2_safe">safe</a>, capability);
    <a href="vec_set.md#0x2_vec_set_remove">vec_set::remove</a>(&<b>mut</b> <a href="safe.md#0x2_safe">safe</a>.allowed_safes, &capability_id);
}
</code></pre>



</details>

<a name="0x2_safe_self_revoke_transfer_capability"></a>

## Function `self_revoke_transfer_capability`

Revoke a <code><a href="safe.md#0x2_safe_TransferCapability">TransferCapability</a></code> as its owner


<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_self_revoke_transfer_capability">self_revoke_transfer_capability</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_TransferCapability">safe::TransferCapability</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> entry <b>fun</b> <a href="safe.md#0x2_safe_self_revoke_transfer_capability">self_revoke_transfer_capability</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_TransferCapability">TransferCapability</a>&lt;T&gt;) {
    <a href="safe.md#0x2_safe_check_capability_validity">check_capability_validity</a>(<a href="safe.md#0x2_safe">safe</a>, capability);
    <a href="vec_set.md#0x2_vec_set_remove">vec_set::remove</a>(&<b>mut</b> <a href="safe.md#0x2_safe">safe</a>.allowed_safes, &<a href="object.md#0x2_object_id">object::id</a>(capability));
}
</code></pre>



</details>

<a name="0x2_safe_create_transfer_capability"></a>

## Function `create_transfer_capability`

Create <code><a href="safe.md#0x2_safe_TransferCapability">TransferCapability</a></code> as an <code><a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a></code> holder


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_create_transfer_capability">create_transfer_capability</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">safe::Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_OwnerCapability">safe::OwnerCapability</a>&lt;T&gt;, withdraw_amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="safe.md#0x2_safe_TransferCapability">safe::TransferCapability</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="safe.md#0x2_safe_create_transfer_capability">create_transfer_capability</a>&lt;T&gt;(<a href="safe.md#0x2_safe">safe</a>: &<b>mut</b> <a href="safe.md#0x2_safe_Safe">Safe</a>&lt;T&gt;, capability: &<a href="safe.md#0x2_safe_OwnerCapability">OwnerCapability</a>&lt;T&gt;, withdraw_amount: u64, ctx: &<b>mut</b> TxContext): <a href="safe.md#0x2_safe_TransferCapability">TransferCapability</a>&lt;T&gt; {
    // Ensures that only the owner can withdraw from the <a href="safe.md#0x2_safe">safe</a>.
    <a href="safe.md#0x2_safe_check_owner_capability_validity">check_owner_capability_validity</a>(<a href="safe.md#0x2_safe">safe</a>, capability);
    <a href="safe.md#0x2_safe_create_capability_">create_capability_</a>(<a href="safe.md#0x2_safe">safe</a>, withdraw_amount, ctx)
}
</code></pre>



</details>
