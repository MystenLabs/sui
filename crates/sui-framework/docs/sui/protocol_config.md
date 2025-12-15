---
title: Module `sui::protocol_config`
---

This module provides access to protocol configuration feature flags.
Feature flags control the availability of various protocol features and
are enabled/disabled at specific protocol versions during epoch changes.


-  [Function `is_feature_enabled`](#sui_protocol_config_is_feature_enabled)
    -  [Arguments](#@Arguments_0)
    -  [Returns](#@Returns_1)
    -  [Example (for framework use only)](#@Example_(for_framework_use_only)_2)


<pre><code></code></pre>



<a name="sui_protocol_config_is_feature_enabled"></a>

## Function `is_feature_enabled`

Checks if a specific protocol feature flag is enabled.

Restricted to internal use within the sui-framework package only.
If we need to use it in sui-system, we can add friend declarations.
We should never need to expose this to user packages.


<a name="@Arguments_0"></a>

### Arguments

* <code>feature_flag_name</code> - The name of the feature flag as bytes (e.g., b"enable_vdf")
- It is expected to be a valid UTF-8 string
- The flag should exist in the protocol config


<a name="@Returns_1"></a>

### Returns

* <code><b>true</b></code> if the feature is enabled in the current protocol version
* <code><b>false</b></code> if the feature is disabled


<a name="@Example_(for_framework_use_only)_2"></a>

### Example (for framework use only)

```move
use sui::protocol_config;

if (protocol_config::is_feature_enabled(b"enable_accumulators")) {
// Accumulators are available
};
```


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/protocol_config.md#sui_protocol_config_is_feature_enabled">is_feature_enabled</a>(feature_flag_name: vector&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/protocol_config.md#sui_protocol_config_is_feature_enabled">is_feature_enabled</a>(feature_flag_name: vector&lt;u8&gt;): bool;
</code></pre>



</details>
