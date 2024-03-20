
<a name="0x2_token"></a>

# Module `0x2::token`

The Token module which implements a Closed Loop Token with a configurable
policy. The policy is defined by a set of rules that must be satisfied for
an action to be performed on the token.

The module is designed to be used with a <code>TreasuryCap</code> to allow for minting
and burning of the <code><a href="token.md#0x2_token_Token">Token</a></code>s. And can act as a replacement / extension or a
companion to existing open-loop (<code>Coin</code>) systems.

```
Module:      sui::balance       sui::coin             sui::token
Main type:   Balance<T>         Coin<T>               Token<T>
Capability:  Supply<T>  <---->  TreasuryCap<T> <----> TreasuryCap<T>
Abilities:   store              key + store           key
```

The Token system allows for fine-grained control over the actions performed
on the token. And hence it is highly suitable for applications that require
control over the currency which a simple open-loop system can't provide.


-  [Resource `Token`](#0x2_token_Token)
-  [Resource `TokenPolicyCap`](#0x2_token_TokenPolicyCap)
-  [Resource `TokenPolicy`](#0x2_token_TokenPolicy)
-  [Struct `ActionRequest`](#0x2_token_ActionRequest)
-  [Struct `RuleKey`](#0x2_token_RuleKey)
-  [Struct `TokenPolicyCreated`](#0x2_token_TokenPolicyCreated)
-  [Constants](#@Constants_0)
-  [Function `new_policy`](#0x2_token_new_policy)
-  [Function `share_policy`](#0x2_token_share_policy)
-  [Function `transfer`](#0x2_token_transfer)
-  [Function `spend`](#0x2_token_spend)
-  [Function `to_coin`](#0x2_token_to_coin)
-  [Function `from_coin`](#0x2_token_from_coin)
-  [Function `join`](#0x2_token_join)
-  [Function `split`](#0x2_token_split)
-  [Function `zero`](#0x2_token_zero)
-  [Function `destroy_zero`](#0x2_token_destroy_zero)
-  [Function `keep`](#0x2_token_keep)
-  [Function `new_request`](#0x2_token_new_request)
-  [Function `confirm_request`](#0x2_token_confirm_request)
-  [Function `confirm_request_mut`](#0x2_token_confirm_request_mut)
-  [Function `confirm_with_policy_cap`](#0x2_token_confirm_with_policy_cap)
-  [Function `confirm_with_treasury_cap`](#0x2_token_confirm_with_treasury_cap)
-  [Function `add_approval`](#0x2_token_add_approval)
-  [Function `add_rule_config`](#0x2_token_add_rule_config)
-  [Function `rule_config`](#0x2_token_rule_config)
-  [Function `rule_config_mut`](#0x2_token_rule_config_mut)
-  [Function `remove_rule_config`](#0x2_token_remove_rule_config)
-  [Function `has_rule_config`](#0x2_token_has_rule_config)
-  [Function `has_rule_config_with_type`](#0x2_token_has_rule_config_with_type)
-  [Function `allow`](#0x2_token_allow)
-  [Function `disallow`](#0x2_token_disallow)
-  [Function `add_rule_for_action`](#0x2_token_add_rule_for_action)
-  [Function `remove_rule_for_action`](#0x2_token_remove_rule_for_action)
-  [Function `mint`](#0x2_token_mint)
-  [Function `burn`](#0x2_token_burn)
-  [Function `flush`](#0x2_token_flush)
-  [Function `is_allowed`](#0x2_token_is_allowed)
-  [Function `rules`](#0x2_token_rules)
-  [Function `spent_balance`](#0x2_token_spent_balance)
-  [Function `value`](#0x2_token_value)
-  [Function `transfer_action`](#0x2_token_transfer_action)
-  [Function `spend_action`](#0x2_token_spend_action)
-  [Function `to_coin_action`](#0x2_token_to_coin_action)
-  [Function `from_coin_action`](#0x2_token_from_coin_action)
-  [Function `action`](#0x2_token_action)
-  [Function `amount`](#0x2_token_amount)
-  [Function `sender`](#0x2_token_sender)
-  [Function `recipient`](#0x2_token_recipient)
-  [Function `approvals`](#0x2_token_approvals)
-  [Function `spent`](#0x2_token_spent)
-  [Function `key`](#0x2_token_key)


<pre><code><b>use</b> <a href="dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="dependencies/move-stdlib/string.md#0x1_string">0x1::string</a>;
<b>use</b> <a href="dependencies/move-stdlib/type_name.md#0x1_type_name">0x1::type_name</a>;
<b>use</b> <a href="balance.md#0x2_balance">0x2::balance</a>;
<b>use</b> <a href="coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="vec_map.md#0x2_vec_map">0x2::vec_map</a>;
<b>use</b> <a href="vec_set.md#0x2_vec_set">0x2::vec_set</a>;
</code></pre>



<a name="0x2_token_Token"></a>

## Resource `Token`

A single <code><a href="token.md#0x2_token_Token">Token</a></code> with <code>Balance</code> inside. Can only be owned by an address,
and actions performed on it must be confirmed in a matching <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>struct</b> <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt; <b>has</b> key
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
 The Balance of the <code><a href="token.md#0x2_token_Token">Token</a></code>.
</dd>
</dl>


</details>

<a name="0x2_token_TokenPolicyCap"></a>

## Resource `TokenPolicyCap`

A Capability that manages a single <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> specified in the <code>for</code>
field. Created together with <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> in the <code>new</code> function.


<pre><code><b>struct</b> <a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt; <b>has</b> store, key
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
<code>for: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_token_TokenPolicy"></a>

## Resource `TokenPolicy`

<code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> represents a set of rules that define what actions can be
performed on a <code><a href="token.md#0x2_token_Token">Token</a></code> and which <code>Rules</code> must be satisfied for the
action to succeed.

- For the sake of availability, <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> is a <code>key</code>-only object.
- Each <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> is managed by a matching <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code>.
- For an action to become available, there needs to be a record in the
<code>rules</code> VecMap. To allow an action to be performed freely, there's an
<code>allow</code> function that can be called by the <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code> owner.


<pre><code><b>struct</b> <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt; <b>has</b> key
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
<code>spent_balance: <a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>
 The balance that is effectively spent by the user on the "spend"
 action. However, actual decrease of the supply can only be done by
 the <code>TreasuryCap</code> owner when <code>flush</code> is called.

 This balance is effectively spent and cannot be accessed by anyone
 but the <code>TreasuryCap</code> owner.
</dd>
<dt>
<code>rules: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<a href="dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>&gt;&gt;</code>
</dt>
<dd>
 The set of rules that define what actions can be performed on the
 token. For each "action" there's a set of Rules that must be
 satisfied for the <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code> to be confirmed.
</dd>
</dl>


</details>

<a name="0x2_token_ActionRequest"></a>

## Struct `ActionRequest`

A request to perform an "Action" on a token. Stores the information
about the action to be performed and must be consumed by the <code>confirm_request</code>
or <code>confirm_request_mut</code> functions when the Rules are satisfied.


<pre><code><b>struct</b> <a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>name: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a></code>
</dt>
<dd>
 Name of the Action to look up in the Policy. Name can be one of the
 default actions: <code><a href="transfer.md#0x2_transfer">transfer</a></code>, <code>spend</code>, <code>to_coin</code>, <code>from_coin</code> or a
 custom action.
</dd>
<dt>
<code>amount: u64</code>
</dt>
<dd>
 Amount is present in all of the txs
</dd>
<dt>
<code>sender: <b>address</b></code>
</dt>
<dd>
 Sender is a permanent field always
</dd>
<dt>
<code>recipient: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>
 Recipient is only available in <code><a href="transfer.md#0x2_transfer">transfer</a></code> action.
</dd>
<dt>
<code>spent_balance: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;&gt;</code>
</dt>
<dd>
 The balance to be "spent" in the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>, only available
 in the <code>spend</code> action.
</dd>
<dt>
<code>approvals: <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<a href="dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>&gt;</code>
</dt>
<dd>
 Collected approvals (stamps) from completed <code>Rules</code>. They're matched
 against <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>.rules</code> to determine if the request can be
 confirmed.
</dd>
</dl>


</details>

<a name="0x2_token_RuleKey"></a>

## Struct `RuleKey`

Dynamic field key for the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> to store the <code>Config</code> for a
specific action <code>Rule</code>. There can be only one configuration per
<code>Rule</code> per <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>struct</b> <a href="token.md#0x2_token_RuleKey">RuleKey</a>&lt;T&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>is_protected: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_token_TokenPolicyCreated"></a>

## Struct `TokenPolicyCreated`

An event emitted when a <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> is created and shared. Because
<code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> can only be shared (and potentially frozen in the future),
we emit this event in the <code>share_policy</code> function and mark it as mutable.


<pre><code><b>struct</b> <a href="token.md#0x2_token_TokenPolicyCreated">TokenPolicyCreated</a>&lt;T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>
 ID of the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> that was created.
</dd>
<dt>
<code>is_mutable: bool</code>
</dt>
<dd>
 Whether the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> is "shared" (mutable) or "frozen"
 (immutable) - TBD.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_token_ENotAuthorized"></a>

Trying to perform an admin action with a wrong cap.


<pre><code><b>const</b> <a href="token.md#0x2_token_ENotAuthorized">ENotAuthorized</a>: u64 = 2;
</code></pre>



<a name="0x2_token_EBalanceTooLow"></a>

The balance is too low to perform the action.


<pre><code><b>const</b> <a href="token.md#0x2_token_EBalanceTooLow">EBalanceTooLow</a>: u64 = 3;
</code></pre>



<a name="0x2_token_ECantConsumeBalance"></a>

The balance is not zero when trying to confirm with <code>TransferPolicyCap</code>.


<pre><code><b>const</b> <a href="token.md#0x2_token_ECantConsumeBalance">ECantConsumeBalance</a>: u64 = 5;
</code></pre>



<a name="0x2_token_ENoConfig"></a>

Rule is trying to access a missing config (with type).


<pre><code><b>const</b> <a href="token.md#0x2_token_ENoConfig">ENoConfig</a>: u64 = 6;
</code></pre>



<a name="0x2_token_ENotApproved"></a>

The rule was not approved.


<pre><code><b>const</b> <a href="token.md#0x2_token_ENotApproved">ENotApproved</a>: u64 = 1;
</code></pre>



<a name="0x2_token_ENotZero"></a>

The balance is not zero.


<pre><code><b>const</b> <a href="token.md#0x2_token_ENotZero">ENotZero</a>: u64 = 4;
</code></pre>



<a name="0x2_token_EUnknownAction"></a>

The action is not allowed (defined) in the policy.


<pre><code><b>const</b> <a href="token.md#0x2_token_EUnknownAction">EUnknownAction</a>: u64 = 0;
</code></pre>



<a name="0x2_token_EUseImmutableConfirm"></a>

Using <code>confirm_request_mut</code> without <code>spent_balance</code>. Immutable version
of the function must be used instead.


<pre><code><b>const</b> <a href="token.md#0x2_token_EUseImmutableConfirm">EUseImmutableConfirm</a>: u64 = 7;
</code></pre>



<a name="0x2_token_FROM_COIN"></a>

A Tag for the <code>from_coin</code> action.


<pre><code><b>const</b> <a href="token.md#0x2_token_FROM_COIN">FROM_COIN</a>: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; = [102, 114, 111, 109, 95, 99, 111, 105, 110];
</code></pre>



<a name="0x2_token_SPEND"></a>

A Tag for the <code>spend</code> action.


<pre><code><b>const</b> <a href="token.md#0x2_token_SPEND">SPEND</a>: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; = [115, 112, 101, 110, 100];
</code></pre>



<a name="0x2_token_TO_COIN"></a>

A Tag for the <code>to_coin</code> action.


<pre><code><b>const</b> <a href="token.md#0x2_token_TO_COIN">TO_COIN</a>: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; = [116, 111, 95, 99, 111, 105, 110];
</code></pre>



<a name="0x2_token_TRANSFER"></a>

A Tag for the <code><a href="transfer.md#0x2_transfer">transfer</a></code> action.


<pre><code><b>const</b> <a href="token.md#0x2_token_TRANSFER">TRANSFER</a>: <a href="dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt; = [116, 114, 97, 110, 115, 102, 101, 114];
</code></pre>



<a name="0x2_token_new_policy"></a>

## Function `new_policy`

Create a new <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> and a matching <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code>.
The <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> must then be shared using the <code>share_policy</code> method.

<code>TreasuryCap</code> guarantees full ownership over the currency, and is unique,
hence it is safe to use it for authorization.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_new_policy">new_policy</a>&lt;T&gt;(_treasury_cap: &<a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, <a href="token.md#0x2_token_TokenPolicyCap">token::TokenPolicyCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_new_policy">new_policy</a>&lt;T&gt;(
    _treasury_cap: &TreasuryCap&lt;T&gt;, ctx: &<b>mut</b> TxContext
): (<a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;, <a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;) {
    <b>let</b> policy = <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        spent_balance: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
        rules: <a href="vec_map.md#0x2_vec_map_empty">vec_map::empty</a>()
    };

    <b>let</b> cap = <a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        for: <a href="object.md#0x2_object_id">object::id</a>(&policy)
    };

    (policy, cap)
}
</code></pre>



</details>

<a name="0x2_token_share_policy"></a>

## Function `share_policy`

Share the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>. Due to <code>key</code>-only restriction, it must be
shared after initialization.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_share_policy">share_policy</a>&lt;T&gt;(policy: <a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_share_policy">share_policy</a>&lt;T&gt;(policy: <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;) {
    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="token.md#0x2_token_TokenPolicyCreated">TokenPolicyCreated</a>&lt;T&gt; {
        id: <a href="object.md#0x2_object_id">object::id</a>(&policy),
        is_mutable: <b>true</b>,
    });

    <a href="transfer.md#0x2_transfer_share_object">transfer::share_object</a>(policy)
}
</code></pre>



</details>

<a name="0x2_token_transfer"></a>

## Function `transfer`

Transfer a <code><a href="token.md#0x2_token_Token">Token</a></code> to a <code>recipient</code>. Creates an <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code> for the
"transfer" action. The <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code> contains the <code>recipient</code> field
to be used in verification.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>&lt;T&gt;(t: <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;, recipient: <b>address</b>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>&lt;T&gt;(
    t: <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt;, recipient: <b>address</b>, ctx: &<b>mut</b> TxContext
): <a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt; {
    <b>let</b> amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&t.<a href="balance.md#0x2_balance">balance</a>);
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(t, recipient);

    <a href="token.md#0x2_token_new_request">new_request</a>(
        <a href="token.md#0x2_token_transfer_action">transfer_action</a>(),
        amount,
        <a href="dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(recipient),
        <a href="dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>(),
        ctx
    )
}
</code></pre>



</details>

<a name="0x2_token_spend"></a>

## Function `spend`

Spend a <code><a href="token.md#0x2_token_Token">Token</a></code> by unwrapping it and storing the <code>Balance</code> in the
<code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code> for the "spend" action. The <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code> contains
the <code>spent_balance</code> field to be used in verification.

Spend action requires <code>confirm_request_mut</code> to be called to confirm the
request and join the spent balance with the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>.spent_balance</code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_spend">spend</a>&lt;T&gt;(t: <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_spend">spend</a>&lt;T&gt;(t: <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt;, ctx: &<b>mut</b> TxContext): <a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt; {
    <b>let</b> <a href="token.md#0x2_token_Token">Token</a> { id, <a href="balance.md#0x2_balance">balance</a> } = t;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);

    <a href="token.md#0x2_token_new_request">new_request</a>(
        <a href="token.md#0x2_token_spend_action">spend_action</a>(),
        <a href="balance.md#0x2_balance_value">balance::value</a>(&<a href="balance.md#0x2_balance">balance</a>),
        <a href="dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>(),
        <a href="dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(<a href="balance.md#0x2_balance">balance</a>),
        ctx
    )
}
</code></pre>



</details>

<a name="0x2_token_to_coin"></a>

## Function `to_coin`

Convert <code><a href="token.md#0x2_token_Token">Token</a></code> into an open <code>Coin</code>. Creates an <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code> for the
"to_coin" action.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_to_coin">to_coin</a>&lt;T&gt;(t: <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, <a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_to_coin">to_coin</a>&lt;T&gt;(
    t: <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt;, ctx: &<b>mut</b> TxContext
): (Coin&lt;T&gt;, <a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;) {
    <b>let</b> <a href="token.md#0x2_token_Token">Token</a> { id, <a href="balance.md#0x2_balance">balance</a> } = t;
    <b>let</b> amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&<a href="balance.md#0x2_balance">balance</a>);
    <a href="object.md#0x2_object_delete">object::delete</a>(id);

    (
        <a href="coin.md#0x2_coin_from_balance">coin::from_balance</a>(<a href="balance.md#0x2_balance">balance</a>, ctx),
        <a href="token.md#0x2_token_new_request">new_request</a>(
            <a href="token.md#0x2_token_to_coin_action">to_coin_action</a>(),
            amount,
            <a href="dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>(),
            <a href="dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>(),
            ctx
        )
    )
}
</code></pre>



</details>

<a name="0x2_token_from_coin"></a>

## Function `from_coin`

Convert an open <code>Coin</code> into a <code><a href="token.md#0x2_token_Token">Token</a></code>. Creates an <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code> for
the "from_coin" action.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_from_coin">from_coin</a>&lt;T&gt;(<a href="coin.md#0x2_coin">coin</a>: <a href="coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;, <a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_from_coin">from_coin</a>&lt;T&gt;(
    <a href="coin.md#0x2_coin">coin</a>: Coin&lt;T&gt;, ctx: &<b>mut</b> TxContext
): (<a href="token.md#0x2_token_Token">Token</a>&lt;T&gt;, <a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;) {
    <b>let</b> amount = <a href="coin.md#0x2_coin_value">coin::value</a>(&<a href="coin.md#0x2_coin">coin</a>);
    <b>let</b> <a href="token.md#0x2_token">token</a> = <a href="token.md#0x2_token_Token">Token</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        <a href="balance.md#0x2_balance">balance</a>: <a href="coin.md#0x2_coin_into_balance">coin::into_balance</a>(<a href="coin.md#0x2_coin">coin</a>)
    };

    (
        <a href="token.md#0x2_token">token</a>,
        <a href="token.md#0x2_token_new_request">new_request</a>(
            <a href="token.md#0x2_token_from_coin_action">from_coin_action</a>(),
            amount,
            <a href="dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>(),
            <a href="dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>(),
            ctx
        )
    )
}
</code></pre>



</details>

<a name="0x2_token_join"></a>

## Function `join`

Join two <code><a href="token.md#0x2_token_Token">Token</a></code>s into one, always available.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_join">join</a>&lt;T&gt;(<a href="token.md#0x2_token">token</a>: &<b>mut</b> <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;, another: <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_join">join</a>&lt;T&gt;(<a href="token.md#0x2_token">token</a>: &<b>mut</b> <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt;, another: <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt;) {
    <b>let</b> <a href="token.md#0x2_token_Token">Token</a> { id, <a href="balance.md#0x2_balance">balance</a> } = another;
    <a href="balance.md#0x2_balance_join">balance::join</a>(&<b>mut</b> <a href="token.md#0x2_token">token</a>.<a href="balance.md#0x2_balance">balance</a>, <a href="balance.md#0x2_balance">balance</a>);
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
}
</code></pre>



</details>

<a name="0x2_token_split"></a>

## Function `split`

Split a <code><a href="token.md#0x2_token_Token">Token</a></code> with <code>amount</code>.
Aborts if the <code><a href="token.md#0x2_token_Token">Token</a>.<a href="balance.md#0x2_balance">balance</a></code> is lower than <code>amount</code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_split">split</a>&lt;T&gt;(<a href="token.md#0x2_token">token</a>: &<b>mut</b> <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;, amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_split">split</a>&lt;T&gt;(
    <a href="token.md#0x2_token">token</a>: &<b>mut</b> <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt;, amount: u64, ctx: &<b>mut</b> TxContext
): <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt; {
    <b>assert</b>!(<a href="balance.md#0x2_balance_value">balance::value</a>(&<a href="token.md#0x2_token">token</a>.<a href="balance.md#0x2_balance">balance</a>) &gt;= amount, <a href="token.md#0x2_token_EBalanceTooLow">EBalanceTooLow</a>);
    <a href="token.md#0x2_token_Token">Token</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        <a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> <a href="token.md#0x2_token">token</a>.<a href="balance.md#0x2_balance">balance</a>, amount),
    }
}
</code></pre>



</details>

<a name="0x2_token_zero"></a>

## Function `zero`

Create a zero <code><a href="token.md#0x2_token_Token">Token</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_zero">zero</a>&lt;T&gt;(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_zero">zero</a>&lt;T&gt;(ctx: &<b>mut</b> TxContext): <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt; {
    <a href="token.md#0x2_token_Token">Token</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        <a href="balance.md#0x2_balance">balance</a>: <a href="balance.md#0x2_balance_zero">balance::zero</a>(),
    }
}
</code></pre>



</details>

<a name="0x2_token_destroy_zero"></a>

## Function `destroy_zero`

Destroy an empty <code><a href="token.md#0x2_token_Token">Token</a></code>, fails if the balance is non-zero.
Aborts if the <code><a href="token.md#0x2_token_Token">Token</a>.<a href="balance.md#0x2_balance">balance</a></code> is not zero.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_destroy_zero">destroy_zero</a>&lt;T&gt;(<a href="token.md#0x2_token">token</a>: <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_destroy_zero">destroy_zero</a>&lt;T&gt;(<a href="token.md#0x2_token">token</a>: <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt;) {
    <b>let</b> <a href="token.md#0x2_token_Token">Token</a> { id, <a href="balance.md#0x2_balance">balance</a> } = <a href="token.md#0x2_token">token</a>;
    <b>assert</b>!(<a href="balance.md#0x2_balance_value">balance::value</a>(&<a href="balance.md#0x2_balance">balance</a>) == 0, <a href="token.md#0x2_token_ENotZero">ENotZero</a>);
    <a href="balance.md#0x2_balance_destroy_zero">balance::destroy_zero</a>(<a href="balance.md#0x2_balance">balance</a>);
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
}
</code></pre>



</details>

<a name="0x2_token_keep"></a>

## Function `keep`

Transfer the <code><a href="token.md#0x2_token_Token">Token</a></code> to the transaction sender.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_keep">keep</a>&lt;T&gt;(<a href="token.md#0x2_token">token</a>: <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_keep">keep</a>&lt;T&gt;(<a href="token.md#0x2_token">token</a>: <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt;, ctx: &<b>mut</b> TxContext) {
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="token.md#0x2_token">token</a>, <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx))
}
</code></pre>



</details>

<a name="0x2_token_new_request"></a>

## Function `new_request`

Create a new <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code>.
Publicly available method to allow for custom actions.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_new_request">new_request</a>&lt;T&gt;(name: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, amount: u64, recipient: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<b>address</b>&gt;, spent_balance: <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="balance.md#0x2_balance_Balance">balance::Balance</a>&lt;T&gt;&gt;, ctx: &<a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_new_request">new_request</a>&lt;T&gt;(
    name: String,
    amount: u64,
    recipient: Option&lt;<b>address</b>&gt;,
    spent_balance: Option&lt;Balance&lt;T&gt;&gt;,
    ctx: &TxContext
): <a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt; {
    <a href="token.md#0x2_token_ActionRequest">ActionRequest</a> {
        name,
        amount,
        recipient,
        spent_balance,
        sender: <a href="tx_context.md#0x2_tx_context_sender">tx_context::sender</a>(ctx),
        approvals: <a href="vec_set.md#0x2_vec_set_empty">vec_set::empty</a>(),
    }
}
</code></pre>



</details>

<a name="0x2_token_confirm_request"></a>

## Function `confirm_request`

Confirm the request against the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> and return the parameters
of the request: (Name, Amount, Sender, Recipient).

Cannot be used for <code>spend</code> and similar actions that deliver <code>spent_balance</code>
to the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>. For those actions use <code>confirm_request_mut</code>.

Aborts if:
- the action is not allowed (missing record in <code>rules</code>)
- action contains <code>spent_balance</code> (use <code>confirm_request_mut</code>)
- the <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code> does not meet the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> rules for the action


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_confirm_request">confirm_request</a>&lt;T&gt;(policy: &<a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, request: <a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, u64, <b>address</b>, <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_confirm_request">confirm_request</a>&lt;T&gt;(
    policy: &<a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    request: <a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext
): (String, u64, <b>address</b>, Option&lt;<b>address</b>&gt;) {
    <b>assert</b>!(<a href="dependencies/move-stdlib/option.md#0x1_option_is_none">option::is_none</a>(&request.spent_balance), <a href="token.md#0x2_token_ECantConsumeBalance">ECantConsumeBalance</a>);
    <b>assert</b>!(<a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&policy.rules, &request.name), <a href="token.md#0x2_token_EUnknownAction">EUnknownAction</a>);

    <b>let</b> <a href="token.md#0x2_token_ActionRequest">ActionRequest</a> {
        name, approvals,
        spent_balance,
        amount, sender, recipient,
    } = request;

    <a href="dependencies/move-stdlib/option.md#0x1_option_destroy_none">option::destroy_none</a>(spent_balance);

    <b>let</b> rules = &<a href="vec_set.md#0x2_vec_set_into_keys">vec_set::into_keys</a>(*<a href="vec_map.md#0x2_vec_map_get">vec_map::get</a>(&policy.rules, &name));
    <b>let</b> rules_len = <a href="dependencies/move-stdlib/vector.md#0x1_vector_length">vector::length</a>(rules);
    <b>let</b> i = 0;

    <b>while</b> (i &lt; rules_len) {
        <b>let</b> rule = <a href="dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(rules, i);
        <b>assert</b>!(<a href="vec_set.md#0x2_vec_set_contains">vec_set::contains</a>(&approvals, rule), <a href="token.md#0x2_token_ENotApproved">ENotApproved</a>);
        i = i + 1;
    };

    (name, amount, sender, recipient)
}
</code></pre>



</details>

<a name="0x2_token_confirm_request_mut"></a>

## Function `confirm_request_mut`

Confirm the request against the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> and return the parameters
of the request: (Name, Amount, Sender, Recipient).

Unlike <code>confirm_request</code> this function requires mutable access to the
<code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> and must be used on <code>spend</code> action. After dealing with the
spent balance it calls <code>confirm_request</code> internally.

See <code>confirm_request</code> for the list of abort conditions.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_confirm_request_mut">confirm_request_mut</a>&lt;T&gt;(policy: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, request: <a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, u64, <b>address</b>, <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_confirm_request_mut">confirm_request_mut</a>&lt;T&gt;(
    policy: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    request: <a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;,
    ctx: &<b>mut</b> TxContext
): (String, u64, <b>address</b>, Option&lt;<b>address</b>&gt;) {
    <b>assert</b>!(<a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&policy.rules, &request.name), <a href="token.md#0x2_token_EUnknownAction">EUnknownAction</a>);
    <b>assert</b>!(<a href="dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&request.spent_balance), <a href="token.md#0x2_token_EUseImmutableConfirm">EUseImmutableConfirm</a>);

    <a href="balance.md#0x2_balance_join">balance::join</a>(
        &<b>mut</b> policy.spent_balance,
        <a href="dependencies/move-stdlib/option.md#0x1_option_extract">option::extract</a>(&<b>mut</b> request.spent_balance)
    );

    <a href="token.md#0x2_token_confirm_request">confirm_request</a>(policy, request, ctx)
}
</code></pre>



</details>

<a name="0x2_token_confirm_with_policy_cap"></a>

## Function `confirm_with_policy_cap`

Confirm an <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code> as the <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code> owner. This function
allows <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> owner to perform Capability-gated actions ignoring
the ruleset specified in the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.

Aborts if request contains <code>spent_balance</code> due to inability of the
<code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code> to decrease supply. For scenarios like this a
<code>TreasuryCap</code> is required (see <code>confirm_with_treasury_cap</code>).


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_confirm_with_policy_cap">confirm_with_policy_cap</a>&lt;T&gt;(_policy_cap: &<a href="token.md#0x2_token_TokenPolicyCap">token::TokenPolicyCap</a>&lt;T&gt;, request: <a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, u64, <b>address</b>, <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_confirm_with_policy_cap">confirm_with_policy_cap</a>&lt;T&gt;(
    _policy_cap: &<a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    request: <a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext
): (String, u64, <b>address</b>, Option&lt;<b>address</b>&gt;) {
    <b>assert</b>!(<a href="dependencies/move-stdlib/option.md#0x1_option_is_none">option::is_none</a>(&request.spent_balance), <a href="token.md#0x2_token_ECantConsumeBalance">ECantConsumeBalance</a>);

    <b>let</b> <a href="token.md#0x2_token_ActionRequest">ActionRequest</a> {
        name, amount, sender, recipient, approvals: _, spent_balance
    } = request;

    <a href="dependencies/move-stdlib/option.md#0x1_option_destroy_none">option::destroy_none</a>(spent_balance);

    (name, amount, sender, recipient)
}
</code></pre>



</details>

<a name="0x2_token_confirm_with_treasury_cap"></a>

## Function `confirm_with_treasury_cap`

Confirm an <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code> as the <code>TreasuryCap</code> owner. This function
allows <code>TreasuryCap</code> owner to perform Capability-gated actions ignoring
the ruleset specified in the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.

Unlike <code>confirm_with_policy_cap</code> this function allows <code>spent_balance</code>
to be consumed, decreasing the <code>total_supply</code> of the <code><a href="token.md#0x2_token_Token">Token</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_confirm_with_treasury_cap">confirm_with_treasury_cap</a>&lt;T&gt;(treasury_cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, request: <a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, u64, <b>address</b>, <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_confirm_with_treasury_cap">confirm_with_treasury_cap</a>&lt;T&gt;(
    treasury_cap: &<b>mut</b> TreasuryCap&lt;T&gt;,
    request: <a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext
): (String, u64, <b>address</b>, Option&lt;<b>address</b>&gt;) {
    <b>let</b> <a href="token.md#0x2_token_ActionRequest">ActionRequest</a> {
        name, amount, sender, recipient, approvals: _,
        spent_balance
    } = request;

    <b>if</b> (<a href="dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&spent_balance)) {
        <a href="balance.md#0x2_balance_decrease_supply">balance::decrease_supply</a>(
            <a href="coin.md#0x2_coin_supply_mut">coin::supply_mut</a>(treasury_cap),
            <a href="dependencies/move-stdlib/option.md#0x1_option_destroy_some">option::destroy_some</a>(spent_balance)
        );
    } <b>else</b> {
        <a href="dependencies/move-stdlib/option.md#0x1_option_destroy_none">option::destroy_none</a>(spent_balance);
    };

    (name, amount, sender, recipient)
}
</code></pre>



</details>

<a name="0x2_token_add_approval"></a>

## Function `add_approval`

Add an "approval" to the <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code> by providing a Witness.
Intended to be used by Rules to add their own approvals, however, can
be used to add arbitrary approvals to the request (not only the ones
required by the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>).


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_add_approval">add_approval</a>&lt;T, W: drop&gt;(_t: W, request: &<b>mut</b> <a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_add_approval">add_approval</a>&lt;T, W: drop&gt;(
    _t: W, request: &<b>mut</b> <a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;, _ctx: &<b>mut</b> TxContext
) {
    <a href="vec_set.md#0x2_vec_set_insert">vec_set::insert</a>(&<b>mut</b> request.approvals, <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;W&gt;())
}
</code></pre>



</details>

<a name="0x2_token_add_rule_config"></a>

## Function `add_rule_config`

Add a <code>Config</code> for a <code>Rule</code> in the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>. Rule configuration is
independent from the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>.rules</code> and needs to be managed by the
Rule itself. Configuration is stored per <code>Rule</code> and not per <code>Rule</code> per
<code>Action</code> to allow reuse in different actions.

- Rule witness guarantees that the <code>Config</code> is approved by the Rule.
- <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code> guarantees that the <code>Config</code> setup is initiated by
the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> owner.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_add_rule_config">add_rule_config</a>&lt;T, Rule: drop, Config: store&gt;(_rule: Rule, self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="token.md#0x2_token_TokenPolicyCap">token::TokenPolicyCap</a>&lt;T&gt;, config: Config, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_add_rule_config">add_rule_config</a>&lt;T, Rule: drop, Config: store&gt;(
    _rule: Rule,
    self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    config: Config,
    _ctx: &<b>mut</b> TxContext
) {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="token.md#0x2_token_ENotAuthorized">ENotAuthorized</a>);
    df::add(&<b>mut</b> self.id, <a href="token.md#0x2_token_key">key</a>&lt;Rule&gt;(), config)
}
</code></pre>



</details>

<a name="0x2_token_rule_config"></a>

## Function `rule_config`

Get a <code>Config</code> for a <code>Rule</code> in the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>. Requires <code>Rule</code>
witness, hence can only be read by the <code>Rule</code> itself. This requirement
guarantees safety of the stored <code>Config</code> and allows for simpler dynamic
field management inside the Rule Config (custom type keys are not needed
for access gating).

Aborts if the Config is not present.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_rule_config">rule_config</a>&lt;T, Rule: drop, Config: store&gt;(_rule: Rule, self: &<a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;): &Config
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_rule_config">rule_config</a>&lt;T, Rule: drop, Config: store&gt;(
    _rule: Rule, self: &<a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;
): &Config {
    <b>assert</b>!(<a href="token.md#0x2_token_has_rule_config_with_type">has_rule_config_with_type</a>&lt;T, Rule, Config&gt;(self), <a href="token.md#0x2_token_ENoConfig">ENoConfig</a>);
    df::borrow(&self.id, <a href="token.md#0x2_token_key">key</a>&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="0x2_token_rule_config_mut"></a>

## Function `rule_config_mut`

Get mutable access to the <code>Config</code> for a <code>Rule</code> in the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.
Requires <code>Rule</code> witness, hence can only be read by the <code>Rule</code> itself,
as well as <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code> to guarantee that the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> owner
is the one who initiated the <code>Config</code> modification.

Aborts if:
- the Config is not present
- <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_rule_config_mut">rule_config_mut</a>&lt;T, Rule: drop, Config: store&gt;(_rule: Rule, self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="token.md#0x2_token_TokenPolicyCap">token::TokenPolicyCap</a>&lt;T&gt;): &<b>mut</b> Config
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_rule_config_mut">rule_config_mut</a>&lt;T, Rule: drop, Config: store&gt;(
    _rule: Rule, self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;, cap: &<a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;
): &<b>mut</b> Config {
    <b>assert</b>!(<a href="token.md#0x2_token_has_rule_config_with_type">has_rule_config_with_type</a>&lt;T, Rule, Config&gt;(self), <a href="token.md#0x2_token_ENoConfig">ENoConfig</a>);
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="token.md#0x2_token_ENotAuthorized">ENotAuthorized</a>);
    df::borrow_mut(&<b>mut</b> self.id, <a href="token.md#0x2_token_key">key</a>&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="0x2_token_remove_rule_config"></a>

## Function `remove_rule_config`

Remove a <code>Config</code> for a <code>Rule</code> in the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.
Unlike the <code>add_rule_config</code>, this function does not require a <code>Rule</code>
witness, hence can be performed by the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> owner on their own.

Rules need to make sure that the <code>Config</code> is present when performing
verification of the <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code>.

Aborts if:
- the Config is not present
- <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_remove_rule_config">remove_rule_config</a>&lt;T, Rule, Config: store&gt;(self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="token.md#0x2_token_TokenPolicyCap">token::TokenPolicyCap</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): Config
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_remove_rule_config">remove_rule_config</a>&lt;T, Rule, Config: store&gt;(
    self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext
): Config {
    <b>assert</b>!(<a href="token.md#0x2_token_has_rule_config_with_type">has_rule_config_with_type</a>&lt;T, Rule, Config&gt;(self), <a href="token.md#0x2_token_ENoConfig">ENoConfig</a>);
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="token.md#0x2_token_ENotAuthorized">ENotAuthorized</a>);
    df::remove(&<b>mut</b> self.id, <a href="token.md#0x2_token_key">key</a>&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="0x2_token_has_rule_config"></a>

## Function `has_rule_config`

Check if a config for a <code>Rule</code> is set in the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> without
checking the type of the <code>Config</code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_has_rule_config">has_rule_config</a>&lt;T, Rule&gt;(self: &<a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_has_rule_config">has_rule_config</a>&lt;T, Rule&gt;(self: &<a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;): bool {
    df::exists_&lt;<a href="token.md#0x2_token_RuleKey">RuleKey</a>&lt;Rule&gt;&gt;(&self.id, <a href="token.md#0x2_token_key">key</a>&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="0x2_token_has_rule_config_with_type"></a>

## Function `has_rule_config_with_type`

Check if a <code>Config</code> for a <code>Rule</code> is set in the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code> and that
it matches the type provided.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_has_rule_config_with_type">has_rule_config_with_type</a>&lt;T, Rule, Config: store&gt;(self: &<a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_has_rule_config_with_type">has_rule_config_with_type</a>&lt;T, Rule, Config: store&gt;(
    self: &<a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;
): bool {
    df::exists_with_type&lt;<a href="token.md#0x2_token_RuleKey">RuleKey</a>&lt;Rule&gt;, Config&gt;(&self.id, <a href="token.md#0x2_token_key">key</a>&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="0x2_token_allow"></a>

## Function `allow`

Allows an <code>action</code> to be performed on the <code><a href="token.md#0x2_token_Token">Token</a></code> freely by adding an
empty set of <code>Rules</code> for the <code>action</code>.

Aborts if the <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_allow">allow</a>&lt;T&gt;(self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="token.md#0x2_token_TokenPolicyCap">token::TokenPolicyCap</a>&lt;T&gt;, action: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_allow">allow</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    action: String,
    _ctx: &<b>mut</b> TxContext
) {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="token.md#0x2_token_ENotAuthorized">ENotAuthorized</a>);
    <a href="vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> self.rules, action, <a href="vec_set.md#0x2_vec_set_empty">vec_set::empty</a>());
}
</code></pre>



</details>

<a name="0x2_token_disallow"></a>

## Function `disallow`

Completely disallows an <code>action</code> on the <code><a href="token.md#0x2_token_Token">Token</a></code> by removing the record
from the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>.rules</code>.

Aborts if the <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_disallow">disallow</a>&lt;T&gt;(self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="token.md#0x2_token_TokenPolicyCap">token::TokenPolicyCap</a>&lt;T&gt;, action: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_disallow">disallow</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    action: String,
    _ctx: &<b>mut</b> TxContext
) {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="token.md#0x2_token_ENotAuthorized">ENotAuthorized</a>);
    <a href="vec_map.md#0x2_vec_map_remove">vec_map::remove</a>(&<b>mut</b> self.rules, &action);
}
</code></pre>



</details>

<a name="0x2_token_add_rule_for_action"></a>

## Function `add_rule_for_action`

Adds a Rule for an action with <code>name</code> in the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.

Aborts if the <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_add_rule_for_action">add_rule_for_action</a>&lt;T, Rule: drop&gt;(self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="token.md#0x2_token_TokenPolicyCap">token::TokenPolicyCap</a>&lt;T&gt;, action: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_add_rule_for_action">add_rule_for_action</a>&lt;T, Rule: drop&gt;(
    self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    action: String,
    ctx: &<b>mut</b> TxContext
) {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="token.md#0x2_token_ENotAuthorized">ENotAuthorized</a>);
    <b>if</b> (!<a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&self.rules, &action)) {
        <a href="token.md#0x2_token_allow">allow</a>(self, cap, action, ctx);
    };

    <a href="vec_set.md#0x2_vec_set_insert">vec_set::insert</a>(
        <a href="vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.rules, &action),
        <a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;Rule&gt;()
    )
}
</code></pre>



</details>

<a name="0x2_token_remove_rule_for_action"></a>

## Function `remove_rule_for_action`

Removes a rule for an action with <code>name</code> in the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>. Returns
the config object to be handled by the sender (or a Rule itself).

Aborts if the <code><a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_remove_rule_for_action">remove_rule_for_action</a>&lt;T, Rule: drop&gt;(self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="token.md#0x2_token_TokenPolicyCap">token::TokenPolicyCap</a>&lt;T&gt;, action: <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_remove_rule_for_action">remove_rule_for_action</a>&lt;T, Rule: drop&gt;(
    self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="token.md#0x2_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    action: String,
    _ctx: &<b>mut</b> TxContext
) {
    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(self) == cap.for, <a href="token.md#0x2_token_ENotAuthorized">ENotAuthorized</a>);

    <a href="vec_set.md#0x2_vec_set_remove">vec_set::remove</a>(
        <a href="vec_map.md#0x2_vec_map_get_mut">vec_map::get_mut</a>(&<b>mut</b> self.rules, &action),
        &<a href="dependencies/move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;Rule&gt;()
    )
}
</code></pre>



</details>

<a name="0x2_token_mint"></a>

## Function `mint`

Mint a <code><a href="token.md#0x2_token_Token">Token</a></code> with a given <code>amount</code> using the <code>TreasuryCap</code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_mint">mint</a>&lt;T&gt;(cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, amount: u64, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_mint">mint</a>&lt;T&gt;(
    cap: &<b>mut</b> TreasuryCap&lt;T&gt;, amount: u64, ctx: &<b>mut</b> TxContext
): <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt; {
    <b>let</b> <a href="balance.md#0x2_balance">balance</a> = <a href="balance.md#0x2_balance_increase_supply">balance::increase_supply</a>(<a href="coin.md#0x2_coin_supply_mut">coin::supply_mut</a>(cap), amount);
    <a href="token.md#0x2_token_Token">Token</a> { id: <a href="object.md#0x2_object_new">object::new</a>(ctx), <a href="balance.md#0x2_balance">balance</a> }
}
</code></pre>



</details>

<a name="0x2_token_burn"></a>

## Function `burn`

Burn a <code><a href="token.md#0x2_token_Token">Token</a></code> using the <code>TreasuryCap</code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_burn">burn</a>&lt;T&gt;(cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, <a href="token.md#0x2_token">token</a>: <a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_burn">burn</a>&lt;T&gt;(cap: &<b>mut</b> TreasuryCap&lt;T&gt;, <a href="token.md#0x2_token">token</a>: <a href="token.md#0x2_token_Token">Token</a>&lt;T&gt;) {
    <b>let</b> <a href="token.md#0x2_token_Token">Token</a> { id, <a href="balance.md#0x2_balance">balance</a> } = <a href="token.md#0x2_token">token</a>;
    <a href="balance.md#0x2_balance_decrease_supply">balance::decrease_supply</a>(<a href="coin.md#0x2_coin_supply_mut">coin::supply_mut</a>(cap), <a href="balance.md#0x2_balance">balance</a>);
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
}
</code></pre>



</details>

<a name="0x2_token_flush"></a>

## Function `flush`

Flush the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>.spent_balance</code> into the <code>TreasuryCap</code>. This
action is only available to the <code>TreasuryCap</code> owner.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_flush">flush</a>&lt;T&gt;(self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, cap: &<b>mut</b> <a href="coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_flush">flush</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<b>mut</b> TreasuryCap&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext
): u64 {
    <b>let</b> amount = <a href="balance.md#0x2_balance_value">balance::value</a>(&self.spent_balance);
    <b>let</b> <a href="balance.md#0x2_balance">balance</a> = <a href="balance.md#0x2_balance_split">balance::split</a>(&<b>mut</b> self.spent_balance, amount);
    <a href="balance.md#0x2_balance_decrease_supply">balance::decrease_supply</a>(<a href="coin.md#0x2_coin_supply_mut">coin::supply_mut</a>(cap), <a href="balance.md#0x2_balance">balance</a>)
}
</code></pre>



</details>

<a name="0x2_token_is_allowed"></a>

## Function `is_allowed`

Check whether an action is present in the rules VecMap.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_is_allowed">is_allowed</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, action: &<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_is_allowed">is_allowed</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;, action: &String): bool {
    <a href="vec_map.md#0x2_vec_map_contains">vec_map::contains</a>(&self.rules, action)
}
</code></pre>



</details>

<a name="0x2_token_rules"></a>

## Function `rules`

Returns the rules required for a specific action.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_rules">rules</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;, action: &<a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>): <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<a href="dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_rules">rules</a>&lt;T&gt;(
    self: &<a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;, action: &String
): VecSet&lt;TypeName&gt; {
    *<a href="vec_map.md#0x2_vec_map_get">vec_map::get</a>(&self.rules, action)
}
</code></pre>



</details>

<a name="0x2_token_spent_balance"></a>

## Function `spent_balance`

Returns the <code>spent_balance</code> of the <code><a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_spent_balance">spent_balance</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_TokenPolicy">token::TokenPolicy</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_spent_balance">spent_balance</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;): u64 {
    <a href="balance.md#0x2_balance_value">balance::value</a>(&self.spent_balance)
}
</code></pre>



</details>

<a name="0x2_token_value"></a>

## Function `value`

Returns the <code><a href="balance.md#0x2_balance">balance</a></code> of the <code><a href="token.md#0x2_token_Token">Token</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_value">value</a>&lt;T&gt;(t: &<a href="token.md#0x2_token_Token">token::Token</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_value">value</a>&lt;T&gt;(t: &<a href="token.md#0x2_token_Token">Token</a>&lt;T&gt;): u64 {
    <a href="balance.md#0x2_balance_value">balance::value</a>(&t.<a href="balance.md#0x2_balance">balance</a>)
}
</code></pre>



</details>

<a name="0x2_token_transfer_action"></a>

## Function `transfer_action`

Name of the Transfer action.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_transfer_action">transfer_action</a>(): <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_transfer_action">transfer_action</a>(): String { <a href="dependencies/move-stdlib/string.md#0x1_string_utf8">string::utf8</a>(<a href="token.md#0x2_token_TRANSFER">TRANSFER</a>) }
</code></pre>



</details>

<a name="0x2_token_spend_action"></a>

## Function `spend_action`

Name of the <code>Spend</code> action.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_spend_action">spend_action</a>(): <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_spend_action">spend_action</a>(): String { <a href="dependencies/move-stdlib/string.md#0x1_string_utf8">string::utf8</a>(<a href="token.md#0x2_token_SPEND">SPEND</a>) }
</code></pre>



</details>

<a name="0x2_token_to_coin_action"></a>

## Function `to_coin_action`

Name of the <code>ToCoin</code> action.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_to_coin_action">to_coin_action</a>(): <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_to_coin_action">to_coin_action</a>(): String { <a href="dependencies/move-stdlib/string.md#0x1_string_utf8">string::utf8</a>(<a href="token.md#0x2_token_TO_COIN">TO_COIN</a>) }
</code></pre>



</details>

<a name="0x2_token_from_coin_action"></a>

## Function `from_coin_action`

Name of the <code>FromCoin</code> action.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_from_coin_action">from_coin_action</a>(): <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_from_coin_action">from_coin_action</a>(): String { <a href="dependencies/move-stdlib/string.md#0x1_string_utf8">string::utf8</a>(<a href="token.md#0x2_token_FROM_COIN">FROM_COIN</a>) }
</code></pre>



</details>

<a name="0x2_token_action"></a>

## Function `action`

The Action in the <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_action">action</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;): <a href="dependencies/move-stdlib/string.md#0x1_string_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_action">action</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;): String { self.name }
</code></pre>



</details>

<a name="0x2_token_amount"></a>

## Function `amount`

Amount of the <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_amount">amount</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_amount">amount</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;): u64 { self.amount }
</code></pre>



</details>

<a name="0x2_token_sender"></a>

## Function `sender`

Sender of the <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_sender">sender</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_sender">sender</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;): <b>address</b> { self.sender }
</code></pre>



</details>

<a name="0x2_token_recipient"></a>

## Function `recipient`

Recipient of the <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_recipient">recipient</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;): <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_recipient">recipient</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;): Option&lt;<b>address</b>&gt; {
    self.recipient
}
</code></pre>



</details>

<a name="0x2_token_approvals"></a>

## Function `approvals`

Approvals of the <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_approvals">approvals</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;): <a href="vec_set.md#0x2_vec_set_VecSet">vec_set::VecSet</a>&lt;<a href="dependencies/move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_approvals">approvals</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;): VecSet&lt;TypeName&gt; {
    self.approvals
}
</code></pre>



</details>

<a name="0x2_token_spent"></a>

## Function `spent`

Burned balance of the <code><a href="token.md#0x2_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_spent">spent</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">token::ActionRequest</a>&lt;T&gt;): <a href="dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="token.md#0x2_token_spent">spent</a>&lt;T&gt;(self: &<a href="token.md#0x2_token_ActionRequest">ActionRequest</a>&lt;T&gt;): Option&lt;u64&gt; {
    <b>if</b> (<a href="dependencies/move-stdlib/option.md#0x1_option_is_some">option::is_some</a>(&self.spent_balance)) {
        <a href="dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(<a href="balance.md#0x2_balance_value">balance::value</a>(<a href="dependencies/move-stdlib/option.md#0x1_option_borrow">option::borrow</a>(&self.spent_balance)))
    } <b>else</b> {
        <a href="dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>()
    }
}
</code></pre>



</details>

<a name="0x2_token_key"></a>

## Function `key`

Create a new <code><a href="token.md#0x2_token_RuleKey">RuleKey</a></code> for a <code>Rule</code>. The <code>is_protected</code> field is kept
for potential future use, if Rules were to have a freely modifiable
storage as addition / replacement for the <code>Config</code> system.

The goal of <code>is_protected</code> is to potentially allow Rules store a mutable
version of their configuration and mutate state on user action.


<pre><code><b>fun</b> <a href="token.md#0x2_token_key">key</a>&lt;Rule&gt;(): <a href="token.md#0x2_token_RuleKey">token::RuleKey</a>&lt;Rule&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="token.md#0x2_token_key">key</a>&lt;Rule&gt;(): <a href="token.md#0x2_token_RuleKey">RuleKey</a>&lt;Rule&gt; { <a href="token.md#0x2_token_RuleKey">RuleKey</a> { is_protected: <b>true</b> } }
</code></pre>



</details>
