---
title: Module `sui::coin_registry`
---

Defines the system object for managing coin data in a central
registry. This module provides a centralized way to store and manage
metadata for all coin types in the Sui ecosystem, including their
supply information, regulatory status, and metadata capabilities.


-  [Struct `CoinRegistry`](#sui_coin_registry_CoinRegistry)
-  [Struct `ExtraField`](#sui_coin_registry_ExtraField)
-  [Struct `CurrencyKey`](#sui_coin_registry_CurrencyKey)
-  [Struct `MetadataCap`](#sui_coin_registry_MetadataCap)
-  [Struct `Currency`](#sui_coin_registry_Currency)
-  [Struct `CurrencyBuilder`](#sui_coin_registry_CurrencyBuilder)
-  [Enum `SupplyState`](#sui_coin_registry_SupplyState)
-  [Enum `RegulatedState`](#sui_coin_registry_RegulatedState)
-  [Enum `MetadataCapState`](#sui_coin_registry_MetadataCapState)
-  [Constants](#@Constants_0)
-  [Function `new_currency`](#sui_coin_registry_new_currency)
-  [Function `new_currency_dyn`](#sui_coin_registry_new_currency_dyn)
-  [Function `claim_metadata_cap`](#sui_coin_registry_claim_metadata_cap)
-  [Function `make_regulated`](#sui_coin_registry_make_regulated)
-  [Function `make_supply_fixed`](#sui_coin_registry_make_supply_fixed)
-  [Function `make_supply_deflationary`](#sui_coin_registry_make_supply_deflationary)
-  [Function `finalize`](#sui_coin_registry_finalize)
-  [Function `finalize_registration`](#sui_coin_registry_finalize_registration)
-  [Function `delete_metadata_cap`](#sui_coin_registry_delete_metadata_cap)
-  [Function `inner_mut`](#sui_coin_registry_inner_mut)
-  [Function `burn`](#sui_coin_registry_burn)
-  [Function `set_name`](#sui_coin_registry_set_name)
-  [Function `set_symbol`](#sui_coin_registry_set_symbol)
-  [Function `set_description`](#sui_coin_registry_set_description)
-  [Function `set_icon_url`](#sui_coin_registry_set_icon_url)
-  [Function `set_treasury_cap_id`](#sui_coin_registry_set_treasury_cap_id)
-  [Function `migrate_legacy_metadata`](#sui_coin_registry_migrate_legacy_metadata)
-  [Function `update_from_legacy_metadata`](#sui_coin_registry_update_from_legacy_metadata)
-  [Function `delete_migrated_legacy_metadata`](#sui_coin_registry_delete_migrated_legacy_metadata)
-  [Function `migrate_regulated_state_by_metadata`](#sui_coin_registry_migrate_regulated_state_by_metadata)
-  [Function `migrate_regulated_state_by_cap`](#sui_coin_registry_migrate_regulated_state_by_cap)
-  [Function `decimals`](#sui_coin_registry_decimals)
-  [Function `name`](#sui_coin_registry_name)
-  [Function `symbol`](#sui_coin_registry_symbol)
-  [Function `description`](#sui_coin_registry_description)
-  [Function `icon_url`](#sui_coin_registry_icon_url)
-  [Function `is_metadata_cap_claimed`](#sui_coin_registry_is_metadata_cap_claimed)
-  [Function `metadata_cap_id`](#sui_coin_registry_metadata_cap_id)
-  [Function `treasury_cap_id`](#sui_coin_registry_treasury_cap_id)
-  [Function `deny_cap_id`](#sui_coin_registry_deny_cap_id)
-  [Function `is_supply_fixed`](#sui_coin_registry_is_supply_fixed)
-  [Function `is_supply_deflationary`](#sui_coin_registry_is_supply_deflationary)
-  [Function `is_regulated`](#sui_coin_registry_is_regulated)
-  [Function `total_supply`](#sui_coin_registry_total_supply)
-  [Function `exists`](#sui_coin_registry_exists)
-  [Function `inner`](#sui_coin_registry_inner)
-  [Function `coin_registry_id`](#sui_coin_registry_coin_registry_id)
-  [Function `create`](#sui_coin_registry_create)
-  [Macro function `is_ascii_printable`](#sui_coin_registry_is_ascii_printable)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/accumulator.md#sui_accumulator">sui::accumulator</a>;
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
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/table.md#sui_table">sui::table</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/url.md#sui_url">sui::url</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
<b>use</b> <a href="../sui/vec_set.md#sui_vec_set">sui::vec_set</a>;
</code></pre>



<a name="sui_coin_registry_CoinRegistry"></a>

## Struct `CoinRegistry`

System object found at address 0xc that stores coin data for all
registered coin types. This is a shared object that acts as a central
registry for coin metadata, supply information, and regulatory status.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a> <b>has</b> key, store
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

<a name="sui_coin_registry_ExtraField"></a>

## Struct `ExtraField`

Store only object that enables more flexible coin data
registration, allowing for additional fields to be added
without changing the Currency structure.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_registry.md#sui_coin_registry_ExtraField">ExtraField</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: <a href="../std/type_name.md#std_type_name_TypeName">std::type_name::TypeName</a></code>
</dt>
<dd>
</dd>
<dt>
<code>1: vector&lt;u8&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_coin_registry_CurrencyKey"></a>

## Struct `CurrencyKey`

Key used to access coin metadata hung off the <code><a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a></code>
object. This key can be versioned to allow for future changes
to the metadata object.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_registry.md#sui_coin_registry_CurrencyKey">CurrencyKey</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_coin_registry_MetadataCap"></a>

## Struct `MetadataCap`

Capability object that gates metadata (name, description, icon_url, symbol)
changes in the <code><a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a></code>. It can only be created (or claimed) once, and can
be deleted to prevent changes to the <code><a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a></code> metadata.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;<b>phantom</b> T&gt; <b>has</b> key, store
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

<a name="sui_coin_registry_Currency"></a>

## Struct `Currency`

Currency object that stores comprehensive information about a coin type.
This includes metadata like name, symbol, and description, as well as
supply and regulatory status information.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;<b>phantom</b> T&gt; <b>has</b> key
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
<code><a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8</code>
</dt>
<dd>
 Number of decimal places the coin uses for display purposes
</dd>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 Human-readable name for the token
</dd>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 Short symbol/ticker for the token
</dd>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 Detailed description of the token
</dd>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
 URL for the token's icon/logo
</dd>
<dt>
<code>supply: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/coin_registry.md#sui_coin_registry_SupplyState">sui::coin_registry::SupplyState</a>&lt;T&gt;&gt;</code>
</dt>
<dd>
 Current supply state of the coin (fixed supply or unknown)
 Note: We're using <code>Option</code> because <code><a href="../sui/coin_registry.md#sui_coin_registry_SupplyState">SupplyState</a></code> does not have drop,
 meaning we cannot swap out its value at a later state.
</dd>
<dt>
<code>regulated: <a href="../sui/coin_registry.md#sui_coin_registry_RegulatedState">sui::coin_registry::RegulatedState</a></code>
</dt>
<dd>
 Regulatory status of the coin (regulated with deny cap or unknown)
</dd>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap_id">treasury_cap_id</a>: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;</code>
</dt>
<dd>
 ID of the treasury cap for this coin type, if registered
</dd>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a>: <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCapState">sui::coin_registry::MetadataCapState</a></code>
</dt>
<dd>
 ID of the metadata capability for this coin type, if claimed
</dd>
<dt>
<code>extra_fields: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_ExtraField">sui::coin_registry::ExtraField</a>&gt;</code>
</dt>
<dd>
 Additional fields for extensibility
</dd>
</dl>


</details>

<a name="sui_coin_registry_CurrencyBuilder"></a>

## Struct `CurrencyBuilder`

Hot potato wrapper to enforce registration after "create_currency" data creation.
Destroyed in the <code><a href="../sui/coin_registry.md#sui_coin_registry_finalize">finalize</a></code> call and either transferred to the <code><a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a></code>
(in case of an OTW registration) or shared directly (for dynamically created
currencies).


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">CurrencyBuilder</a>&lt;<b>phantom</b> T&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>data: <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>is_otw: bool</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_coin_registry_SupplyState"></a>

## Enum `SupplyState`

Supply state of a coin type, which can be fixed (with a known supply)
or unknown (supply not yet registered in the registry).


<pre><code><b>public</b> <b>enum</b> <a href="../sui/coin_registry.md#sui_coin_registry_SupplyState">SupplyState</a>&lt;<b>phantom</b> T&gt; <b>has</b> store
</code></pre>



<details>
<summary>Variants</summary>


<dl>
<dt>
Variant <code>Fixed</code>
</dt>
<dd>
 Coin has a fixed supply with the given Supply object
</dd>

<dl>
<dt>
<code>0: <a href="../sui/balance.md#sui_balance_Supply">sui::balance::Supply</a>&lt;T&gt;</code>
</dt>
<dd>
</dd>
</dl>

<dt>
Variant <code>Deflationary</code>
</dt>
<dd>
 Coin has a supply that can ONLY decrease.
 TODO: Public burn function OR capability? :)
</dd>

<dl>
<dt>
<code>0: <a href="../sui/balance.md#sui_balance_Supply">sui::balance::Supply</a>&lt;T&gt;</code>
</dt>
<dd>
</dd>
</dl>

<dt>
Variant <code>Unknown</code>
</dt>
<dd>
 Supply information is not yet known or registered
</dd>
</dl>


</details>

<a name="sui_coin_registry_RegulatedState"></a>

## Enum `RegulatedState`

Regulated state of a coin type, which can be regulated with a deny cap
for address restrictions, or unknown if not regulated.


<pre><code><b>public</b> <b>enum</b> <a href="../sui/coin_registry.md#sui_coin_registry_RegulatedState">RegulatedState</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Variants</summary>


<dl>
<dt>
Variant <code>Regulated</code>
</dt>
<dd>
 Coin is regulated with a deny cap for address restrictions
</dd>

<dl>
<dt>
<code>cap: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>


<dl>
<dt>
<code>variant: u8</code>
</dt>
<dd>
</dd>
</dl>

<dt>
Variant <code>Unregulated</code>
</dt>
<dd>
 The coin has been created without deny list
</dd>
<dt>
Variant <code>Unknown</code>
</dt>
<dd>
 Coin is not regulated or regulatory status is unknown.
 This is the result of a legacy migration for that coin (from <code><a href="../sui/coin.md#sui_coin">coin</a>.<b>move</b></code> constructors)
</dd>
</dl>


</details>

<a name="sui_coin_registry_MetadataCapState"></a>

## Enum `MetadataCapState`

State of the <code><a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a></code> for a single <code><a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a></code>.


<pre><code><b>public</b> <b>enum</b> <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCapState">MetadataCapState</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Variants</summary>


<dl>
<dt>
Variant <code>Claimed</code>
</dt>
<dd>
 The metadata cap has been claimed.
</dd>

<dl>
<dt>
<code>0: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>

<dt>
Variant <code>Unclaimed</code>
</dt>
<dd>
 The metadata cap has not been claimed.
</dd>
<dt>
Variant <code>Deleted</code>
</dt>
<dd>
 The metadata cap has been deleted (so the <code><a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a></code> metadata cannot be updated).
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_coin_registry_ECurrencyNotFound"></a>

No Currency found for this coin type.


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_ECurrencyNotFound">ECurrencyNotFound</a>: u64 = 0;
</code></pre>



<a name="sui_coin_registry_EMetadataCapAlreadyClaimed"></a>

Metadata cap already claimed


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_EMetadataCapAlreadyClaimed">EMetadataCapAlreadyClaimed</a>: u64 = 1;
</code></pre>



<a name="sui_coin_registry_ENotSystemAddress"></a>

Only the system address can create the registry


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_ENotSystemAddress">ENotSystemAddress</a>: u64 = 2;
</code></pre>



<a name="sui_coin_registry_ECurrencyAlreadyExists"></a>

Currency for this coin type already exists


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_ECurrencyAlreadyExists">ECurrencyAlreadyExists</a>: u64 = 3;
</code></pre>



<a name="sui_coin_registry_EDenyListStateAlreadySet"></a>

Attempt to set the deny list state permissionlessly while it has already been set.


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_EDenyListStateAlreadySet">EDenyListStateAlreadySet</a>: u64 = 4;
</code></pre>



<a name="sui_coin_registry_EMetadataCapNotClaimed"></a>

Tries to delete legacy metadata without having claimed the management capability.


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_EMetadataCapNotClaimed">EMetadataCapNotClaimed</a>: u64 = 5;
</code></pre>



<a name="sui_coin_registry_ECannotUpdateManagedMetadata"></a>

Attempt to update <code><a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a></code> with legacy metadata after the <code><a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a></code> has
been claimed. Updates are only allowed if the <code><a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a></code> has not yet been
claimed or deleted.


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_ECannotUpdateManagedMetadata">ECannotUpdateManagedMetadata</a>: u64 = 6;
</code></pre>



<a name="sui_coin_registry_EInvalidSymbol"></a>

Attempt to set the symbol to a non-ASCII printable character


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_EInvalidSymbol">EInvalidSymbol</a>: u64 = 7;
</code></pre>



<a name="sui_coin_registry_EDenyCapAlreadyCreated"></a>

Attempt to set the deny cap twice.


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_EDenyCapAlreadyCreated">EDenyCapAlreadyCreated</a>: u64 = 8;
</code></pre>



<a name="sui_coin_registry_ECurrencyAlreadyRegistered"></a>

Attempt to migrate legacy metadata for a <code><a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a></code> that already exists.


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_ECurrencyAlreadyRegistered">ECurrencyAlreadyRegistered</a>: u64 = 9;
</code></pre>



<a name="sui_coin_registry_REGULATED_COIN_VARIANT"></a>

Incremental identifier for regulated coin versions in the deny list.
0 here matches DenyCapV2 world.
TODO: Fix wording here.


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_REGULATED_COIN_VARIANT">REGULATED_COIN_VARIANT</a>: u8 = 0;
</code></pre>



<a name="sui_coin_registry_new_currency"></a>

## Function `new_currency`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_new_currency">new_currency</a>&lt;T: drop&gt;(otw: T, <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8, <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">sui::coin_registry::CurrencyBuilder</a>&lt;T&gt;, <a href="../sui/coin.md#sui_coin_TreasuryCap">sui::coin::TreasuryCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_new_currency">new_currency</a>&lt;T: drop&gt;(
    otw: T,
    <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8,
    <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: String,
    ctx: &<b>mut</b> TxContext,
): (<a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">CurrencyBuilder</a>&lt;T&gt;, TreasuryCap&lt;T&gt;) {
    // Make sure there's only one instance of the type T, using an OTW check.
    <b>assert</b>!(<a href="../sui/types.md#sui_types_is_one_time_witness">sui::types::is_one_time_witness</a>(&otw));
    // Hacky check to make sure the Symbol is ASCII.
    <b>assert</b>!(<a href="../sui/coin_registry.md#sui_coin_registry_is_ascii_printable">is_ascii_printable</a>!(&<a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>), <a href="../sui/coin_registry.md#sui_coin_registry_EInvalidSymbol">EInvalidSymbol</a>);
    <b>let</b> treasury_cap = <a href="../sui/coin.md#sui_coin_new_treasury_cap">coin::new_treasury_cap</a>(ctx);
    <b>let</b> metadata = <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt; {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>,
        supply: option::some(SupplyState::Unknown),
        regulated: RegulatedState::Unregulated,
        <a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap_id">treasury_cap_id</a>: option::some(<a href="../sui/object.md#sui_object_id">object::id</a>(&treasury_cap)),
        <a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a>: MetadataCapState::Unclaimed,
        extra_fields: <a href="../sui/vec_map.md#sui_vec_map_empty">vec_map::empty</a>(),
    };
    (<a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">CurrencyBuilder</a> { data: metadata, is_otw: <b>true</b> }, treasury_cap)
}
</code></pre>



</details>

<a name="sui_coin_registry_new_currency_dyn"></a>

## Function `new_currency_dyn`

Create a currency dynamically.
TODO: Add verifier rule, as this needs to only be callable by the module that defines <code>T</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_new_currency_dyn">new_currency_dyn</a>&lt;T: key&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>, <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8, <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): (<a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">sui::coin_registry::CurrencyBuilder</a>&lt;T&gt;, <a href="../sui/coin.md#sui_coin_TreasuryCap">sui::coin::TreasuryCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_new_currency_dyn">new_currency_dyn</a>&lt;T: /* internal */ key&gt;(
    registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>,
    <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8,
    <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: String,
    ctx: &<b>mut</b> TxContext,
): (<a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">CurrencyBuilder</a>&lt;T&gt;, TreasuryCap&lt;T&gt;) {
    // Unlike OTW creation, the guarantee on not having duplicate currencies come from the
    // <a href="../sui/coin.md#sui_coin">coin</a> registry.
    <b>assert</b>!(!registry.<a href="../sui/coin_registry.md#sui_coin_registry_exists">exists</a>&lt;T&gt;());
    <b>let</b> treasury_cap = <a href="../sui/coin.md#sui_coin_new_treasury_cap">coin::new_treasury_cap</a>(ctx);
    <b>let</b> metadata = <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt; {
        // TODO: <b>use</b> `derived_object::claim(&<b>mut</b> registry.id, CoinKey&lt;T&gt;())`
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>,
        supply: option::some(SupplyState::Unknown),
        regulated: RegulatedState::Unregulated,
        <a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap_id">treasury_cap_id</a>: option::some(<a href="../sui/object.md#sui_object_id">object::id</a>(&treasury_cap)),
        <a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a>: MetadataCapState::Unclaimed,
        extra_fields: <a href="../sui/vec_map.md#sui_vec_map_empty">vec_map::empty</a>(),
    };
    (<a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">CurrencyBuilder</a> { data: metadata, is_otw: <b>false</b> }, treasury_cap)
}
</code></pre>



</details>

<a name="sui_coin_registry_claim_metadata_cap"></a>

## Function `claim_metadata_cap`

Claim a MetadataCap for a coin type. This is only allowed from the owner of
<code>TreasuryCap</code>, and only once.

Aborts if the metadata capability has already been claimed.
If <code><a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a></code> was deleted, it cannot be claimed!


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_claim_metadata_cap">claim_metadata_cap</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, _: &<a href="../sui/coin.md#sui_coin_TreasuryCap">sui::coin::TreasuryCap</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_claim_metadata_cap">claim_metadata_cap</a>&lt;T&gt;(
    data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;,
    _: &TreasuryCap&lt;T&gt;,
    ctx: &<b>mut</b> TxContext,
): <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt; {
    <b>assert</b>!(!data.<a href="../sui/coin_registry.md#sui_coin_registry_is_metadata_cap_claimed">is_metadata_cap_claimed</a>(), <a href="../sui/coin_registry.md#sui_coin_registry_EMetadataCapAlreadyClaimed">EMetadataCapAlreadyClaimed</a>);
    <b>let</b> id = <a href="../sui/object.md#sui_object_new">object::new</a>(ctx);
    data.<a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a> = MetadataCapState::Claimed(id.to_inner());
    <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a> { id }
}
</code></pre>



</details>

<a name="sui_coin_registry_make_regulated"></a>

## Function `make_regulated`

Allows converting a currency, on init, to regulated, which creates
a <code>DenyCapV2</code> object, and a denylist entry.

This is only possible when initializing a coin (cannot be done for existing coins).


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_make_regulated">make_regulated</a>&lt;T&gt;(init: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">sui::coin_registry::CurrencyBuilder</a>&lt;T&gt;, allow_global_pause: bool, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin.md#sui_coin_DenyCapV2">sui::coin::DenyCapV2</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_make_regulated">make_regulated</a>&lt;T&gt;(
    init: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">CurrencyBuilder</a>&lt;T&gt;,
    allow_global_pause: bool,
    ctx: &<b>mut</b> TxContext,
): DenyCapV2&lt;T&gt; {
    <b>assert</b>!(init.data.regulated == RegulatedState::Unregulated, <a href="../sui/coin_registry.md#sui_coin_registry_EDenyCapAlreadyCreated">EDenyCapAlreadyCreated</a>);
    <b>let</b> deny_cap = <a href="../sui/coin.md#sui_coin_new_deny_cap_v2">coin::new_deny_cap_v2</a>&lt;T&gt;(allow_global_pause, ctx);
    init.<a href="../sui/coin_registry.md#sui_coin_registry_inner_mut">inner_mut</a>().regulated =
        RegulatedState::Regulated {
            cap: <a href="../sui/object.md#sui_object_id">object::id</a>(&deny_cap),
            variant: <a href="../sui/coin_registry.md#sui_coin_registry_REGULATED_COIN_VARIANT">REGULATED_COIN_VARIANT</a>,
        };
    deny_cap
}
</code></pre>



</details>

<a name="sui_coin_registry_make_supply_fixed"></a>

## Function `make_supply_fixed`

Freeze the supply by destroying the TreasuryCap and storing it in the Currency.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_make_supply_fixed">make_supply_fixed</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, cap: <a href="../sui/coin.md#sui_coin_TreasuryCap">sui::coin::TreasuryCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_make_supply_fixed">make_supply_fixed</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, cap: TreasuryCap&lt;T&gt;) {
    match (data.supply.swap(SupplyState::Fixed(cap.into_supply()))) {
        // Impossible: We cannot fix a supply or make a supply deflationary twice.
        SupplyState::Fixed(_supply) | SupplyState::Deflationary(_supply) =&gt; <b>abort</b>,
        // We replaced "unknown" with fixed supply.
        SupplyState::Unknown =&gt; (),
    };
}
</code></pre>



</details>

<a name="sui_coin_registry_make_supply_deflationary"></a>

## Function `make_supply_deflationary`

Make the supply "deflatinary" by destroying the TreasuryCap and taking control of the
supply through the Currency.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_make_supply_deflationary">make_supply_deflationary</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, cap: <a href="../sui/coin.md#sui_coin_TreasuryCap">sui::coin::TreasuryCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_make_supply_deflationary">make_supply_deflationary</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, cap: TreasuryCap&lt;T&gt;) {
    match (data.supply.swap(SupplyState::Deflationary(cap.into_supply()))) {
        // Impossible: We cannot fix a supply or make a supply deflationary twice.
        SupplyState::Fixed(_supply) | SupplyState::Deflationary(_supply) =&gt; <b>abort</b>,
        // We replaced "unknown" with frozen supply.
        SupplyState::Unknown =&gt; (),
    };
}
</code></pre>



</details>

<a name="sui_coin_registry_finalize"></a>

## Function `finalize`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_finalize">finalize</a>&lt;T&gt;(builder: <a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">sui::coin_registry::CurrencyBuilder</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_finalize">finalize</a>&lt;T&gt;(builder: <a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">CurrencyBuilder</a>&lt;T&gt;, ctx: &<b>mut</b> TxContext): <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt; {
    <b>let</b> <a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">CurrencyBuilder</a> { <b>mut</b> data, is_otw } = builder;
    <b>let</b> id = <a href="../sui/object.md#sui_object_new">object::new</a>(ctx);
    data.<a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a> = MetadataCapState::Claimed(id.to_inner());
    <b>if</b> (is_otw) <a href="../sui/transfer.md#sui_transfer_transfer">transfer::transfer</a>(data, <a href="../sui/coin_registry.md#sui_coin_registry_coin_registry_id">coin_registry_id</a>().to_address())
    <b>else</b> <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(data);
    <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt; { id }
}
</code></pre>



</details>

<a name="sui_coin_registry_finalize_registration"></a>

## Function `finalize_registration`

The second step in the "otw" initialization of coin metadata, that takes in the <code><a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;</code> that was
transferred from init, and transforms it in to a "derived address" shared object.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_finalize_registration">finalize_registration</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>, coin_data: <a href="../sui/transfer.md#sui_transfer_Receiving">sui::transfer::Receiving</a>&lt;<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_finalize_registration">finalize_registration</a>&lt;T&gt;(
    registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>,
    coin_data: Receiving&lt;<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    // 1. Consume <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>
    // 2. Re-<a href="../sui/coin_registry.md#sui_coin_registry_create">create</a> it with a "derived" <b>address</b>.
    <b>let</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a> {
        id,
        <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>,
        supply,
        regulated,
        <a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap_id">treasury_cap_id</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a>,
        extra_fields,
    } = <a href="../sui/transfer.md#sui_transfer_receive">transfer::receive</a>(&<b>mut</b> registry.id, coin_data);
    id.delete();
    // Now, <a href="../sui/coin_registry.md#sui_coin_registry_create">create</a> the shared version of the <a href="../sui/coin.md#sui_coin">coin</a> data.
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a> {
        // TODO: Replace this with `derived_object::claim()`
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>,
        supply,
        regulated,
        <a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap_id">treasury_cap_id</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a>,
        extra_fields,
    })
}
</code></pre>



</details>

<a name="sui_coin_registry_delete_metadata_cap"></a>

## Function `delete_metadata_cap`

Delete the metadata cap making further updates of <code><a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a></code> metadata impossible.
This action is IRREVERSIBLE, and the <code><a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a></code> can no longer be claimed.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_delete_metadata_cap">delete_metadata_cap</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, cap: <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_delete_metadata_cap">delete_metadata_cap</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, cap: <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt;) {
    <b>let</b> <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a> { id } = cap;
    data.<a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a> = MetadataCapState::Deleted;
    id.delete();
}
</code></pre>



</details>

<a name="sui_coin_registry_inner_mut"></a>

## Function `inner_mut`

Get mutable reference to the coin data from CurrencyBuilder.
This function is package-private and should only be called by the coin module.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_inner_mut">inner_mut</a>&lt;T&gt;(init: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">sui::coin_registry::CurrencyBuilder</a>&lt;T&gt;): &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_inner_mut">inner_mut</a>&lt;T&gt;(init: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">CurrencyBuilder</a>&lt;T&gt;): &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt; {
    &<b>mut</b> init.data
}
</code></pre>



</details>

<a name="sui_coin_registry_burn"></a>

## Function `burn`

Allows burning coins for deflationary


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_burn">burn</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, <a href="../sui/coin.md#sui_coin">coin</a>: <a href="../sui/coin.md#sui_coin_Coin">sui::coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_burn">burn</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, <a href="../sui/coin.md#sui_coin">coin</a>: Coin&lt;T&gt;) {
    <b>assert</b>!(data.<a href="../sui/coin_registry.md#sui_coin_registry_is_supply_deflationary">is_supply_deflationary</a>());
    match (data.supply.borrow_mut()) {
        SupplyState::Deflationary(supply) =&gt; { supply.decrease_supply(<a href="../sui/coin.md#sui_coin">coin</a>.into_balance()); },
        _ =&gt; <b>abort</b>,
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_set_name"></a>

## Function `set_name`

Enables a metadata cap holder to update a coin's name.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_name">set_name</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_name">set_name</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: String) {
    <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>.to_ascii();
    data.<a href="../sui/coin_registry.md#sui_coin_registry_name">name</a> = <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>;
}
</code></pre>



</details>

<a name="sui_coin_registry_set_symbol"></a>

## Function `set_symbol`

Enables a metadata cap holder to update a coin's symbol.
TODO: Should we kill this? :)


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_symbol">set_symbol</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: <a href="../std/string.md#std_string_String">std::string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_symbol">set_symbol</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: String) {
    <b>assert</b>!(<a href="../sui/coin_registry.md#sui_coin_registry_is_ascii_printable">is_ascii_printable</a>!(&<a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>), <a href="../sui/coin_registry.md#sui_coin_registry_EInvalidSymbol">EInvalidSymbol</a>);
    data.<a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a> = <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>;
}
</code></pre>



</details>

<a name="sui_coin_registry_set_description"></a>

## Function `set_description`

Enables a metadata cap holder to update a coin's description.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_description">set_description</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: <a href="../std/string.md#std_string_String">std::string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_description">set_description</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: String) {
    data.<a href="../sui/coin_registry.md#sui_coin_registry_description">description</a> = <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>;
}
</code></pre>



</details>

<a name="sui_coin_registry_set_icon_url"></a>

## Function `set_icon_url`

Enables a metadata cap holder to update a coin's icon URL.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_icon_url">set_icon_url</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: <a href="../std/string.md#std_string_String">std::string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_icon_url">set_icon_url</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: String) {
    data.<a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a> = <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>;
}
</code></pre>



</details>

<a name="sui_coin_registry_set_treasury_cap_id"></a>

## Function `set_treasury_cap_id`

Register the treasury cap ID for a coin type at a later point.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_treasury_cap_id">set_treasury_cap_id</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, cap: &<a href="../sui/coin.md#sui_coin_TreasuryCap">sui::coin::TreasuryCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_treasury_cap_id">set_treasury_cap_id</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, cap: &TreasuryCap&lt;T&gt;) {
    data.<a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap_id">treasury_cap_id</a>.fill(<a href="../sui/object.md#sui_object_id">object::id</a>(cap));
}
</code></pre>



</details>

<a name="sui_coin_registry_migrate_legacy_metadata"></a>

## Function `migrate_legacy_metadata`

TODO: Register legacy coin metadata to the registry --
This should:
1. Take the old metadata
2. Create a <code><a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;</code> object with a derived address (and share it!)


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_migrate_legacy_metadata">migrate_legacy_metadata</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>, legacy: &<a href="../sui/coin.md#sui_coin_CoinMetadata">sui::coin::CoinMetadata</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_migrate_legacy_metadata">migrate_legacy_metadata</a>&lt;T&gt;(
    registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>,
    legacy: &CoinMetadata&lt;T&gt;,
    ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(!registry.<a href="../sui/coin_registry.md#sui_coin_registry_exists">exists</a>&lt;T&gt;(), <a href="../sui/coin_registry.md#sui_coin_registry_ECurrencyAlreadyRegistered">ECurrencyAlreadyRegistered</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt; {
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx), // TODO: <b>use</b> derived_object::claim()
        <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: legacy.get_decimals(),
        <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: legacy.get_name(),
        <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: legacy.get_symbol().to_string(),
        <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: legacy.get_description(),
        <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: legacy // TODO: not a fan of this!
            .get_icon_url()
            .map!(|<a href="../sui/url.md#sui_url">url</a>| <a href="../sui/url.md#sui_url">url</a>.inner_url().to_string())
            .destroy_or!(b"".to_string()),
        supply: option::some(SupplyState::Unknown),
        regulated: RegulatedState::Unregulated,
        <a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap_id">treasury_cap_id</a>: option::none(),
        <a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a>: MetadataCapState::Unclaimed,
        extra_fields: <a href="../sui/vec_map.md#sui_vec_map_empty">vec_map::empty</a>(),
    });
}
</code></pre>



</details>

<a name="sui_coin_registry_update_from_legacy_metadata"></a>

## Function `update_from_legacy_metadata`

TODO: Allow coin metadata to be updated from legacy as described in our docs.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_update_from_legacy_metadata">update_from_legacy_metadata</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, legacy: &<a href="../sui/coin.md#sui_coin_CoinMetadata">sui::coin::CoinMetadata</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_update_from_legacy_metadata">update_from_legacy_metadata</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, legacy: &CoinMetadata&lt;T&gt;) {
    <b>assert</b>!(!data.<a href="../sui/coin_registry.md#sui_coin_registry_is_metadata_cap_claimed">is_metadata_cap_claimed</a>(), <a href="../sui/coin_registry.md#sui_coin_registry_ECannotUpdateManagedMetadata">ECannotUpdateManagedMetadata</a>);
    data.<a href="../sui/coin_registry.md#sui_coin_registry_name">name</a> = legacy.get_name();
    data.<a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a> = legacy.get_symbol().to_string();
    data.<a href="../sui/coin_registry.md#sui_coin_registry_description">description</a> = legacy.get_description();
    data.<a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a> = legacy.get_decimals();
    data.<a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a> =
        legacy // TODO: not a fan of this!
            .get_icon_url()
            .map!(|<a href="../sui/url.md#sui_url">url</a>| <a href="../sui/url.md#sui_url">url</a>.inner_url().to_string())
            .destroy_or!(b"".to_string());
}
</code></pre>



</details>

<a name="sui_coin_registry_delete_migrated_legacy_metadata"></a>

## Function `delete_migrated_legacy_metadata`

Delete the legacy <code>CoinMetadata</code> object if the metadata cap for the new registry
has already been claimed.

This function is only callable after there's "proof" that the author of the coin
can manage the metadata using the registry system (so having a metadata cap claimed).


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_delete_migrated_legacy_metadata">delete_migrated_legacy_metadata</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, legacy: <a href="../sui/coin.md#sui_coin_CoinMetadata">sui::coin::CoinMetadata</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_delete_migrated_legacy_metadata">delete_migrated_legacy_metadata</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, legacy: CoinMetadata&lt;T&gt;) {
    <b>assert</b>!(data.<a href="../sui/coin_registry.md#sui_coin_registry_is_metadata_cap_claimed">is_metadata_cap_claimed</a>(), <a href="../sui/coin_registry.md#sui_coin_registry_EMetadataCapNotClaimed">EMetadataCapNotClaimed</a>);
    legacy.destroy_metadata();
}
</code></pre>



</details>

<a name="sui_coin_registry_migrate_regulated_state_by_metadata"></a>

## Function `migrate_regulated_state_by_metadata`

Allow migrating the regulated state by access to <code>RegulatedCoinMetadata</code> frozen object.
This is a permissionless operation.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_migrate_regulated_state_by_metadata">migrate_regulated_state_by_metadata</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, metadata: &<a href="../sui/coin.md#sui_coin_RegulatedCoinMetadata">sui::coin::RegulatedCoinMetadata</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_migrate_regulated_state_by_metadata">migrate_regulated_state_by_metadata</a>&lt;T&gt;(
    data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;,
    metadata: &RegulatedCoinMetadata&lt;T&gt;,
) {
    // Only allow <b>if</b> this hasn't been migrated before.
    <b>assert</b>!(data.regulated == RegulatedState::Unknown, <a href="../sui/coin_registry.md#sui_coin_registry_EDenyListStateAlreadySet">EDenyListStateAlreadySet</a>);
    data.regulated =
        RegulatedState::Regulated {
            cap: metadata.<a href="../sui/coin_registry.md#sui_coin_registry_deny_cap_id">deny_cap_id</a>(),
            variant: <a href="../sui/coin_registry.md#sui_coin_registry_REGULATED_COIN_VARIANT">REGULATED_COIN_VARIANT</a>,
        };
}
</code></pre>



</details>

<a name="sui_coin_registry_migrate_regulated_state_by_cap"></a>

## Function `migrate_regulated_state_by_cap`

Allow migrating the regulated state by a <code>DenyCapV2</code> object.
This is a permissioned operation.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_migrate_regulated_state_by_cap">migrate_regulated_state_by_cap</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;, cap: &<a href="../sui/coin.md#sui_coin_DenyCapV2">sui::coin::DenyCapV2</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_migrate_regulated_state_by_cap">migrate_regulated_state_by_cap</a>&lt;T&gt;(data: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;, cap: &DenyCapV2&lt;T&gt;) {
    data.regulated =
        RegulatedState::Regulated {
            cap: <a href="../sui/object.md#sui_object_id">object::id</a>(cap),
            variant: <a href="../sui/coin_registry.md#sui_coin_registry_REGULATED_COIN_VARIANT">REGULATED_COIN_VARIANT</a>,
        };
}
</code></pre>



</details>

<a name="sui_coin_registry_decimals"></a>

## Function `decimals`

Get the number of decimal places for the coin type.a


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): u8 { coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a> }
</code></pre>



</details>

<a name="sui_coin_registry_name"></a>

## Function `name`

Get the human-readable name of the coin.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): String { coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_name">name</a> }
</code></pre>



</details>

<a name="sui_coin_registry_symbol"></a>

## Function `symbol`

Get the symbol/ticker of the coin.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): String { coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a> }
</code></pre>



</details>

<a name="sui_coin_registry_description"></a>

## Function `description`

Get the description of the coin.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): String {
    coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>
}
</code></pre>



</details>

<a name="sui_coin_registry_icon_url"></a>

## Function `icon_url`

Get the icon URL for the coin.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): String { coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a> }
</code></pre>



</details>

<a name="sui_coin_registry_is_metadata_cap_claimed"></a>

## Function `is_metadata_cap_claimed`

Check if the metadata capability has been claimed for this coin type.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_is_metadata_cap_claimed">is_metadata_cap_claimed</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_is_metadata_cap_claimed">is_metadata_cap_claimed</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): bool {
    match (coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a>) {
        MetadataCapState::Claimed(_) | MetadataCapState::Deleted =&gt; <b>true</b>,
        _ =&gt; <b>false</b>,
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_metadata_cap_id"></a>

## Function `metadata_cap_id`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): Option&lt;ID&gt; {
    match (coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_metadata_cap_id">metadata_cap_id</a>) {
        MetadataCapState::Claimed(id) =&gt; option::some(id),
        _ =&gt; option::none(),
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_treasury_cap_id"></a>

## Function `treasury_cap_id`

Get the treasury cap ID for this coin type, if registered.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap_id">treasury_cap_id</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap_id">treasury_cap_id</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): Option&lt;ID&gt; {
    coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap_id">treasury_cap_id</a>
}
</code></pre>



</details>

<a name="sui_coin_registry_deny_cap_id"></a>

## Function `deny_cap_id`

Get the deny cap ID for this coin type, if it's a regulated coin.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_deny_cap_id">deny_cap_id</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_deny_cap_id">deny_cap_id</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): Option&lt;ID&gt; {
    match (coin_data.regulated) {
        RegulatedState::Regulated { cap, .. } =&gt; option::some(cap),
        RegulatedState::Unregulated =&gt; option::none(),
        RegulatedState::Unknown =&gt; option::none(),
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_is_supply_fixed"></a>

## Function `is_supply_fixed`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_is_supply_fixed">is_supply_fixed</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_is_supply_fixed">is_supply_fixed</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): bool {
    match (coin_data.supply.<a href="../sui/borrow.md#sui_borrow">borrow</a>()) {
        SupplyState::Fixed(_) =&gt; <b>true</b>,
        _ =&gt; <b>false</b>,
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_is_supply_deflationary"></a>

## Function `is_supply_deflationary`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_is_supply_deflationary">is_supply_deflationary</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_is_supply_deflationary">is_supply_deflationary</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): bool {
    match (coin_data.supply.<a href="../sui/borrow.md#sui_borrow">borrow</a>()) {
        SupplyState::Deflationary(_) =&gt; <b>true</b>,
        _ =&gt; <b>false</b>,
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_is_regulated"></a>

## Function `is_regulated`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_is_regulated">is_regulated</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_is_regulated">is_regulated</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): bool {
    match (coin_data.regulated) {
        RegulatedState::Regulated { .. } =&gt; <b>true</b>,
        _ =&gt; <b>false</b>,
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_total_supply"></a>

## Function `total_supply`

Get the total supply for the <code><a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;</code> if the Supply is in fixed or
deflationary state. Returns <code>None</code> if the supply is unknown.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_total_supply">total_supply</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_total_supply">total_supply</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt;): Option&lt;u64&gt; {
    match (coin_data.supply.<a href="../sui/borrow.md#sui_borrow">borrow</a>()) {
        SupplyState::Fixed(supply) =&gt; option::some(supply.value()),
        SupplyState::Deflationary(supply) =&gt; option::some(supply.value()),
        SupplyState::Unknown =&gt; option::none(),
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_exists"></a>

## Function `exists`

Check if coin data exists for the given type T in the registry.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_exists">exists</a>&lt;T&gt;(_registry: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_exists">exists</a>&lt;T&gt;(_registry: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>): bool {
    // TODO: `<b>use</b> derived_object::exists()`
    <b>false</b> // TODO: <b>return</b> function call once derived addresses are in!
}
</code></pre>



</details>

<a name="sui_coin_registry_inner"></a>

## Function `inner`

Get immutable reference to the coin data from CurrencyBuilder.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_inner">inner</a>&lt;T&gt;(init: &<a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">sui::coin_registry::CurrencyBuilder</a>&lt;T&gt;): &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">sui::coin_registry::Currency</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_inner">inner</a>&lt;T&gt;(init: &<a href="../sui/coin_registry.md#sui_coin_registry_CurrencyBuilder">CurrencyBuilder</a>&lt;T&gt;): &<a href="../sui/coin_registry.md#sui_coin_registry_Currency">Currency</a>&lt;T&gt; {
    &init.data
}
</code></pre>



</details>

<a name="sui_coin_registry_coin_registry_id"></a>

## Function `coin_registry_id`

Return the ID of the system coin registry object located at address 0xc.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_coin_registry_id">coin_registry_id</a>(): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_coin_registry_id">coin_registry_id</a>(): ID {
    @0xc.to_id()
}
</code></pre>



</details>

<a name="sui_coin_registry_create"></a>

## Function `create`

Create and share the singleton Registry -- this function is
called exactly once, during the upgrade epoch.
Only the system address (0x0) can create the registry.

TODO: use <code>&TxContext</code> and use correct id.


<pre><code><b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_create">create</a>(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_create">create</a>(ctx: &<b>mut</b> TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/coin_registry.md#sui_coin_registry_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a> {
        // id: <a href="../sui/object.md#sui_object_sui_coin_registry_object_id">object::sui_coin_registry_object_id</a>(),
        id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
    });
}
</code></pre>



</details>

<a name="sui_coin_registry_is_ascii_printable"></a>

## Macro function `is_ascii_printable`

Nit: consider adding this function to <code><a href="../std/string.md#std_string">std::string</a></code> in the future.


<pre><code><b>macro</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_is_ascii_printable">is_ascii_printable</a>($s: &<a href="../std/string.md#std_string_String">std::string::String</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>macro</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_is_ascii_printable">is_ascii_printable</a>($s: &String): bool {
    <b>let</b> s = $s;
    s.as_bytes().all!(|b| ascii::is_printable_char(*b))
}
</code></pre>



</details>
