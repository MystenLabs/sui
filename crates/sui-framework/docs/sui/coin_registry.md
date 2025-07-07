---
title: Module `sui::coin_registry`
---

Defines the system object for managing coin data in a central
registry.


-  [Struct `CoinRegistry`](#sui_coin_registry_CoinRegistry)
-  [Struct `ExtraField`](#sui_coin_registry_ExtraField)
-  [Struct `CoinDataKey`](#sui_coin_registry_CoinDataKey)
-  [Struct `MetadataCap`](#sui_coin_registry_MetadataCap)
-  [Struct `CoinData`](#sui_coin_registry_CoinData)
-  [Struct `InitCoinData`](#sui_coin_registry_InitCoinData)
-  [Enum `SupplyState`](#sui_coin_registry_SupplyState)
-  [Enum `RegulatedState`](#sui_coin_registry_RegulatedState)
-  [Constants](#@Constants_0)
-  [Function `coin_registry_id`](#sui_coin_registry_coin_registry_id)
-  [Function `id`](#sui_coin_registry_id)
-  [Function `transfer_to_registry`](#sui_coin_registry_transfer_to_registry)
-  [Function `migrate_receiving`](#sui_coin_registry_migrate_receiving)
-  [Function `set_name`](#sui_coin_registry_set_name)
-  [Function `set_symbol`](#sui_coin_registry_set_symbol)
-  [Function `set_description`](#sui_coin_registry_set_description)
-  [Function `set_icon_url`](#sui_coin_registry_set_icon_url)
-  [Function `data`](#sui_coin_registry_data)
-  [Function `decimals`](#sui_coin_registry_decimals)
-  [Function `name`](#sui_coin_registry_name)
-  [Function `symbol`](#sui_coin_registry_symbol)
-  [Function `description`](#sui_coin_registry_description)
-  [Function `icon_url`](#sui_coin_registry_icon_url)
-  [Function `meta_data_cap_claimed`](#sui_coin_registry_meta_data_cap_claimed)
-  [Function `treasury_cap`](#sui_coin_registry_treasury_cap)
-  [Function `deny_cap`](#sui_coin_registry_deny_cap)
-  [Function `supply_registered`](#sui_coin_registry_supply_registered)
-  [Function `exists`](#sui_coin_registry_exists)
-  [Function `inner`](#sui_coin_registry_inner)
-  [Function `register_supply`](#sui_coin_registry_register_supply)
-  [Function `register_regulated`](#sui_coin_registry_register_regulated)
-  [Function `set_decimals`](#sui_coin_registry_set_decimals)
-  [Function `set_supply`](#sui_coin_registry_set_supply)
-  [Function `set_regulated`](#sui_coin_registry_set_regulated)
-  [Function `data_mut`](#sui_coin_registry_data_mut)
-  [Function `register_coin_data`](#sui_coin_registry_register_coin_data)
-  [Function `inner_mut`](#sui_coin_registry_inner_mut)
-  [Function `create_coin_data_init`](#sui_coin_registry_create_coin_data_init)
-  [Function `create_coin_data`](#sui_coin_registry_create_coin_data)
-  [Function `empty`](#sui_coin_registry_empty)
-  [Function `create_cap`](#sui_coin_registry_create_cap)
-  [Function `create`](#sui_coin_registry_create)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/balance.md#sui_balance">sui::balance</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_coin_registry_CoinRegistry"></a>

## Struct `CoinRegistry`

System object found at address 0xc that stores coin data


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a> <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_coin_registry_ExtraField"></a>

## Struct `ExtraField`

Store only object that enables more flexible coin data
registration, allowing for additional fields to be added
without changing the CoinData structure.


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

<a name="sui_coin_registry_CoinDataKey"></a>

## Struct `CoinDataKey`

Key used to access coin metadata hung off the <code><a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a></code>
object. This key can be versioned to allow for future changes
to the metadata object.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinDataKey">CoinDataKey</a>&lt;<b>phantom</b> T&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_coin_registry_MetadataCap"></a>

## Struct `MetadataCap`

Capability object that enables coin metadata to be updated.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;<b>phantom</b> T&gt; <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_coin_registry_CoinData"></a>

## Struct `CoinData`

CoinData object that stores information about a coin type.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;<b>phantom</b> T&gt; <b>has</b> key, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8</code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
</dd>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: <a href="../std/string.md#std_string_String">std::string::String</a></code>
</dt>
<dd>
</dd>
<dt>
<code>supply: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/coin_registry.md#sui_coin_registry_SupplyState">sui::coin_registry::SupplyState</a>&lt;T&gt;&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>regulated: <a href="../sui/coin_registry.md#sui_coin_registry_RegulatedState">sui::coin_registry::RegulatedState</a></code>
</dt>
<dd>
</dd>
<dt>
<code>treasury_cap_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>metadata_cap_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>extra_fields: <a href="../sui/vec_map.md#sui_vec_map_VecMap">sui::vec_map::VecMap</a>&lt;<a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_ExtraField">sui::coin_registry::ExtraField</a>&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_coin_registry_InitCoinData"></a>

## Struct `InitCoinData`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/coin_registry.md#sui_coin_registry_InitCoinData">InitCoinData</a>&lt;<b>phantom</b> T&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_coin_registry_SupplyState"></a>

## Enum `SupplyState`

Supply state of a coin type, which can be fixed or unknown.


<pre><code><b>public</b> <b>enum</b> <a href="../sui/coin_registry.md#sui_coin_registry_SupplyState">SupplyState</a>&lt;<b>phantom</b> T&gt; <b>has</b> store
</code></pre>



<details>
<summary>Variants</summary>


<dl>
<dt>
Variant <code>Fixed</code>
</dt>
<dd>
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
</dd>
</dl>


</details>

<a name="sui_coin_registry_RegulatedState"></a>

## Enum `RegulatedState`

Regulated state of a coin type, which can be regulated with a deny cap,


<pre><code><b>public</b> <b>enum</b> <a href="../sui/coin_registry.md#sui_coin_registry_RegulatedState">RegulatedState</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Variants</summary>


<dl>
<dt>
Variant <code>Regulated</code>
</dt>
<dd>
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
Variant <code>Unknown</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_coin_registry_ECoinDataNotFound"></a>

No CoinData found for this coin type.


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_ECoinDataNotFound">ECoinDataNotFound</a>: u64 = 0;
</code></pre>



<a name="sui_coin_registry_EMetadataCapAlreadyClaimed"></a>

Metadata cap already claimed


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_EMetadataCapAlreadyClaimed">EMetadataCapAlreadyClaimed</a>: u64 = 1;
</code></pre>



<a name="sui_coin_registry_ENotSystemAddress"></a>

Only the system address can create the registry


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_ENotSystemAddress">ENotSystemAddress</a>: u64 = 2;
</code></pre>



<a name="sui_coin_registry_ECoinDataAlreadyExists"></a>

CoinData for this coin type already exists


<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_ECoinDataAlreadyExists">ECoinDataAlreadyExists</a>: u64 = 3;
</code></pre>



<a name="sui_coin_registry_REGULATED_COIN_VARIANT"></a>



<pre><code><b>const</b> <a href="../sui/coin_registry.md#sui_coin_registry_REGULATED_COIN_VARIANT">REGULATED_COIN_VARIANT</a>: u8 = 0;
</code></pre>



<a name="sui_coin_registry_coin_registry_id"></a>

## Function `coin_registry_id`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_coin_registry_id">coin_registry_id</a>(): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_coin_registry_id">coin_registry_id</a>(): ID {
    @0xc.to_id()
}
</code></pre>



</details>

<a name="sui_coin_registry_id"></a>

## Function `id`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>(registry: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>): <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>(registry: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>): ID {
    registry.<a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>.to_inner()
}
</code></pre>



</details>

<a name="sui_coin_registry_transfer_to_registry"></a>

## Function `transfer_to_registry`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_transfer_to_registry">transfer_to_registry</a>&lt;T&gt;(init: <a href="../sui/coin_registry.md#sui_coin_registry_InitCoinData">sui::coin_registry::InitCoinData</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_transfer_to_registry">transfer_to_registry</a>&lt;T&gt;(init: <a href="../sui/coin_registry.md#sui_coin_registry_InitCoinData">InitCoinData</a>&lt;T&gt;) {
    <b>let</b> <a href="../sui/coin_registry.md#sui_coin_registry_InitCoinData">InitCoinData</a> { <a href="../sui/coin_registry.md#sui_coin_registry_data">data</a> } = init;
    <a href="../sui/transfer.md#sui_transfer_transfer">transfer::transfer</a>(
        <a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_coin_registry_id">coin_registry_id</a>().to_address(),
    );
}
</code></pre>



</details>

<a name="sui_coin_registry_migrate_receiving"></a>

## Function `migrate_receiving`

Enables CoinData to be registreed in the <code><a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a></code> object
via TTO.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_migrate_receiving">migrate_receiving</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>, coin_data: <a href="../sui/transfer.md#sui_transfer_Receiving">sui::transfer::Receiving</a>&lt;<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_migrate_receiving">migrate_receiving</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>, coin_data: Receiving&lt;<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;&gt;) {
    <b>let</b> received_data = <a href="../sui/transfer.md#sui_transfer_public_receive">transfer::public_receive</a>(&<b>mut</b> registry.<a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>, coin_data);
    registry.<a href="../sui/coin_registry.md#sui_coin_registry_register_coin_data">register_coin_data</a>(received_data);
}
</code></pre>



</details>

<a name="sui_coin_registry_set_name"></a>

## Function `set_name`

Enables a metadata cap holder to update a coin's name.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_name">set_name</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_name">set_name</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: String) {
    registry.<a href="../sui/coin_registry.md#sui_coin_registry_data_mut">data_mut</a>&lt;T&gt;().<a href="../sui/coin_registry.md#sui_coin_registry_name">name</a> = <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>;
}
</code></pre>



</details>

<a name="sui_coin_registry_set_symbol"></a>

## Function `set_symbol`

Enables a metadata cap holder to update a coin's symbol.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_symbol">set_symbol</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: <a href="../std/string.md#std_string_String">std::string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_symbol">set_symbol</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: String) {
    registry.<a href="../sui/coin_registry.md#sui_coin_registry_data_mut">data_mut</a>&lt;T&gt;().<a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a> = <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>;
}
</code></pre>



</details>

<a name="sui_coin_registry_set_description"></a>

## Function `set_description`

Enables a metadata cap holder to update a coin's description.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_description">set_description</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: <a href="../std/string.md#std_string_String">std::string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_description">set_description</a>&lt;T&gt;(
    registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>,
    _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt;,
    <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: String,
) {
    registry.<a href="../sui/coin_registry.md#sui_coin_registry_data_mut">data_mut</a>&lt;T&gt;().<a href="../sui/coin_registry.md#sui_coin_registry_description">description</a> = <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>;
}
</code></pre>



</details>

<a name="sui_coin_registry_set_icon_url"></a>

## Function `set_icon_url`

Enables a metadata cap holder to update a coin's icon URL.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_icon_url">set_icon_url</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: <a href="../std/string.md#std_string_String">std::string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_icon_url">set_icon_url</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>, _: &<a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: String) {
    registry.<a href="../sui/coin_registry.md#sui_coin_registry_data_mut">data_mut</a>&lt;T&gt;().<a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a> = <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>;
}
</code></pre>



</details>

<a name="sui_coin_registry_data"></a>

## Function `data`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>&lt;T&gt;(registry: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>): &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>&lt;T&gt;(registry: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>): &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt; {
    <b>assert</b>!(registry.<a href="../sui/coin_registry.md#sui_coin_registry_exists">exists</a>&lt;T&gt;(), <a href="../sui/coin_registry.md#sui_coin_registry_ECoinDataNotFound">ECoinDataNotFound</a>);
    registry.<a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>.borrow_dof(<a href="../sui/coin_registry.md#sui_coin_registry_CoinDataKey">CoinDataKey</a>&lt;T&gt;())
}
</code></pre>



</details>

<a name="sui_coin_registry_decimals"></a>

## Function `decimals`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;): u8 { coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a> }
</code></pre>



</details>

<a name="sui_coin_registry_name"></a>

## Function `name`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;): String { coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_name">name</a> }
</code></pre>



</details>

<a name="sui_coin_registry_symbol"></a>

## Function `symbol`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;): String { coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a> }
</code></pre>



</details>

<a name="sui_coin_registry_description"></a>

## Function `description`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;): String { coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_description">description</a> }
</code></pre>



</details>

<a name="sui_coin_registry_icon_url"></a>

## Function `icon_url`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;): <a href="../std/string.md#std_string_String">std::string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;): String { coin_data.<a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a> }
</code></pre>



</details>

<a name="sui_coin_registry_meta_data_cap_claimed"></a>

## Function `meta_data_cap_claimed`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_meta_data_cap_claimed">meta_data_cap_claimed</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_meta_data_cap_claimed">meta_data_cap_claimed</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;): bool {
    coin_data.metadata_cap_id.is_some()
}
</code></pre>



</details>

<a name="sui_coin_registry_treasury_cap"></a>

## Function `treasury_cap`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap">treasury_cap</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_treasury_cap">treasury_cap</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;): Option&lt;ID&gt; { coin_data.treasury_cap_id }
</code></pre>



</details>

<a name="sui_coin_registry_deny_cap"></a>

## Function `deny_cap`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_deny_cap">deny_cap</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_deny_cap">deny_cap</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;): Option&lt;ID&gt; {
    match (coin_data.regulated) {
        RegulatedState::Regulated { cap, .. } =&gt; option::some(cap),
        RegulatedState::Unknown =&gt; option::none(),
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_supply_registered"></a>

## Function `supply_registered`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_supply_registered">supply_registered</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_supply_registered">supply_registered</a>&lt;T&gt;(coin_data: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;): bool {
    match (coin_data.supply.<a href="../sui/borrow.md#sui_borrow">borrow</a>()) {
        SupplyState::Fixed(_) =&gt; <b>true</b>,
        SupplyState::Unknown =&gt; <b>false</b>,
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_exists"></a>

## Function `exists`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_exists">exists</a>&lt;T&gt;(registry: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_exists">exists</a>&lt;T&gt;(registry: &<a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>): bool {
    registry.<a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>.exists_dof(<a href="../sui/coin_registry.md#sui_coin_registry_CoinDataKey">CoinDataKey</a>&lt;T&gt;())
}
</code></pre>



</details>

<a name="sui_coin_registry_inner"></a>

## Function `inner`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_inner">inner</a>&lt;T&gt;(init: &<a href="../sui/coin_registry.md#sui_coin_registry_InitCoinData">sui::coin_registry::InitCoinData</a>&lt;T&gt;): &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_inner">inner</a>&lt;T&gt;(init: &<a href="../sui/coin_registry.md#sui_coin_registry_InitCoinData">InitCoinData</a>&lt;T&gt;): &<a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt; {
    &init.<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>
}
</code></pre>



</details>

<a name="sui_coin_registry_register_supply"></a>

## Function `register_supply`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_register_supply">register_supply</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>, supply: <a href="../sui/balance.md#sui_balance_Supply">sui::balance::Supply</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_register_supply">register_supply</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>, supply: Supply&lt;T&gt;) {
    <b>assert</b>!(registry.<a href="../sui/coin_registry.md#sui_coin_registry_exists">exists</a>&lt;T&gt;(), <a href="../sui/coin_registry.md#sui_coin_registry_ECoinDataNotFound">ECoinDataNotFound</a>);
    match (registry.<a href="../sui/coin_registry.md#sui_coin_registry_data_mut">data_mut</a>&lt;T&gt;().supply.swap(SupplyState::Fixed(supply))) {
        SupplyState::Fixed(_supply) =&gt; <b>abort</b>,
        SupplyState::Unknown =&gt; (),
    };
}
</code></pre>



</details>

<a name="sui_coin_registry_register_regulated"></a>

## Function `register_regulated`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_register_regulated">register_regulated</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>, deny_cap_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_register_regulated">register_regulated</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>, deny_cap_id: ID) {
    <b>assert</b>!(registry.<a href="../sui/coin_registry.md#sui_coin_registry_exists">exists</a>&lt;T&gt;(), <a href="../sui/coin_registry.md#sui_coin_registry_ECoinDataNotFound">ECoinDataNotFound</a>);
    registry.<a href="../sui/coin_registry.md#sui_coin_registry_data_mut">data_mut</a>&lt;T&gt;().regulated =
        RegulatedState::Regulated {
            cap: deny_cap_id,
            variant: <a href="../sui/coin_registry.md#sui_coin_registry_REGULATED_COIN_VARIANT">REGULATED_COIN_VARIANT</a>,
        };
}
</code></pre>



</details>

<a name="sui_coin_registry_set_decimals"></a>

## Function `set_decimals`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_decimals">set_decimals</a>&lt;T&gt;(<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_decimals">set_decimals</a>&lt;T&gt;(<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;, <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8) {
    <a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>.<a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a> = <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>;
}
</code></pre>



</details>

<a name="sui_coin_registry_set_supply"></a>

## Function `set_supply`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_supply">set_supply</a>&lt;T&gt;(<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;, supply: <a href="../sui/balance.md#sui_balance_Supply">sui::balance::Supply</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_supply">set_supply</a>&lt;T&gt;(<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;, supply: Supply&lt;T&gt;) {
    match (<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>.supply.swap(SupplyState::Fixed(supply))) {
        SupplyState::Fixed(_supply) =&gt; <b>abort</b>,
        SupplyState::Unknown =&gt; (),
    };
}
</code></pre>



</details>

<a name="sui_coin_registry_set_regulated"></a>

## Function `set_regulated`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_regulated">set_regulated</a>&lt;T&gt;(<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;, deny_cap_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_set_regulated">set_regulated</a>&lt;T&gt;(<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;, deny_cap_id: ID) {
    <a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>.regulated =
        RegulatedState::Regulated {
            cap: deny_cap_id,
            variant: <a href="../sui/coin_registry.md#sui_coin_registry_REGULATED_COIN_VARIANT">REGULATED_COIN_VARIANT</a>,
        };
}
</code></pre>



</details>

<a name="sui_coin_registry_data_mut"></a>

## Function `data_mut`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_data_mut">data_mut</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>): &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_data_mut">data_mut</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>): &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt; {
    <b>assert</b>!(registry.<a href="../sui/coin_registry.md#sui_coin_registry_exists">exists</a>&lt;T&gt;(), <a href="../sui/coin_registry.md#sui_coin_registry_ECoinDataNotFound">ECoinDataNotFound</a>);
    registry.<a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>.borrow_dof_mut(<a href="../sui/coin_registry.md#sui_coin_registry_CoinDataKey">CoinDataKey</a>&lt;T&gt;())
}
</code></pre>



</details>

<a name="sui_coin_registry_register_coin_data"></a>

## Function `register_coin_data`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_register_coin_data">register_coin_data</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">sui::coin_registry::CoinRegistry</a>, <a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_register_coin_data">register_coin_data</a>&lt;T&gt;(registry: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a>, <a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;) {
    <b>assert</b>!(!registry.<a href="../sui/coin_registry.md#sui_coin_registry_exists">exists</a>&lt;T&gt;(), <a href="../sui/coin_registry.md#sui_coin_registry_ECoinDataAlreadyExists">ECoinDataAlreadyExists</a>);
    registry.<a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>.add_dof(<a href="../sui/coin_registry.md#sui_coin_registry_CoinDataKey">CoinDataKey</a>&lt;T&gt;(), <a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>);
}
</code></pre>



</details>

<a name="sui_coin_registry_inner_mut"></a>

## Function `inner_mut`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_inner_mut">inner_mut</a>&lt;T&gt;(init: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_InitCoinData">sui::coin_registry::InitCoinData</a>&lt;T&gt;): &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_inner_mut">inner_mut</a>&lt;T&gt;(init: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_InitCoinData">InitCoinData</a>&lt;T&gt;): &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt; {
    &<b>mut</b> init.<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>
}
</code></pre>



</details>

<a name="sui_coin_registry_create_coin_data_init"></a>

## Function `create_coin_data_init`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_create_coin_data_init">create_coin_data_init</a>&lt;T&gt;(<a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8, <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, supply: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/balance.md#sui_balance_Supply">sui::balance::Supply</a>&lt;T&gt;&gt;, treasury_cap_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;, metadata_cap_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;, deny_cap_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin_registry.md#sui_coin_registry_InitCoinData">sui::coin_registry::InitCoinData</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_create_coin_data_init">create_coin_data_init</a>&lt;T&gt;(
    <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8,
    <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: String,
    supply: Option&lt;Supply&lt;T&gt;&gt;,
    treasury_cap_id: Option&lt;ID&gt;,
    metadata_cap_id: Option&lt;ID&gt;,
    deny_cap_id: Option&lt;ID&gt;,
    ctx: &<b>mut</b> TxContext,
): <a href="../sui/coin_registry.md#sui_coin_registry_InitCoinData">InitCoinData</a>&lt;T&gt; {
    <a href="../sui/coin_registry.md#sui_coin_registry_InitCoinData">InitCoinData</a> {
        <a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: <a href="../sui/coin_registry.md#sui_coin_registry_create_coin_data">create_coin_data</a>(
            <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>,
            <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>,
            <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>,
            <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>,
            <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>,
            supply,
            treasury_cap_id,
            metadata_cap_id,
            deny_cap_id,
            ctx,
        ),
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_create_coin_data"></a>

## Function `create_coin_data`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_create_coin_data">create_coin_data</a>&lt;T&gt;(<a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8, <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: <a href="../std/string.md#std_string_String">std::string::String</a>, supply: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/balance.md#sui_balance_Supply">sui::balance::Supply</a>&lt;T&gt;&gt;, treasury_cap_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;, metadata_cap_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;, deny_cap_id: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_create_coin_data">create_coin_data</a>&lt;T&gt;(
    <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: u8,
    <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: String,
    <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: String,
    supply: Option&lt;Supply&lt;T&gt;&gt;,
    treasury_cap_id: Option&lt;ID&gt;,
    metadata_cap_id: Option&lt;ID&gt;,
    deny_cap_id: Option&lt;ID&gt;,
    ctx: &<b>mut</b> TxContext,
): <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt; {
    <b>let</b> supply = supply
        .map!(|supply| SupplyState::Fixed(supply))
        .or!(option::some(SupplyState::Unknown));
    <b>let</b> regulated_state = deny_cap_id
        .map!(|cap| RegulatedState::Regulated { cap, variant: <a href="../sui/coin_registry.md#sui_coin_registry_REGULATED_COIN_VARIANT">REGULATED_COIN_VARIANT</a> })
        .destroy_or!(RegulatedState::Unknown);
    <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a> {
        <a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>,
        <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>,
        supply,
        regulated: regulated_state,
        treasury_cap_id,
        metadata_cap_id,
        extra_fields: <a href="../sui/vec_map.md#sui_vec_map_empty">vec_map::empty</a>(),
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_empty"></a>

## Function `empty`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_empty">empty</a>&lt;T&gt;(ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_empty">empty</a>&lt;T&gt;(ctx: &<b>mut</b> TxContext): <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt; {
    <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a> {
        <a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx),
        <a href="../sui/coin_registry.md#sui_coin_registry_decimals">decimals</a>: 0,
        <a href="../sui/coin_registry.md#sui_coin_registry_name">name</a>: b"".to_string(),
        <a href="../sui/coin_registry.md#sui_coin_registry_symbol">symbol</a>: b"".to_string(),
        <a href="../sui/coin_registry.md#sui_coin_registry_description">description</a>: b"".to_string(),
        <a href="../sui/coin_registry.md#sui_coin_registry_icon_url">icon_url</a>: b"".to_string(),
        regulated: RegulatedState::Unknown,
        supply: option::some(SupplyState::Unknown),
        treasury_cap_id: option::none(),
        metadata_cap_id: option::none(),
        extra_fields: <a href="../sui/vec_map.md#sui_vec_map_empty">vec_map::empty</a>(),
    }
}
</code></pre>



</details>

<a name="sui_coin_registry_create_cap"></a>

## Function `create_cap`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_create_cap">create_cap</a>&lt;T&gt;(<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">sui::coin_registry::CoinData</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">sui::coin_registry::MetadataCap</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_create_cap">create_cap</a>&lt;T&gt;(<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>: &<b>mut</b> <a href="../sui/coin_registry.md#sui_coin_registry_CoinData">CoinData</a>&lt;T&gt;, ctx: &<b>mut</b> TxContext): <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a>&lt;T&gt; {
    <b>assert</b>!(!<a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>.<a href="../sui/coin_registry.md#sui_coin_registry_meta_data_cap_claimed">meta_data_cap_claimed</a>(), <a href="../sui/coin_registry.md#sui_coin_registry_EMetadataCapAlreadyClaimed">EMetadataCapAlreadyClaimed</a>);
    <b>let</b> <a href="../sui/coin_registry.md#sui_coin_registry_id">id</a> = <a href="../sui/object.md#sui_object_new">object::new</a>(ctx);
    <b>let</b> metadata_cap_id = <a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>.to_inner();
    <a href="../sui/coin_registry.md#sui_coin_registry_data">data</a>.metadata_cap_id.fill(metadata_cap_id);
    <a href="../sui/coin_registry.md#sui_coin_registry_MetadataCap">MetadataCap</a> { <a href="../sui/coin_registry.md#sui_coin_registry_id">id</a> }
}
</code></pre>



</details>

<a name="sui_coin_registry_create"></a>

## Function `create`

Create and share the singleton Registry -- this function is
called exactly once, during the upgrade epoch.


<pre><code><b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_create">create</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/coin_registry.md#sui_coin_registry_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/coin_registry.md#sui_coin_registry_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/coin_registry.md#sui_coin_registry_CoinRegistry">CoinRegistry</a> {
        <a href="../sui/coin_registry.md#sui_coin_registry_id">id</a>: <a href="../sui/object.md#sui_object_sui_coin_registry_object_id">object::sui_coin_registry_object_id</a>(),
    });
}
</code></pre>



</details>
