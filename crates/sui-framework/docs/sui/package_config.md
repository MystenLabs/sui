---
title: Module `sui::package_config`
---

On-chain package configuration, keyed by original package ID.

The configuration hierarchy is:
```
PackageConfig
  └── PackageMetadataKey(original package ID)
        └── Config<PackageConfigCap>
              └── VersionForbiddenKey(package version number)
                    └── Setting<u64> flags
```
Each version has an independent <code>Setting</code> containg a flag. The flag word's high eight bits
(<code>63..56</code>) are a schema version, and its low 56 bits are schema-specific policy flags.
Schema version zero uses bit zero to mark the version forbidden. Readers treat unsupported
schema versions as forbidden, and mutation APIs reject them.


-  [Struct `PackageConfig`](#sui_package_config_PackageConfig)
-  [Struct `PackageConfigCap`](#sui_package_config_PackageConfigCap)
-  [Struct `PackageMetadataKey`](#sui_package_config_PackageMetadataKey)
-  [Struct `VersionForbiddenKey`](#sui_package_config_VersionForbiddenKey)
-  [Constants](#@Constants_0)
-  [Function `create`](#sui_package_config_create)
-  [Function `add_per_package_config`](#sui_package_config_add_per_package_config)
-  [Function `borrow_per_package_config_mut`](#sui_package_config_borrow_per_package_config_mut)
-  [Function `borrow_per_package_config`](#sui_package_config_borrow_per_package_config)
-  [Function `per_package_metadata_exists`](#sui_package_config_per_package_metadata_exists)
-  [Macro function `per_package_config_entry`](#sui_package_config_per_package_config_entry)
-  [Function `is_supported_bitset_version`](#sui_package_config_is_supported_bitset_version)
-  [Function `is_version_forbidden`](#sui_package_config_is_version_forbidden)
-  [Function `forbid_version_impl`](#sui_package_config_forbid_version_impl)
-  [Function `forbid_version`](#sui_package_config_forbid_version)
-  [Function `forbid_version_range`](#sui_package_config_forbid_version_range)
-  [Function `is_version_forbidden_for_next_epoch`](#sui_package_config_is_version_forbidden_for_next_epoch)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/config.md#sui_config">sui::config</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/dynamic_object_field.md#sui_dynamic_object_field">sui::dynamic_object_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/package.md#sui_package">sui::package</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_package_config_PackageConfig"></a>

## Struct `PackageConfig`

A shared singleton that stores per-package configuration metadata.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a> <b>has</b> key
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

<a name="sui_package_config_PackageConfigCap"></a>

## Struct `PackageConfigCap`

The capability used to write to package configs. Ensures that per-package <code>Config</code>s are
modified only by this module.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/package_config.md#sui_package_config_PackageConfigCap">PackageConfigCap</a> <b>has</b> drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
</dl>


</details>

<a name="sui_package_config_PackageMetadataKey"></a>

## Struct `PackageMetadataKey`

Dynamic object field key used to store a <code>Config</code> for an original package ID.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/package_config.md#sui_package_config_PackageMetadataKey">PackageMetadataKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_package_config_VersionForbiddenKey"></a>

## Struct `VersionForbiddenKey`

Setting key used to store forbid-list flags for one package version.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/package_config.md#sui_package_config_VersionForbiddenKey">VersionForbiddenKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>0: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_package_config_ENotSystemAddress"></a>

Trying to create the package config object when not called by the system address.


<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="sui_package_config_EInvalidVersion"></a>

The supplied version is not historical for the package controlled by the provided cap.


<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_EInvalidVersion">EInvalidVersion</a>: u64 = 1;
</code></pre>



<a name="sui_package_config_EInvalidVersionRange"></a>

The start of a version range is greater than its end.


<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_EInvalidVersionRange">EInvalidVersionRange</a>: u64 = 2;
</code></pre>



<a name="sui_package_config_EUnsupportedBitsetVersion"></a>

The forbid-list bitset schema version is not supported by this implementation.


<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_EUnsupportedBitsetVersion">EUnsupportedBitsetVersion</a>: u64 = 3;
</code></pre>



<a name="sui_package_config_BITSET_VERSION_SHIFT"></a>

The high eight bits identify the bitset schema version.


<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_BITSET_VERSION_SHIFT">BITSET_VERSION_SHIFT</a>: u8 = 56;
</code></pre>



<a name="sui_package_config_VERSION_FORBIDDEN"></a>

Schema version zero's bit indicating that the package version is forbidden.


<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_VERSION_FORBIDDEN">VERSION_FORBIDDEN</a>: u64 = 1;
</code></pre>



<a name="sui_package_config_create"></a>

## Function `create`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_create">create</a>(ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_create">create</a>(ctx: &TxContext) {
    <b>assert</b>!(ctx.sender() == @0x0, <a href="../sui/package_config.md#sui_package_config_ENotSystemAddress">ENotSystemAddress</a>);
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a> {
        id: <a href="../sui/object.md#sui_object_sui_package_config_object_id">object::sui_package_config_object_id</a>(),
    });
}
</code></pre>



</details>

<a name="sui_package_config_add_per_package_config"></a>

## Function `add_per_package_config`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_add_per_package_config">add_per_package_config</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_add_per_package_config">add_per_package_config</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> key = <a href="../sui/package_config.md#sui_package_config_PackageMetadataKey">PackageMetadataKey</a>(original_id);
    <b>let</b> <a href="../sui/config.md#sui_config">config</a> = <a href="../sui/config.md#sui_config_new">config::new</a>(&<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfigCap">PackageConfigCap</a>(), ctx);
    ofield::internal_add(&<b>mut</b> <a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key, <a href="../sui/config.md#sui_config">config</a>);
}
</code></pre>



</details>

<a name="sui_package_config_borrow_per_package_config_mut"></a>

## Function `borrow_per_package_config_mut`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_borrow_per_package_config_mut">borrow_per_package_config_mut</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): &<b>mut</b> <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;<a href="../sui/package_config.md#sui_package_config_PackageConfigCap">sui::package_config::PackageConfigCap</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_borrow_per_package_config_mut">borrow_per_package_config_mut</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
): &<b>mut</b> Config&lt;<a href="../sui/package_config.md#sui_package_config_PackageConfigCap">PackageConfigCap</a>&gt; {
    <b>let</b> key = <a href="../sui/package_config.md#sui_package_config_PackageMetadataKey">PackageMetadataKey</a>(original_id);
    ofield::internal_borrow_mut(&<b>mut</b> <a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key)
}
</code></pre>



</details>

<a name="sui_package_config_borrow_per_package_config"></a>

## Function `borrow_per_package_config`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_borrow_per_package_config">borrow_per_package_config</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): &<a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;<a href="../sui/package_config.md#sui_package_config_PackageConfigCap">sui::package_config::PackageConfigCap</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_borrow_per_package_config">borrow_per_package_config</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
): &Config&lt;<a href="../sui/package_config.md#sui_package_config_PackageConfigCap">PackageConfigCap</a>&gt; {
    <b>let</b> key = <a href="../sui/package_config.md#sui_package_config_PackageMetadataKey">PackageMetadataKey</a>(original_id);
    ofield::internal_borrow(&<a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key)
}
</code></pre>



</details>

<a name="sui_package_config_per_package_metadata_exists"></a>

## Function `per_package_metadata_exists`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_per_package_metadata_exists">per_package_metadata_exists</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_per_package_metadata_exists">per_package_metadata_exists</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>, original_id: ID): bool {
    <b>let</b> key = <a href="../sui/package_config.md#sui_package_config_PackageMetadataKey">PackageMetadataKey</a>(original_id);
    ofield::exists(&<a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key)
}
</code></pre>



</details>

<a name="sui_package_config_per_package_config_entry"></a>

## Macro function `per_package_config_entry`



<pre><code><b>macro</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_per_package_config_entry">per_package_config_entry</a>($<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, $original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, $ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): &<b>mut</b> <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;<a href="../sui/package_config.md#sui_package_config_PackageConfigCap">sui::package_config::PackageConfigCap</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>macro</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_per_package_config_entry">per_package_config_entry</a>(
    $<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    $original_id: ID,
    $ctx: &<b>mut</b> TxContext,
): &<b>mut</b> Config&lt;<a href="../sui/package_config.md#sui_package_config_PackageConfigCap">PackageConfigCap</a>&gt; {
    <b>let</b> <a href="../sui/package_config.md#sui_package_config">package_config</a> = $<a href="../sui/package_config.md#sui_package_config">package_config</a>;
    <b>let</b> original_id = $original_id;
    <b>let</b> ctx = $ctx;
    <b>if</b> (!<a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_per_package_metadata_exists">per_package_metadata_exists</a>(original_id)) {
        <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_add_per_package_config">add_per_package_config</a>(original_id, ctx);
    };
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_borrow_per_package_config_mut">borrow_per_package_config_mut</a>(original_id)
}
</code></pre>



</details>

<a name="sui_package_config_is_supported_bitset_version"></a>

## Function `is_supported_bitset_version`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_is_supported_bitset_version">is_supported_bitset_version</a>(flags: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_is_supported_bitset_version">is_supported_bitset_version</a>(flags: u64): bool {
    (flags &gt;&gt; <a href="../sui/package_config.md#sui_package_config_BITSET_VERSION_SHIFT">BITSET_VERSION_SHIFT</a>) == 0
}
</code></pre>



</details>

<a name="sui_package_config_is_version_forbidden"></a>

## Function `is_version_forbidden`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_is_version_forbidden">is_version_forbidden</a>(flags: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_is_version_forbidden">is_version_forbidden</a>(flags: u64): bool {
    !<a href="../sui/package_config.md#sui_package_config_is_supported_bitset_version">is_supported_bitset_version</a>(flags) || (flags & <a href="../sui/package_config.md#sui_package_config_VERSION_FORBIDDEN">VERSION_FORBIDDEN</a>) != 0
}
</code></pre>



</details>

<a name="sui_package_config_forbid_version_impl"></a>

## Function `forbid_version_impl`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_impl">forbid_version_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, version: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_impl">forbid_version_impl</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    version: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> <a href="../sui/config.md#sui_config">config</a> = <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_per_package_config_entry">per_package_config_entry</a>!(original_id, ctx);
    <a href="../sui/config.md#sui_config">config</a>.update!(
        &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfigCap">PackageConfigCap</a>(),
        <a href="../sui/package_config.md#sui_package_config_VersionForbiddenKey">VersionForbiddenKey</a>(version),
        |_package_config, _cap, _ctx| 0,
        |old_value, flags| {
            <b>if</b> (old_value.is_some()) *flags = old_value.destroy_some();
            <b>assert</b>!(<a href="../sui/package_config.md#sui_package_config_is_supported_bitset_version">is_supported_bitset_version</a>(*flags), <a href="../sui/package_config.md#sui_package_config_EUnsupportedBitsetVersion">EUnsupportedBitsetVersion</a>);
            *flags = *flags | <a href="../sui/package_config.md#sui_package_config_VERSION_FORBIDDEN">VERSION_FORBIDDEN</a>;
        },
        ctx,
    );
}
</code></pre>



</details>

<a name="sui_package_config_forbid_version"></a>

## Function `forbid_version`

Forbid a historical version of the package controlled by <code>cap</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version">forbid_version</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, cap: &<a href="../sui/package.md#sui_package_UpgradeCap">sui::package::UpgradeCap</a>, version: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version">forbid_version</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    cap: &UpgradeCap,
    version: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> current_version = <a href="../sui/package.md#sui_package_version">package::version</a>(cap);
    <b>assert</b>!(version &gt; 0 && version &lt; current_version, <a href="../sui/package_config.md#sui_package_config_EInvalidVersion">EInvalidVersion</a>);
    <a href="../sui/package_config.md#sui_package_config_forbid_version_impl">forbid_version_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>, <a href="../sui/package.md#sui_package_original_package_id">package::original_package_id</a>(cap), version, ctx);
}
</code></pre>



</details>

<a name="sui_package_config_forbid_version_range"></a>

## Function `forbid_version_range`

Forbid all historical versions in the inclusive range <code>[start, end]</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_range">forbid_version_range</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, cap: &<a href="../sui/package.md#sui_package_UpgradeCap">sui::package::UpgradeCap</a>, start: u64, end: u64, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_range">forbid_version_range</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    cap: &UpgradeCap,
    start: u64,
    end: u64,
    ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(start &lt;= end, <a href="../sui/package_config.md#sui_package_config_EInvalidVersionRange">EInvalidVersionRange</a>);
    start.range_do_eq!(end, |version| {
        <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_forbid_version">forbid_version</a>(cap, version, ctx);
    });
}
</code></pre>



</details>

<a name="sui_package_config_is_version_forbidden_for_next_epoch"></a>

## Function `is_version_forbidden_for_next_epoch`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/package_config.md#sui_package_config_is_version_forbidden_for_next_epoch">is_version_forbidden_for_next_epoch</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, version: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/package_config.md#sui_package_config_is_version_forbidden_for_next_epoch">is_version_forbidden_for_next_epoch</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    version: u64,
): bool {
    <b>if</b> (!<a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_per_package_metadata_exists">per_package_metadata_exists</a>(original_id)) <b>return</b> <b>false</b>;
    <b>let</b> <a href="../sui/config.md#sui_config">config</a> = <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_borrow_per_package_config">borrow_per_package_config</a>(original_id);
    <b>let</b> flags = <a href="../sui/config.md#sui_config">config</a>.read_setting_for_next_epoch&lt;_, _, u64&gt;(<a href="../sui/package_config.md#sui_package_config_VersionForbiddenKey">VersionForbiddenKey</a>(version));
    <b>if</b> (flags.is_none()) <b>return</b> <b>false</b>;
    <a href="../sui/package_config.md#sui_package_config_is_version_forbidden">is_version_forbidden</a>(flags.destroy_some())
}
</code></pre>



</details>
