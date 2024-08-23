---
title: Module `0xb::treasury`
---



-  [Struct `BridgeTreasury`](#0xb_treasury_BridgeTreasury)
-  [Struct `BridgeTokenMetadata`](#0xb_treasury_BridgeTokenMetadata)
-  [Struct `ForeignTokenRegistration`](#0xb_treasury_ForeignTokenRegistration)
-  [Struct `UpdateTokenPriceEvent`](#0xb_treasury_UpdateTokenPriceEvent)
-  [Struct `NewTokenEvent`](#0xb_treasury_NewTokenEvent)
-  [Struct `TokenRegistrationEvent`](#0xb_treasury_TokenRegistrationEvent)
-  [Constants](#@Constants_0)
-  [Function `token_id`](#0xb_treasury_token_id)
-  [Function `decimal_multiplier`](#0xb_treasury_decimal_multiplier)
-  [Function `notional_value`](#0xb_treasury_notional_value)
-  [Function `register_foreign_token`](#0xb_treasury_register_foreign_token)
-  [Function `add_new_token`](#0xb_treasury_add_new_token)
-  [Function `create`](#0xb_treasury_create)
-  [Function `burn`](#0xb_treasury_burn)
-  [Function `mint`](#0xb_treasury_mint)
-  [Function `update_asset_notional_price`](#0xb_treasury_update_asset_notional_price)
-  [Function `get_token_metadata`](#0xb_treasury_get_token_metadata)


<pre><code><b>use</b> <a href="../move-stdlib/ascii.md#0x1_ascii">0x1::ascii</a>;
<b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../move-stdlib/type_name.md#0x1_type_name">0x1::type_name</a>;
<b>use</b> <a href="../move-stdlib/u64.md#0x1_u64">0x1::u64</a>;
<b>use</b> <a href="../sui-framework/address.md#0x2_address">0x2::address</a>;
<b>use</b> <a href="../sui-framework/bag.md#0x2_bag">0x2::bag</a>;
<b>use</b> <a href="../sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="../sui-framework/event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="../sui-framework/hex.md#0x2_hex">0x2::hex</a>;
<b>use</b> <a href="../sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../sui-framework/object_bag.md#0x2_object_bag">0x2::object_bag</a>;
<b>use</b> <a href="../sui-framework/package.md#0x2_package">0x2::package</a>;
<b>use</b> <a href="../sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="../sui-framework/vec_map.md#0x2_vec_map">0x2::vec_map</a>;
</code></pre>



<a name="0xb_treasury_BridgeTreasury"></a>

## Struct `BridgeTreasury`



<pre><code><b>struct</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>treasuries: <a href="../sui-framework/object_bag.md#0x2_object_bag_ObjectBag">object_bag::ObjectBag</a></code>
</dt>
<dd>

</dd>
<dt>
<code>supported_tokens: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="../move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>, <a href="treasury.md#0xb_treasury_BridgeTokenMetadata">treasury::BridgeTokenMetadata</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>id_token_type_map: <a href="../sui-framework/vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;u8, <a href="../move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>waiting_room: <a href="../sui-framework/bag.md#0x2_bag_Bag">bag::Bag</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_treasury_BridgeTokenMetadata"></a>

## Struct `BridgeTokenMetadata`



<pre><code><b>struct</b> <a href="treasury.md#0xb_treasury_BridgeTokenMetadata">BridgeTokenMetadata</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>decimal_multiplier: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>notional_value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>native_token: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_treasury_ForeignTokenRegistration"></a>

## Struct `ForeignTokenRegistration`



<pre><code><b>struct</b> <a href="treasury.md#0xb_treasury_ForeignTokenRegistration">ForeignTokenRegistration</a> <b>has</b> store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>: <a href="../move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a></code>
</dt>
<dd>

</dd>
<dt>
<code>uc: <a href="../sui-framework/package.md#0x2_package_UpgradeCap">package::UpgradeCap</a></code>
</dt>
<dd>

</dd>
<dt>
<code>decimal: u8</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_treasury_UpdateTokenPriceEvent"></a>

## Struct `UpdateTokenPriceEvent`



<pre><code><b>struct</b> <a href="treasury.md#0xb_treasury_UpdateTokenPriceEvent">UpdateTokenPriceEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>token_id: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>new_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_treasury_NewTokenEvent"></a>

## Struct `NewTokenEvent`



<pre><code><b>struct</b> <a href="treasury.md#0xb_treasury_NewTokenEvent">NewTokenEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>token_id: u8</code>
</dt>
<dd>

</dd>
<dt>
<code><a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>: <a href="../move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a></code>
</dt>
<dd>

</dd>
<dt>
<code>native_token: bool</code>
</dt>
<dd>

</dd>
<dt>
<code>decimal_multiplier: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>notional_value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0xb_treasury_TokenRegistrationEvent"></a>

## Struct `TokenRegistrationEvent`



<pre><code><b>struct</b> <a href="treasury.md#0xb_treasury_TokenRegistrationEvent">TokenRegistrationEvent</a> <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>: <a href="../move-stdlib/type_name.md#0x1_type_name_TypeName">type_name::TypeName</a></code>
</dt>
<dd>

</dd>
<dt>
<code>decimal: u8</code>
</dt>
<dd>

</dd>
<dt>
<code>native_token: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0xb_treasury_EInvalidNotionalValue"></a>



<pre><code><b>const</b> <a href="treasury.md#0xb_treasury_EInvalidNotionalValue">EInvalidNotionalValue</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 4;
</code></pre>



<a name="0xb_treasury_EInvalidUpgradeCap"></a>



<pre><code><b>const</b> <a href="treasury.md#0xb_treasury_EInvalidUpgradeCap">EInvalidUpgradeCap</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0xb_treasury_ETokenSupplyNonZero"></a>



<pre><code><b>const</b> <a href="treasury.md#0xb_treasury_ETokenSupplyNonZero">ETokenSupplyNonZero</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 3;
</code></pre>



<a name="0xb_treasury_EUnsupportedTokenType"></a>



<pre><code><b>const</b> <a href="treasury.md#0xb_treasury_EUnsupportedTokenType">EUnsupportedTokenType</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0xb_treasury_token_id"></a>

## Function `token_id`



<pre><code><b>public</b> <b>fun</b> <a href="treasury.md#0xb_treasury_token_id">token_id</a>&lt;T&gt;(self: &<a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>): u8
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="treasury.md#0xb_treasury_token_id">token_id</a>&lt;T&gt;(self: &<a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>): u8 {
    <b>let</b> metadata = self.<a href="treasury.md#0xb_treasury_get_token_metadata">get_token_metadata</a>&lt;T&gt;();
    metadata.id
}
</code></pre>



</details>

<a name="0xb_treasury_decimal_multiplier"></a>

## Function `decimal_multiplier`



<pre><code><b>public</b> <b>fun</b> <a href="treasury.md#0xb_treasury_decimal_multiplier">decimal_multiplier</a>&lt;T&gt;(self: &<a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="treasury.md#0xb_treasury_decimal_multiplier">decimal_multiplier</a>&lt;T&gt;(self: &<a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <b>let</b> metadata = self.<a href="treasury.md#0xb_treasury_get_token_metadata">get_token_metadata</a>&lt;T&gt;();
    metadata.decimal_multiplier
}
</code></pre>



</details>

<a name="0xb_treasury_notional_value"></a>

## Function `notional_value`



<pre><code><b>public</b> <b>fun</b> <a href="treasury.md#0xb_treasury_notional_value">notional_value</a>&lt;T&gt;(self: &<a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="treasury.md#0xb_treasury_notional_value">notional_value</a>&lt;T&gt;(self: &<a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>): <a href="../move-stdlib/u64.md#0x1_u64">u64</a> {
    <b>let</b> metadata = self.<a href="treasury.md#0xb_treasury_get_token_metadata">get_token_metadata</a>&lt;T&gt;();
    metadata.notional_value
}
</code></pre>



</details>

<a name="0xb_treasury_register_foreign_token"></a>

## Function `register_foreign_token`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_register_foreign_token">register_foreign_token</a>&lt;T&gt;(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>, tc: <a href="../sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;T&gt;, uc: <a href="../sui-framework/package.md#0x2_package_UpgradeCap">package::UpgradeCap</a>, metadata: &<a href="../sui-framework/coin.md#0x2_coin_CoinMetadata">coin::CoinMetadata</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="treasury.md#0xb_treasury_register_foreign_token">register_foreign_token</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>,
    tc: TreasuryCap&lt;T&gt;,
    uc: UpgradeCap,
    metadata: &CoinMetadata&lt;T&gt;,
) {
    // Make sure TreasuryCap <b>has</b> not been minted before.
    <b>assert</b>!(<a href="../sui-framework/coin.md#0x2_coin_total_supply">coin::total_supply</a>(&tc) == 0, <a href="treasury.md#0xb_treasury_ETokenSupplyNonZero">ETokenSupplyNonZero</a>);
    <b>let</b> <a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a> = <a href="../move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;T&gt;();
    <b>let</b> address_bytes = <a href="../sui-framework/hex.md#0x2_hex_decode">hex::decode</a>(<a href="../move-stdlib/ascii.md#0x1_ascii_into_bytes">ascii::into_bytes</a>(<a href="../move-stdlib/type_name.md#0x1_type_name_get_address">type_name::get_address</a>(&<a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>)));
    <b>let</b> coin_address = address::from_bytes(address_bytes);
    // Make sure upgrade cap is for the Coin <a href="../sui-framework/package.md#0x2_package">package</a>
    // FIXME: add test
    <b>assert</b>!(
        <a href="../sui-framework/object.md#0x2_object_id_to_address">object::id_to_address</a>(&<a href="../sui-framework/package.md#0x2_package_upgrade_package">package::upgrade_package</a>(&uc))
            == coin_address, <a href="treasury.md#0xb_treasury_EInvalidUpgradeCap">EInvalidUpgradeCap</a>
    );
    <b>let</b> registration = <a href="treasury.md#0xb_treasury_ForeignTokenRegistration">ForeignTokenRegistration</a> {
        <a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>,
        uc,
        decimal: <a href="../sui-framework/coin.md#0x2_coin_get_decimals">coin::get_decimals</a>(metadata),
    };
    self.waiting_room.add(<a href="../move-stdlib/type_name.md#0x1_type_name_into_string">type_name::into_string</a>(<a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>), registration);
    self.treasuries.add(<a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>, tc);

    emit(<a href="treasury.md#0xb_treasury_TokenRegistrationEvent">TokenRegistrationEvent</a>{
        <a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>,
        decimal: <a href="../sui-framework/coin.md#0x2_coin_get_decimals">coin::get_decimals</a>(metadata),
        native_token: <b>false</b>
    });
}
</code></pre>



</details>

<a name="0xb_treasury_add_new_token"></a>

## Function `add_new_token`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_add_new_token">add_new_token</a>(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>, token_name: <a href="../move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>, token_id: u8, native_token: bool, notional_value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="treasury.md#0xb_treasury_add_new_token">add_new_token</a>(
    self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>,
    token_name: String,
    token_id: u8,
    native_token: bool,
    notional_value: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
) {
    <b>if</b> (!native_token){
        <b>assert</b>!(notional_value &gt; 0, <a href="treasury.md#0xb_treasury_EInvalidNotionalValue">EInvalidNotionalValue</a>);
        <b>let</b> <a href="treasury.md#0xb_treasury_ForeignTokenRegistration">ForeignTokenRegistration</a>{
            <a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>,
            uc,
            decimal,
        } = self.waiting_room.remove&lt;String, <a href="treasury.md#0xb_treasury_ForeignTokenRegistration">ForeignTokenRegistration</a>&gt;(token_name);
        <b>let</b> decimal_multiplier = 10u64.pow(decimal);
        self.supported_tokens.insert(
            <a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>,
            <a href="treasury.md#0xb_treasury_BridgeTokenMetadata">BridgeTokenMetadata</a>{
                id: token_id,
                decimal_multiplier,
                notional_value,
                native_token
            },
        );
        self.id_token_type_map.insert(token_id, <a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>);

        // Freeze upgrade cap <b>to</b> prevent changes <b>to</b> the <a href="../sui-framework/coin.md#0x2_coin">coin</a>
        <a href="../sui-framework/transfer.md#0x2_transfer_public_freeze_object">transfer::public_freeze_object</a>(uc);

        emit(<a href="treasury.md#0xb_treasury_NewTokenEvent">NewTokenEvent</a>{
            token_id,
            <a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>,
            native_token,
            decimal_multiplier,
            notional_value
        })
    } <b>else</b> {
        // Not implemented for V1
    }
}
</code></pre>



</details>

<a name="0xb_treasury_create"></a>

## Function `create`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_create">create</a>(ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="treasury.md#0xb_treasury_create">create</a>(ctx: &<b>mut</b> TxContext): <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a> {
    <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a> {
        treasuries: <a href="../sui-framework/object_bag.md#0x2_object_bag_new">object_bag::new</a>(ctx),
        supported_tokens: <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        id_token_type_map: <a href="../sui-framework/vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        waiting_room: <a href="../sui-framework/bag.md#0x2_bag_new">bag::new</a>(ctx),
    }
}
</code></pre>



</details>

<a name="0xb_treasury_burn"></a>

## Function `burn`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_burn">burn</a>&lt;T&gt;(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>, token: <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="treasury.md#0xb_treasury_burn">burn</a>&lt;T&gt;(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>, token: Coin&lt;T&gt;) {
    <b>let</b> <a href="treasury.md#0xb_treasury">treasury</a> = &<b>mut</b> self.treasuries[<a href="../move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;T&gt;()];
    <a href="../sui-framework/coin.md#0x2_coin_burn">coin::burn</a>(<a href="treasury.md#0xb_treasury">treasury</a>, token);
}
</code></pre>



</details>

<a name="0xb_treasury_mint"></a>

## Function `mint`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_mint">mint</a>&lt;T&gt;(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>, amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/coin.md#0x2_coin_Coin">coin::Coin</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="treasury.md#0xb_treasury_mint">mint</a>&lt;T&gt;(
    self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>,
    amount: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
    ctx: &<b>mut</b> TxContext,
): Coin&lt;T&gt; {
    <b>let</b> <a href="treasury.md#0xb_treasury">treasury</a> = &<b>mut</b> self.treasuries[<a href="../move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;T&gt;()];
    <a href="../sui-framework/coin.md#0x2_coin_mint">coin::mint</a>(<a href="treasury.md#0xb_treasury">treasury</a>, amount, ctx)
}
</code></pre>



</details>

<a name="0xb_treasury_update_asset_notional_price"></a>

## Function `update_asset_notional_price`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="treasury.md#0xb_treasury_update_asset_notional_price">update_asset_notional_price</a>(self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>, token_id: u8, new_usd_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui-framework/package.md#0x2_package">package</a>) <b>fun</b> <a href="treasury.md#0xb_treasury_update_asset_notional_price">update_asset_notional_price</a>(
    self: &<b>mut</b> <a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>,
    token_id: u8,
    new_usd_price: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
) {
    <b>let</b> <a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a> = self.id_token_type_map.try_get(&token_id);
    <b>assert</b>!(<a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>.is_some(), <a href="treasury.md#0xb_treasury_EUnsupportedTokenType">EUnsupportedTokenType</a>);
    <b>assert</b>!(new_usd_price &gt; 0, <a href="treasury.md#0xb_treasury_EInvalidNotionalValue">EInvalidNotionalValue</a>);
    <b>let</b> <a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a> = <a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>.destroy_some();
    <b>let</b> metadata = self.supported_tokens.get_mut(&<a href="../move-stdlib/type_name.md#0x1_type_name">type_name</a>);
    metadata.notional_value = new_usd_price;

    emit(<a href="treasury.md#0xb_treasury_UpdateTokenPriceEvent">UpdateTokenPriceEvent</a> {
        token_id,
        new_price: new_usd_price,
    })
}
</code></pre>



</details>

<a name="0xb_treasury_get_token_metadata"></a>

## Function `get_token_metadata`



<pre><code><b>fun</b> <a href="treasury.md#0xb_treasury_get_token_metadata">get_token_metadata</a>&lt;T&gt;(self: &<a href="treasury.md#0xb_treasury_BridgeTreasury">treasury::BridgeTreasury</a>): <a href="treasury.md#0xb_treasury_BridgeTokenMetadata">treasury::BridgeTokenMetadata</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="treasury.md#0xb_treasury_get_token_metadata">get_token_metadata</a>&lt;T&gt;(self: &<a href="treasury.md#0xb_treasury_BridgeTreasury">BridgeTreasury</a>): <a href="treasury.md#0xb_treasury_BridgeTokenMetadata">BridgeTokenMetadata</a> {
    <b>let</b> coin_type = <a href="../move-stdlib/type_name.md#0x1_type_name_get">type_name::get</a>&lt;T&gt;();
    <b>let</b> metadata = self.supported_tokens.try_get(&coin_type);
    <b>assert</b>!(metadata.is_some(), <a href="treasury.md#0xb_treasury_EUnsupportedTokenType">EUnsupportedTokenType</a>);
    metadata.destroy_some()
}
</code></pre>



</details>
