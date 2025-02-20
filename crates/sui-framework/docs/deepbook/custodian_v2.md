---
title: Module `deepbook::custodian_v2`
---



-  [Struct `Account`](#deepbook_custodian_v2_Account)
-  [Struct `AccountCap`](#deepbook_custodian_v2_AccountCap)
-  [Struct `Custodian`](#deepbook_custodian_v2_Custodian)
-  [Constants](#@Constants_0)
-  [Function `mint_account_cap`](#deepbook_custodian_v2_mint_account_cap)
-  [Function `create_child_account_cap`](#deepbook_custodian_v2_create_child_account_cap)
-  [Function `delete_account_cap`](#deepbook_custodian_v2_delete_account_cap)
-  [Function `account_owner`](#deepbook_custodian_v2_account_owner)
-  [Function `account_balance`](#deepbook_custodian_v2_account_balance)
-  [Function `new`](#deepbook_custodian_v2_new)
-  [Function `withdraw_asset`](#deepbook_custodian_v2_withdraw_asset)
-  [Function `increase_user_available_balance`](#deepbook_custodian_v2_increase_user_available_balance)
-  [Function `decrease_user_available_balance`](#deepbook_custodian_v2_decrease_user_available_balance)
-  [Function `increase_user_locked_balance`](#deepbook_custodian_v2_increase_user_locked_balance)
-  [Function `decrease_user_locked_balance`](#deepbook_custodian_v2_decrease_user_locked_balance)
-  [Function `lock_balance`](#deepbook_custodian_v2_lock_balance)
-  [Function `unlock_balance`](#deepbook_custodian_v2_unlock_balance)
-  [Function `account_available_balance`](#deepbook_custodian_v2_account_available_balance)
-  [Function `account_locked_balance`](#deepbook_custodian_v2_account_locked_balance)
-  [Function `borrow_mut_account_balance`](#deepbook_custodian_v2_borrow_mut_account_balance)


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



<a name="deepbook_custodian_v2_Account"></a>

## Struct `Account`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Account">Account</a>&lt;<b>phantom</b> T&gt; <b>has</b> store
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

<a name="deepbook_custodian_v2_AccountCap"></a>

## Struct `AccountCap`

Capability granting permission to access an entry in <code><a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>.account_balances</code>.
Calling <code><a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_mint_account_cap">mint_account_cap</a></code> creates an "admin account cap" such that id == owner with
the permission to both access funds and create new <code><a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a></code>s.
Calling <code><a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_create_child_account_cap">create_child_account_cap</a></code> creates a "child account cap" such that id != owner
that can access funds, but cannot create new <code><a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a></code>s.


<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a> <b>has</b> key, store
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
<code>owner: <b>address</b></code>
</dt>
<dd>
 The owner of this AccountCap. Note: this is
 derived from an object ID, not a user address
</dd>
</dl>


</details>

<a name="deepbook_custodian_v2_Custodian"></a>

## Struct `Custodian`



<pre><code><b>public</b> <b>struct</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;<b>phantom</b> T&gt; <b>has</b> key, store
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
<code>account_balances: <a href="../sui/table.md#sui_table_Table">sui::table::Table</a>&lt;<b>address</b>, <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Account">deepbook::custodian_v2::Account</a>&lt;T&gt;&gt;</code>
</dt>
<dd>
 Map from the owner address of AccountCap object to an Account object
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="deepbook_custodian_v2_EAdminAccountCapRequired"></a>



<pre><code><b>const</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_EAdminAccountCapRequired">EAdminAccountCapRequired</a>: u64 = 2;
</code></pre>



<a name="deepbook_custodian_v2_mint_account_cap"></a>

## Function `mint_account_cap`

Create an admin <code><a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a></code> that can be used across all DeepBook pools, and has
the permission to create new <code><a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a></code>s that can access the same source of funds


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_mint_account_cap">mint_account_cap</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_mint_account_cap">mint_account_cap</a>(ctx: &<b>mut</b> TxContext): <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a> {
    <b>let</b> id = object::new(ctx);
    <b>let</b> owner = object::uid_to_address(&id);
    <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a> { id, owner }
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_create_child_account_cap"></a>

## Function `create_child_account_cap`

Create a "child account cap" such that id != owner
that can access funds, but cannot create new <code><a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a></code>s.


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_create_child_account_cap">create_child_account_cap</a>(admin_account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_create_child_account_cap">create_child_account_cap</a>(admin_account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a>, ctx: &<b>mut</b> TxContext): <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a> {
    // Only the admin account cap can create <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_new">new</a> account caps
    <b>assert</b>!(object::uid_to_address(&admin_account_cap.id) == admin_account_cap.owner, <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_EAdminAccountCapRequired">EAdminAccountCapRequired</a>);
    <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a> {
        id: object::new(ctx),
        owner: admin_account_cap.owner
    }
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_delete_account_cap"></a>

## Function `delete_account_cap`

Destroy the given <code>account_cap</code> object


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_delete_account_cap">delete_account_cap</a>(account_cap: <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_delete_account_cap">delete_account_cap</a>(account_cap: <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a>) {
    <b>let</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a> { id, owner: _ } = account_cap;
    object::delete(id)
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_account_owner"></a>

## Function `account_owner`

Return the owner of an AccountCap


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_account_owner">account_owner</a>(account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_account_owner">account_owner</a>(account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a>): <b>address</b> {
    account_cap.owner
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_account_balance"></a>

## Function `account_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_account_balance">account_balance</a>&lt;Asset&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;Asset&gt;, owner: <b>address</b>): (u64, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_account_balance">account_balance</a>&lt;Asset&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;Asset&gt;,
    owner: <b>address</b>
): (u64, u64) {
    // <b>if</b> <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a> account is not created yet, directly <b>return</b> (0, 0) rather than <b>abort</b>
    <b>if</b> (!table::contains(&<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, owner)) {
        <b>return</b> (0, 0)
    };
    <b>let</b> account_balances = table::borrow(&<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, owner);
    <b>let</b> avail_balance = balance::value(&account_balances.available_balance);
    <b>let</b> locked_balance = balance::value(&account_balances.locked_balance);
    (avail_balance, locked_balance)
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_new"></a>

## Function `new`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_new">new</a>&lt;T&gt;(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_new">new</a>&lt;T&gt;(ctx: &<b>mut</b> TxContext): <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;T&gt; {
    <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;T&gt; {
        id: object::new(ctx),
        account_balances: table::new(ctx),
    }
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_withdraw_asset"></a>

## Function `withdraw_asset`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_withdraw_asset">withdraw_asset</a>&lt;Asset&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;Asset&gt;, quantity: u64, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;Asset&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_withdraw_asset">withdraw_asset</a>&lt;Asset&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;Asset&gt;,
    quantity: u64,
    account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a>,
    ctx: &<b>mut</b> TxContext
): Coin&lt;Asset&gt; {
    coin::from_balance(<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_decrease_user_available_balance">decrease_user_available_balance</a>&lt;Asset&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, account_cap, quantity), ctx)
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_increase_user_available_balance"></a>

## Function `increase_user_available_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>, quantity: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
    quantity: Balance&lt;T&gt;,
) {
    <b>let</b> account = <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, owner);
    balance::join(&<b>mut</b> account.available_balance, quantity);
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_decrease_user_available_balance"></a>

## Function `decrease_user_available_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_decrease_user_available_balance">decrease_user_available_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;T&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, quantity: u64): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_decrease_user_available_balance">decrease_user_available_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a>,
    quantity: u64,
): Balance&lt;T&gt; {
    <b>let</b> account = <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, account_cap.owner);
    balance::split(&<b>mut</b> account.available_balance, quantity)
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_increase_user_locked_balance"></a>

## Function `increase_user_locked_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_increase_user_locked_balance">increase_user_locked_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;T&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, quantity: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_increase_user_locked_balance">increase_user_locked_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a>,
    quantity: Balance&lt;T&gt;,
) {
    <b>let</b> account = <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, account_cap.owner);
    balance::join(&<b>mut</b> account.locked_balance, quantity);
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_decrease_user_locked_balance"></a>

## Function `decrease_user_locked_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>, quantity: u64): <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
    quantity: u64,
): Balance&lt;T&gt; {
    <b>let</b> account = <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, owner);
    split(&<b>mut</b> account.locked_balance, quantity)
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_lock_balance"></a>

## Function `lock_balance`

Move <code>quantity</code> from the unlocked balance of <code>user</code> to the locked balance of <code>user</code>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_lock_balance">lock_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;T&gt;, account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">deepbook::custodian_v2::AccountCap</a>, quantity: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_lock_balance">lock_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    account_cap: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_AccountCap">AccountCap</a>,
    quantity: u64,
) {
    <b>let</b> to_lock = <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_decrease_user_available_balance">decrease_user_available_balance</a>(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, account_cap, quantity);
    <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_increase_user_locked_balance">increase_user_locked_balance</a>(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, account_cap, to_lock);
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_unlock_balance"></a>

## Function `unlock_balance`

Move <code>quantity</code> from the locked balance of <code>user</code> to the unlocked balance of <code>user</code>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_unlock_balance">unlock_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>, quantity: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_unlock_balance">unlock_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
    quantity: u64,
) {
    <b>let</b> locked_balance = <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_decrease_user_locked_balance">decrease_user_locked_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, owner, quantity);
    <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_increase_user_available_balance">increase_user_available_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>, owner, locked_balance)
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_account_available_balance"></a>

## Function `account_available_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_account_available_balance">account_available_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_account_available_balance">account_available_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
): u64 {
    balance::value(&table::borrow(&<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, owner).available_balance)
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_account_locked_balance"></a>

## Function `account_locked_balance`



<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_account_locked_balance">account_locked_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_account_locked_balance">account_locked_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
): u64 {
    balance::value(&table::borrow(&<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, owner).locked_balance)
}
</code></pre>



</details>

<a name="deepbook_custodian_v2_borrow_mut_account_balance"></a>

## Function `borrow_mut_account_balance`



<pre><code><b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">deepbook::custodian_v2::Custodian</a>&lt;T&gt;, owner: <b>address</b>): &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Account">deepbook::custodian_v2::Account</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_borrow_mut_account_balance">borrow_mut_account_balance</a>&lt;T&gt;(
    <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>: &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Custodian">Custodian</a>&lt;T&gt;,
    owner: <b>address</b>,
): &<b>mut</b> <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Account">Account</a>&lt;T&gt; {
    <b>if</b> (!table::contains(&<a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, owner)) {
        table::add(
            &<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances,
            owner,
            <a href="../deepbook/custodian_v2.md#deepbook_custodian_v2_Account">Account</a> { available_balance: balance::zero(), locked_balance: balance::zero() }
        );
    };
    table::borrow_mut(&<b>mut</b> <a href="../deepbook/custodian.md#deepbook_custodian">custodian</a>.account_balances, owner)
}
</code></pre>



</details>
