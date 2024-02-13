
<a name="0xb_btc"></a>

# Module `0xb::btc`



-  [Struct `BTC`](#0xb_btc_BTC)
-  [Function `create`](#0xb_btc_create)


<pre><code><b>use</b> <a href="dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="dependencies/sui-framework/coin.md#0x2_coin">0x2::coin</a>;
<b>use</b> <a href="dependencies/sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="dependencies/sui-framework/url.md#0x2_url">0x2::url</a>;
</code></pre>



<a name="0xb_btc_BTC"></a>

## Struct `BTC`



<pre><code><b>struct</b> <a href="btc.md#0xb_btc_BTC">BTC</a> <b>has</b> drop
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

<a name="0xb_btc_create"></a>

## Function `create`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="btc.md#0xb_btc_create">create</a>(ctx: &<b>mut</b> <a href="dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="dependencies/sui-framework/coin.md#0x2_coin_TreasuryCap">coin::TreasuryCap</a>&lt;<a href="btc.md#0xb_btc_BTC">btc::BTC</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="btc.md#0xb_btc_create">create</a>(ctx: &<b>mut</b> TxContext): TreasuryCap&lt;<a href="btc.md#0xb_btc_BTC">BTC</a>&gt; {
    <b>let</b> (treasury_cap, metadata) = <a href="dependencies/sui-framework/coin.md#0x2_coin_create_currency">coin::create_currency</a>(
        <a href="btc.md#0xb_btc_BTC">BTC</a> {},
        8,
        b"<a href="btc.md#0xb_btc_BTC">BTC</a>",
        b"Bitcoin",
        b"Bridged Bitcoin token",
        <a href="dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>(),
        ctx
    );
    <a href="dependencies/sui-framework/transfer.md#0x2_transfer_public_freeze_object">transfer::public_freeze_object</a>(metadata);
    treasury_cap
}
</code></pre>



</details>
