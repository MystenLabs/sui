
<a name="0x2_kiosk_lock_rule"></a>

# Module `0x2::kiosk_lock_rule`

Description:
This module defines a Rule which forces buyers to put the purchased
item into the Kiosk and lock it. The most common use case for the
Rule is making sure an item never leaves Kiosks and has policies
enforced on every transfer.

Configuration:
- None

Use cases:
- Enforcing policies on every trade
- Making sure an item never leaves the Kiosk / certain ecosystem

Notes:
- "locking" mechanic disallows the <code><a href="kiosk.md#0x2_kiosk_take">kiosk::take</a></code> function and forces
the owner to use <code>list</code> or <code>list_with_purchase_cap</code> methods if they
wish to move the item somewhere else.


-  [Struct `Rule`](#0x2_kiosk_lock_rule_Rule)
-  [Struct `Config`](#0x2_kiosk_lock_rule_Config)
-  [Constants](#@Constants_0)
-  [Function `add`](#0x2_kiosk_lock_rule_add)
-  [Function `prove`](#0x2_kiosk_lock_rule_prove)


<pre><code><b>use</b> <a href="kiosk.md#0x2_kiosk">0x2::kiosk</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer_policy.md#0x2_transfer_policy">0x2::transfer_policy</a>;
</code></pre>



<a name="0x2_kiosk_lock_rule_Rule"></a>

## Struct `Rule`

The type identifier for the Rule.


<pre><code><b>struct</b> <a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule_Rule">Rule</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>dummy_field: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_kiosk_lock_rule_Config"></a>

## Struct `Config`

An empty configuration for the Rule.


<pre><code><b>struct</b> <a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule_Config">Config</a> <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>dummy_field: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_kiosk_lock_rule_ENotInKiosk"></a>

Item is not in the <code>Kiosk</code>.


<pre><code><b>const</b> <a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule_ENotInKiosk">ENotInKiosk</a>: u64 = 0;
</code></pre>



<a name="0x2_kiosk_lock_rule_add"></a>

## Function `add`

Creator: Adds a <code><a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule">kiosk_lock_rule</a></code> Rule to the <code>TransferPolicy</code> forcing
buyers to lock the item in a Kiosk on purchase.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule_add">add</a>&lt;T&gt;(policy: &<b>mut</b> <a href="transfer_policy.md#0x2_transfer_policy_TransferPolicy">transfer_policy::TransferPolicy</a>&lt;T&gt;, cap: &<a href="transfer_policy.md#0x2_transfer_policy_TransferPolicyCap">transfer_policy::TransferPolicyCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule_add">add</a>&lt;T&gt;(policy: &<b>mut</b> TransferPolicy&lt;T&gt;, cap: &TransferPolicyCap&lt;T&gt;) {
    policy::add_rule(<a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule_Rule">Rule</a> {}, policy, cap, <a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule_Config">Config</a> {})
}
</code></pre>



</details>

<a name="0x2_kiosk_lock_rule_prove"></a>

## Function `prove`

Buyer: Prove the item was locked in the Kiosk to get the receipt and
unblock the transfer request confirmation.


<pre><code><b>public</b> <b>fun</b> <a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule_prove">prove</a>&lt;T&gt;(request: &<b>mut</b> <a href="transfer_policy.md#0x2_transfer_policy_TransferRequest">transfer_policy::TransferRequest</a>&lt;T&gt;, <a href="kiosk.md#0x2_kiosk">kiosk</a>: &<a href="kiosk.md#0x2_kiosk_Kiosk">kiosk::Kiosk</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule_prove">prove</a>&lt;T&gt;(request: &<b>mut</b> TransferRequest&lt;T&gt;, <a href="kiosk.md#0x2_kiosk">kiosk</a>: &Kiosk) {
    <b>let</b> item = policy::item(request);
    <b>assert</b>!(<a href="kiosk.md#0x2_kiosk_has_item">kiosk::has_item</a>(<a href="kiosk.md#0x2_kiosk">kiosk</a>, item) && <a href="kiosk.md#0x2_kiosk_is_locked">kiosk::is_locked</a>(<a href="kiosk.md#0x2_kiosk">kiosk</a>, item), <a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule_ENotInKiosk">ENotInKiosk</a>);
    policy::add_receipt(<a href="kiosk_lock_rule.md#0x2_kiosk_lock_rule_Rule">Rule</a> {}, request)
}
</code></pre>



</details>
