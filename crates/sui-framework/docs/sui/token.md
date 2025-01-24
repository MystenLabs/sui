---
title: Module `sui::token`
---

The Token module which implements a Closed Loop Token with a configurable
policy. The policy is defined by a set of rules that must be satisfied for
an action to be performed on the token.

The module is designed to be used with a <code>TreasuryCap</code> to allow for minting
and burning of the <code><a href="../sui/token.md#sui_token_Token">Token</a></code>s. And can act as a replacement / extension or a
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


-  [Struct `Token`](#sui_token_Token)
-  [Struct `TokenPolicyCap`](#sui_token_TokenPolicyCap)
-  [Struct `TokenPolicy`](#sui_token_TokenPolicy)
-  [Struct `ActionRequest`](#sui_token_ActionRequest)
-  [Struct `RuleKey`](#sui_token_RuleKey)
-  [Struct `TokenPolicyCreated`](#sui_token_TokenPolicyCreated)
-  [Constants](#@Constants_0)
-  [Function `new_policy`](#sui_token_new_policy)
-  [Function `share_policy`](#sui_token_share_policy)
-  [Function `transfer`](#sui_token_transfer)
-  [Function `spend`](#sui_token_spend)
-  [Function `to_coin`](#sui_token_to_coin)
-  [Function `from_coin`](#sui_token_from_coin)
-  [Function `join`](#sui_token_join)
-  [Function `split`](#sui_token_split)
-  [Function `zero`](#sui_token_zero)
-  [Function `destroy_zero`](#sui_token_destroy_zero)
-  [Function `keep`](#sui_token_keep)
-  [Function `new_request`](#sui_token_new_request)
-  [Function `confirm_request`](#sui_token_confirm_request)
-  [Function `confirm_request_mut`](#sui_token_confirm_request_mut)
-  [Function `confirm_with_policy_cap`](#sui_token_confirm_with_policy_cap)
-  [Function `confirm_with_treasury_cap`](#sui_token_confirm_with_treasury_cap)
-  [Function `add_approval`](#sui_token_add_approval)
-  [Function `add_rule_config`](#sui_token_add_rule_config)
-  [Function `rule_config`](#sui_token_rule_config)
-  [Function `rule_config_mut`](#sui_token_rule_config_mut)
-  [Function `remove_rule_config`](#sui_token_remove_rule_config)
-  [Function `has_rule_config`](#sui_token_has_rule_config)
-  [Function `has_rule_config_with_type`](#sui_token_has_rule_config_with_type)
-  [Function `allow`](#sui_token_allow)
-  [Function `disallow`](#sui_token_disallow)
-  [Function `add_rule_for_action`](#sui_token_add_rule_for_action)
-  [Function `remove_rule_for_action`](#sui_token_remove_rule_for_action)
-  [Function `mint`](#sui_token_mint)
-  [Function `burn`](#sui_token_burn)
-  [Function `flush`](#sui_token_flush)
-  [Function `is_allowed`](#sui_token_is_allowed)
-  [Function `rules`](#sui_token_rules)
-  [Function `spent_balance`](#sui_token_spent_balance)
-  [Function `value`](#sui_token_value)
-  [Function `transfer_action`](#sui_token_transfer_action)
-  [Function `spend_action`](#sui_token_spend_action)
-  [Function `to_coin_action`](#sui_token_to_coin_action)
-  [Function `from_coin_action`](#sui_token_from_coin_action)
-  [Function `action`](#sui_token_action)
-  [Function `amount`](#sui_token_amount)
-  [Function `sender`](#sui_token_sender)
-  [Function `recipient`](#sui_token_recipient)
-  [Function `approvals`](#sui_token_approvals)
-  [Function `spent`](#sui_token_spent)
-  [Function `key`](#sui_token_key)


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
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="sui_token_Token"></a>

## Struct `Token`

A single <code><a href="../sui/token.md#sui_token_Token">Token</a></code> with <code>Balance</code> inside. Can only be owned by an address,
and actions performed on it must be confirmed in a matching <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/token.md#sui_token_Token">Token</a>&lt;<b>phantom</b> T&gt; <b>has</b> <a href="../sui/token.md#sui_token_key">key</a>
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
<code><a href="../sui/balance.md#sui_balance">balance</a>: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>
 The Balance of the <code><a href="../sui/token.md#sui_token_Token">Token</a></code>.
</dd>
</dl>


</details>

<a name="sui_token_TokenPolicyCap"></a>

## Struct `TokenPolicyCap`

A Capability that manages a single <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> specified in the <code><b>for</b></code>
field. Created together with <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> in the <code>new</code> function.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a>&lt;<b>phantom</b> T&gt; <b>has</b> <a href="../sui/token.md#sui_token_key">key</a>, store
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
<code><b>for</b>: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_token_TokenPolicy"></a>

## Struct `TokenPolicy`

<code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> represents a set of rules that define what actions can be
performed on a <code><a href="../sui/token.md#sui_token_Token">Token</a></code> and which <code>Rules</code> must be satisfied for the
action to succeed.

- For the sake of availability, <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> is a <code><a href="../sui/token.md#sui_token_key">key</a></code>-only object.
- Each <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> is managed by a matching <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code>.
- For an action to become available, there needs to be a record in the
<code><a href="../sui/token.md#sui_token_rules">rules</a></code> VecMap. To allow an action to be performed freely, there's an
<code><a href="../sui/token.md#sui_token_allow">allow</a></code> function that can be called by the <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code> owner.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;<b>phantom</b> T&gt; <b>has</b> <a href="../sui/token.md#sui_token_key">key</a>
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
<code><a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>: <a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;</code>
</dt>
<dd>
 The balance that is effectively spent by the user on the "spend"
 action. However, actual decrease of the supply can only be done by
 the <code>TreasuryCap</code> owner when <code><a href="../sui/token.md#sui_token_flush">flush</a></code> is called.
 This balance is effectively spent and cannot be accessed by anyone
 but the <code>TreasuryCap</code> owner.
</dd>
<dt>
<code><a href="../sui/token.md#sui_token_rules">rules</a>: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>&gt;&gt;</code>
</dt>
<dd>
 The set of rules that define what actions can be performed on the
 token. For each "action" there's a set of Rules that must be
 satisfied for the <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code> to be confirmed.
</dd>
</dl>


</details>

<a name="sui_token_ActionRequest"></a>

## Struct `ActionRequest`

A request to perform an "Action" on a token. Stores the information
about the action to be performed and must be consumed by the <code><a href="../sui/token.md#sui_token_confirm_request">confirm_request</a></code>
or <code><a href="../sui/token.md#sui_token_confirm_request_mut">confirm_request_mut</a></code> functions when the Rules are satisfied.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;<b>phantom</b> T&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>name: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 Name of the Action to look up in the Policy. Name can be one of the
 default actions: <code><a href="../sui/transfer.md#sui_transfer">transfer</a></code>, <code><a href="../sui/token.md#sui_token_spend">spend</a></code>, <code><a href="../sui/token.md#sui_token_to_coin">to_coin</a></code>, <code><a href="../sui/token.md#sui_token_from_coin">from_coin</a></code> or a
 custom action.
</dd>
<dt>
<code><a href="../sui/token.md#sui_token_amount">amount</a>: u64</code>
</dt>
<dd>
 Amount is present in all of the txs
</dd>
<dt>
<code><a href="../sui/token.md#sui_token_sender">sender</a>: <b>address</b></code>
</dt>
<dd>
 Sender is a permanent field always
</dd>
<dt>
<code><a href="../sui/token.md#sui_token_recipient">recipient</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>
 Recipient is only available in <code><a href="../sui/transfer.md#sui_transfer">transfer</a></code> action.
</dd>
<dt>
<code><a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;&gt;</code>
</dt>
<dd>
 The balance to be "spent" in the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>, only available
 in the <code><a href="../sui/token.md#sui_token_spend">spend</a></code> action.
</dd>
<dt>
<code><a href="../sui/token.md#sui_token_approvals">approvals</a>: <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>&gt;</code>
</dt>
<dd>
 Collected approvals (stamps) from completed <code>Rules</code>. They're matched
 against <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>.<a href="../sui/token.md#sui_token_rules">rules</a></code> to determine if the request can be
 confirmed.
</dd>
</dl>


</details>

<a name="sui_token_RuleKey"></a>

## Struct `RuleKey`

Dynamic field key for the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> to store the <code>Config</code> for a
specific action <code>Rule</code>. There can be only one configuration per
<code>Rule</code> per <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/token.md#sui_token_RuleKey">RuleKey</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop, store
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

<a name="sui_token_TokenPolicyCreated"></a>

## Struct `TokenPolicyCreated`

An event emitted when a <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> is created and shared. Because
<code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> can only be shared (and potentially frozen in the future),
we emit this event in the <code><a href="../sui/token.md#sui_token_share_policy">share_policy</a></code> function and mark it as mutable.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/token.md#sui_token_TokenPolicyCreated">TokenPolicyCreated</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 ID of the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> that was created.
</dd>
<dt>
<code>is_mutable: bool</code>
</dt>
<dd>
 Whether the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> is "shared" (mutable) or "frozen"
 (immutable) - TBD.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_token_EBalanceTooLow"></a>

The balance is too low to perform the action.


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_EBalanceTooLow">EBalanceTooLow</a>: u64 = 3;
</code></pre>



<a name="sui_token_ECantConsumeBalance"></a>

The balance is not zero when trying to confirm with <code>TransferPolicyCap</code>.


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_ECantConsumeBalance">ECantConsumeBalance</a>: u64 = 5;
</code></pre>



<a name="sui_token_ENoConfig"></a>

Rule is trying to access a missing config (with type).


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_ENoConfig">ENoConfig</a>: u64 = 6;
</code></pre>



<a name="sui_token_ENotApproved"></a>

The rule was not approved.


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_ENotApproved">ENotApproved</a>: u64 = 1;
</code></pre>



<a name="sui_token_ENotAuthorized"></a>

Trying to perform an admin action with a wrong cap.


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_ENotAuthorized">ENotAuthorized</a>: u64 = 2;
</code></pre>



<a name="sui_token_ENotZero"></a>

The balance is not zero.


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_ENotZero">ENotZero</a>: u64 = 4;
</code></pre>



<a name="sui_token_EUnknownAction"></a>

The action is not allowed (defined) in the policy.


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_EUnknownAction">EUnknownAction</a>: u64 = 0;
</code></pre>



<a name="sui_token_EUseImmutableConfirm"></a>

Using <code><a href="../sui/token.md#sui_token_confirm_request_mut">confirm_request_mut</a></code> without <code><a href="../sui/token.md#sui_token_spent_balance">spent_balance</a></code>. Immutable version
of the function must be used instead.


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_EUseImmutableConfirm">EUseImmutableConfirm</a>: u64 = 7;
</code></pre>



<a name="sui_token_FROM_COIN"></a>

A Tag for the <code><a href="../sui/token.md#sui_token_from_coin">from_coin</a></code> action.


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_FROM_COIN">FROM_COIN</a>: vector&lt;u8&gt; = vector[102, 114, 111, 109, 95, 99, 111, 105, 110];
</code></pre>



<a name="sui_token_SPEND"></a>

A Tag for the <code><a href="../sui/token.md#sui_token_spend">spend</a></code> action.


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_SPEND">SPEND</a>: vector&lt;u8&gt; = vector[115, 112, 101, 110, 100];
</code></pre>



<a name="sui_token_TO_COIN"></a>

A Tag for the <code><a href="../sui/token.md#sui_token_to_coin">to_coin</a></code> action.


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_TO_COIN">TO_COIN</a>: vector&lt;u8&gt; = vector[116, 111, 95, 99, 111, 105, 110];
</code></pre>



<a name="sui_token_TRANSFER"></a>

A Tag for the <code><a href="../sui/transfer.md#sui_transfer">transfer</a></code> action.


<pre><code><b>const</b> <a href="../sui/token.md#sui_token_TRANSFER">TRANSFER</a>: vector&lt;u8&gt; = vector[116, 114, 97, 110, 115, 102, 101, 114];
</code></pre>



<a name="sui_token_new_policy"></a>

## Function `new_policy`

Create a new <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> and a matching <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code>.
The <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> must then be shared using the <code><a href="../sui/token.md#sui_token_share_policy">share_policy</a></code> method.

<code>TreasuryCap</code> guarantees full ownership over the currency, and is unique,
hence it is safe to use it for authorization.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_new_policy">new_policy</a>&lt;T&gt;(_treasury_cap: &<a href="../sui/coin.md#sui_coin_TreasuryCap">sui::coin::TreasuryCap</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_TokenPolicyCap">sui::token::TokenPolicyCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_new_policy">new_policy</a>&lt;T&gt;(
    _treasury_cap: &TreasuryCap&lt;T&gt;,
    ctx: &<b>mut</b> TxContext,
): (<a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;) {
    <b>let</b> policy = <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>: <a href="../sui/balance.md#sui_balance_zero">balance::zero</a>(),
        <a href="../sui/token.md#sui_token_rules">rules</a>: <a href="../sui/vec_map.md#sui_vec_map_empty">vec_map::empty</a>(),
    };
    <b>let</b> cap = <a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        `<b>for</b>`: <a href="../sui/object.md#sui_object_id">object::id</a>(&policy),
    };
    (policy, cap)
}
</code></pre>



</details>

<a name="sui_token_share_policy"></a>

## Function `share_policy`

Share the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>. Due to <code><a href="../sui/token.md#sui_token_key">key</a></code>-only restriction, it must be
shared after initialization.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_share_policy">share_policy</a>&lt;T&gt;(policy: <a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_share_policy">share_policy</a>&lt;T&gt;(policy: <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;) {
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/token.md#sui_token_TokenPolicyCreated">TokenPolicyCreated</a>&lt;T&gt; {
        id: <a href="../sui/object.md#sui_object_id">object::id</a>(&policy),
        is_mutable: <b>true</b>,
    });
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(policy)
}
</code></pre>



</details>

<a name="sui_token_transfer"></a>

## Function `transfer`

Transfer a <code><a href="../sui/token.md#sui_token_Token">Token</a></code> to a <code><a href="../sui/token.md#sui_token_recipient">recipient</a></code>. Creates an <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code> for the
"transfer" action. The <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code> contains the <code><a href="../sui/token.md#sui_token_recipient">recipient</a></code> field
to be used in verification.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer">transfer</a>&lt;T&gt;(t: <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_recipient">recipient</a>: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/transfer.md#sui_transfer">transfer</a>&lt;T&gt;(t: <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_recipient">recipient</a>: <b>address</b>, ctx: &<b>mut</b> TxContext): <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt; {
    <b>let</b> <a href="../sui/token.md#sui_token_amount">amount</a> = t.<a href="../sui/balance.md#sui_balance">balance</a>.<a href="../sui/token.md#sui_token_value">value</a>();
    <a href="../sui/transfer.md#sui_transfer_transfer">transfer::transfer</a>(t, <a href="../sui/token.md#sui_token_recipient">recipient</a>);
    <a href="../sui/token.md#sui_token_new_request">new_request</a>(
        <a href="../sui/token.md#sui_token_transfer_action">transfer_action</a>(),
        <a href="../sui/token.md#sui_token_amount">amount</a>,
        option::some(<a href="../sui/token.md#sui_token_recipient">recipient</a>),
        option::none(),
        ctx,
    )
}
</code></pre>



</details>

<a name="sui_token_spend"></a>

## Function `spend`

Spend a <code><a href="../sui/token.md#sui_token_Token">Token</a></code> by unwrapping it and storing the <code>Balance</code> in the
<code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code> for the "spend" action. The <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code> contains
the <code><a href="../sui/token.md#sui_token_spent_balance">spent_balance</a></code> field to be used in verification.

Spend action requires <code><a href="../sui/token.md#sui_token_confirm_request_mut">confirm_request_mut</a></code> to be called to confirm the
request and join the spent balance with the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_spend">spend</a>&lt;T&gt;(t: <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_spend">spend</a>&lt;T&gt;(t: <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt;, ctx: &<b>mut</b> TxContext): <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt; {
    <b>let</b> <a href="../sui/token.md#sui_token_Token">Token</a> { id, <a href="../sui/balance.md#sui_balance">balance</a> } = t;
    id.delete();
    <a href="../sui/token.md#sui_token_new_request">new_request</a>(
        <a href="../sui/token.md#sui_token_spend_action">spend_action</a>(),
        <a href="../sui/balance.md#sui_balance">balance</a>.<a href="../sui/token.md#sui_token_value">value</a>(),
        option::none(),
        option::some(<a href="../sui/balance.md#sui_balance">balance</a>),
        ctx,
    )
}
</code></pre>



</details>

<a name="sui_token_to_coin"></a>

## Function `to_coin`

Convert <code><a href="../sui/token.md#sui_token_Token">Token</a></code> into an open <code>Coin</code>. Creates an <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code> for the
"to_coin" action.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_to_coin">to_coin</a>&lt;T&gt;(t: <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_to_coin">to_coin</a>&lt;T&gt;(t: <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt;, ctx: &<b>mut</b> TxContext): (Coin&lt;T&gt;, <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;) {
    <b>let</b> <a href="../sui/token.md#sui_token_Token">Token</a> { id, <a href="../sui/balance.md#sui_balance">balance</a> } = t;
    <b>let</b> <a href="../sui/token.md#sui_token_amount">amount</a> = <a href="../sui/balance.md#sui_balance">balance</a>.<a href="../sui/token.md#sui_token_value">value</a>();
    id.delete();
    (
        <a href="../sui/balance.md#sui_balance">balance</a>.into_coin(ctx),
        <a href="../sui/token.md#sui_token_new_request">new_request</a>(
            <a href="../sui/token.md#sui_token_to_coin_action">to_coin_action</a>(),
            <a href="../sui/token.md#sui_token_amount">amount</a>,
            option::none(),
            option::none(),
            ctx,
        ),
    )
}
</code></pre>



</details>

<a name="sui_token_from_coin"></a>

## Function `from_coin`

Convert an open <code>Coin</code> into a <code><a href="../sui/token.md#sui_token_Token">Token</a></code>. Creates an <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code> for
the "from_coin" action.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_from_coin">from_coin</a>&lt;T&gt;(<a href="../sui/coin.md#sui_coin">coin</a>: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_from_coin">from_coin</a>&lt;T&gt;(<a href="../sui/coin.md#sui_coin">coin</a>: Coin&lt;T&gt;, ctx: &<b>mut</b> TxContext): (<a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;) {
    <b>let</b> <a href="../sui/token.md#sui_token_amount">amount</a> = <a href="../sui/coin.md#sui_coin">coin</a>.<a href="../sui/token.md#sui_token_value">value</a>();
    <b>let</b> <a href="../sui/token.md#sui_token">token</a> = <a href="../sui/token.md#sui_token_Token">Token</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/balance.md#sui_balance">balance</a>: <a href="../sui/coin.md#sui_coin">coin</a>.into_balance(),
    };
    (
        <a href="../sui/token.md#sui_token">token</a>,
        <a href="../sui/token.md#sui_token_new_request">new_request</a>(
            <a href="../sui/token.md#sui_token_from_coin_action">from_coin_action</a>(),
            <a href="../sui/token.md#sui_token_amount">amount</a>,
            option::none(),
            option::none(),
            ctx,
        ),
    )
}
</code></pre>



</details>

<a name="sui_token_join"></a>

## Function `join`

Join two <code><a href="../sui/token.md#sui_token_Token">Token</a></code>s into one, always available.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_join">join</a>&lt;T&gt;(<a href="../sui/token.md#sui_token">token</a>: &<b>mut</b> <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;, another: <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_join">join</a>&lt;T&gt;(<a href="../sui/token.md#sui_token">token</a>: &<b>mut</b> <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt;, another: <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt;) {
    <b>let</b> <a href="../sui/token.md#sui_token_Token">Token</a> { id, <a href="../sui/balance.md#sui_balance">balance</a> } = another;
    <a href="../sui/token.md#sui_token">token</a>.<a href="../sui/balance.md#sui_balance">balance</a>.<a href="../sui/token.md#sui_token_join">join</a>(<a href="../sui/balance.md#sui_balance">balance</a>);
    id.delete();
}
</code></pre>



</details>

<a name="sui_token_split"></a>

## Function `split`

Split a <code><a href="../sui/token.md#sui_token_Token">Token</a></code> with <code><a href="../sui/token.md#sui_token_amount">amount</a></code>.
Aborts if the <code><a href="../sui/token.md#sui_token_Token">Token</a>.<a href="../sui/balance.md#sui_balance">balance</a></code> is lower than <code><a href="../sui/token.md#sui_token_amount">amount</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_split">split</a>&lt;T&gt;(<a href="../sui/token.md#sui_token">token</a>: &<b>mut</b> <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_amount">amount</a>: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_split">split</a>&lt;T&gt;(<a href="../sui/token.md#sui_token">token</a>: &<b>mut</b> <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_amount">amount</a>: u64, ctx: &<b>mut</b> TxContext): <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt; {
    <b>assert</b>!(<a href="../sui/token.md#sui_token">token</a>.<a href="../sui/balance.md#sui_balance">balance</a>.<a href="../sui/token.md#sui_token_value">value</a>() &gt;= <a href="../sui/token.md#sui_token_amount">amount</a>, <a href="../sui/token.md#sui_token_EBalanceTooLow">EBalanceTooLow</a>);
    <a href="../sui/token.md#sui_token_Token">Token</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/balance.md#sui_balance">balance</a>: <a href="../sui/token.md#sui_token">token</a>.<a href="../sui/balance.md#sui_balance">balance</a>.<a href="../sui/token.md#sui_token_split">split</a>(<a href="../sui/token.md#sui_token_amount">amount</a>),
    }
}
</code></pre>



</details>

<a name="sui_token_zero"></a>

## Function `zero`

Create a zero <code><a href="../sui/token.md#sui_token_Token">Token</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_zero">zero</a>&lt;T&gt;(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_zero">zero</a>&lt;T&gt;(ctx: &<b>mut</b> TxContext): <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt; {
    <a href="../sui/token.md#sui_token_Token">Token</a> {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/balance.md#sui_balance">balance</a>: <a href="../sui/balance.md#sui_balance_zero">balance::zero</a>(),
    }
}
</code></pre>



</details>

<a name="sui_token_destroy_zero"></a>

## Function `destroy_zero`

Destroy an empty <code><a href="../sui/token.md#sui_token_Token">Token</a></code>, fails if the balance is non-zero.
Aborts if the <code><a href="../sui/token.md#sui_token_Token">Token</a>.<a href="../sui/balance.md#sui_balance">balance</a></code> is not zero.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_destroy_zero">destroy_zero</a>&lt;T&gt;(<a href="../sui/token.md#sui_token">token</a>: <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_destroy_zero">destroy_zero</a>&lt;T&gt;(<a href="../sui/token.md#sui_token">token</a>: <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt;) {
    <b>let</b> <a href="../sui/token.md#sui_token_Token">Token</a> { id, <a href="../sui/balance.md#sui_balance">balance</a> } = <a href="../sui/token.md#sui_token">token</a>;
    <b>assert</b>!(<a href="../sui/balance.md#sui_balance">balance</a>.<a href="../sui/token.md#sui_token_value">value</a>() == 0, <a href="../sui/token.md#sui_token_ENotZero">ENotZero</a>);
    <a href="../sui/balance.md#sui_balance">balance</a>.<a href="../sui/token.md#sui_token_destroy_zero">destroy_zero</a>();
    id.delete();
}
</code></pre>



</details>

<a name="sui_token_keep"></a>

## Function `keep`

Transfer the <code><a href="../sui/token.md#sui_token_Token">Token</a></code> to the transaction sender.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_keep">keep</a>&lt;T&gt;(<a href="../sui/token.md#sui_token">token</a>: <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_keep">keep</a>&lt;T&gt;(<a href="../sui/token.md#sui_token">token</a>: <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt;, ctx: &<b>mut</b> TxContext) {
    <a href="../sui/transfer.md#sui_transfer_transfer">transfer::transfer</a>(<a href="../sui/token.md#sui_token">token</a>, ctx.<a href="../sui/token.md#sui_token_sender">sender</a>())
}
</code></pre>



</details>

<a name="sui_token_new_request"></a>

## Function `new_request`

Create a new <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code>.
Publicly available method to allow for custom actions.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_new_request">new_request</a>&lt;T&gt;(name: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/token.md#sui_token_amount">amount</a>: u64, <a href="../sui/token.md#sui_token_recipient">recipient</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;, <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/balance.md#sui_balance_Balance">sui::balance::Balance</a>&lt;T&gt;&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_new_request">new_request</a>&lt;T&gt;(
    name: String,
    <a href="../sui/token.md#sui_token_amount">amount</a>: u64,
    <a href="../sui/token.md#sui_token_recipient">recipient</a>: Option&lt;<b>address</b>&gt;,
    <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>: Option&lt;Balance&lt;T&gt;&gt;,
    ctx: &TxContext,
): <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt; {
    <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a> {
        name,
        <a href="../sui/token.md#sui_token_amount">amount</a>,
        <a href="../sui/token.md#sui_token_recipient">recipient</a>,
        <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>,
        <a href="../sui/token.md#sui_token_sender">sender</a>: ctx.<a href="../sui/token.md#sui_token_sender">sender</a>(),
        <a href="../sui/token.md#sui_token_approvals">approvals</a>: <a href="../sui/vec_set.md#sui_vec_set_empty">vec_set::empty</a>(),
    }
}
</code></pre>



</details>

<a name="sui_token_confirm_request"></a>

## Function `confirm_request`

Confirm the request against the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> and return the parameters
of the request: (Name, Amount, Sender, Recipient).

Cannot be used for <code><a href="../sui/token.md#sui_token_spend">spend</a></code> and similar actions that deliver <code><a href="../sui/token.md#sui_token_spent_balance">spent_balance</a></code>
to the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>. For those actions use <code><a href="../sui/token.md#sui_token_confirm_request_mut">confirm_request_mut</a></code>.

Aborts if:
- the action is not allowed (missing record in <code><a href="../sui/token.md#sui_token_rules">rules</a></code>)
- action contains <code><a href="../sui/token.md#sui_token_spent_balance">spent_balance</a></code> (use <code><a href="../sui/token.md#sui_token_confirm_request_mut">confirm_request_mut</a></code>)
- the <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code> does not meet the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> rules for the action


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_confirm_request">confirm_request</a>&lt;T&gt;(policy: &<a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, request: <a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../std/string.md#std_string_String">std::string::String</a>, u64, <b>address</b>, <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_confirm_request">confirm_request</a>&lt;T&gt;(
    policy: &<a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    request: <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext,
): (String, u64, <b>address</b>, Option&lt;<b>address</b>&gt;) {
    <b>assert</b>!(request.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.is_none(), <a href="../sui/token.md#sui_token_ECantConsumeBalance">ECantConsumeBalance</a>);
    <b>assert</b>!(policy.<a href="../sui/token.md#sui_token_rules">rules</a>.contains(&request.name), <a href="../sui/token.md#sui_token_EUnknownAction">EUnknownAction</a>);
    <b>let</b> <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a> {
        name,
        <a href="../sui/token.md#sui_token_approvals">approvals</a>,
        <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>,
        <a href="../sui/token.md#sui_token_amount">amount</a>,
        <a href="../sui/token.md#sui_token_sender">sender</a>,
        <a href="../sui/token.md#sui_token_recipient">recipient</a>,
    } = request;
    <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.destroy_none();
    <b>let</b> <a href="../sui/token.md#sui_token_rules">rules</a> = &(*policy.<a href="../sui/token.md#sui_token_rules">rules</a>.get(&name)).into_keys();
    <b>let</b> rules_len = <a href="../sui/token.md#sui_token_rules">rules</a>.length();
    <b>let</b> <b>mut</b> i = 0;
    <b>while</b> (i &lt; rules_len) {
        <b>let</b> rule = &<a href="../sui/token.md#sui_token_rules">rules</a>[i];
        <b>assert</b>!(<a href="../sui/token.md#sui_token_approvals">approvals</a>.contains(rule), <a href="../sui/token.md#sui_token_ENotApproved">ENotApproved</a>);
        i = i + 1;
    };
    (name, <a href="../sui/token.md#sui_token_amount">amount</a>, <a href="../sui/token.md#sui_token_sender">sender</a>, <a href="../sui/token.md#sui_token_recipient">recipient</a>)
}
</code></pre>



</details>

<a name="sui_token_confirm_request_mut"></a>

## Function `confirm_request_mut`

Confirm the request against the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> and return the parameters
of the request: (Name, Amount, Sender, Recipient).

Unlike <code><a href="../sui/token.md#sui_token_confirm_request">confirm_request</a></code> this function requires mutable access to the
<code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> and must be used on <code><a href="../sui/token.md#sui_token_spend">spend</a></code> action. After dealing with the
spent balance it calls <code><a href="../sui/token.md#sui_token_confirm_request">confirm_request</a></code> internally.

See <code><a href="../sui/token.md#sui_token_confirm_request">confirm_request</a></code> for the list of abort conditions.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_confirm_request_mut">confirm_request_mut</a>&lt;T&gt;(policy: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, request: <a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../std/string.md#std_string_String">std::string::String</a>, u64, <b>address</b>, <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_confirm_request_mut">confirm_request_mut</a>&lt;T&gt;(
    policy: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    <b>mut</b> request: <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;,
    ctx: &<b>mut</b> TxContext,
): (String, u64, <b>address</b>, Option&lt;<b>address</b>&gt;) {
    <b>assert</b>!(policy.<a href="../sui/token.md#sui_token_rules">rules</a>.contains(&request.name), <a href="../sui/token.md#sui_token_EUnknownAction">EUnknownAction</a>);
    <b>assert</b>!(request.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.is_some(), <a href="../sui/token.md#sui_token_EUseImmutableConfirm">EUseImmutableConfirm</a>);
    policy.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.<a href="../sui/token.md#sui_token_join">join</a>(request.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.extract());
    <a href="../sui/token.md#sui_token_confirm_request">confirm_request</a>(policy, request, ctx)
}
</code></pre>



</details>

<a name="sui_token_confirm_with_policy_cap"></a>

## Function `confirm_with_policy_cap`

Confirm an <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code> as the <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code> owner. This function
allows <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> owner to perform Capability-gated actions ignoring
the ruleset specified in the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.

Aborts if request contains <code><a href="../sui/token.md#sui_token_spent_balance">spent_balance</a></code> due to inability of the
<code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code> to decrease supply. For scenarios like this a
<code>TreasuryCap</code> is required (see <code><a href="../sui/token.md#sui_token_confirm_with_treasury_cap">confirm_with_treasury_cap</a></code>).


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_confirm_with_policy_cap">confirm_with_policy_cap</a>&lt;T&gt;(_policy_cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">sui::token::TokenPolicyCap</a>&lt;T&gt;, request: <a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../std/string.md#std_string_String">std::string::String</a>, u64, <b>address</b>, <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_confirm_with_policy_cap">confirm_with_policy_cap</a>&lt;T&gt;(
    _policy_cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    request: <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext,
): (String, u64, <b>address</b>, Option&lt;<b>address</b>&gt;) {
    <b>assert</b>!(request.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.is_none(), <a href="../sui/token.md#sui_token_ECantConsumeBalance">ECantConsumeBalance</a>);
    <b>let</b> <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a> {
        name,
        <a href="../sui/token.md#sui_token_amount">amount</a>,
        <a href="../sui/token.md#sui_token_sender">sender</a>,
        <a href="../sui/token.md#sui_token_recipient">recipient</a>,
        <a href="../sui/token.md#sui_token_approvals">approvals</a>: _,
        <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>,
    } = request;
    <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.destroy_none();
    (name, <a href="../sui/token.md#sui_token_amount">amount</a>, <a href="../sui/token.md#sui_token_sender">sender</a>, <a href="../sui/token.md#sui_token_recipient">recipient</a>)
}
</code></pre>



</details>

<a name="sui_token_confirm_with_treasury_cap"></a>

## Function `confirm_with_treasury_cap`

Confirm an <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code> as the <code>TreasuryCap</code> owner. This function
allows <code>TreasuryCap</code> owner to perform Capability-gated actions ignoring
the ruleset specified in the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.

Unlike <code><a href="../sui/token.md#sui_token_confirm_with_policy_cap">confirm_with_policy_cap</a></code> this function allows <code><a href="../sui/token.md#sui_token_spent_balance">spent_balance</a></code>
to be consumed, decreasing the <code>total_supply</code> of the <code><a href="../sui/token.md#sui_token_Token">Token</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_confirm_with_treasury_cap">confirm_with_treasury_cap</a>&lt;T&gt;(treasury_cap: &<b>mut</b> <a href="../sui/coin.md#sui_coin_TreasuryCap">sui::coin::TreasuryCap</a>&lt;T&gt;, request: <a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../std/string.md#std_string_String">std::string::String</a>, u64, <b>address</b>, <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_confirm_with_treasury_cap">confirm_with_treasury_cap</a>&lt;T&gt;(
    treasury_cap: &<b>mut</b> TreasuryCap&lt;T&gt;,
    request: <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext,
): (String, u64, <b>address</b>, Option&lt;<b>address</b>&gt;) {
    <b>let</b> <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a> {
        name,
        <a href="../sui/token.md#sui_token_amount">amount</a>,
        <a href="../sui/token.md#sui_token_sender">sender</a>,
        <a href="../sui/token.md#sui_token_recipient">recipient</a>,
        <a href="../sui/token.md#sui_token_approvals">approvals</a>: _,
        <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>,
    } = request;
    <b>if</b> (<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.is_some()) {
        treasury_cap.supply_mut().decrease_supply(<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.destroy_some());
    } <b>else</b> {
        <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.destroy_none();
    };
    (name, <a href="../sui/token.md#sui_token_amount">amount</a>, <a href="../sui/token.md#sui_token_sender">sender</a>, <a href="../sui/token.md#sui_token_recipient">recipient</a>)
}
</code></pre>



</details>

<a name="sui_token_add_approval"></a>

## Function `add_approval`

Add an "approval" to the <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code> by providing a Witness.
Intended to be used by Rules to add their own approvals, however, can
be used to add arbitrary approvals to the request (not only the ones
required by the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>).


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_add_approval">add_approval</a>&lt;T, W: drop&gt;(_t: W, request: &<b>mut</b> <a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_add_approval">add_approval</a>&lt;T, W: drop&gt;(_t: W, request: &<b>mut</b> <a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;, _ctx: &<b>mut</b> TxContext) {
    request.<a href="../sui/token.md#sui_token_approvals">approvals</a>.insert(type_name::get&lt;W&gt;())
}
</code></pre>



</details>

<a name="sui_token_add_rule_config"></a>

## Function `add_rule_config`

Add a <code>Config</code> for a <code>Rule</code> in the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>. Rule configuration is
independent from the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>.<a href="../sui/token.md#sui_token_rules">rules</a></code> and needs to be managed by the
Rule itself. Configuration is stored per <code>Rule</code> and not per <code>Rule</code> per
<code>Action</code> to allow reuse in different actions.

- Rule witness guarantees that the <code>Config</code> is approved by the Rule.
- <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code> guarantees that the <code>Config</code> setup is initiated by
the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> owner.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_add_rule_config">add_rule_config</a>&lt;T, Rule: drop, Config: store&gt;(_rule: Rule, self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">sui::token::TokenPolicyCap</a>&lt;T&gt;, <a href="../sui/config.md#sui_config">config</a>: Config, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_add_rule_config">add_rule_config</a>&lt;T, Rule: drop, Config: store&gt;(
    _rule: Rule,
    self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    <a href="../sui/config.md#sui_config">config</a>: Config,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(<a href="../sui/object.md#sui_object_id">object::id</a>(self) == cap.`<b>for</b>`, <a href="../sui/token.md#sui_token_ENotAuthorized">ENotAuthorized</a>);
    df::add(&<b>mut</b> self.id, <a href="../sui/token.md#sui_token_key">key</a>&lt;Rule&gt;(), <a href="../sui/config.md#sui_config">config</a>)
}
</code></pre>



</details>

<a name="sui_token_rule_config"></a>

## Function `rule_config`

Get a <code>Config</code> for a <code>Rule</code> in the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>. Requires <code>Rule</code>
witness, hence can only be read by the <code>Rule</code> itself. This requirement
guarantees safety of the stored <code>Config</code> and allows for simpler dynamic
field management inside the Rule Config (custom type keys are not needed
for access gating).

Aborts if the Config is not present.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_rule_config">rule_config</a>&lt;T, Rule: drop, Config: store&gt;(_rule: Rule, self: &<a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;): &Config
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_rule_config">rule_config</a>&lt;T, Rule: drop, Config: store&gt;(_rule: Rule, self: &<a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;): &Config {
    <b>assert</b>!(<a href="../sui/token.md#sui_token_has_rule_config_with_type">has_rule_config_with_type</a>&lt;T, Rule, Config&gt;(self), <a href="../sui/token.md#sui_token_ENoConfig">ENoConfig</a>);
    df::borrow(&self.id, <a href="../sui/token.md#sui_token_key">key</a>&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="sui_token_rule_config_mut"></a>

## Function `rule_config_mut`

Get mutable access to the <code>Config</code> for a <code>Rule</code> in the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.
Requires <code>Rule</code> witness, hence can only be read by the <code>Rule</code> itself,
as well as <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code> to guarantee that the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> owner
is the one who initiated the <code>Config</code> modification.

Aborts if:
- the Config is not present
- <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_rule_config_mut">rule_config_mut</a>&lt;T, Rule: drop, Config: store&gt;(_rule: Rule, self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">sui::token::TokenPolicyCap</a>&lt;T&gt;): &<b>mut</b> Config
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_rule_config_mut">rule_config_mut</a>&lt;T, Rule: drop, Config: store&gt;(
    _rule: Rule,
    self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
): &<b>mut</b> Config {
    <b>assert</b>!(<a href="../sui/token.md#sui_token_has_rule_config_with_type">has_rule_config_with_type</a>&lt;T, Rule, Config&gt;(self), <a href="../sui/token.md#sui_token_ENoConfig">ENoConfig</a>);
    <b>assert</b>!(<a href="../sui/object.md#sui_object_id">object::id</a>(self) == cap.`<b>for</b>`, <a href="../sui/token.md#sui_token_ENotAuthorized">ENotAuthorized</a>);
    df::borrow_mut(&<b>mut</b> self.id, <a href="../sui/token.md#sui_token_key">key</a>&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="sui_token_remove_rule_config"></a>

## Function `remove_rule_config`

Remove a <code>Config</code> for a <code>Rule</code> in the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.
Unlike the <code><a href="../sui/token.md#sui_token_add_rule_config">add_rule_config</a></code>, this function does not require a <code>Rule</code>
witness, hence can be performed by the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> owner on their own.

Rules need to make sure that the <code>Config</code> is present when performing
verification of the <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code>.

Aborts if:
- the Config is not present
- <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_remove_rule_config">remove_rule_config</a>&lt;T, Rule, Config: store&gt;(self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">sui::token::TokenPolicyCap</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): Config
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_remove_rule_config">remove_rule_config</a>&lt;T, Rule, Config: store&gt;(
    self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext,
): Config {
    <b>assert</b>!(<a href="../sui/token.md#sui_token_has_rule_config_with_type">has_rule_config_with_type</a>&lt;T, Rule, Config&gt;(self), <a href="../sui/token.md#sui_token_ENoConfig">ENoConfig</a>);
    <b>assert</b>!(<a href="../sui/object.md#sui_object_id">object::id</a>(self) == cap.`<b>for</b>`, <a href="../sui/token.md#sui_token_ENotAuthorized">ENotAuthorized</a>);
    df::remove(&<b>mut</b> self.id, <a href="../sui/token.md#sui_token_key">key</a>&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="sui_token_has_rule_config"></a>

## Function `has_rule_config`

Check if a config for a <code>Rule</code> is set in the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> without
checking the type of the <code>Config</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_has_rule_config">has_rule_config</a>&lt;T, Rule&gt;(self: &<a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_has_rule_config">has_rule_config</a>&lt;T, Rule&gt;(self: &<a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;): bool {
    df::exists_&lt;<a href="../sui/token.md#sui_token_RuleKey">RuleKey</a>&lt;Rule&gt;&gt;(&self.id, <a href="../sui/token.md#sui_token_key">key</a>&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="sui_token_has_rule_config_with_type"></a>

## Function `has_rule_config_with_type`

Check if a <code>Config</code> for a <code>Rule</code> is set in the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code> and that
it matches the type provided.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_has_rule_config_with_type">has_rule_config_with_type</a>&lt;T, Rule, Config: store&gt;(self: &<a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_has_rule_config_with_type">has_rule_config_with_type</a>&lt;T, Rule, Config: store&gt;(self: &<a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;): bool {
    df::exists_with_type&lt;<a href="../sui/token.md#sui_token_RuleKey">RuleKey</a>&lt;Rule&gt;, Config&gt;(&self.id, <a href="../sui/token.md#sui_token_key">key</a>&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="sui_token_allow"></a>

## Function `allow`

Allows an <code><a href="../sui/token.md#sui_token_action">action</a></code> to be performed on the <code><a href="../sui/token.md#sui_token_Token">Token</a></code> freely by adding an
empty set of <code>Rules</code> for the <code><a href="../sui/token.md#sui_token_action">action</a></code>.

Aborts if the <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_allow">allow</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">sui::token::TokenPolicyCap</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_action">action</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_allow">allow</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    <a href="../sui/token.md#sui_token_action">action</a>: String,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(<a href="../sui/object.md#sui_object_id">object::id</a>(self) == cap.`<b>for</b>`, <a href="../sui/token.md#sui_token_ENotAuthorized">ENotAuthorized</a>);
    self.<a href="../sui/token.md#sui_token_rules">rules</a>.insert(<a href="../sui/token.md#sui_token_action">action</a>, <a href="../sui/vec_set.md#sui_vec_set_empty">vec_set::empty</a>());
}
</code></pre>



</details>

<a name="sui_token_disallow"></a>

## Function `disallow`

Completely disallows an <code><a href="../sui/token.md#sui_token_action">action</a></code> on the <code><a href="../sui/token.md#sui_token_Token">Token</a></code> by removing the record
from the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>.<a href="../sui/token.md#sui_token_rules">rules</a></code>.

Aborts if the <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_disallow">disallow</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">sui::token::TokenPolicyCap</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_action">action</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_disallow">disallow</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    <a href="../sui/token.md#sui_token_action">action</a>: String,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(<a href="../sui/object.md#sui_object_id">object::id</a>(self) == cap.`<b>for</b>`, <a href="../sui/token.md#sui_token_ENotAuthorized">ENotAuthorized</a>);
    self.<a href="../sui/token.md#sui_token_rules">rules</a>.remove(&<a href="../sui/token.md#sui_token_action">action</a>);
}
</code></pre>



</details>

<a name="sui_token_add_rule_for_action"></a>

## Function `add_rule_for_action`

Adds a Rule for an action with <code>name</code> in the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.

Aborts if the <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_add_rule_for_action">add_rule_for_action</a>&lt;T, Rule: drop&gt;(self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">sui::token::TokenPolicyCap</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_action">action</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_add_rule_for_action">add_rule_for_action</a>&lt;T, Rule: drop&gt;(
    self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    <a href="../sui/token.md#sui_token_action">action</a>: String,
    ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(<a href="../sui/object.md#sui_object_id">object::id</a>(self) == cap.`<b>for</b>`, <a href="../sui/token.md#sui_token_ENotAuthorized">ENotAuthorized</a>);
    <b>if</b> (!self.<a href="../sui/token.md#sui_token_rules">rules</a>.contains(&<a href="../sui/token.md#sui_token_action">action</a>)) {
        <a href="../sui/token.md#sui_token_allow">allow</a>(self, cap, <a href="../sui/token.md#sui_token_action">action</a>, ctx);
    };
    self.<a href="../sui/token.md#sui_token_rules">rules</a>.get_mut(&<a href="../sui/token.md#sui_token_action">action</a>).insert(type_name::get&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="sui_token_remove_rule_for_action"></a>

## Function `remove_rule_for_action`

Removes a rule for an action with <code>name</code> in the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>. Returns
the config object to be handled by the sender (or a Rule itself).

Aborts if the <code><a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a></code> is not matching the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_remove_rule_for_action">remove_rule_for_action</a>&lt;T, Rule: drop&gt;(self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">sui::token::TokenPolicyCap</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_action">action</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_remove_rule_for_action">remove_rule_for_action</a>&lt;T, Rule: drop&gt;(
    self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<a href="../sui/token.md#sui_token_TokenPolicyCap">TokenPolicyCap</a>&lt;T&gt;,
    <a href="../sui/token.md#sui_token_action">action</a>: String,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(<a href="../sui/object.md#sui_object_id">object::id</a>(self) == cap.`<b>for</b>`, <a href="../sui/token.md#sui_token_ENotAuthorized">ENotAuthorized</a>);
    self.<a href="../sui/token.md#sui_token_rules">rules</a>.get_mut(&<a href="../sui/token.md#sui_token_action">action</a>).remove(&type_name::get&lt;Rule&gt;())
}
</code></pre>



</details>

<a name="sui_token_mint"></a>

## Function `mint`

Mint a <code><a href="../sui/token.md#sui_token_Token">Token</a></code> with a given <code><a href="../sui/token.md#sui_token_amount">amount</a></code> using the <code>TreasuryCap</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_mint">mint</a>&lt;T&gt;(cap: &<b>mut</b> <a href="../sui/coin.md#sui_coin_TreasuryCap">sui::coin::TreasuryCap</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_amount">amount</a>: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_mint">mint</a>&lt;T&gt;(cap: &<b>mut</b> TreasuryCap&lt;T&gt;, <a href="../sui/token.md#sui_token_amount">amount</a>: u64, ctx: &<b>mut</b> TxContext): <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt; {
    <b>let</b> <a href="../sui/balance.md#sui_balance">balance</a> = cap.supply_mut().increase_supply(<a href="../sui/token.md#sui_token_amount">amount</a>);
    <a href="../sui/token.md#sui_token_Token">Token</a> { id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx), <a href="../sui/balance.md#sui_balance">balance</a> }
}
</code></pre>



</details>

<a name="sui_token_burn"></a>

## Function `burn`

Burn a <code><a href="../sui/token.md#sui_token_Token">Token</a></code> using the <code>TreasuryCap</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_burn">burn</a>&lt;T&gt;(cap: &<b>mut</b> <a href="../sui/coin.md#sui_coin_TreasuryCap">sui::coin::TreasuryCap</a>&lt;T&gt;, <a href="../sui/token.md#sui_token">token</a>: <a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_burn">burn</a>&lt;T&gt;(cap: &<b>mut</b> TreasuryCap&lt;T&gt;, <a href="../sui/token.md#sui_token">token</a>: <a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt;) {
    <b>let</b> <a href="../sui/token.md#sui_token_Token">Token</a> { id, <a href="../sui/balance.md#sui_balance">balance</a> } = <a href="../sui/token.md#sui_token">token</a>;
    cap.supply_mut().decrease_supply(<a href="../sui/balance.md#sui_balance">balance</a>);
    id.delete();
}
</code></pre>



</details>

<a name="sui_token_flush"></a>

## Function `flush`

Flush the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a></code> into the <code>TreasuryCap</code>. This
action is only available to the <code>TreasuryCap</code> owner.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_flush">flush</a>&lt;T&gt;(self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, cap: &<b>mut</b> <a href="../sui/coin.md#sui_coin_TreasuryCap">sui::coin::TreasuryCap</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_flush">flush</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;,
    cap: &<b>mut</b> TreasuryCap&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext,
): u64 {
    <b>let</b> <a href="../sui/token.md#sui_token_amount">amount</a> = self.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.<a href="../sui/token.md#sui_token_value">value</a>();
    <b>let</b> <a href="../sui/balance.md#sui_balance">balance</a> = self.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.<a href="../sui/token.md#sui_token_split">split</a>(<a href="../sui/token.md#sui_token_amount">amount</a>);
    cap.supply_mut().decrease_supply(<a href="../sui/balance.md#sui_balance">balance</a>)
}
</code></pre>



</details>

<a name="sui_token_is_allowed"></a>

## Function `is_allowed`

Check whether an action is present in the rules VecMap.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_is_allowed">is_allowed</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_action">action</a>: &<a href="../std/string.md#std_string_String">std::string::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_is_allowed">is_allowed</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_action">action</a>: &String): bool {
    self.<a href="../sui/token.md#sui_token_rules">rules</a>.contains(<a href="../sui/token.md#sui_token_action">action</a>)
}
</code></pre>



</details>

<a name="sui_token_rules"></a>

## Function `rules`

Returns the rules required for a specific action.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_rules">rules</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_action">action</a>: &<a href="../std/string.md#std_string_String">std::string::String</a>): <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_rules">rules</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;, <a href="../sui/token.md#sui_token_action">action</a>: &String): VecSet&lt;TypeName&gt; {
    *self.<a href="../sui/token.md#sui_token_rules">rules</a>.get(<a href="../sui/token.md#sui_token_action">action</a>)
}
</code></pre>



</details>

<a name="sui_token_spent_balance"></a>

## Function `spent_balance`

Returns the <code><a href="../sui/token.md#sui_token_spent_balance">spent_balance</a></code> of the <code><a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_TokenPolicy">sui::token::TokenPolicy</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_TokenPolicy">TokenPolicy</a>&lt;T&gt;): u64 {
    self.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.<a href="../sui/token.md#sui_token_value">value</a>()
}
</code></pre>



</details>

<a name="sui_token_value"></a>

## Function `value`

Returns the <code><a href="../sui/balance.md#sui_balance">balance</a></code> of the <code><a href="../sui/token.md#sui_token_Token">Token</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_value">value</a>&lt;T&gt;(t: &<a href="../sui/token.md#sui_token_Token">sui::token::Token</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_value">value</a>&lt;T&gt;(t: &<a href="../sui/token.md#sui_token_Token">Token</a>&lt;T&gt;): u64 {
    t.<a href="../sui/balance.md#sui_balance">balance</a>.<a href="../sui/token.md#sui_token_value">value</a>()
}
</code></pre>



</details>

<a name="sui_token_transfer_action"></a>

## Function `transfer_action`

Name of the Transfer action.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_transfer_action">transfer_action</a>(): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_transfer_action">transfer_action</a>(): String {
    <b>let</b> transfer_str = <a href="../sui/token.md#sui_token_TRANSFER">TRANSFER</a>;
    transfer_str.to_string()
}
</code></pre>



</details>

<a name="sui_token_spend_action"></a>

## Function `spend_action`

Name of the <code>Spend</code> action.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_spend_action">spend_action</a>(): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_spend_action">spend_action</a>(): String {
    <b>let</b> spend_str = <a href="../sui/token.md#sui_token_SPEND">SPEND</a>;
    spend_str.to_string()
}
</code></pre>



</details>

<a name="sui_token_to_coin_action"></a>

## Function `to_coin_action`

Name of the <code>ToCoin</code> action.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_to_coin_action">to_coin_action</a>(): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_to_coin_action">to_coin_action</a>(): String {
    <b>let</b> to_coin_str = <a href="../sui/token.md#sui_token_TO_COIN">TO_COIN</a>;
    to_coin_str.to_string()
}
</code></pre>



</details>

<a name="sui_token_from_coin_action"></a>

## Function `from_coin_action`

Name of the <code>FromCoin</code> action.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_from_coin_action">from_coin_action</a>(): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_from_coin_action">from_coin_action</a>(): String {
    <b>let</b> from_coin_str = <a href="../sui/token.md#sui_token_FROM_COIN">FROM_COIN</a>;
    from_coin_str.to_string()
}
</code></pre>



</details>

<a name="sui_token_action"></a>

## Function `action`

The Action in the <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_action">action</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_action">action</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;): String { self.name }
</code></pre>



</details>

<a name="sui_token_amount"></a>

## Function `amount`

Amount of the <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_amount">amount</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_amount">amount</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;): u64 { self.<a href="../sui/token.md#sui_token_amount">amount</a> }
</code></pre>



</details>

<a name="sui_token_sender"></a>

## Function `sender`

Sender of the <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_sender">sender</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_sender">sender</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;): <b>address</b> { self.<a href="../sui/token.md#sui_token_sender">sender</a> }
</code></pre>



</details>

<a name="sui_token_recipient"></a>

## Function `recipient`

Recipient of the <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_recipient">recipient</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<b>address</b>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_recipient">recipient</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;): Option&lt;<b>address</b>&gt; {
    self.<a href="../sui/token.md#sui_token_recipient">recipient</a>
}
</code></pre>



</details>

<a name="sui_token_approvals"></a>

## Function `approvals`

Approvals of the <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_approvals">approvals</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;): <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_approvals">approvals</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;): VecSet&lt;TypeName&gt; {
    self.<a href="../sui/token.md#sui_token_approvals">approvals</a>
}
</code></pre>



</details>

<a name="sui_token_spent"></a>

## Function `spent`

Burned balance of the <code><a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_spent">spent</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">sui::token::ActionRequest</a>&lt;T&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/token.md#sui_token_spent">spent</a>&lt;T&gt;(self: &<a href="../sui/token.md#sui_token_ActionRequest">ActionRequest</a>&lt;T&gt;): Option&lt;u64&gt; {
    <b>if</b> (self.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.is_some()) {
        option::some(self.<a href="../sui/token.md#sui_token_spent_balance">spent_balance</a>.<a href="../sui/borrow.md#sui_borrow">borrow</a>().<a href="../sui/token.md#sui_token_value">value</a>())
    } <b>else</b> {
        option::none()
    }
}
</code></pre>



</details>

<a name="sui_token_key"></a>

## Function `key`

Create a new <code><a href="../sui/token.md#sui_token_RuleKey">RuleKey</a></code> for a <code>Rule</code>. The <code>is_protected</code> field is kept
for potential future use, if Rules were to have a freely modifiable
storage as addition / replacement for the <code>Config</code> system.

The goal of <code>is_protected</code> is to potentially allow Rules store a mutable
version of their configuration and mutate state on user action.


<pre><code><b>fun</b> <a href="../sui/token.md#sui_token_key">key</a>&lt;Rule&gt;(): <a href="../sui/token.md#sui_token_RuleKey">sui::token::RuleKey</a>&lt;Rule&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/token.md#sui_token_key">key</a>&lt;Rule&gt;(): <a href="../sui/token.md#sui_token_RuleKey">RuleKey</a>&lt;Rule&gt; { <a href="../sui/token.md#sui_token_RuleKey">RuleKey</a> { is_protected: <b>true</b> } }
</code></pre>



</details>
