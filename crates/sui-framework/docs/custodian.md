
<a name="0xdee9_custodian"></a>

# Module `0xdee9::custodian`

[DEPRECATED]
This module is deprecated and is no longer used in the DeepBook codebase,
Use <code><a href="custodian_v2.md#0xdee9_custodian_v2">custodian_v2</a></code> instead (paired with <code><a href="clob_v2.md#0xdee9_clob_v2">clob_v2</a></code>).

Legacy type definitions and public functions are kept due to package upgrade
constraints.


-  [Struct `Account`](#0xdee9_custodian_Account)
-  [Resource `AccountCap`](#0xdee9_custodian_AccountCap)
-  [Resource `Custodian`](#0xdee9_custodian_Custodian)
-  [Constants](#@Constants_0)
-  [Function `mint_account_cap`](#0xdee9_custodian_mint_account_cap)


<pre><code><b>use</b> <a href="../../../.././build/Sui/docs/balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/table.md#0x2_table">0x2::table</a>;
<b>use</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0xdee9_custodian_Account"></a>

## Struct `Account`

A single account stored in the <code><a href="custodian.md#0xdee9_custodian_Custodian">Custodian</a></code> object in the <code>account_balances</code>
table.


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

Custodian for limit orders.


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

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0xdee9_custodian_EDeprecated"></a>

Deprecated methods.


<pre><code><b>const</b> <a href="custodian.md#0xdee9_custodian_EDeprecated">EDeprecated</a>: u64 = 1337;
</code></pre>



<a name="0xdee9_custodian_mint_account_cap"></a>

## Function `mint_account_cap`

Deprecated: use <code><a href="custodian_v2.md#0xdee9_custodian_v2_mint_account_cap">custodian_v2::mint_account_cap</a></code> instead.


<pre><code><b>public</b> <b>fun</b> <a href="custodian.md#0xdee9_custodian_mint_account_cap">mint_account_cap</a>(_ctx: &<b>mut</b> <a href="../../../.././build/Sui/docs/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="custodian.md#0xdee9_custodian_AccountCap">custodian::AccountCap</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="custodian.md#0xdee9_custodian_mint_account_cap">mint_account_cap</a>(_ctx: &<b>mut</b> TxContext): <a href="custodian.md#0xdee9_custodian_AccountCap">AccountCap</a> {
    <b>abort</b> <a href="custodian.md#0xdee9_custodian_EDeprecated">EDeprecated</a>
}
</code></pre>



</details>
