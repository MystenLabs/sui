---
title: Module `0xdee9::custodian_v2`
---



-  [Struct `Account`](#0xdee9_custodian_v2_Account)
-  [Resource `AccountCap`](#0xdee9_custodian_v2_AccountCap)
-  [Resource `Custodian`](#0xdee9_custodian_v2_Custodian)
-  [Constants](#@Constants_0)
-  [Function `mint_account_cap`](#0xdee9_custodian_v2_mint_account_cap)
-  [Function `create_child_account_cap`](#0xdee9_custodian_v2_create_child_account_cap)
-  [Function `delete_account_cap`](#0xdee9_custodian_v2_delete_account_cap)
-  [Function `account_owner`](#0xdee9_custodian_v2_account_owner)
-  [Function `account_balance`](#0xdee9_custodian_v2_account_balance)
-  [Function `new`](#0xdee9_custodian_v2_new)
-  [Function `withdraw_asset`](#0xdee9_custodian_v2_withdraw_asset)
-  [Function `increase_user_available_balance`](#0xdee9_custodian_v2_increase_user_available_balance)
-  [Function `decrease_user_available_balance`](#0xdee9_custodian_v2_decrease_user_available_balance)
-  [Function `increase_user_locked_balance`](#0xdee9_custodian_v2_increase_user_locked_balance)
-  [Function `decrease_user_locked_balance`](#0xdee9_custodian_v2_decrease_user_locked_balance)
-  [Function `lock_balance`](#0xdee9_custodian_v2_lock_balance)
-  [Function `unlock_balance`](#0xdee9_custodian_v2_unlock_balance)
-  [Function `account_available_balance`](#0xdee9_custodian_v2_account_available_balance)
-  [Function `account_locked_balance`](#0xdee9_custodian_v2_account_locked_balance)
-  [Function `borrow_mut_account_balance`](#0xdee9_custodian_v2_borrow_mut_account_balance)


<pre><code><b>use</b> <a href="../sui-framework/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../sui-framework/table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0xdee9_custodian_v2_Account"></a>

## Struct `Account`



<pre><code><b>struct</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Account">Account</a>&lt;T&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>available_balance: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>locked_balance: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xdee9_custodian_v2_AccountCap"></a>

## Resource `AccountCap`

Capability granting permission to access an entry in <code><a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>.account_balances</code>.
Calling <code>mint_account_cap</code> creates an "admin account cap" such that id == owner with
the permission to both access funds and create new <code><a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a></code>s.
Calling <code>create_child_account_cap</code> creates a "child account cap" such that id != owner
that can access funds, but cannot create new <code><a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a></code>s.


<pre><code><b>struct</b> <a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>owner: <b>address</b></code>
</dt>
<dd>
 The owner of this AccountCap. Note: this is
 derived from an object ID, not a user address
</dd>
</dl>


</details>

<a name="0xdee9_custodian_v2_Custodian"></a>

## Resource `Custodian`



<pre><code><b>struct</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>account_balances: <a href="../sui-framework/table.md#0x2_table_Table">table::Table</a>&lt;<b>address</b>, <a href="custodian_v2.md#0xdee9_custodian_v2_Account">custodian_v2::Account</a>&lt;T&gt;&gt;</code>
</dt>
<dd>
 Map from the owner address of AccountCap object to an Account object
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0xdee9_custodian_v2_EAdminAccountCapRequired"></a>



<pre><code><b>const</b> <a href="custodian_v2.md#0xdee9_custodian_v2_EAdminAccountCapRequired">EAdminAccountCapRequired</a>: u64 = 2;
</code></pre>



<a name="0xdee9_custodian_v2_mint_account_cap"></a>

## Function `mint_account_cap`

Create an admin <code><a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a></code> that can be used across all DeepBook pools, and has
the permission to create new <code><a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a></code>s that can access the same source of funds


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_mint_account_cap">mint_account_cap</a>(ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">custodian_v2::AccountCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_mint_account_cap">mint_account_cap</a>(ctx: &<b>mut</b> TxContext): <a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a> {
    <b>let</b> id = <a href="../sui-framework/object.md#0x2_object_new">object::new</a>(ctx);
    <b>let</b> owner = <a href="../sui-framework/object.md#0x2_object_uid_to_address">object::uid_to_address</a>(&id);
    <a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a> { id, owner }
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_create_child_account_cap"></a>

## Function `create_child_account_cap`

Create a "child account cap" such that id != owner
that can access funds, but cannot create new <code><a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a></code>s.


<pre><code><b>public</b> <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_create_child_account_cap">create_child_account_cap</a>(admin_account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">custodian_v2::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">custodian_v2::AccountCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_create_child_account_cap">create_child_account_cap</a>(admin_account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a>, ctx: &<b>mut</b> TxContext): <a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a> {
    // Only the admin account cap can create new account caps
    <b>assert</b>!(<a href="../sui-framework/object.md#0x2_object_uid_to_address">object::uid_to_address</a>(&admin_account_cap.id) == admin_account_cap.owner, <a href="custodian_v2.md#0xdee9_custodian_v2_EAdminAccountCapRequired">EAdminAccountCapRequired</a>);

    <a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a> {
        id: <a href="../sui-framework/object.md#0x2_object_new">object::new</a>(ctx),
        owner: admin_account_cap.owner
    }
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_delete_account_cap"></a>

## Function `delete_account_cap`

Destroy the given <code>account_cap</code> object


<pre><code><b>public</b> <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_delete_account_cap">delete_account_cap</a>(account_cap: <a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">custodian_v2::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_delete_account_cap">delete_account_cap</a>(account_cap: <a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a>) {
    <b>let</b> <a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a> { id, owner: _ } = account_cap;
    <a href="../sui-framework/object.md#0x2_object_delete">object::delete</a>(id)
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_account_owner"></a>

## Function `account_owner`

Return the owner of an AccountCap


<pre><code><b>public</b> <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_account_owner">account_owner</a>(account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">custodian_v2::AccountCap</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_account_owner">account_owner</a>(account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a>): <b>address</b> {
    account_cap.owner
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_account_balance"></a>

## Function `account_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_account_balance">account_balance</a>&lt;Asset&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;Asset&gt;, owner: <b>address</b>): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_account_balance">account_balance</a>&lt;Asset&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;Asset&gt;,
    owner: <b>address</b>
): (u64, u64) {
    // <b>if</b> <a href="custodian.md#0xdee9_custodian">custodian</a> account is not created yet, directly <b>return</b> (0, 0) rather than <b>abort</b>
    <b>if</b> (!<a href="../sui-framework/table.md#0x2_table_contains">table::contains</a>(&<a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, owner)) {
        <b>return</b> (0, 0)
    };
    <b>let</b> account_balances = <a href="../sui-framework/table.md#0x2_table_borrow">table::borrow</a>(&<a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, owner);
    <b>let</b> avail_balance = <a href="../sui-framework/balance.md#0x2_balance_value">balance::value</a>(&account_balances.available_balance);
    <b>let</b> locked_balance = <a href="../sui-framework/balance.md#0x2_balance_value">balance::value</a>(&account_balances.locked_balance);
    (avail_balance, locked_balance)
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_new"></a>

## Function `new`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_new">new</a>&lt;T&gt;(ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_new">new</a>&lt;T&gt;(ctx: &<b>mut</b> TxContext): <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt; {
    <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt; {
        id: <a href="../sui-framework/object.md#0x2_object_new">object::new</a>(ctx),
        account_balances: <a href="../sui-framework/table.md#0x2_table_new">table::new</a>(ctx),
    }
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_withdraw_asset"></a>

## Function `withdraw_asset`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_withdraw_asset">withdraw_asset</a>&lt;Asset&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;Asset&gt;, quantity: u64, account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">custodian_v2::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;Asset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_withdraw_asset">withdraw_asset</a>&lt;Asset&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;Asset&gt;,
    quantity: u64,
    account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a>,
    ctx: &<b>mut</b> TxContext
): Coin&lt;Asset&gt; {
    <a href="../sui-framework/coin.md#0x2_coin_from_balance">coin::from_balance</a>(<a href="custodian_v2.md#0xdee9_custodian_v2_decrease_user_available_balance">decrease_user_available_balance</a>&lt;Asset&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, account_cap, quantity), ctx)
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_increase_user_available_balance"></a>

## Function `increase_user_available_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>, quantity: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
    quantity: Balance&lt;T&gt;,
) {
    <b>let</b> account = <a href="custodian_v2.md#0xdee9_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, owner);
    <a href="../sui-framework/balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> account.available_balance, quantity);
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_decrease_user_available_balance"></a>

## Function `decrease_user_available_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_decrease_user_available_balance">decrease_user_available_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;T&gt;, account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">custodian_v2::AccountCap</a>, quantity: u64): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_decrease_user_available_balance">decrease_user_available_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a>,
    quantity: u64,
): Balance&lt;T&gt; {
    <b>let</b> account = <a href="custodian_v2.md#0xdee9_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, account_cap.owner);
    <a href="../sui-framework/balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> account.available_balance, quantity)
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_increase_user_locked_balance"></a>

## Function `increase_user_locked_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_increase_user_locked_balance">increase_user_locked_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;T&gt;, account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">custodian_v2::AccountCap</a>, quantity: <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_increase_user_locked_balance">increase_user_locked_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a>,
    quantity: Balance&lt;T&gt;,
) {
    <b>let</b> account = <a href="custodian_v2.md#0xdee9_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, account_cap.owner);
    <a href="../sui-framework/balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> account.locked_balance, quantity);
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_decrease_user_locked_balance"></a>

## Function `decrease_user_locked_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>, quantity: u64): <a href="../sui-framework/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
    quantity: u64,
): Balance&lt;T&gt; {
    <b>let</b> account = <a href="custodian_v2.md#0xdee9_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, owner);
    split(&<b>mut</b> account.locked_balance, quantity)
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_lock_balance"></a>

## Function `lock_balance`

Move <code>quantity</code> from the unlocked balance of <code>user</code> to the locked balance of <code>user</code>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_lock_balance">lock_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;T&gt;, account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">custodian_v2::AccountCap</a>, quantity: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_lock_balance">lock_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="custodian_v2.md#0xdee9_custodian_v2_AccountCap">AccountCap</a>,
    quantity: u64,
) {
    <b>let</b> to_lock = <a href="custodian_v2.md#0xdee9_custodian_v2_decrease_user_available_balance">decrease_user_available_balance</a>(<a href="custodian.md#0xdee9_custodian">custodian</a>, account_cap, quantity);
    <a href="custodian_v2.md#0xdee9_custodian_v2_increase_user_locked_balance">increase_user_locked_balance</a>(<a href="custodian.md#0xdee9_custodian">custodian</a>, account_cap, to_lock);
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_unlock_balance"></a>

## Function `unlock_balance`

Move <code>quantity</code> from the locked balance of <code>user</code> to the unlocked balance of <code>user</code>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_unlock_balance">unlock_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>, quantity: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_unlock_balance">unlock_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
    quantity: u64,
) {
    <b>let</b> locked_balance = <a href="custodian_v2.md#0xdee9_custodian_v2_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, owner, quantity);
    <a href="custodian_v2.md#0xdee9_custodian_v2_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, owner, locked_balance)
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_account_available_balance"></a>

## Function `account_available_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_account_available_balance">account_available_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_account_available_balance">account_available_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
): u64 {
    <a href="../sui-framework/balance.md#0x2_balance_value">balance::value</a>(&<a href="../sui-framework/table.md#0x2_table_borrow">table::borrow</a>(&<a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, owner).available_balance)
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_account_locked_balance"></a>

## Function `account_locked_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_account_locked_balance">account_locked_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_account_locked_balance">account_locked_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
): u64 {
    <a href="../sui-framework/balance.md#0x2_balance_value">balance::value</a>(&<a href="../sui-framework/table.md#0x2_table_borrow">table::borrow</a>(&<a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, owner).locked_balance)
}
</code></pre>



</details>

<a name="0xdee9_custodian_v2_borrow_mut_account_balance"></a>

## Function `borrow_mut_account_balance`



<pre><code><b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>): &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Account">custodian_v2::Account</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="custodian_v2.md#0xdee9_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
): &<b>mut</b> <a href="custodian_v2.md#0xdee9_custodian_v2_Account">Account</a>&lt;T&gt; {
    <b>if</b> (!<a href="../sui-framework/table.md#0x2_table_contains">table::contains</a>(&<a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, owner)) {
        <a href="../sui-framework/table.md#0x2_table_add">table::add</a>(
            &<b>mut</b> <a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances,
            owner,
            <a href="custodian_v2.md#0xdee9_custodian_v2_Account">Account</a> { available_balance: <a href="../sui-framework/balance.md#0x2_balance_zero">balance::zero</a>(), locked_balance: <a href="../sui-framework/balance.md#0x2_balance_zero">balance::zero</a>() }
        );
    };
    <a href="../sui-framework/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> <a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, owner)
}
</code></pre>



</details>
