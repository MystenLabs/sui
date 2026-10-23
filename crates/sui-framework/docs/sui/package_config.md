---
title: Module `sui::package_config`
---

On-chain package configuration, keyed directly by original package ID.

```text
PackageConfig
├── MinVersionKey { original_id }
│   └── MinVersion { version, package_id }
├── VersionsForbiddenThroughKey { original_id }
│   └── a version through which every version is known to be forbidden
└── VersionForbiddenKey { original_id, chunk }
    └── a <code>u256</code> bitmap for 256 consecutive package versions
```


-  [Struct `PackageConfig`](#sui_package_config_PackageConfig)
-  [Struct `VersionForbiddenKey`](#sui_package_config_VersionForbiddenKey)
-  [Struct `VersionsForbiddenThroughKey`](#sui_package_config_VersionsForbiddenThroughKey)
-  [Struct `MinVersionKey`](#sui_package_config_MinVersionKey)
-  [Struct `MinVersion`](#sui_package_config_MinVersion)
-  [Constants](#@Constants_0)
-  [Function `forbid_version`](#sui_package_config_forbid_version)
-  [Function `forbid_version_range`](#sui_package_config_forbid_version_range)
-  [Function `is_version_forbidden`](#sui_package_config_is_version_forbidden)
-  [Function `record_minversion_enrollment`](#sui_package_config_record_minversion_enrollment)
-  [Function `record_minversion_upgrade`](#sui_package_config_record_minversion_upgrade)
-  [Function `record_minversion_upgrade_and_forbid_previous_versions`](#sui_package_config_record_minversion_upgrade_and_forbid_previous_versions)
-  [Function `create`](#sui_package_config_create)
-  [Function `record_minversion_upgrade_impl`](#sui_package_config_record_minversion_upgrade_impl)
-  [Function `replace_minversion`](#sui_package_config_replace_minversion)
-  [Function `cap_package_info`](#sui_package_config_cap_package_info)
-  [Function `assert_historical_version`](#sui_package_config_assert_historical_version)
-  [Function `assert_valid_forbid_range`](#sui_package_config_assert_valid_forbid_range)
-  [Function `version_location`](#sui_package_config_version_location)
-  [Function `bit_mask`](#sui_package_config_bit_mask)
-  [Function `prefix_mask`](#sui_package_config_prefix_mask)
-  [Function `range_mask`](#sui_package_config_range_mask)
-  [Function `forbidden_chunk_bits`](#sui_package_config_forbidden_chunk_bits)
-  [Function `mark_chunk_bits_forbidden`](#sui_package_config_mark_chunk_bits_forbidden)
-  [Function `forbid_version_impl`](#sui_package_config_forbid_version_impl)
-  [Function `forbid_version_range_impl`](#sui_package_config_forbid_version_range_impl)
-  [Function `forbid_previous_versions_impl`](#sui_package_config_forbid_previous_versions_impl)
-  [Function `scan_forbidden_through`](#sui_package_config_scan_forbidden_through)
-  [Function `advance_versions_forbidden_through`](#sui_package_config_advance_versions_forbidden_through)
-  [Function `versions_forbidden_through`](#sui_package_config_versions_forbidden_through)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/internal.md#std_internal">std::internal</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/type_name.md#std_type_name">std::type_name</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/package.md#sui_package">sui::package</a>;
<b>use</b> <a href="../sui/party.md#sui_party">sui::party</a>;
<b>use</b> <a href="../sui/scratch.md#sui_scratch">sui::scratch</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
<b>use</b> <a href="../sui/types.md#sui_types">sui::types</a>;
<b>use</b> <a href="../sui/vec_map.md#sui_vec_map">sui::vec_map</a>;
</code></pre>



<a name="sui_package_config_PackageConfig"></a>

## Struct `PackageConfig`

A shared singleton that stores package policy dynamic fields.


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

<a name="sui_package_config_VersionForbiddenKey"></a>

## Struct `VersionForbiddenKey`

Dynamic field key used to store a bitmap of 256 forbidden package versions.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/package_config.md#sui_package_config_VersionForbiddenKey">VersionForbiddenKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
<dt>
<code>chunk: u64</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_package_config_VersionsForbiddenThroughKey"></a>

## Struct `VersionsForbiddenThroughKey`

Dynamic field key recording a version through which every version in a package family is
known to be forbidden.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/package_config.md#sui_package_config_VersionsForbiddenThroughKey">VersionsForbiddenThroughKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_package_config_MinVersionKey"></a>

## Struct `MinVersionKey`

Dynamic field key used to store the stable minversion selection for a package family.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/package_config.md#sui_package_config_MinVersionKey">MinVersionKey</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_package_config_MinVersion"></a>

## Struct `MinVersion`

The package version selected by minversion.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/package_config.md#sui_package_config_MinVersion">MinVersion</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>version: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>package_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_package_config_ENotSystemAddress"></a>



<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_ENotSystemAddress">ENotSystemAddress</a>: u64 = 0;
</code></pre>



<a name="sui_package_config_EInvalidVersion"></a>



<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_EInvalidVersion">EInvalidVersion</a>: u64 = 1;
</code></pre>



<a name="sui_package_config_EInvalidVersionRange"></a>



<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_EInvalidVersionRange">EInvalidVersionRange</a>: u64 = 2;
</code></pre>



<a name="sui_package_config_EInvalidMinVersionState"></a>



<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_EInvalidMinVersionState">EInvalidMinVersionState</a>: u64 = 3;
</code></pre>



<a name="sui_package_config_ETooManyVersionsToForbid"></a>



<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_ETooManyVersionsToForbid">ETooManyVersionsToForbid</a>: u64 = 4;
</code></pre>



<a name="sui_package_config_VERSIONS_PER_FORBID_CHUNK"></a>



<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_VERSIONS_PER_FORBID_CHUNK">VERSIONS_PER_FORBID_CHUNK</a>: u64 = 256;
</code></pre>



<a name="sui_package_config_MAX_VERSIONS_PER_FORBID"></a>



<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_MAX_VERSIONS_PER_FORBID">MAX_VERSIONS_PER_FORBID</a>: u64 = 512;
</code></pre>



<a name="sui_package_config_forbid_version"></a>

## Function `forbid_version`

Forbid a historical version of the package controlled by <code>cap</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version">forbid_version</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, cap: &<b>mut</b> <a href="../sui/package.md#sui_package_UpgradeCap">sui::package::UpgradeCap</a>, version: u64, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version">forbid_version</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    cap: &<b>mut</b> UpgradeCap,
    version: u64,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> (original_id, current_version) = <a href="../sui/package_config.md#sui_package_config_cap_package_info">cap_package_info</a>(cap);
    <a href="../sui/package_config.md#sui_package_config_assert_historical_version">assert_historical_version</a>(version, current_version);
    <a href="../sui/package_config.md#sui_package_config_forbid_version_impl">forbid_version_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>, original_id, version);
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_advance_versions_forbidden_through">advance_versions_forbidden_through</a>(
        original_id,
        version,
        version,
        <a href="../sui/package_config.md#sui_package_config_MAX_VERSIONS_PER_FORBID">MAX_VERSIONS_PER_FORBID</a> - 1,
    );
}
</code></pre>



</details>

<a name="sui_package_config_forbid_version_range"></a>

## Function `forbid_version_range`

Forbid at most 512 historical versions in the inclusive range <code>[start, end]</code>.
If the range connects to the tracked forbidden prefix, extend the prefix through <code>end</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_range">forbid_version_range</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, cap: &<b>mut</b> <a href="../sui/package.md#sui_package_UpgradeCap">sui::package::UpgradeCap</a>, start: u64, end: u64, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_range">forbid_version_range</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    cap: &<b>mut</b> UpgradeCap,
    start: u64,
    end: u64,
    _ctx: &<b>mut</b> TxContext,
) {
    <a href="../sui/package_config.md#sui_package_config_assert_valid_forbid_range">assert_valid_forbid_range</a>(start, end);
    <b>let</b> range_size = end - start + 1;
    <b>let</b> (original_id, current_version) = <a href="../sui/package_config.md#sui_package_config_cap_package_info">cap_package_info</a>(cap);
    // A valid range and a historical end establish only the upper bound.
    <b>assert</b>!(start &gt; 0, <a href="../sui/package_config.md#sui_package_config_EInvalidVersion">EInvalidVersion</a>);
    <a href="../sui/package_config.md#sui_package_config_assert_historical_version">assert_historical_version</a>(end, current_version);
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_forbid_version_range_impl">forbid_version_range_impl</a>(original_id, start, end);
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_advance_versions_forbidden_through">advance_versions_forbidden_through</a>(
        original_id,
        start,
        end,
        <a href="../sui/package_config.md#sui_package_config_MAX_VERSIONS_PER_FORBID">MAX_VERSIONS_PER_FORBID</a> - range_size,
    );
}
</code></pre>



</details>

<a name="sui_package_config_is_version_forbidden"></a>

## Function `is_version_forbidden`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/package_config.md#sui_package_config_is_version_forbidden">is_version_forbidden</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, version: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/package_config.md#sui_package_config_is_version_forbidden">is_version_forbidden</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    version: u64,
): bool {
    <b>assert</b>!(version &gt; 0, <a href="../sui/package_config.md#sui_package_config_EInvalidVersion">EInvalidVersion</a>);
    <b>let</b> (chunk, bit) = <a href="../sui/package_config.md#sui_package_config_version_location">version_location</a>(version);
    <b>let</b> bits = <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_forbidden_chunk_bits">forbidden_chunk_bits</a>(original_id, chunk);
    bits & <a href="../sui/package_config.md#sui_package_config_bit_mask">bit_mask</a>(bit) != 0
}
</code></pre>



</details>

<a name="sui_package_config_record_minversion_enrollment"></a>

## Function `record_minversion_enrollment`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_enrollment">record_minversion_enrollment</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, enrollment: <a href="../sui/package.md#sui_package_MinVersionEnrollment">sui::package::MinVersionEnrollment</a>, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_enrollment">record_minversion_enrollment</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    enrollment: MinVersionEnrollment,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> (original_id, version, package_id) = <a href="../sui/package.md#sui_package_minversion_enrollment_info">package::minversion_enrollment_info</a>(enrollment);
    <b>let</b> old = <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_replace_minversion">replace_minversion</a>(original_id, version, package_id);
    <b>assert</b>!(old.is_none(), <a href="../sui/package_config.md#sui_package_config_EInvalidMinVersionState">EInvalidMinVersionState</a>);
    old.destroy_none();
}
</code></pre>



</details>

<a name="sui_package_config_record_minversion_upgrade"></a>

## Function `record_minversion_upgrade`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade">record_minversion_upgrade</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, cap: &<a href="../sui/package.md#sui_package_UpgradeCap">sui::package::UpgradeCap</a>, receipt: &<a href="../sui/package.md#sui_package_UpgradeReceipt">sui::package::UpgradeReceipt</a>, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade">record_minversion_upgrade</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    cap: &UpgradeCap,
    receipt: &UpgradeReceipt,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> (original_id, previous_version, previous_package_id, package_id) =
        <a href="../sui/package.md#sui_package_record_minversion_upgrade_info">package::record_minversion_upgrade_info</a>(cap, receipt);
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade_impl">record_minversion_upgrade_impl</a>(
        original_id,
        previous_version,
        previous_package_id,
        package_id,
    );
}
</code></pre>



</details>

<a name="sui_package_config_record_minversion_upgrade_and_forbid_previous_versions"></a>

## Function `record_minversion_upgrade_and_forbid_previous_versions`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade_and_forbid_previous_versions">record_minversion_upgrade_and_forbid_previous_versions</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, cap: &<a href="../sui/package.md#sui_package_UpgradeCap">sui::package::UpgradeCap</a>, receipt: &<a href="../sui/package.md#sui_package_UpgradeReceipt">sui::package::UpgradeReceipt</a>, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade_and_forbid_previous_versions">record_minversion_upgrade_and_forbid_previous_versions</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    cap: &UpgradeCap,
    receipt: &UpgradeReceipt,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> (original_id, previous_version, previous_package_id, package_id) =
        <a href="../sui/package.md#sui_package_record_minversion_upgrade_info">package::record_minversion_upgrade_info</a>(cap, receipt);
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade_impl">record_minversion_upgrade_impl</a>(
        original_id,
        previous_version,
        previous_package_id,
        package_id,
    );
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_forbid_previous_versions_impl">forbid_previous_versions_impl</a>(original_id, previous_version);
}
</code></pre>



</details>

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

<a name="sui_package_config_record_minversion_upgrade_impl"></a>

## Function `record_minversion_upgrade_impl`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade_impl">record_minversion_upgrade_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, previous_version: u64, previous_package_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, package_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade_impl">record_minversion_upgrade_impl</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    previous_version: u64,
    previous_package_id: ID,
    package_id: ID,
) {
    <b>let</b> old = <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_replace_minversion">replace_minversion</a>(original_id, previous_version + 1, package_id);
    <b>assert</b>!(old.is_some(), <a href="../sui/package_config.md#sui_package_config_EInvalidMinVersionState">EInvalidMinVersionState</a>);
    <b>let</b> <a href="../sui/package_config.md#sui_package_config_MinVersion">MinVersion</a> { version, package_id } = old.destroy_some();
    <b>assert</b>!(
        version == previous_version && package_id == previous_package_id,
        <a href="../sui/package_config.md#sui_package_config_EInvalidMinVersionState">EInvalidMinVersionState</a>,
    );
}
</code></pre>



</details>

<a name="sui_package_config_replace_minversion"></a>

## Function `replace_minversion`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_replace_minversion">replace_minversion</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, version: u64, package_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/package_config.md#sui_package_config_MinVersion">sui::package_config::MinVersion</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_replace_minversion">replace_minversion</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    version: u64,
    package_id: ID,
): Option&lt;<a href="../sui/package_config.md#sui_package_config_MinVersion">MinVersion</a>&gt; {
    <b>let</b> key = <a href="../sui/package_config.md#sui_package_config_MinVersionKey">MinVersionKey</a> { original_id };
    <b>let</b> value = <a href="../sui/package_config.md#sui_package_config_MinVersion">MinVersion</a> { version, package_id };
    field::replace&lt;<a href="../sui/package_config.md#sui_package_config_MinVersionKey">MinVersionKey</a>, <a href="../sui/package_config.md#sui_package_config_MinVersion">MinVersion</a>, <a href="../sui/package_config.md#sui_package_config_MinVersion">MinVersion</a>&gt;(&<b>mut</b> <a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key, value)
}
</code></pre>



</details>

<a name="sui_package_config_cap_package_info"></a>

## Function `cap_package_info`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_cap_package_info">cap_package_info</a>(cap: &<a href="../sui/package.md#sui_package_UpgradeCap">sui::package::UpgradeCap</a>): (<a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_cap_package_info">cap_package_info</a>(cap: &UpgradeCap): (ID, u64) {
    (cap.original_package_id(), <a href="../sui/package.md#sui_package_version">package::version</a>(cap))
}
</code></pre>



</details>

<a name="sui_package_config_assert_historical_version"></a>

## Function `assert_historical_version`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_assert_historical_version">assert_historical_version</a>(version: u64, current_version: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_assert_historical_version">assert_historical_version</a>(version: u64, current_version: u64) {
    <b>assert</b>!(version &gt; 0 && version &lt; current_version, <a href="../sui/package_config.md#sui_package_config_EInvalidVersion">EInvalidVersion</a>);
}
</code></pre>



</details>

<a name="sui_package_config_assert_valid_forbid_range"></a>

## Function `assert_valid_forbid_range`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_assert_valid_forbid_range">assert_valid_forbid_range</a>(start: u64, end: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_assert_valid_forbid_range">assert_valid_forbid_range</a>(start: u64, end: u64) {
    <b>assert</b>!(start &lt;= end, <a href="../sui/package_config.md#sui_package_config_EInvalidVersionRange">EInvalidVersionRange</a>);
    <b>assert</b>!(end - start &lt; <a href="../sui/package_config.md#sui_package_config_MAX_VERSIONS_PER_FORBID">MAX_VERSIONS_PER_FORBID</a>, <a href="../sui/package_config.md#sui_package_config_ETooManyVersionsToForbid">ETooManyVersionsToForbid</a>);
}
</code></pre>



</details>

<a name="sui_package_config_version_location"></a>

## Function `version_location`

Returns the bitmap chunk and bit position for a nonzero package version.
For example, versions 1, 256, and 257 map to <code>(0, 0)</code>, <code>(0, 255)</code>, and <code>(1, 0)</code>.


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_version_location">version_location</a>(version: u64): (u64, u8)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_version_location">version_location</a>(version: u64): (u64, u8) {
    <b>let</b> index = version - 1;
    (
        index / <a href="../sui/package_config.md#sui_package_config_VERSIONS_PER_FORBID_CHUNK">VERSIONS_PER_FORBID_CHUNK</a>,
        (index % <a href="../sui/package_config.md#sui_package_config_VERSIONS_PER_FORBID_CHUNK">VERSIONS_PER_FORBID_CHUNK</a>) <b>as</b> u8,
    )
}
</code></pre>



</details>

<a name="sui_package_config_bit_mask"></a>

## Function `bit_mask`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_bit_mask">bit_mask</a>(bit: u8): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_bit_mask">bit_mask</a>(bit: u8): u256 {
    1u256 &lt;&lt; bit
}
</code></pre>



</details>

<a name="sui_package_config_prefix_mask"></a>

## Function `prefix_mask`

Returns a mask with bits zero through <code>bit</code> set, inclusive.
For example, <code><a href="../sui/package_config.md#sui_package_config_prefix_mask">prefix_mask</a>(3)</code> is <code>0b1111</code>.


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_prefix_mask">prefix_mask</a>(bit: u8): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_prefix_mask">prefix_mask</a>(bit: u8): u256 {
    <b>if</b> (bit == 255) <a href="../std/u256.md#std_u256_max_value">std::u256::max_value</a>!() <b>else</b> (1u256 &lt;&lt; (bit + 1)) - 1
}
</code></pre>



</details>

<a name="sui_package_config_range_mask"></a>

## Function `range_mask`

Returns a mask with bits <code>first</code> through <code>last</code> set, inclusive.
For example, <code><a href="../sui/package_config.md#sui_package_config_range_mask">range_mask</a>(2, 4)</code> is <code>0b1_1100</code>.


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_range_mask">range_mask</a>(first: u8, last: u8): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_range_mask">range_mask</a>(first: u8, last: u8): u256 {
    <b>let</b> before_first = <b>if</b> (first == 0) 0 <b>else</b> <a href="../sui/package_config.md#sui_package_config_prefix_mask">prefix_mask</a>(first - 1);
    <a href="../sui/package_config.md#sui_package_config_prefix_mask">prefix_mask</a>(last) ^ before_first
}
</code></pre>



</details>

<a name="sui_package_config_forbidden_chunk_bits"></a>

## Function `forbidden_chunk_bits`

Returns the stored forbidden-version bitmap for <code>chunk</code>, or zero if it does not exist.
For example, <code>0b101</code> in chunk zero means package versions 1 and 3 are forbidden.


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbidden_chunk_bits">forbidden_chunk_bits</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, chunk: u64): u256
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbidden_chunk_bits">forbidden_chunk_bits</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>, original_id: ID, chunk: u64): u256 {
    <b>let</b> key = <a href="../sui/package_config.md#sui_package_config_VersionForbiddenKey">VersionForbiddenKey</a> { original_id, chunk };
    field::get_fold!(&<a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key, 0u256, |bits: &u256| *bits)
}
</code></pre>



</details>

<a name="sui_package_config_mark_chunk_bits_forbidden"></a>

## Function `mark_chunk_bits_forbidden`

Marks each bit in <code>mask</code> as forbidden in <code>chunk</code>, creating the chunk if it does not exist.


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_mark_chunk_bits_forbidden">mark_chunk_bits_forbidden</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, chunk: u64, mask: u256)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_mark_chunk_bits_forbidden">mark_chunk_bits_forbidden</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    chunk: u64,
    mask: u256,
) {
    <b>let</b> key = <a href="../sui/package_config.md#sui_package_config_VersionForbiddenKey">VersionForbiddenKey</a> { original_id, chunk };
    <b>let</b> bits = field::borrow_mut_or_add!(&<b>mut</b> <a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key, 0u256);
    *bits = *bits | mask;
}
</code></pre>



</details>

<a name="sui_package_config_forbid_version_impl"></a>

## Function `forbid_version_impl`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_impl">forbid_version_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, version: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_impl">forbid_version_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>, original_id: ID, version: u64) {
    <b>let</b> (chunk, bit) = <a href="../sui/package_config.md#sui_package_config_version_location">version_location</a>(version);
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_mark_chunk_bits_forbidden">mark_chunk_bits_forbidden</a>(original_id, chunk, <a href="../sui/package_config.md#sui_package_config_bit_mask">bit_mask</a>(bit));
}
</code></pre>



</details>

<a name="sui_package_config_forbid_version_range_impl"></a>

## Function `forbid_version_range_impl`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_range_impl">forbid_version_range_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, start: u64, end: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_range_impl">forbid_version_range_impl</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    start: u64,
    end: u64,
) {
    <b>let</b> (first_chunk, first_bit) = <a href="../sui/package_config.md#sui_package_config_version_location">version_location</a>(start);
    <b>let</b> (last_chunk, last_bit) = <a href="../sui/package_config.md#sui_package_config_version_location">version_location</a>(end);
    first_chunk.range_do_eq!(last_chunk, |chunk| {
        <b>let</b> first = <b>if</b> (chunk == first_chunk) first_bit <b>else</b> 0;
        <b>let</b> last = <b>if</b> (chunk == last_chunk) last_bit <b>else</b> 255;
        <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_mark_chunk_bits_forbidden">mark_chunk_bits_forbidden</a>(original_id, chunk, <a href="../sui/package_config.md#sui_package_config_range_mask">range_mask</a>(first, last));
    });
}
</code></pre>



</details>

<a name="sui_package_config_forbid_previous_versions_impl"></a>

## Function `forbid_previous_versions_impl`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_previous_versions_impl">forbid_previous_versions_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, end: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_previous_versions_impl">forbid_previous_versions_impl</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    end: u64,
) {
    <b>let</b> forbidden_through = <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_versions_forbidden_through">versions_forbidden_through</a>(original_id);
    <b>if</b> (forbidden_through &gt;= end) <b>return</b>;
    // `forbidden_through &lt; end` guarantees that this addition cannot overflow.
    <b>let</b> start = forbidden_through + 1;
    <a href="../sui/package_config.md#sui_package_config_assert_valid_forbid_range">assert_valid_forbid_range</a>(start, end);
    <b>let</b> range_size = end - start + 1;
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_forbid_version_range_impl">forbid_version_range_impl</a>(original_id, start, end);
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_advance_versions_forbidden_through">advance_versions_forbidden_through</a>(
        original_id,
        start,
        end,
        <a href="../sui/package_config.md#sui_package_config_MAX_VERSIONS_PER_FORBID">MAX_VERSIONS_PER_FORBID</a> - range_size,
    );
}
</code></pre>



</details>

<a name="sui_package_config_scan_forbidden_through"></a>

## Function `scan_forbidden_through`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_scan_forbidden_through">scan_forbidden_through</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, forbidden_through: u64, remaining: u64): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_scan_forbidden_through">scan_forbidden_through</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    <b>mut</b> forbidden_through: u64,
    <b>mut</b> remaining: u64,
): u64 {
    <b>while</b> (remaining &gt; 0 && forbidden_through &lt; <a href="../std/u64.md#std_u64_max_value">std::u64::max_value</a>!()) {
        <b>let</b> (chunk, <b>mut</b> bit) = <a href="../sui/package_config.md#sui_package_config_version_location">version_location</a>(forbidden_through + 1);
        <b>let</b> bits = <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_forbidden_chunk_bits">forbidden_chunk_bits</a>(original_id, chunk);
        <b>while</b> (remaining &gt; 0) {
            remaining = remaining - 1;
            <b>if</b> (bits & <a href="../sui/package_config.md#sui_package_config_bit_mask">bit_mask</a>(bit) == 0) <b>return</b> forbidden_through;
            forbidden_through = forbidden_through + 1;
            <b>if</b> (bit == 255 || forbidden_through == <a href="../std/u64.md#std_u64_max_value">std::u64::max_value</a>!()) <b>break</b>;
            bit = bit + 1;
        };
    };
    forbidden_through
}
</code></pre>



</details>

<a name="sui_package_config_advance_versions_forbidden_through"></a>

## Function `advance_versions_forbidden_through`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_advance_versions_forbidden_through">advance_versions_forbidden_through</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, start: u64, end: u64, remaining: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_advance_versions_forbidden_through">advance_versions_forbidden_through</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    start: u64,
    end: u64,
    remaining: u64,
) {
    <b>let</b> old_forbidden_through = <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_versions_forbidden_through">versions_forbidden_through</a>(original_id);
    <b>if</b> (end &gt; old_forbidden_through && start &gt; old_forbidden_through + 1) <b>return</b>;
    <b>let</b> forbidden_through = <b>if</b> (end &gt; old_forbidden_through) end <b>else</b> old_forbidden_through;
    <b>let</b> forbidden_through = <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_scan_forbidden_through">scan_forbidden_through</a>(
        original_id,
        forbidden_through,
        remaining,
    );
    <b>if</b> (forbidden_through == old_forbidden_through) <b>return</b>;
    <b>let</b> key = <a href="../sui/package_config.md#sui_package_config_VersionsForbiddenThroughKey">VersionsForbiddenThroughKey</a> { original_id };
    field::replace&lt;<a href="../sui/package_config.md#sui_package_config_VersionsForbiddenThroughKey">VersionsForbiddenThroughKey</a>, u64, u64&gt;(
        &<b>mut</b> <a href="../sui/package_config.md#sui_package_config">package_config</a>.id,
        key,
        forbidden_through,
    );
}
</code></pre>



</details>

<a name="sui_package_config_versions_forbidden_through"></a>

## Function `versions_forbidden_through`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_versions_forbidden_through">versions_forbidden_through</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_versions_forbidden_through">versions_forbidden_through</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>, original_id: ID): u64 {
    <b>let</b> key = <a href="../sui/package_config.md#sui_package_config_VersionsForbiddenThroughKey">VersionsForbiddenThroughKey</a> { original_id };
    field::get_fold!(&<a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key, 0, |version: &u64| *version)
}
</code></pre>



</details>
