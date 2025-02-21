---
title: Module `deepbook::custodian`
---



-  [Struct `Account`](#deepbook_custodian_Account)
-  [Struct `AccountCap`](#deepbook_custodian_AccountCap)
-  [Struct `Custodian`](#deepbook_custodian_Custodian)
-  [Function `mint_account_cap`](#deepbook_custodian_mint_account_cap)
-  [Function `account_balance`](#deepbook_custodian_account_balance)
-  [Function `new`](#deepbook_custodian_new)
-  [Function `withdraw_asset`](#deepbook_custodian_withdraw_asset)
-  [Function `increase_user_available_balance`](#deepbook_custodian_increase_user_available_balance)
-  [Function `decrease_user_available_balance`](#deepbook_custodian_decrease_user_available_balance)
-  [Function `increase_user_locked_balance`](#deepbook_custodian_increase_user_locked_balance)
-  [Function `decrease_user_locked_balance`](#deepbook_custodian_decrease_user_locked_balance)
-  [Function `lock_balance`](#deepbook_custodian_lock_balance)
-  [Function `unlock_balance`](#deepbook_custodian_unlock_balance)
-  [Function `account_available_balance`](#deepbook_custodian_account_available_balance)
-  [Function `account_locked_balance`](#deepbook_custodian_account_locked_balance)
-  [Function `borrow_mut_account_balance`](#deepbook_custodian_borrow_mut_account_balance)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/balance.md#sui_balance">sui::balance</a>;
<b>use</b> <a href="../sui/coin.md#sui_coin">sui::coin</a>;
<b>use</b> <a href="../sui/config.md#sui_config">sui::config</a>;
<b>use</b> <a href="../sui/deny_list.md#sui_deny_list">sui::deny_list</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="deepbook_custodian_Account"></a>

## Struct `Account`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/custodian.md#deepbook_custodian_Account">Account</a>&lt;<b>phantom</b> T&gt; <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>available_balance: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>locked_balance: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="deepbook_custodian_AccountCap"></a>

## Struct `AccountCap`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">AccountCap</a> <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="deepbook_custodian_Custodian"></a>

## Struct `Custodian`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;<b>phantom</b> T&gt; <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
<dt>
<code>account_balances: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, <a href="../deepbook/custodian.md#deepbook_custodian_Account">deepbook::custodian::Account</a>&lt;T&gt;&gt;</code>
</dt>
<dd>
 Map from an AccountCap object ID to an Account object
</dd>
</dl>


</details>

<a name="deepbook_custodian_mint_account_cap"></a>

## Function `mint_account_cap`

Create an <code><a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">AccountCap</a></code> that can be used across all DeepBook pool


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_mint_account_cap">mint_account_cap</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_mint_account_cap">mint_account_cap</a>(ctx: &<b>mut</b> TxContext): <a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">AccountCap</a> {
    <a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">AccountCap</a> { id: object::new(ctx) }
}
</code></pre>



</details>

<a name="deepbook_custodian_account_balance"></a>

## Function `account_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_account_balance">account_balance</a>&lt;Asset&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;Asset&gt;, user: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_account_balance">account_balance</a>&lt;Asset&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;Asset&gt;,
    user: ID
): (u64, u64) {
    // <b>if</b> <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a> account is not created yet, directly <b>return</b> (0, 0) rather than <b>abort</b>
    <b>if</b> (!table::contains(&<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, user)) {
        <b>return</b> (0, 0)
    };
    <b>let</b> account_balances = table::borrow(&<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, user);
    <b>let</b> avail_balance = balance::value(&account_balances.available_balance);
    <b>let</b> locked_balance = balance::value(&account_balances.locked_balance);
    (avail_balance, locked_balance)
}
</code></pre>



</details>

<a name="deepbook_custodian_new"></a>

## Function `new`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_new">new</a>&lt;T&gt;(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_new">new</a>&lt;T&gt;(ctx: &<b>mut</b> TxContext): <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;T&gt; {
    <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;T&gt; {
        id: object::new(ctx),
        account_balances: table::new(ctx),
    }
}
</code></pre>



</details>

<a name="deepbook_custodian_withdraw_asset"></a>

## Function `withdraw_asset`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_withdraw_asset">withdraw_asset</a>&lt;Asset&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;Asset&gt;, quantity: u64, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;Asset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_withdraw_asset">withdraw_asset</a>&lt;Asset&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;Asset&gt;,
    quantity: u64,
    account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">AccountCap</a>,
    ctx: &<b>mut</b> TxContext
): Coin&lt;Asset&gt; {
    coin::from_balance(<a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_available_balance">decrease_user_available_balance</a>&lt;Asset&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, account_cap, quantity), ctx)
}
</code></pre>



</details>

<a name="deepbook_custodian_increase_user_available_balance"></a>

## Function `increase_user_available_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;T&gt;, user: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, quantity: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
    quantity: Balance&lt;T&gt;,
) {
    <b>let</b> account = <a href="../deepbook/custodian.md#deepbook_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, user);
    balance::join(&<b>mut</b> account.available_balance, quantity);
}
</code></pre>



</details>

<a name="deepbook_custodian_decrease_user_available_balance"></a>

## Function `decrease_user_available_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_available_balance">decrease_user_available_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;T&gt;, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>, quantity: u64): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_available_balance">decrease_user_available_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">AccountCap</a>,
    quantity: u64,
): Balance&lt;T&gt; {
    <b>let</b> account = <a href="../deepbook/custodian.md#deepbook_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, object::uid_to_inner(&account_cap.id));
    balance::split(&<b>mut</b> account.available_balance, quantity)
}
</code></pre>



</details>

<a name="deepbook_custodian_increase_user_locked_balance"></a>

## Function `increase_user_locked_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_locked_balance">increase_user_locked_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;T&gt;, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>, quantity: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_locked_balance">increase_user_locked_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">AccountCap</a>,
    quantity: Balance&lt;T&gt;,
) {
    <b>let</b> account = <a href="../deepbook/custodian.md#deepbook_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, object::uid_to_inner(&account_cap.id));
    balance::join(&<b>mut</b> account.locked_balance, quantity);
}
</code></pre>



</details>

<a name="deepbook_custodian_decrease_user_locked_balance"></a>

## Function `decrease_user_locked_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;T&gt;, user: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, quantity: u64): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
    quantity: u64,
): Balance&lt;T&gt; {
    <b>let</b> account = <a href="../deepbook/custodian.md#deepbook_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, user);
    split(&<b>mut</b> account.locked_balance, quantity)
}
</code></pre>



</details>

<a name="deepbook_custodian_lock_balance"></a>

## Function `lock_balance`

Move <code>quantity</code> from the unlocked balance of <code>user</code> to the locked balance of <code>user</code>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_lock_balance">lock_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;T&gt;, account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">deepbook::custodian::AccountCap</a>, quantity: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_lock_balance">lock_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="../deepbook/custodian.md#deepbook_custodian_AccountCap">AccountCap</a>,
    quantity: u64,
) {
    <b>let</b> to_lock = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_available_balance">decrease_user_available_balance</a>(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, account_cap, quantity);
    <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_locked_balance">increase_user_locked_balance</a>(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, account_cap, to_lock);
}
</code></pre>



</details>

<a name="deepbook_custodian_unlock_balance"></a>

## Function `unlock_balance`

Move <code>quantity</code> from the locked balance of <code>user</code> to the unlocked balacne of <code>user</code>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">unlock_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;T&gt;, user: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, quantity: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_unlock_balance">unlock_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
    quantity: u64,
) {
    <b>let</b> locked_balance = <a href="../deepbook/custodian.md#deepbook_custodian_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, user, quantity);
    <a href="../deepbook/custodian.md#deepbook_custodian_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, user, locked_balance)
}
</code></pre>



</details>

<a name="deepbook_custodian_account_available_balance"></a>

## Function `account_available_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_account_available_balance">account_available_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;T&gt;, user: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_account_available_balance">account_available_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
): u64 {
    balance::value(&table::borrow(&<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, user).available_balance)
}
</code></pre>



</details>

<a name="deepbook_custodian_account_locked_balance"></a>

## Function `account_locked_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_account_locked_balance">account_locked_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;T&gt;, user: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_account_locked_balance">account_locked_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
): u64 {
    balance::value(&table::borrow(&<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, user).locked_balance)
}
</code></pre>



</details>

<a name="deepbook_custodian_borrow_mut_account_balance"></a>

## Function `borrow_mut_account_balance`



<pre><code><b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">deepbook::custodian::Custodian</a>&lt;T&gt;, user: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Account">deepbook::custodian::Account</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/custodian.md#deepbook_custodian_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Custodian">Custodian</a>&lt;T&gt;,
    user: ID,
): &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian_Account">Account</a>&lt;T&gt; {
    <b>if</b> (!table::contains(&<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, user)) {
        table::add(
            &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances,
            user,
            <a href="../deepbook/custodian.md#deepbook_custodian_Account">Account</a> { available_balance: balance::zero(), locked_balance: balance::zero() }
        );
    };
    table::borrow_mut(&<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, user)
}
</code></pre>



</details>
