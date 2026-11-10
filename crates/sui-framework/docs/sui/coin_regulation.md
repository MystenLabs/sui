---
title: Module `sui::coin_regulation`
---

Protocol-level address freezing, coin pausing, and seizing for regulated coins.

Seizing from an address is allowed only in an epoch after it was frozen. An address can be
unfrozen only in an epoch after the last <code><a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a></code> for it was destroyed.


-  [Struct `FreezeList`](#sui_coin_regulation_FreezeList)
-  [Struct `ComplianceCap`](#sui_coin_regulation_ComplianceCap)
-  [Struct `SeizeAuth`](#sui_coin_regulation_SeizeAuth)
-  [Struct `CoinTypeKey`](#sui_coin_regulation_CoinTypeKey)
-  [Struct `CoinTypeState`](#sui_coin_regulation_CoinTypeState)
-  [Struct `FrozenAddressKey`](#sui_coin_regulation_FrozenAddressKey)
-  [Struct `FrozenAddressState`](#sui_coin_regulation_FrozenAddressState)
-  [Struct `AddressFrozen`](#sui_coin_regulation_AddressFrozen)
-  [Struct `AddressUnfrozen`](#sui_coin_regulation_AddressUnfrozen)
-  [Struct `AddressPermanentlyFrozen`](#sui_coin_regulation_AddressPermanentlyFrozen)
-  [Struct `SeizeAuthIssued`](#sui_coin_regulation_SeizeAuthIssued)
-  [Struct `SeizeAuthDestroyed`](#sui_coin_regulation_SeizeAuthDestroyed)
-  [Struct `CoinTypePaused`](#sui_coin_regulation_CoinTypePaused)
-  [Struct `CoinTypeUnpaused`](#sui_coin_regulation_CoinTypeUnpaused)
-  [Struct `DenyCapMigrated`](#sui_coin_regulation_DenyCapMigrated)
-  [Constants](#@Constants_0)
-  [Function `freeze_address`](#sui_coin_regulation_freeze_address)
-  [Function `unfreeze_address`](#sui_coin_regulation_unfreeze_address)
-  [Function `permanently_freeze_address`](#sui_coin_regulation_permanently_freeze_address)
-  [Function `issue_seize_auth`](#sui_coin_regulation_issue_seize_auth)
-  [Function `destroy_seize_auth`](#sui_coin_regulation_destroy_seize_auth)
-  [Function `transfer_seize_auth`](#sui_coin_regulation_transfer_seize_auth)
-  [Function `enable_global_pause`](#sui_coin_regulation_enable_global_pause)
-  [Function `disable_global_pause`](#sui_coin_regulation_disable_global_pause)
-  [Function `is_frozen`](#sui_coin_regulation_is_frozen)
-  [Function `is_permanently_frozen`](#sui_coin_regulation_is_permanently_frozen)
-  [Function `is_global_pause`](#sui_coin_regulation_is_global_pause)
-  [Function `seize_auths`](#sui_coin_regulation_seize_auths)
-  [Function `allow_global_pause`](#sui_coin_regulation_allow_global_pause)
-  [Function `frozen_address`](#sui_coin_regulation_frozen_address)
-  [Function `frozen_epoch`](#sui_coin_regulation_frozen_epoch)
-  [Function `new_compliance_cap`](#sui_coin_regulation_new_compliance_cap)
-  [Function `migrate_from_deny_list`](#sui_coin_regulation_migrate_from_deny_list)
-  [Function `create`](#sui_coin_regulation_create)
-  [Function `add_frozen_address`](#sui_coin_regulation_add_frozen_address)
-  [Function `coin_type_state_mut_or_add`](#sui_coin_regulation_coin_type_state_mut_or_add)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/internal.md#std_internal">std::internal</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/u128.md#std_u128">std::u128</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/accumulator.md#sui_accumulator">sui::accumulator</a>;
<b>use</b> <a href="../sui/accumulator_settlement.md#sui_accumulator_settlement">sui::accumulator_settlement</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/bag.md#sui_bag">sui::bag</a>;
<b>use</b> <a href="../sui/balance.md#sui_balance">sui::balance</a>;
<b>use</b> <a href="../sui/bcs.md#sui_bcs">sui::bcs</a>;
<b>use</b> <a href="../sui/coin.md#sui_coin">sui::coin</a>;
<b>use</b> <a href="../sui/config.md#sui_config">sui::config</a>;
<b>use</b> <a href="../sui/deny_list.md#sui_deny_list">sui::deny_list</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/event.md#sui_event">sui::event</a>;
<b>use</b> <a href="../sui/funds_accumulator.md#sui_funds_accumulator">sui::funds_accumulator</a>;
<b>use</b> <a href="../sui/hash.md#sui_hash">sui::hash</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/protocol_config.md#sui_protocol_config">sui::protocol_config</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="sui_coin_regulation_FreezeList"></a>

## Struct `FreezeList`

Singleton shared object holding the freeze and pause state of regulated coins.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a> <b>has</b> key
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

<a name="sui_coin_regulation_ComplianceCap"></a>

## Struct `ComplianceCap`

Authorizes freezing, seizing, and pausing coin type <code>T</code>. A cap migrated from a <code>DenyCapV2</code>
keeps its object ID.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a>&lt;<b>phantom</b> T&gt; <b>has</b> key, store
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
<code><a href="../sui/coin_regulation.md#sui_coin_regulation_allow_global_pause">allow_global_pause</a>: bool</code>
</dt>
<dd>
 Whether this cap may globally pause <code>T</code>.
</dd>
</dl>


</details>

<a name="sui_coin_regulation_SeizeAuth"></a>

## Struct `SeizeAuth`

Authorizes seizing coin type <code>T</code> from <code><a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a></code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a>&lt;<b>phantom</b> T&gt; <b>has</b> key
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
<code><a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a>: <b>address</b></code>
</dt>
<dd>
 The address this authorizes seizing from.
</dd>
<dt>
<code><a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_epoch">frozen_epoch</a>: u64</code>
</dt>
<dd>
 The epoch in which <code><a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a></code> was frozen.
</dd>
</dl>


</details>

<a name="sui_coin_regulation_CoinTypeKey"></a>

## Struct `CoinTypeKey`

Dynamic field key on <code><a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a></code>. A missing key means <code>T</code> is not paused and has no frozen
addresses.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeKey">CoinTypeKey</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_coin_regulation_CoinTypeState"></a>

## Struct `CoinTypeState`

The value under <code><a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeKey">CoinTypeKey</a>&lt;T&gt;</code>: the freeze and pause state of <code>T</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeState">CoinTypeState</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>paused: bool</code>
</dt>
<dd>
 Whether every transaction touching <code>T</code> is rejected.
</dd>
<dt>
<code>num_frozen_addresses: u64</code>
</dt>
<dd>
 The number of addresses frozen for <code>T</code>.
</dd>
</dl>


</details>

<a name="sui_coin_regulation_FrozenAddressKey"></a>

## Struct `FrozenAddressKey`

Dynamic field key on <code><a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a></code>, holding the frozen address. Exists iff that address is
frozen for <code>T</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressKey">FrozenAddressKey</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_coin_regulation_FrozenAddressState"></a>

## Struct `FrozenAddressState`

The value under <code><a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressKey">FrozenAddressKey</a>&lt;T&gt;</code>: the state of one frozen address.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressState">FrozenAddressState</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/coin_regulation.md#sui_coin_regulation_seize_auths">seize_auths</a>: <a href="../sui/vec_set.md#sui_vec_set_VecSet">sui::vec_set::VecSet</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;</code>
</dt>
<dd>
 The IDs of the outstanding <code><a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a></code>s for this address.
</dd>
<dt>
<code>permanent: bool</code>
</dt>
<dd>
 Whether this address can never be unfrozen.
</dd>
<dt>
<code><a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_epoch">frozen_epoch</a>: u64</code>
</dt>
<dd>
 The epoch in which this address was frozen.
</dd>
<dt>
<code>unfreeze_after_epoch: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
 The epoch of the last <code><a href="../sui/coin_regulation.md#sui_coin_regulation_destroy_seize_auth">destroy_seize_auth</a></code> for this address, if any.
</dd>
</dl>


</details>

<a name="sui_coin_regulation_AddressFrozen"></a>

## Struct `AddressFrozen`

Event: the address was frozen for <code>T</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_AddressFrozen">AddressFrozen</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_coin_regulation_AddressUnfrozen"></a>

## Struct `AddressUnfrozen`

Event: the address was unfrozen for <code>T</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_AddressUnfrozen">AddressUnfrozen</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_coin_regulation_AddressPermanentlyFrozen"></a>

## Struct `AddressPermanentlyFrozen`

Event: the address was permanently frozen for <code>T</code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_AddressPermanentlyFrozen">AddressPermanentlyFrozen</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: <b>address</b></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_coin_regulation_SeizeAuthIssued"></a>

## Struct `SeizeAuthIssued`

Event: a <code><a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a>&lt;T&gt;</code> was issued for <code><a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a></code>.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuthIssued">SeizeAuthIssued</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 The ID of the issued <code><a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a></code>.
</dd>
<dt>
<code><a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a>: <b>address</b></code>
</dt>
<dd>
 The address it authorizes seizing from.
</dd>
</dl>


</details>

<a name="sui_coin_regulation_SeizeAuthDestroyed"></a>

## Struct `SeizeAuthDestroyed`

Event: a <code><a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a>&lt;T&gt;</code> for <code><a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a></code> was destroyed.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuthDestroyed">SeizeAuthDestroyed</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 The ID of the destroyed <code><a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a></code>.
</dd>
<dt>
<code><a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a>: <b>address</b></code>
</dt>
<dd>
 The address it authorized seizing from.
</dd>
</dl>


</details>

<a name="sui_coin_regulation_CoinTypePaused"></a>

## Struct `CoinTypePaused`

Event: <code>T</code> was globally paused.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypePaused">CoinTypePaused</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_coin_regulation_CoinTypeUnpaused"></a>

## Struct `CoinTypeUnpaused`

Event: the global pause of <code>T</code> was lifted.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeUnpaused">CoinTypeUnpaused</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_coin_regulation_DenyCapMigrated"></a>

## Struct `DenyCapMigrated`

Event: the <code>DenyCapV2</code> for <code>T</code> was replaced by the <code><a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a></code> <code>cap</code>, keeping its ID.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_DenyCapMigrated">DenyCapMigrated</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>cap: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
 The ID of the <code><a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a></code>, which is also the ID of the replaced <code>DenyCapV2</code>.
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_coin_regulation_ENotSystemAddress"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ENotSystemAddress">ENotSystemAddress</a>: vector&lt;u8&gt; = b"Only the system can <a href="../sui/coin_regulation.md#sui_coin_regulation_create">create</a> the <b>freeze</b> list.";
</code></pre>



<a name="sui_coin_regulation_EAlreadyFrozen"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_EAlreadyFrozen">EAlreadyFrozen</a>: vector&lt;u8&gt; = b"The <b>address</b> is already frozen <b>for</b> this <a href="../sui/coin.md#sui_coin">coin</a> type.";
</code></pre>



<a name="sui_coin_regulation_ENotFrozen"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ENotFrozen">ENotFrozen</a>: vector&lt;u8&gt; = b"The <b>address</b> is not frozen <b>for</b> this <a href="../sui/coin.md#sui_coin">coin</a> type.";
</code></pre>



<a name="sui_coin_regulation_ESeizeAuthsOutstanding"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ESeizeAuthsOutstanding">ESeizeAuthsOutstanding</a>: vector&lt;u8&gt; = b"The <b>address</b> cannot be unfrozen <b>while</b> any <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a> <b>for</b> it exists.";
</code></pre>



<a name="sui_coin_regulation_EPermanentlyFrozen"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_EPermanentlyFrozen">EPermanentlyFrozen</a>: vector&lt;u8&gt; = b"The <b>address</b> is permanently frozen <b>for</b> this <a href="../sui/coin.md#sui_coin">coin</a> type.";
</code></pre>



<a name="sui_coin_regulation_EGlobalPauseNotAllowed"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_EGlobalPauseNotAllowed">EGlobalPauseNotAllowed</a>: vector&lt;u8&gt; = b"The <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a> does not allow global pause.";
</code></pre>



<a name="sui_coin_regulation_EUnfreezeTooEarly"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_EUnfreezeTooEarly">EUnfreezeTooEarly</a>: vector&lt;u8&gt; = b"The <b>address</b> cannot be unfrozen until the epoch after its last <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a> was destroyed.";
</code></pre>



<a name="sui_coin_regulation_ECoinTypeStateExists"></a>



<pre><code>#[error]
<b>const</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ECoinTypeStateExists">ECoinTypeStateExists</a>: vector&lt;u8&gt; = b"The <a href="../sui/coin.md#sui_coin">coin</a> type already <b>has</b> <b>freeze</b> list state.";
</code></pre>



<a name="sui_coin_regulation_freeze_address"></a>

## Function `freeze_address`

Freeze <code>addr</code> for <code>T</code>. Aborts if <code>addr</code> is already frozen.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_freeze_address">freeze_address</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, _cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">sui::coin_regulation::ComplianceCap</a>&lt;T&gt;, addr: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_freeze_address">freeze_address</a>&lt;T&gt;(
    list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>,
    _cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a>&lt;T&gt;,
    addr: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    list.<a href="../sui/coin_regulation.md#sui_coin_regulation_add_frozen_address">add_frozen_address</a>&lt;T&gt;(addr, <b>false</b>, ctx);
}
</code></pre>



</details>

<a name="sui_coin_regulation_unfreeze_address"></a>

## Function `unfreeze_address`

Unfreeze <code>addr</code> for <code>T</code>. Aborts unless <code>addr</code> is frozen, not permanently, has no outstanding
<code><a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a></code>, and the current epoch is later than that of its last <code><a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a></code> destroy, if any.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_unfreeze_address">unfreeze_address</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, _cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">sui::coin_regulation::ComplianceCap</a>&lt;T&gt;, addr: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_unfreeze_address">unfreeze_address</a>&lt;T&gt;(
    list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>,
    _cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a>&lt;T&gt;,
    addr: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> key = <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressKey">FrozenAddressKey</a>&lt;T&gt;(addr);
    <b>assert</b>!(df::exists(&list.id, key), <a href="../sui/coin_regulation.md#sui_coin_regulation_ENotFrozen">ENotFrozen</a>);
    <b>let</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressState">FrozenAddressState</a> {
        <a href="../sui/coin_regulation.md#sui_coin_regulation_seize_auths">seize_auths</a>,
        permanent,
        <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_epoch">frozen_epoch</a>: _,
        unfreeze_after_epoch,
    } = df::remove(&<b>mut</b> list.id, key);
    <b>assert</b>!(<a href="../sui/coin_regulation.md#sui_coin_regulation_seize_auths">seize_auths</a>.is_empty(), <a href="../sui/coin_regulation.md#sui_coin_regulation_ESeizeAuthsOutstanding">ESeizeAuthsOutstanding</a>);
    <b>assert</b>!(!permanent, <a href="../sui/coin_regulation.md#sui_coin_regulation_EPermanentlyFrozen">EPermanentlyFrozen</a>);
    unfreeze_after_epoch.do!(|epoch| <b>assert</b>!(ctx.epoch() &gt; epoch, <a href="../sui/coin_regulation.md#sui_coin_regulation_EUnfreezeTooEarly">EUnfreezeTooEarly</a>));
    <b>let</b> state: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeState">CoinTypeState</a> = df::borrow_mut(&<b>mut</b> list.id, <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeKey">CoinTypeKey</a>&lt;T&gt;());
    state.num_frozen_addresses = state.num_frozen_addresses - 1;
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/coin_regulation.md#sui_coin_regulation_AddressUnfrozen">AddressUnfrozen</a>&lt;T&gt;(addr));
}
</code></pre>



</details>

<a name="sui_coin_regulation_permanently_freeze_address"></a>

## Function `permanently_freeze_address`

Freeze <code>addr</code> for <code>T</code> if it is not frozen, and prevent it from ever being unfrozen.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_permanently_freeze_address">permanently_freeze_address</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, _cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">sui::coin_regulation::ComplianceCap</a>&lt;T&gt;, addr: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_permanently_freeze_address">permanently_freeze_address</a>&lt;T&gt;(
    list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>,
    _cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a>&lt;T&gt;,
    addr: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> key = <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressKey">FrozenAddressKey</a>&lt;T&gt;(addr);
    <b>if</b> (df::exists(&list.id, key)) {
        df::borrow_mut&lt;_, <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressState">FrozenAddressState</a>&gt;(&<b>mut</b> list.id, key).permanent = <b>true</b>;
    } <b>else</b> {
        list.<a href="../sui/coin_regulation.md#sui_coin_regulation_add_frozen_address">add_frozen_address</a>&lt;T&gt;(addr, <b>true</b>, ctx);
    };
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/coin_regulation.md#sui_coin_regulation_AddressPermanentlyFrozen">AddressPermanentlyFrozen</a>&lt;T&gt;(addr));
}
</code></pre>



</details>

<a name="sui_coin_regulation_issue_seize_auth"></a>

## Function `issue_seize_auth`

Issue a <code><a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a></code> for the frozen <code>addr</code>. Aborts if <code>addr</code> is not frozen.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_issue_seize_auth">issue_seize_auth</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, _cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">sui::coin_regulation::ComplianceCap</a>&lt;T&gt;, addr: <b>address</b>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">sui::coin_regulation::SeizeAuth</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_issue_seize_auth">issue_seize_auth</a>&lt;T&gt;(
    list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>,
    _cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a>&lt;T&gt;,
    addr: <b>address</b>,
    ctx: &<b>mut</b> TxContext,
): <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a>&lt;T&gt; {
    <b>let</b> key = <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressKey">FrozenAddressKey</a>&lt;T&gt;(addr);
    <b>assert</b>!(df::exists(&list.id, key), <a href="../sui/coin_regulation.md#sui_coin_regulation_ENotFrozen">ENotFrozen</a>);
    <b>let</b> state: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressState">FrozenAddressState</a> = df::borrow_mut(&<b>mut</b> list.id, key);
    <b>let</b> auth = <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a>&lt;T&gt; {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a>: addr,
        <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_epoch">frozen_epoch</a>: state.<a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_epoch">frozen_epoch</a>,
    };
    <b>let</b> id = <a href="../sui/object.md#sui_object_id">object::id</a>(&auth);
    state.<a href="../sui/coin_regulation.md#sui_coin_regulation_seize_auths">seize_auths</a>.insert(id);
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuthIssued">SeizeAuthIssued</a>&lt;T&gt; { id, <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a>: addr });
    auth
}
</code></pre>



</details>

<a name="sui_coin_regulation_destroy_seize_auth"></a>

## Function `destroy_seize_auth`

Destroy <code>auth</code> and remove it from its address's outstanding <code><a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a></code>s.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_destroy_seize_auth">destroy_seize_auth</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, auth: <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">sui::coin_regulation::SeizeAuth</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_destroy_seize_auth">destroy_seize_auth</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>, auth: <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a>&lt;T&gt;, ctx: &<b>mut</b> TxContext) {
    <b>let</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a> { id, <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a>, <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_epoch">frozen_epoch</a>: _ } = auth;
    <b>let</b> auth_id = id.to_inner();
    <b>let</b> state: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressState">FrozenAddressState</a> = df::borrow_mut(
        &<b>mut</b> list.id,
        <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressKey">FrozenAddressKey</a>&lt;T&gt;(<a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a>),
    );
    state.<a href="../sui/coin_regulation.md#sui_coin_regulation_seize_auths">seize_auths</a>.remove(&auth_id);
    state.unfreeze_after_epoch = option::some(ctx.epoch());
    id.delete();
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuthDestroyed">SeizeAuthDestroyed</a>&lt;T&gt; { id: auth_id, <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a> });
}
</code></pre>



</details>

<a name="sui_coin_regulation_transfer_seize_auth"></a>

## Function `transfer_seize_auth`

Transfer <code>auth</code> to <code>recipient</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_transfer_seize_auth">transfer_seize_auth</a>&lt;T&gt;(auth: <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">sui::coin_regulation::SeizeAuth</a>&lt;T&gt;, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_transfer_seize_auth">transfer_seize_auth</a>&lt;T&gt;(auth: <a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a>&lt;T&gt;, recipient: <b>address</b>) {
    <a href="../sui/transfer.md#sui_transfer_transfer">transfer::transfer</a>(auth, recipient)
}
</code></pre>



</details>

<a name="sui_coin_regulation_enable_global_pause"></a>

## Function `enable_global_pause`

Globally pause <code>T</code>. Aborts if <code>cap</code> does not allow global pause.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_enable_global_pause">enable_global_pause</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">sui::coin_regulation::ComplianceCap</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_enable_global_pause">enable_global_pause</a>&lt;T&gt;(
    list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>,
    cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a>&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(cap.<a href="../sui/coin_regulation.md#sui_coin_regulation_allow_global_pause">allow_global_pause</a>, <a href="../sui/coin_regulation.md#sui_coin_regulation_EGlobalPauseNotAllowed">EGlobalPauseNotAllowed</a>);
    list.<a href="../sui/coin_regulation.md#sui_coin_regulation_coin_type_state_mut_or_add">coin_type_state_mut_or_add</a>&lt;T&gt;().paused = <b>true</b>;
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypePaused">CoinTypePaused</a>&lt;T&gt;());
}
</code></pre>



</details>

<a name="sui_coin_regulation_disable_global_pause"></a>

## Function `disable_global_pause`

Lift the global pause of <code>T</code>. Aborts if <code>cap</code> does not allow global pause.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_disable_global_pause">disable_global_pause</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">sui::coin_regulation::ComplianceCap</a>&lt;T&gt;, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_disable_global_pause">disable_global_pause</a>&lt;T&gt;(
    list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>,
    cap: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a>&lt;T&gt;,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(cap.<a href="../sui/coin_regulation.md#sui_coin_regulation_allow_global_pause">allow_global_pause</a>, <a href="../sui/coin_regulation.md#sui_coin_regulation_EGlobalPauseNotAllowed">EGlobalPauseNotAllowed</a>);
    df::get_mut_do!(&<b>mut</b> list.id, <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeKey">CoinTypeKey</a>&lt;T&gt;(), |state: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeState">CoinTypeState</a>| {
        state.paused = <b>false</b>
    });
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeUnpaused">CoinTypeUnpaused</a>&lt;T&gt;());
}
</code></pre>



</details>

<a name="sui_coin_regulation_is_frozen"></a>

## Function `is_frozen`

Whether <code>addr</code> is frozen for <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_is_frozen">is_frozen</a>&lt;T&gt;(list: &<a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_is_frozen">is_frozen</a>&lt;T&gt;(list: &<a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>, addr: <b>address</b>): bool {
    df::exists(&list.id, <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressKey">FrozenAddressKey</a>&lt;T&gt;(addr))
}
</code></pre>



</details>

<a name="sui_coin_regulation_is_permanently_frozen"></a>

## Function `is_permanently_frozen`

Whether <code>addr</code> is permanently frozen for <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_is_permanently_frozen">is_permanently_frozen</a>&lt;T&gt;(list: &<a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_is_permanently_frozen">is_permanently_frozen</a>&lt;T&gt;(list: &<a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>, addr: <b>address</b>): bool {
    df::get_fold!(&list.id, <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressKey">FrozenAddressKey</a>&lt;T&gt;(addr), <b>false</b>, |state: &<a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressState">FrozenAddressState</a>| {
        state.permanent
    })
}
</code></pre>



</details>

<a name="sui_coin_regulation_is_global_pause"></a>

## Function `is_global_pause`

Whether <code>T</code> is globally paused.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_is_global_pause">is_global_pause</a>&lt;T&gt;(list: &<a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_is_global_pause">is_global_pause</a>&lt;T&gt;(list: &<a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>): bool {
    df::get_fold!(&list.id, <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeKey">CoinTypeKey</a>&lt;T&gt;(), <b>false</b>, |state: &<a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeState">CoinTypeState</a>| state.paused)
}
</code></pre>



</details>

<a name="sui_coin_regulation_seize_auths"></a>

## Function `seize_auths`

The IDs of the outstanding <code><a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a></code>s for <code>addr</code>, empty if <code>addr</code> is not frozen.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_seize_auths">seize_auths</a>&lt;T&gt;(list: &<a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, addr: <b>address</b>): vector&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_seize_auths">seize_auths</a>&lt;T&gt;(list: &<a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>, addr: <b>address</b>): vector&lt;ID&gt; {
    df::get_fold!(&list.id, <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressKey">FrozenAddressKey</a>&lt;T&gt;(addr), vector[], |state: &<a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressState">FrozenAddressState</a>| {
        *state.<a href="../sui/coin_regulation.md#sui_coin_regulation_seize_auths">seize_auths</a>.keys()
    })
}
</code></pre>



</details>

<a name="sui_coin_regulation_allow_global_pause"></a>

## Function `allow_global_pause`

Whether <code>cap</code> may globally pause <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_allow_global_pause">allow_global_pause</a>&lt;T&gt;(cap: &<a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">sui::coin_regulation::ComplianceCap</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_allow_global_pause">allow_global_pause</a>&lt;T&gt;(cap: &<a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a>&lt;T&gt;): bool {
    cap.<a href="../sui/coin_regulation.md#sui_coin_regulation_allow_global_pause">allow_global_pause</a>
}
</code></pre>



</details>

<a name="sui_coin_regulation_frozen_address"></a>

## Function `frozen_address`

The address <code>auth</code> authorizes seizing from.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a>&lt;T&gt;(auth: &<a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">sui::coin_regulation::SeizeAuth</a>&lt;T&gt;): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a>&lt;T&gt;(auth: &<a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a>&lt;T&gt;): <b>address</b> {
    auth.<a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a>
}
</code></pre>



</details>

<a name="sui_coin_regulation_frozen_epoch"></a>

## Function `frozen_epoch`

The epoch in which <code><a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_address">frozen_address</a></code> was frozen. Seizing is only allowed in a later epoch.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_epoch">frozen_epoch</a>&lt;T&gt;(auth: &<a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">sui::coin_regulation::SeizeAuth</a>&lt;T&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_epoch">frozen_epoch</a>&lt;T&gt;(auth: &<a href="../sui/coin_regulation.md#sui_coin_regulation_SeizeAuth">SeizeAuth</a>&lt;T&gt;): u64 {
    auth.<a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_epoch">frozen_epoch</a>
}
</code></pre>



</details>

<a name="sui_coin_regulation_new_compliance_cap"></a>

## Function `new_compliance_cap`

A new <code><a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a></code> for <code>T</code>, for a coin created through the coin registry.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_new_compliance_cap">new_compliance_cap</a>&lt;T&gt;(<a href="../sui/coin_regulation.md#sui_coin_regulation_allow_global_pause">allow_global_pause</a>: bool, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">sui::coin_regulation::ComplianceCap</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_new_compliance_cap">new_compliance_cap</a>&lt;T&gt;(
    <a href="../sui/coin_regulation.md#sui_coin_regulation_allow_global_pause">allow_global_pause</a>: bool,
    ctx: &<b>mut</b> TxContext,
): <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a>&lt;T&gt; {
    <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a> { id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx), <a href="../sui/coin_regulation.md#sui_coin_regulation_allow_global_pause">allow_global_pause</a> }
}
</code></pre>



</details>

<a name="sui_coin_regulation_migrate_from_deny_list"></a>

## Function `migrate_from_deny_list`

Replace <code>cap</code> with a <code><a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a></code> that keeps its object ID, carrying over <code>T</code>'s legacy
next-epoch global pause and freezing each of <code>frozen_addresses</code> denied in the legacy next-epoch
list.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_migrate_from_deny_list">migrate_from_deny_list</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> <a href="../sui/deny_list.md#sui_deny_list_DenyList">sui::deny_list::DenyList</a>, cap: <a href="../sui/coin.md#sui_coin_DenyCapV2">sui::coin::DenyCapV2</a>&lt;T&gt;, frozen_addresses: vector&lt;<b>address</b>&gt;, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">sui::coin_regulation::ComplianceCap</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_migrate_from_deny_list">migrate_from_deny_list</a>&lt;T&gt;(
    list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>,
    <a href="../sui/deny_list.md#sui_deny_list">deny_list</a>: &<b>mut</b> DenyList,
    cap: DenyCapV2&lt;T&gt;,
    frozen_addresses: vector&lt;<b>address</b>&gt;,
    ctx: &TxContext,
): <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a>&lt;T&gt; {
    <b>let</b> key = <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeKey">CoinTypeKey</a>&lt;T&gt;();
    <b>assert</b>!(!df::exists(&list.id, key), <a href="../sui/coin_regulation.md#sui_coin_regulation_ECoinTypeStateExists">ECoinTypeStateExists</a>);
    <b>let</b> paused = <a href="../sui/coin.md#sui_coin_deny_list_v2_is_global_pause_enabled_next_epoch">coin::deny_list_v2_is_global_pause_enabled_next_epoch</a>&lt;T&gt;(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>);
    df::add(&<b>mut</b> list.id, key, <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeState">CoinTypeState</a> { paused, num_frozen_addresses: 0 });
    <b>let</b> (id, <a href="../sui/coin_regulation.md#sui_coin_regulation_allow_global_pause">allow_global_pause</a>) = <a href="../sui/coin.md#sui_coin_destroy_deny_cap_v2">coin::destroy_deny_cap_v2</a>(cap);
    <b>let</b> cap = <a href="../sui/coin_regulation.md#sui_coin_regulation_ComplianceCap">ComplianceCap</a>&lt;T&gt; { id, <a href="../sui/coin_regulation.md#sui_coin_regulation_allow_global_pause">allow_global_pause</a> };
    frozen_addresses.do!(|addr| {
        <b>if</b> (
            <a href="../sui/coin.md#sui_coin_deny_list_v2_contains_next_epoch">coin::deny_list_v2_contains_next_epoch</a>&lt;T&gt;(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>, addr)
                && !list.<a href="../sui/coin_regulation.md#sui_coin_regulation_is_frozen">is_frozen</a>&lt;T&gt;(addr)
        ) list.<a href="../sui/coin_regulation.md#sui_coin_regulation_add_frozen_address">add_frozen_address</a>&lt;T&gt;(addr, <b>false</b>, ctx)
    });
    <a href="../sui/coin.md#sui_coin_deny_list_v2_mark_migrated">coin::deny_list_v2_mark_migrated</a>&lt;T&gt;(<a href="../sui/deny_list.md#sui_deny_list">deny_list</a>);
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/coin_regulation.md#sui_coin_regulation_DenyCapMigrated">DenyCapMigrated</a>&lt;T&gt; { cap: <a href="../sui/object.md#sui_object_id">object::id</a>(&cap) });
    cap
}
</code></pre>



</details>

<a name="sui_coin_regulation_create"></a>

## Function `create`

Create and share the <code><a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a></code>. Only callable by genesis or change_epoch transactions.


<pre><code><b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_create">create</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/coin_regulation.md#sui_coin_regulation_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a> { id: <a href="../sui/object.md#sui_object_freeze_list">object::freeze_list</a>() });
}
</code></pre>



</details>

<a name="sui_coin_regulation_add_frozen_address"></a>

## Function `add_frozen_address`

Freeze <code>addr</code> for <code>T</code> and count it in <code>T</code>'s state. Aborts if <code>addr</code> is already frozen.


<pre><code><b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_add_frozen_address">add_frozen_address</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>, addr: <b>address</b>, permanent: bool, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_add_frozen_address">add_frozen_address</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>, addr: <b>address</b>, permanent: bool, ctx: &TxContext) {
    <b>let</b> key = <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressKey">FrozenAddressKey</a>&lt;T&gt;(addr);
    <b>assert</b>!(!df::exists(&list.id, key), <a href="../sui/coin_regulation.md#sui_coin_regulation_EAlreadyFrozen">EAlreadyFrozen</a>);
    <b>let</b> state = list.<a href="../sui/coin_regulation.md#sui_coin_regulation_coin_type_state_mut_or_add">coin_type_state_mut_or_add</a>&lt;T&gt;();
    state.num_frozen_addresses = state.num_frozen_addresses + 1;
    df::add(
        &<b>mut</b> list.id,
        key,
        <a href="../sui/coin_regulation.md#sui_coin_regulation_FrozenAddressState">FrozenAddressState</a> {
            <a href="../sui/coin_regulation.md#sui_coin_regulation_seize_auths">seize_auths</a>: <a href="../sui/vec_set.md#sui_vec_set_empty">vec_set::empty</a>(),
            permanent,
            <a href="../sui/coin_regulation.md#sui_coin_regulation_frozen_epoch">frozen_epoch</a>: ctx.epoch(),
            unfreeze_after_epoch: option::none(),
        },
    );
    <a href="../sui/event.md#sui_event_emit">event::emit</a>(<a href="../sui/coin_regulation.md#sui_coin_regulation_AddressFrozen">AddressFrozen</a>&lt;T&gt;(addr));
}
</code></pre>



</details>

<a name="sui_coin_regulation_coin_type_state_mut_or_add"></a>

## Function `coin_type_state_mut_or_add`

The state of <code>T</code>, added if <code>T</code> has none.


<pre><code><b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_coin_type_state_mut_or_add">coin_type_state_mut_or_add</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">sui::coin_regulation::FreezeList</a>): &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeState">sui::coin_regulation::CoinTypeState</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_coin_type_state_mut_or_add">coin_type_state_mut_or_add</a>&lt;T&gt;(list: &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_FreezeList">FreezeList</a>): &<b>mut</b> <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeState">CoinTypeState</a> {
    df::borrow_mut_or_add!(
        &<b>mut</b> list.id,
        <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeKey">CoinTypeKey</a>&lt;T&gt;(),
        <a href="../sui/coin_regulation.md#sui_coin_regulation_CoinTypeState">CoinTypeState</a> { paused: <b>false</b>, num_frozen_addresses: 0 },
    )
}
</code></pre>



</details>
