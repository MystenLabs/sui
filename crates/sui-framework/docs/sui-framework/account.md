---
title: Module `0x2::account`
---



-  [Struct `AccountKey`](#0x2_account_AccountKey)
-  [Struct `Reservation`](#0x2_account_Reservation)
-  [Struct `Merge`](#0x2_account_Merge)
-  [Struct `Split`](#0x2_account_Split)
-  [Constants](#@Constants_0)
-  [Function `get_account_field_address`](#0x2_account_get_account_field_address)
-  [Function `decrement`](#0x2_account_decrement)
-  [Function `reserve`](#0x2_account_reserve)
-  [Function `withdraw_from_account`](#0x2_account_withdraw_from_account)
-  [Function `transfer_to_account`](#0x2_account_transfer_to_account)
-  [Function `merge_to_account`](#0x2_account_merge_to_account)
-  [Function `split_from_account`](#0x2_account_split_from_account)
-  [Function `emit_account_event`](#0x2_account_emit_account_event)


<pre><code><b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_account_AccountKey"></a>

## Struct `AccountKey`



<pre><code><b>struct</b> <a href="../sui-framework/account.md#0x2_account_AccountKey">AccountKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><b>address</b>: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>ty: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_account_Reservation"></a>

## Struct `Reservation`



<pre><code><b>struct</b> <a href="../sui-framework/account.md#0x2_account_Reservation">Reservation</a>&lt;T&gt; <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>owner: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>limit: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_account_Merge"></a>

## Struct `Merge`



<pre><code><b>struct</b> <a href="../sui-framework/account.md#0x2_account_Merge">Merge</a>&lt;T&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><b>address</b>: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>value: T</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_account_Split"></a>

## Struct `Split`



<pre><code><b>struct</b> <a href="../sui-framework/account.md#0x2_account_Split">Split</a>&lt;T&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><b>address</b>: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>value: T</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_account_EInsufficientFunds"></a>



<pre><code><b>const</b> <a href="../sui-framework/account.md#0x2_account_EInsufficientFunds">EInsufficientFunds</a>: u64 = 1;
</code></pre>



<a name="0x2_account_ENotAccountOwner"></a>



<pre><code><b>const</b> <a href="../sui-framework/account.md#0x2_account_ENotAccountOwner">ENotAccountOwner</a>: u64 = 0;
</code></pre>



<a name="0x2_account_get_account_field_address"></a>

## Function `get_account_field_address`



<pre><code><b>fun</b> <a href="../sui-framework/account.md#0x2_account_get_account_field_address">get_account_field_address</a>&lt;T&gt;(<b>address</b>: <b>address</b>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/account.md#0x2_account_get_account_field_address">get_account_field_address</a>&lt;T&gt;(<b>address</b>: <b>address</b>): <b>address</b> {
    <b>let</b> ty = <a href="../move-stdlib/type_name.md#0x1_type_name_get_with_original_ids">type_name::get_with_original_ids</a>&lt;T&gt;().into_string().into_bytes();
    <b>let</b> key = <a href="../sui-framework/account.md#0x2_account_AccountKey">AccountKey</a> { <b>address</b>, ty };
    <b>return</b> field::hash_type_and_key(sui_account_root_address(), key)
}
</code></pre>



</details>

<a name="0x2_account_decrement"></a>

## Function `decrement`



<pre><code><b>fun</b> <a href="../sui-framework/account.md#0x2_account_decrement">decrement</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/account.md#0x2_account_Reservation">account::Reservation</a>&lt;T&gt;, amount: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/account.md#0x2_account_decrement">decrement</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui-framework/account.md#0x2_account_Reservation">Reservation</a>&lt;T&gt;, amount: u64) {
    <b>assert</b>!(self.limit &gt;= amount, <a href="../sui-framework/account.md#0x2_account_EInsufficientFunds">EInsufficientFunds</a>);
    self.limit = self.limit - amount;
}
</code></pre>



</details>

<a name="0x2_account_reserve"></a>

## Function `reserve`

The scheduling/execution layer ensures that a reserve() call is never permitted unless
there are sufficient funds. <code><a href="../sui-framework/account.md#0x2_account_Reservation">Reservation</a>&lt;T&gt;</code> thus serves as a proof that the account
cannot be overdrawn.


<pre><code>entry <b>fun</b> <a href="../sui-framework/account.md#0x2_account_reserve">reserve</a>&lt;T&gt;(owner: <b>address</b>, limit: u64, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/account.md#0x2_account_Reservation">account::Reservation</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>fun</b> <a href="../sui-framework/account.md#0x2_account_reserve">reserve</a>&lt;T&gt;(owner: <b>address</b>, limit: u64, ctx: &TxContext): <a href="../sui-framework/account.md#0x2_account_Reservation">Reservation</a>&lt;T&gt; {
    // TODO: handle sponsored transactions and (in the future) multi-agent transactions
    <b>assert</b>!(ctx.sender() == owner, <a href="../sui-framework/account.md#0x2_account_ENotAccountOwner">ENotAccountOwner</a>);
    <b>return</b> <a href="../sui-framework/account.md#0x2_account_Reservation">Reservation</a> { owner, limit }
}
</code></pre>



</details>

<a name="0x2_account_withdraw_from_account"></a>

## Function `withdraw_from_account`

Withdraw from an account.
Requires a reservation of the appropriate type, with proof of enough funds to cover the
withdrawal.

<code>value</code> is an amount to be debited from the account. Therefore, modules who wish to
ensure conservation should use the following pattern, in which identical and offsetting
credits and debits are created at the same time:

fun withdraw(reservation: &mut Reservation<Foo>, amount: u64, ctx: &TxContext): Foo {
let debit = MergableFoo { value: amount };
let credit = Foo { value: amount };
account::withdraw_from_account(reservation, debit, amount, ctx.sender());
credit
}


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/account.md#0x2_account_withdraw_from_account">withdraw_from_account</a>&lt;T&gt;(reservation: &<b>mut</b> <a href="../sui-framework/account.md#0x2_account_Reservation">account::Reservation</a>&lt;T&gt;, debit: T, amount: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/account.md#0x2_account_withdraw_from_account">withdraw_from_account</a>&lt;T&gt;(
    // reservation proves that the <a href="../sui-framework/account.md#0x2_account">account</a> <b>has</b> enough funds, and that the withdrawal
    // is authorized
    reservation: &<b>mut</b> <a href="../sui-framework/account.md#0x2_account_Reservation">Reservation</a>&lt;T&gt;,
    // debit is a typed wrapper around `amount`. It must contain the same value
    // <b>as</b> is stored in `amount`. iiuc we should be able <b>to</b> remove this duplication when
    // signatures are available.
    debit: T,
    amount: u64,
) {
    // Conservation: aborts <b>if</b> reservation is insufficient
    reservation.<a href="../sui-framework/account.md#0x2_account_decrement">decrement</a>(amount);
    <b>let</b> account_address = <a href="../sui-framework/account.md#0x2_account_get_account_field_address">get_account_field_address</a>&lt;T&gt;(reservation.owner);
    // Conservation:
    // - `debit` will be subtracted from the <a href="../sui-framework/account.md#0x2_account">account</a>
    // - No new reservations will be issued without taking into <a href="../sui-framework/account.md#0x2_account">account</a> the debit.
    <a href="../sui-framework/account.md#0x2_account_split_from_account">split_from_account</a>(debit, account_address);
}
</code></pre>



</details>

<a name="0x2_account_transfer_to_account"></a>

## Function `transfer_to_account`

Transfer a value to an account.

TODO: requires move verifier changes (analagous to the <code><a href="../sui-framework/transfer.md#0x2_transfer">transfer</a></code> checks) that ensures that
this can only be called from the module in which T is defined. Modules can implement secure
accounts without this by using a private type that cannot be constructed outside the module.

Because types must explicitly implement a conversion from their ordinary type to a mergable
type (i.e. one made only of types defined in mergable.move), there is no need for an analogue
to <code>public_transfer</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/account.md#0x2_account_transfer_to_account">transfer_to_account</a>&lt;T&gt;(deposit: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui-framework/account.md#0x2_account_transfer_to_account">transfer_to_account</a>&lt;T&gt;(deposit: T, recipient: <b>address</b>) {
    // Conservation: deposit is consumed here, and is guaranteed <b>to</b> be merged
    // into the recipient <a href="../sui-framework/account.md#0x2_account">account</a>.
    <b>let</b> account_address = <a href="../sui-framework/account.md#0x2_account_get_account_field_address">get_account_field_address</a>&lt;T&gt;(recipient);
    <a href="../sui-framework/account.md#0x2_account_merge_to_account">merge_to_account</a>(deposit, account_address)
}
</code></pre>



</details>

<a name="0x2_account_merge_to_account"></a>

## Function `merge_to_account`



<pre><code><b>fun</b> <a href="../sui-framework/account.md#0x2_account_merge_to_account">merge_to_account</a>&lt;T&gt;(value: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/account.md#0x2_account_merge_to_account">merge_to_account</a>&lt;T&gt;(value: T, recipient: <b>address</b>) {
    <a href="../sui-framework/account.md#0x2_account_emit_account_event">emit_account_event</a>(<a href="../sui-framework/account.md#0x2_account_Merge">Merge</a> { <b>address</b>: recipient, value });
}
</code></pre>



</details>

<a name="0x2_account_split_from_account"></a>

## Function `split_from_account`



<pre><code><b>fun</b> <a href="../sui-framework/account.md#0x2_account_split_from_account">split_from_account</a>&lt;T&gt;(value: T, holder: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui-framework/account.md#0x2_account_split_from_account">split_from_account</a>&lt;T&gt;(value: T, holder: <b>address</b>) {
    <a href="../sui-framework/account.md#0x2_account_emit_account_event">emit_account_event</a>(<a href="../sui-framework/account.md#0x2_account_Split">Split</a> { <b>address</b>: holder, value });
}
</code></pre>



</details>

<a name="0x2_account_emit_account_event"></a>

## Function `emit_account_event`

TODO: this must abort if <code>value</code> contains any "naked" primitives - it must be built
solely from types defined in mergable.move


<pre><code><b>fun</b> <a href="../sui-framework/account.md#0x2_account_emit_account_event">emit_account_event</a>&lt;T&gt;(value: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui-framework/account.md#0x2_account_emit_account_event">emit_account_event</a>&lt;T&gt;(value: T);
</code></pre>



</details>
