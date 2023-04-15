
<a name="0xdee9_custodian"></a>

# Module `0xdee9::custodian`



-  [Struct `Account`](#0xdee9_custodian_Account)
-  [Resource `AccountCap`](#0xdee9_custodian_AccountCap)
-  [Resource `Custodian`](#0xdee9_custodian_Custodian)
-  [Constants](#@Constants_0)
-  [Function `mint_account_cap`](#0xdee9_custodian_mint_account_cap)
-  [Function `account_balance`](#0xdee9_custodian_account_balance)
-  [Function `new`](#0xdee9_custodian_new)
-  [Function `withdraw_base_asset`](#0xdee9_custodian_withdraw_base_asset)
-  [Function `withdraw_quote_asset`](#0xdee9_custodian_withdraw_quote_asset)
-  [Function `increase_user_available_balance`](#0xdee9_custodian_increase_user_available_balance)
-  [Function `decrease_user_available_balance`](#0xdee9_custodian_decrease_user_available_balance)
-  [Function `increase_user_locked_balance`](#0xdee9_custodian_increase_user_locked_balance)
-  [Function `decrease_user_locked_balance`](#0xdee9_custodian_decrease_user_locked_balance)
-  [Function `lock_balance`](#0xdee9_custodian_lock_balance)
-  [Function `unlock_balance`](#0xdee9_custodian_unlock_balance)
-  [Function `account_available_balance`](#0xdee9_custodian_account_available_balance)
-  [Function `account_locked_balance`](#0xdee9_custodian_account_locked_balance)
-  [Function `borrow_mut_account_balance`](#0xdee9_custodian_borrow_mut_account_balance)
-  [Function `borrow_account_balance`](#0xdee9_custodian_borrow_account_balance)


<pre><code><b>use</b> <a href="../../../.././build/Sui/docs/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0xdee9_custodian_Account"></a>

## Struct `Account`



<pre><code><b>struct</b> <a href="custodian.md#0xdee9_custodian_Account">Account</a>&lt;T&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>available_balance: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>locked_balance: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xdee9_custodian_AccountCap"></a>

## Resource `AccountCap`



<pre><code><b>struct</b> <a href="custodian.md#0xdee9_custodian_AccountCap">AccountCap</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xdee9_custodian_Custodian"></a>

## Resource `Custodian`



<pre><code><b>struct</b> <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt; <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>account_balances: <a href="../../../.././build/Sui/docs/table.md#0x2_table_Table">table::Table</a>&lt;<a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>, <a href="custodian.md#0xdee9_custodian_Account">custodian::Account</a>&lt;T&gt;&gt;</code>
</dt>
<dd>
 Map from an AccountCap object ID to an Account object
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0xdee9_custodian_EUserBalanceDoesNotExist"></a>



<pre><code><b>const</b> <a href="custodian.md#0xdee9_custodian_EUserBalanceDoesNotExist">EUserBalanceDoesNotExist</a>: u64 = 1;
</code></pre>



<a name="0xdee9_custodian_mint_account_cap"></a>

## Function `mint_account_cap`

Create an <code><a href="custodian.md#0xdee9_custodian_AccountCap">AccountCap</a></code> that can be used across all DeepBook pool


<pre><code><b>public</b> <b>fun</b> <a href="custodian.md#0xdee9_custodian_mint_account_cap">mint_account_cap</a>(ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="custodian.md#0xdee9_custodian_mint_account_cap">mint_account_cap</a>(ctx: &<b>mut</b> TxContext): <a href="custodian.md#0xdee9_custodian_AccountCap">AccountCap</a> {
    <a href="custodian.md#0xdee9_custodian_AccountCap">AccountCap</a> { id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_new">object::new</a>(ctx) }
}
</code></pre>



</details>

<a name="0xdee9_custodian_account_balance"></a>

## Function `account_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_account_balance">account_balance</a>&lt;Asset&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;Asset&gt;, user: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_account_balance">account_balance</a>&lt;Asset&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;Asset&gt;,
    user: ID
): (u64, u64){
    <b>let</b> account_balances = <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&<a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, user);
    <b>let</b> avail_balance = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&account_balances.available_balance);
    <b>let</b> locked_balance = <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&account_balances.locked_balance);
    (avail_balance, locked_balance)
}
</code></pre>



</details>

<a name="0xdee9_custodian_new"></a>

## Function `new`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_new">new</a>&lt;T&gt;(ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_new">new</a>&lt;T&gt;(ctx: &<b>mut</b> TxContext): <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt; {
    <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt; {
        id: <a href="../../../.././build/Sui/docs/object.md#0x2_object_new">object::new</a>(ctx),
        account_balances: <a href="../../../.././build/Sui/docs/table.md#0x2_table_new">table::new</a>(ctx),
    }
}
</code></pre>



</details>

<a name="0xdee9_custodian_withdraw_base_asset"></a>

## Function `withdraw_base_asset`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_withdraw_base_asset">withdraw_base_asset</a>&lt;BaseAsset&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;BaseAsset&gt;, quantity: u64, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;BaseAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_withdraw_base_asset">withdraw_base_asset</a>&lt;BaseAsset&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;BaseAsset&gt;,
    quantity: u64,
    account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">AccountCap</a>,
    ctx: &<b>mut</b> TxContext
): Coin&lt;BaseAsset&gt; {
    <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_from_balance">coin::from_balance</a>(<a href="custodian.md#0xdee9_custodian_decrease_user_available_balance">decrease_user_available_balance</a>&lt;BaseAsset&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, account_cap, quantity), ctx)
}
</code></pre>



</details>

<a name="0xdee9_custodian_withdraw_quote_asset"></a>

## Function `withdraw_quote_asset`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_withdraw_quote_asset">withdraw_quote_asset</a>&lt;QuoteAsset&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;QuoteAsset&gt;, quantity: u64, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;QuoteAsset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_withdraw_quote_asset">withdraw_quote_asset</a>&lt;QuoteAsset&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;QuoteAsset&gt;,
    quantity: u64,
    account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">AccountCap</a>,
    ctx: &<b>mut</b> TxContext
): Coin&lt;QuoteAsset&gt; {
    <a href="../../../.././build/Sui/docs/coin.md#0x2_coin_from_balance">coin::from_balance</a>(<a href="custodian.md#0xdee9_custodian_decrease_user_available_balance">decrease_user_available_balance</a>&lt;QuoteAsset&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, account_cap, quantity), ctx)
}
</code></pre>



</details>

<a name="0xdee9_custodian_increase_user_available_balance"></a>

## Function `increase_user_available_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;T&gt;, user: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>, quantity: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
    quantity: Balance&lt;T&gt;,
) {
    <b>let</b> account = <a href="custodian.md#0xdee9_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, user);
    <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> account.available_balance, quantity);
}
</code></pre>



</details>

<a name="0xdee9_custodian_decrease_user_available_balance"></a>

## Function `decrease_user_available_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_decrease_user_available_balance">decrease_user_available_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;T&gt;, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, quantity: u64): <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_decrease_user_available_balance">decrease_user_available_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">AccountCap</a>,
    quantity: u64,
): Balance&lt;T&gt; {
    <b>let</b> account = <a href="custodian.md#0xdee9_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, <a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&account_cap.id));
    <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> account.available_balance, quantity)
}
</code></pre>



</details>

<a name="0xdee9_custodian_increase_user_locked_balance"></a>

## Function `increase_user_locked_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_increase_user_locked_balance">increase_user_locked_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;T&gt;, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, quantity: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_increase_user_locked_balance">increase_user_locked_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">AccountCap</a>,
    quantity: Balance&lt;T&gt;,
) {
    <b>let</b> account = <a href="custodian.md#0xdee9_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, <a href="../../../.././build/Sui/docs/object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&account_cap.id));
    <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> account.locked_balance, quantity);
}
</code></pre>



</details>

<a name="0xdee9_custodian_decrease_user_locked_balance"></a>

## Function `decrease_user_locked_balance`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;T&gt;, user: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>, quantity: u64): <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
    quantity: u64,
): Balance&lt;T&gt; {
    <b>let</b> account = <a href="custodian.md#0xdee9_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, user);
    split(&<b>mut</b> account.locked_balance, quantity)
}
</code></pre>



</details>

<a name="0xdee9_custodian_lock_balance"></a>

## Function `lock_balance`

Move <code>quantity</code> from the unlocked balance of <code>user</code> to the locked balance of <code>user</code>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_lock_balance">lock_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;T&gt;, account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>, quantity: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_lock_balance">lock_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="custodian.md#0xdee9_custodian_AccountCap">AccountCap</a>,
    quantity: u64,
) {
    <b>let</b> to_lock = <a href="custodian.md#0xdee9_custodian_decrease_user_available_balance">decrease_user_available_balance</a>(<a href="custodian.md#0xdee9_custodian">custodian</a>, account_cap, quantity);
    <a href="custodian.md#0xdee9_custodian_increase_user_locked_balance">increase_user_locked_balance</a>(<a href="custodian.md#0xdee9_custodian">custodian</a>, account_cap, to_lock);
}
</code></pre>



</details>

<a name="0xdee9_custodian_unlock_balance"></a>

## Function `unlock_balance`

Move <code>quantity</code> from the locked balance of <code>user</code> to the unlocked balacne of <code>user</code>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_unlock_balance">unlock_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;T&gt;, user: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>, quantity: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="custodian.md#0xdee9_custodian_unlock_balance">unlock_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
    quantity: u64,
) {
    <b>let</b> locked_balance = <a href="custodian.md#0xdee9_custodian_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, user, quantity);
    <a href="custodian.md#0xdee9_custodian_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>, user, locked_balance)
}
</code></pre>



</details>

<a name="0xdee9_custodian_account_available_balance"></a>

## Function `account_available_balance`



<pre><code><b>public</b> <b>fun</b> <a href="custodian.md#0xdee9_custodian_account_available_balance">account_available_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;T&gt;, user: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="custodian.md#0xdee9_custodian_account_available_balance">account_available_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
): u64 {
    <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&<a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&<a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, user).available_balance)
}
</code></pre>



</details>

<a name="0xdee9_custodian_account_locked_balance"></a>

## Function `account_locked_balance`



<pre><code><b>public</b> <b>fun</b> <a href="custodian.md#0xdee9_custodian_account_locked_balance">account_locked_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;T&gt;, user: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="custodian.md#0xdee9_custodian_account_locked_balance">account_locked_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
): u64 {
    <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_value">balance::value</a>(&<a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&<a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, user).locked_balance)
}
</code></pre>



</details>

<a name="0xdee9_custodian_borrow_mut_account_balance"></a>

## Function `borrow_mut_account_balance`



<pre><code><b>fun</b> <a href="custodian.md#0xdee9_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;T&gt;, user: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>): &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Account">custodian::Account</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="custodian.md#0xdee9_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
): &<b>mut</b> <a href="custodian.md#0xdee9_custodian_Account">Account</a>&lt;T&gt; {
    <b>if</b> (!<a href="../../../.././build/Sui/docs/table.md#0x2_table_contains">table::contains</a>(&<a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, user)) {
        <a href="../../../.././build/Sui/docs/table.md#0x2_table_add">table::add</a>(
            &<b>mut</b> <a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances,
            user,
            <a href="custodian.md#0xdee9_custodian_Account">Account</a> { available_balance: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_zero">balance::zero</a>(), locked_balance: <a href="../../../.././build/Sui/docs/balance.md#0x2_balance_zero">balance::zero</a>() }
        );
    };
    <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow_mut">table::borrow_mut</a>(&<b>mut</b> <a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, user)
}
</code></pre>



</details>

<a name="0xdee9_custodian_borrow_account_balance"></a>

## Function `borrow_account_balance`



<pre><code><b>fun</b> <a href="custodian.md#0xdee9_custodian_borrow_account_balance">borrow_account_balance</a>&lt;T&gt;(<a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian.md#0xdee9_custodian_Custodian">custodian::Custodian</a>&lt;T&gt;, user: <a href="../../../.././build/Sui/docs/object.md#0x2_object_ID">object::ID</a>): &<a href="custodian.md#0xdee9_custodian_Account">custodian::Account</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="custodian.md#0xdee9_custodian_borrow_account_balance">borrow_account_balance</a>&lt;T&gt;(
    <a href="custodian.md#0xdee9_custodian">custodian</a>: &<a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
): &<a href="custodian.md#0xdee9_custodian_Account">Account</a>&lt;T&gt; {
    <b>assert</b>!(
        <a href="../../../.././build/Sui/docs/table.md#0x2_table_contains">table::contains</a>(&<a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, user),
        <a href="custodian.md#0xdee9_custodian_EUserBalanceDoesNotExist">EUserBalanceDoesNotExist</a>
    );
    <a href="../../../.././build/Sui/docs/table.md#0x2_table_borrow">table::borrow</a>(&<a href="custodian.md#0xdee9_custodian">custodian</a>.account_balances, user)
}
</code></pre>



</details>
