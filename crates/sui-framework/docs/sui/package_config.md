---
title: Module `sui::package_config`
---

On-chain package configuration, keyed directly by original package ID.

```text
PackageConfig
├── MinVersionKey { original_id }
│   └── MinVersion { version, package_id }
└── VersionForbiddenKey { original_id, version }
    └── VERSION_FORBIDDEN
```


-  [Struct `PackageConfig`](#sui_package_config_PackageConfig)
-  [Struct `VersionForbiddenKey`](#sui_package_config_VersionForbiddenKey)
-  [Struct `MinVersionKey`](#sui_package_config_MinVersionKey)
-  [Struct `MinVersion`](#sui_package_config_MinVersion)
-  [Constants](#@Constants_0)
-  [Function `forbid_version`](#sui_package_config_forbid_version)
-  [Function `forbid_version_range`](#sui_package_config_forbid_version_range)
-  [Function `is_version_forbidden`](#sui_package_config_is_version_forbidden)
-  [Function `record_minversion_enrollment`](#sui_package_config_record_minversion_enrollment)
-  [Function `record_minversion_upgrade`](#sui_package_config_record_minversion_upgrade)
-  [Function `record_minversion_upgrade_and_forbid_previous`](#sui_package_config_record_minversion_upgrade_and_forbid_previous)
-  [Function `create`](#sui_package_config_create)
-  [Function `is_forbidden_value`](#sui_package_config_is_forbidden_value)
-  [Function `record_minversion_impl`](#sui_package_config_record_minversion_impl)
-  [Function `cap_package_info`](#sui_package_config_cap_package_info)
-  [Function `assert_historical_version`](#sui_package_config_assert_historical_version)
-  [Function `forbid_version_impl`](#sui_package_config_forbid_version_impl)


<pre><code><b>use</b> <a href="../std/address.md#std_address">std::address</a>;
<b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
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

Dynamic field key used to store the forbid-list value for one package version.


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
<code>version: u64</code>
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



<a name="sui_package_config_VERSION_FORBIDDEN"></a>



<pre><code><b>const</b> <a href="../sui/package_config.md#sui_package_config_VERSION_FORBIDDEN">VERSION_FORBIDDEN</a>: u64 = 1;
</code></pre>



<a name="sui_package_config_forbid_version"></a>

## Function `forbid_version`

Forbid a historical version of the package controlled by <code>cap</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version">forbid_version</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, cap: &<a href="../sui/package.md#sui_package_UpgradeCap">sui::package::UpgradeCap</a>, version: u64, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version">forbid_version</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    cap: &UpgradeCap,
    version: u64,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> (original_id, current_version) = <a href="../sui/package_config.md#sui_package_config_cap_package_info">cap_package_info</a>(cap);
    <a href="../sui/package_config.md#sui_package_config_assert_historical_version">assert_historical_version</a>(version, current_version);
    <a href="../sui/package_config.md#sui_package_config_forbid_version_impl">forbid_version_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>, original_id, version);
}
</code></pre>



</details>

<a name="sui_package_config_forbid_version_range"></a>

## Function `forbid_version_range`

Forbid all historical versions in the inclusive range <code>[start, end]</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_range">forbid_version_range</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, cap: &<a href="../sui/package.md#sui_package_UpgradeCap">sui::package::UpgradeCap</a>, start: u64, end: u64, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_range">forbid_version_range</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    cap: &UpgradeCap,
    start: u64,
    end: u64,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>assert</b>!(start &lt;= end, <a href="../sui/package_config.md#sui_package_config_EInvalidVersionRange">EInvalidVersionRange</a>);
    <b>let</b> (original_id, current_version) = <a href="../sui/package_config.md#sui_package_config_cap_package_info">cap_package_info</a>(cap);
    // `start &lt;= end` and a historical end establish only the upper bound.
    <b>assert</b>!(start &gt; 0, <a href="../sui/package_config.md#sui_package_config_EInvalidVersion">EInvalidVersion</a>);
    <a href="../sui/package_config.md#sui_package_config_assert_historical_version">assert_historical_version</a>(end, current_version);
    start.range_do_eq!(end, |version| {
        <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_forbid_version_impl">forbid_version_impl</a>(original_id, version);
    });
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
    <b>let</b> forbid_key = <a href="../sui/package_config.md#sui_package_config_VersionForbiddenKey">VersionForbiddenKey</a> { original_id, version };
    <b>if</b> (!field::exists_with_type&lt;_, u64&gt;(&<a href="../sui/package_config.md#sui_package_config">package_config</a>.id, forbid_key)) <b>return</b> <b>false</b>;
    <a href="../sui/package_config.md#sui_package_config_is_forbidden_value">is_forbidden_value</a>(*field::borrow(&<a href="../sui/package_config.md#sui_package_config">package_config</a>.id, forbid_key))
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
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_record_minversion_impl">record_minversion_impl</a>(original_id, version, package_id);
}
</code></pre>



</details>

<a name="sui_package_config_record_minversion_upgrade"></a>

## Function `record_minversion_upgrade`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade">record_minversion_upgrade</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, upgrade: <a href="../sui/package.md#sui_package_MinVersionUpgrade">sui::package::MinVersionUpgrade</a>, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade">record_minversion_upgrade</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    upgrade: MinVersionUpgrade,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> (original_id, _previous_version, version, package_id) =
        <a href="../sui/package.md#sui_package_minversion_upgrade_info">package::minversion_upgrade_info</a>(upgrade);
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_record_minversion_impl">record_minversion_impl</a>(original_id, version, package_id);
}
</code></pre>



</details>

<a name="sui_package_config_record_minversion_upgrade_and_forbid_previous"></a>

## Function `record_minversion_upgrade_and_forbid_previous`



<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade_and_forbid_previous">record_minversion_upgrade_and_forbid_previous</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, upgrade: <a href="../sui/package.md#sui_package_MinVersionUpgrade">sui::package::MinVersionUpgrade</a>, _ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_upgrade_and_forbid_previous">record_minversion_upgrade_and_forbid_previous</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    upgrade: MinVersionUpgrade,
    _ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> (original_id, previous_version, version, package_id) =
        <a href="../sui/package.md#sui_package_minversion_upgrade_info">package::minversion_upgrade_info</a>(upgrade);
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_record_minversion_impl">record_minversion_impl</a>(original_id, version, package_id);
    <a href="../sui/package_config.md#sui_package_config">package_config</a>.<a href="../sui/package_config.md#sui_package_config_forbid_version_impl">forbid_version_impl</a>(original_id, previous_version);
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

<a name="sui_package_config_is_forbidden_value"></a>

## Function `is_forbidden_value`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_is_forbidden_value">is_forbidden_value</a>(value: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_is_forbidden_value">is_forbidden_value</a>(value: u64): bool {
    value == <a href="../sui/package_config.md#sui_package_config_VERSION_FORBIDDEN">VERSION_FORBIDDEN</a>
}
</code></pre>



</details>

<a name="sui_package_config_record_minversion_impl"></a>

## Function `record_minversion_impl`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_impl">record_minversion_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, version: u64, package_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_record_minversion_impl">record_minversion_impl</a>(
    <a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>,
    original_id: ID,
    version: u64,
    package_id: ID,
) {
    <b>let</b> key = <a href="../sui/package_config.md#sui_package_config_MinVersionKey">MinVersionKey</a> { original_id };
    <b>let</b> value = <a href="../sui/package_config.md#sui_package_config_MinVersion">MinVersion</a> { version, package_id };
    <b>if</b> (field::exists_with_type&lt;_, <a href="../sui/package_config.md#sui_package_config_MinVersion">MinVersion</a>&gt;(&<a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key)) {
        *field::borrow_mut(&<b>mut</b> <a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key) = value;
    } <b>else</b> {
        field::add(&<b>mut</b> <a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key, value);
    }
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

<a name="sui_package_config_forbid_version_impl"></a>

## Function `forbid_version_impl`



<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_impl">forbid_version_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">sui::package_config::PackageConfig</a>, original_id: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, version: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="../sui/package_config.md#sui_package_config_forbid_version_impl">forbid_version_impl</a>(<a href="../sui/package_config.md#sui_package_config">package_config</a>: &<b>mut</b> <a href="../sui/package_config.md#sui_package_config_PackageConfig">PackageConfig</a>, original_id: ID, version: u64) {
    <b>let</b> key = <a href="../sui/package_config.md#sui_package_config_VersionForbiddenKey">VersionForbiddenKey</a> { original_id, version };
    <b>if</b> (field::exists_with_type&lt;_, u64&gt;(&<a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key)) {
        *field::borrow_mut(&<b>mut</b> <a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key) = <a href="../sui/package_config.md#sui_package_config_VERSION_FORBIDDEN">VERSION_FORBIDDEN</a>;
    } <b>else</b> {
        field::add(&<b>mut</b> <a href="../sui/package_config.md#sui_package_config">package_config</a>.id, key, <a href="../sui/package_config.md#sui_package_config_VERSION_FORBIDDEN">VERSION_FORBIDDEN</a>);
    }
}
</code></pre>



</details>
