---
title: Module `sui::config`
---



-  [Struct `Config`](#sui_config_Config)
-  [Struct `Setting`](#sui_config_Setting)
-  [Struct `SettingData`](#sui_config_SettingData)
-  [Constants](#@Constants_0)
-  [Function `new`](#sui_config_new)
-  [Function `share`](#sui_config_share)
-  [Function `transfer`](#sui_config_transfer)
-  [Function `add_for_next_epoch`](#sui_config_add_for_next_epoch)
-  [Function `remove_for_next_epoch`](#sui_config_remove_for_next_epoch)
-  [Function `exists_with_type`](#sui_config_exists_with_type)
-  [Function `exists_with_type_for_next_epoch`](#sui_config_exists_with_type_for_next_epoch)
-  [Function `borrow_for_next_epoch_mut`](#sui_config_borrow_for_next_epoch_mut)
-  [Function `read_setting_for_next_epoch`](#sui_config_read_setting_for_next_epoch)
-  [Macro function `entry`](#sui_config_entry)
-  [Macro function `update`](#sui_config_update)
-  [Function `read_setting`](#sui_config_read_setting)
-  [Function `read_setting_impl`](#sui_config_read_setting_impl)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field">sui::dynamic_field</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/transfer.md#sui_transfer">sui::transfer</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="sui_config_Config"></a>

## Struct `Config`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/config.md#sui_config_Config">Config</a>&lt;<b>phantom</b> WriteCap&gt; <b>has</b> key
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

<a name="sui_config_Setting"></a>

## Struct `Setting`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value: <b>copy</b>, drop, store&gt; <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>data: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;<a href="../sui/config.md#sui_config_SettingData">sui::config::SettingData</a>&lt;Value&gt;&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_config_SettingData"></a>

## Struct `SettingData`



<pre><code><b>public</b> <b>struct</b> <a href="../sui/config.md#sui_config_SettingData">SettingData</a>&lt;Value: <b>copy</b>, drop, store&gt; <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>newer_value_epoch: u64</code>
</dt>
<dd>
</dd>
<dt>
<code>newer_value: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Value&gt;</code>
</dt>
<dd>
</dd>
<dt>
<code>older_value_opt: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Value&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_config_EAlreadySetForEpoch"></a>



<pre><code><b>const</b> <a href="../sui/config.md#sui_config_EAlreadySetForEpoch">EAlreadySetForEpoch</a>: u64 = 0;
</code></pre>



<a name="sui_config_EBCSSerializationFailure"></a>



<pre><code><b>const</b> <a href="../sui/config.md#sui_config_EBCSSerializationFailure">EBCSSerializationFailure</a>: u64 = 2;
</code></pre>



<a name="sui_config_ENotSetForEpoch"></a>



<pre><code><b>const</b> <a href="../sui/config.md#sui_config_ENotSetForEpoch">ENotSetForEpoch</a>: u64 = 1;
</code></pre>



<a name="sui_config_new"></a>

## Function `new`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_new">new</a>&lt;WriteCap&gt;(_cap: &<b>mut</b> WriteCap, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;WriteCap&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_new">new</a>&lt;WriteCap&gt;(_cap: &<b>mut</b> WriteCap, ctx: &<b>mut</b> TxContext): <a href="../sui/config.md#sui_config_Config">Config</a>&lt;WriteCap&gt; {
    <a href="../sui/config.md#sui_config_Config">Config</a>&lt;WriteCap&gt; { id: <a href="../sui/object.md#sui_object_new">object::new</a>(ctx) }
}
</code></pre>



</details>

<a name="sui_config_share"></a>

## Function `share`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_share">share</a>&lt;WriteCap&gt;(<a href="../sui/config.md#sui_config">config</a>: <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;WriteCap&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_share">share</a>&lt;WriteCap&gt;(<a href="../sui/config.md#sui_config">config</a>: <a href="../sui/config.md#sui_config_Config">Config</a>&lt;WriteCap&gt;) {
    <a href="../sui/transfer.md#sui_transfer_share_object">transfer::share_object</a>(<a href="../sui/config.md#sui_config">config</a>)
}
</code></pre>



</details>

<a name="sui_config_transfer"></a>

## Function `transfer`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/transfer.md#sui_transfer">transfer</a>&lt;WriteCap&gt;(<a href="../sui/config.md#sui_config">config</a>: <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;WriteCap&gt;, owner: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/transfer.md#sui_transfer">transfer</a>&lt;WriteCap&gt;(<a href="../sui/config.md#sui_config">config</a>: <a href="../sui/config.md#sui_config_Config">Config</a>&lt;WriteCap&gt;, owner: <b>address</b>) {
    <a href="../sui/transfer.md#sui_transfer_transfer">transfer::transfer</a>(<a href="../sui/config.md#sui_config">config</a>, owner)
}
</code></pre>



</details>

<a name="sui_config_add_for_next_epoch"></a>

## Function `add_for_next_epoch`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_add_for_next_epoch">add_for_next_epoch</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui/config.md#sui_config">config</a>: &<b>mut</b> <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;WriteCap&gt;, _cap: &<b>mut</b> WriteCap, name: Name, value: Value, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Value&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_add_for_next_epoch">add_for_next_epoch</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui/config.md#sui_config">config</a>: &<b>mut</b> <a href="../sui/config.md#sui_config_Config">Config</a>&lt;WriteCap&gt;,
    _cap: &<b>mut</b> WriteCap,
    name: Name,
    value: Value,
    ctx: &<b>mut</b> TxContext,
): Option&lt;Value&gt; {
    <b>let</b> epoch = ctx.epoch();
    <b>if</b> (!field::exists_(&<a href="../sui/config.md#sui_config">config</a>.id, name)) {
        <b>let</b> sobj = <a href="../sui/config.md#sui_config_Setting">Setting</a> {
            data: option::some(<a href="../sui/config.md#sui_config_SettingData">SettingData</a> {
                newer_value_epoch: epoch,
                newer_value: option::some(value),
                older_value_opt: option::none(),
            }),
        };
        field::add(&<b>mut</b> <a href="../sui/config.md#sui_config">config</a>.id, name, sobj);
        option::none()
    } <b>else</b> {
        <b>let</b> sobj: &<b>mut</b> <a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value&gt; = field::borrow_mut(&<b>mut</b> <a href="../sui/config.md#sui_config">config</a>.id, name);
        <b>let</b> <a href="../sui/config.md#sui_config_SettingData">SettingData</a> {
            newer_value_epoch,
            newer_value,
            older_value_opt,
        } = sobj.data.extract();
        <b>let</b> (older_value_opt, removed_value) =
            <b>if</b> (epoch &gt; newer_value_epoch) {
                // <b>if</b> the `newer_value` is <b>for</b> a previous epoch, <b>move</b> it to `older_value_opt`
                (<b>move</b> newer_value, <b>move</b> older_value_opt)
            } <b>else</b> {
                // the current epoch cannot be less than the `newer_value_epoch`
                <b>assert</b>!(epoch == newer_value_epoch);
                // <b>if</b> the `newer_value` is <b>for</b> the current epoch, then the option must be `none`
                <b>assert</b>!(newer_value.is_none(), <a href="../sui/config.md#sui_config_EAlreadySetForEpoch">EAlreadySetForEpoch</a>);
                (<b>move</b> older_value_opt, option::none())
            };
        sobj.data.fill(<a href="../sui/config.md#sui_config_SettingData">SettingData</a> {
            newer_value_epoch: epoch,
            newer_value: option::some(value),
            older_value_opt,
        });
        removed_value
    }
}
</code></pre>



</details>

<a name="sui_config_remove_for_next_epoch"></a>

## Function `remove_for_next_epoch`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_remove_for_next_epoch">remove_for_next_epoch</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui/config.md#sui_config">config</a>: &<b>mut</b> <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;WriteCap&gt;, _cap: &<b>mut</b> WriteCap, name: Name, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Value&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_remove_for_next_epoch">remove_for_next_epoch</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui/config.md#sui_config">config</a>: &<b>mut</b> <a href="../sui/config.md#sui_config_Config">Config</a>&lt;WriteCap&gt;,
    _cap: &<b>mut</b> WriteCap,
    name: Name,
    ctx: &<b>mut</b> TxContext,
): Option&lt;Value&gt; {
    <b>let</b> epoch = ctx.epoch();
    <b>if</b> (!field::exists_(&<a href="../sui/config.md#sui_config">config</a>.id, name)) <b>return</b> option::none();
    <b>let</b> sobj: &<b>mut</b> <a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value&gt; = field::borrow_mut(&<b>mut</b> <a href="../sui/config.md#sui_config">config</a>.id, name);
    <b>let</b> <a href="../sui/config.md#sui_config_SettingData">SettingData</a> {
        newer_value_epoch,
        newer_value,
        older_value_opt,
    } = sobj.data.extract();
    <b>let</b> (older_value_opt, removed_value) =
        <b>if</b> (epoch &gt; newer_value_epoch) {
            // <b>if</b> the `newer_value` is <b>for</b> a previous epoch, <b>move</b> it to `older_value_opt`
            (<b>move</b> newer_value, option::none())
        } <b>else</b> {
            // the current epoch cannot be less than the `newer_value_epoch`
            <b>assert</b>!(epoch == newer_value_epoch);
            (<b>move</b> older_value_opt, <b>move</b> newer_value)
        };
    <b>let</b> older_value_opt_is_none = older_value_opt.is_none();
    sobj.data.fill(<a href="../sui/config.md#sui_config_SettingData">SettingData</a> {
        newer_value_epoch: epoch,
        newer_value: option::none(),
        older_value_opt,
    });
    <b>if</b> (older_value_opt_is_none) {
        field::remove&lt;_, <a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value&gt;&gt;(&<b>mut</b> <a href="../sui/config.md#sui_config">config</a>.id, name);
    };
    removed_value
}
</code></pre>



</details>

<a name="sui_config_exists_with_type"></a>

## Function `exists_with_type`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_exists_with_type">exists_with_type</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui/config.md#sui_config">config</a>: &<a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;WriteCap&gt;, name: Name): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_exists_with_type">exists_with_type</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui/config.md#sui_config">config</a>: &<a href="../sui/config.md#sui_config_Config">Config</a>&lt;WriteCap&gt;,
    name: Name,
): bool {
    field::exists_with_type&lt;_, <a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value&gt;&gt;(&<a href="../sui/config.md#sui_config">config</a>.id, name)
}
</code></pre>



</details>

<a name="sui_config_exists_with_type_for_next_epoch"></a>

## Function `exists_with_type_for_next_epoch`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_exists_with_type_for_next_epoch">exists_with_type_for_next_epoch</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui/config.md#sui_config">config</a>: &<a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;WriteCap&gt;, name: Name, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_exists_with_type_for_next_epoch">exists_with_type_for_next_epoch</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui/config.md#sui_config">config</a>: &<a href="../sui/config.md#sui_config_Config">Config</a>&lt;WriteCap&gt;,
    name: Name,
    ctx: &TxContext,
): bool {
    field::exists_with_type&lt;_, <a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value&gt;&gt;(&<a href="../sui/config.md#sui_config">config</a>.id, name) && {
        <b>let</b> epoch = ctx.epoch();
        <b>let</b> sobj: &<a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value&gt; = field::borrow(&<a href="../sui/config.md#sui_config">config</a>.id, name);
        epoch == sobj.data.<a href="../sui/borrow.md#sui_borrow">borrow</a>().newer_value_epoch &&
        sobj.data.<a href="../sui/borrow.md#sui_borrow">borrow</a>().newer_value.is_some()
    }
}
</code></pre>



</details>

<a name="sui_config_borrow_for_next_epoch_mut"></a>

## Function `borrow_for_next_epoch_mut`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_borrow_for_next_epoch_mut">borrow_for_next_epoch_mut</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui/config.md#sui_config">config</a>: &<b>mut</b> <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;WriteCap&gt;, _cap: &<b>mut</b> WriteCap, name: Name, ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): &<b>mut</b> Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_borrow_for_next_epoch_mut">borrow_for_next_epoch_mut</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui/config.md#sui_config">config</a>: &<b>mut</b> <a href="../sui/config.md#sui_config_Config">Config</a>&lt;WriteCap&gt;,
    _cap: &<b>mut</b> WriteCap,
    name: Name,
    ctx: &<b>mut</b> TxContext,
): &<b>mut</b> Value {
    <b>let</b> epoch = ctx.epoch();
    <b>let</b> sobj: &<b>mut</b> <a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value&gt; = field::borrow_mut(&<b>mut</b> <a href="../sui/config.md#sui_config">config</a>.id, name);
    <b>let</b> data = sobj.data.borrow_mut();
    <b>assert</b>!(data.newer_value_epoch == epoch, <a href="../sui/config.md#sui_config_ENotSetForEpoch">ENotSetForEpoch</a>);
    <b>assert</b>!(data.newer_value.is_some(), <a href="../sui/config.md#sui_config_ENotSetForEpoch">ENotSetForEpoch</a>);
    data.newer_value.borrow_mut()
}
</code></pre>



</details>

<a name="sui_config_read_setting_for_next_epoch"></a>

## Function `read_setting_for_next_epoch`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_read_setting_for_next_epoch">read_setting_for_next_epoch</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui/config.md#sui_config">config</a>: &<a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;WriteCap&gt;, name: Name): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Value&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_read_setting_for_next_epoch">read_setting_for_next_epoch</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui/config.md#sui_config">config</a>: &<a href="../sui/config.md#sui_config_Config">Config</a>&lt;WriteCap&gt;,
    name: Name,
): Option&lt;Value&gt; {
    <b>if</b> (!field::exists_with_type&lt;_, <a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value&gt;&gt;(&<a href="../sui/config.md#sui_config">config</a>.id, name)) <b>return</b> option::none();
    <b>let</b> sobj: &<a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value&gt; = field::borrow(&<a href="../sui/config.md#sui_config">config</a>.id, name);
    <b>let</b> data = sobj.data.<a href="../sui/borrow.md#sui_borrow">borrow</a>();
    data.newer_value
}
</code></pre>



</details>

<a name="sui_config_entry"></a>

## Macro function `entry`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>macro</b> <b>fun</b> <b>entry</b>&lt;$WriteCap, $Name: <b>copy</b>, drop, store, $Value: <b>copy</b>, drop, store&gt;($<a href="../sui/config.md#sui_config">config</a>: &<b>mut</b> <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;$WriteCap&gt;, $cap: &<b>mut</b> $WriteCap, $name: $Name, $initial_for_next_epoch: |&<b>mut</b> <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;$WriteCap&gt;, &<b>mut</b> $WriteCap, &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>| -&gt; $Value, $ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): &<b>mut</b> $Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>macro</b> <b>fun</b> <b>entry</b>&lt;
    $WriteCap,
    $Name: <b>copy</b> + drop + store,
    $Value: <b>copy</b> + drop + store,
&gt;(
    $<a href="../sui/config.md#sui_config">config</a>: &<b>mut</b> <a href="../sui/config.md#sui_config_Config">Config</a>&lt;$WriteCap&gt;,
    $cap: &<b>mut</b> $WriteCap,
    $name: $Name,
    $initial_for_next_epoch: |&<b>mut</b> <a href="../sui/config.md#sui_config_Config">Config</a>&lt;$WriteCap&gt;, &<b>mut</b> $WriteCap, &<b>mut</b> TxContext| -&gt; $Value,
    $ctx: &<b>mut</b> TxContext,
): &<b>mut</b> $Value {
    <b>let</b> <a href="../sui/config.md#sui_config">config</a> = $<a href="../sui/config.md#sui_config">config</a>;
    <b>let</b> cap = $cap;
    <b>let</b> name = $name;
    <b>let</b> ctx = $ctx;
    <b>if</b> (!<a href="../sui/config.md#sui_config">config</a>.<a href="../sui/config.md#sui_config_exists_with_type_for_next_epoch">exists_with_type_for_next_epoch</a>&lt;_, _, $Value&gt;(name, ctx)) {
        <b>let</b> initial = $initial_for_next_epoch(<a href="../sui/config.md#sui_config">config</a>, cap, ctx);
        <a href="../sui/config.md#sui_config">config</a>.<a href="../sui/config.md#sui_config_add_for_next_epoch">add_for_next_epoch</a>(cap, name, initial, ctx);
    };
    <a href="../sui/config.md#sui_config">config</a>.<a href="../sui/config.md#sui_config_borrow_for_next_epoch_mut">borrow_for_next_epoch_mut</a>(cap, name, ctx)
}
</code></pre>



</details>

<a name="sui_config_update"></a>

## Macro function `update`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>macro</b> <b>fun</b> <a href="../sui/config.md#sui_config_update">update</a>&lt;$WriteCap, $Name: <b>copy</b>, drop, store, $Value: <b>copy</b>, drop, store&gt;($<a href="../sui/config.md#sui_config">config</a>: &<b>mut</b> <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;$WriteCap&gt;, $cap: &<b>mut</b> $WriteCap, $name: $Name, $initial_for_next_epoch: |&<b>mut</b> <a href="../sui/config.md#sui_config_Config">sui::config::Config</a>&lt;$WriteCap&gt;, &<b>mut</b> $WriteCap, &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>| -&gt; $Value, $update_for_next_epoch: |<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$Value&gt;, &<b>mut</b> $Value| -&gt; (), $ctx: &<b>mut</b> <a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>macro</b> <b>fun</b> <a href="../sui/config.md#sui_config_update">update</a>&lt;
    $WriteCap,
    $Name: <b>copy</b> + drop + store,
    $Value: <b>copy</b> + drop + store,
&gt;(
    $<a href="../sui/config.md#sui_config">config</a>: &<b>mut</b> <a href="../sui/config.md#sui_config_Config">Config</a>&lt;$WriteCap&gt;,
    $cap: &<b>mut</b> $WriteCap,
    $name: $Name,
    $initial_for_next_epoch: |&<b>mut</b> <a href="../sui/config.md#sui_config_Config">Config</a>&lt;$WriteCap&gt;, &<b>mut</b> $WriteCap, &<b>mut</b> TxContext| -&gt; $Value,
    $update_for_next_epoch: |Option&lt;$Value&gt;, &<b>mut</b> $Value|,
    $ctx: &<b>mut</b> TxContext,
) {
    <b>let</b> <a href="../sui/config.md#sui_config">config</a> = $<a href="../sui/config.md#sui_config">config</a>;
    <b>let</b> cap = $cap;
    <b>let</b> name = $name;
    <b>let</b> ctx = $ctx;
    <b>let</b> old_value_opt =
        <b>if</b> (!<a href="../sui/config.md#sui_config">config</a>.<a href="../sui/config.md#sui_config_exists_with_type_for_next_epoch">exists_with_type_for_next_epoch</a>&lt;_, _, $Value&gt;(name, ctx)) {
            <b>let</b> initial = $initial_for_next_epoch(<a href="../sui/config.md#sui_config">config</a>, cap, ctx);
            <a href="../sui/config.md#sui_config">config</a>.<a href="../sui/config.md#sui_config_add_for_next_epoch">add_for_next_epoch</a>(cap, name, initial, ctx)
        } <b>else</b> {
            option::none()
        };
    $update_for_next_epoch(old_value_opt, <a href="../sui/config.md#sui_config">config</a>.<a href="../sui/config.md#sui_config_borrow_for_next_epoch_mut">borrow_for_next_epoch_mut</a>(cap, name, ctx));
}
</code></pre>



</details>

<a name="sui_config_read_setting"></a>

## Function `read_setting`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_read_setting">read_setting</a>&lt;Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui/config.md#sui_config">config</a>: <a href="../sui/object.md#sui_object_ID">sui::object::ID</a>, name: Name, ctx: &<a href="../sui/tx_context.md#sui_tx_context_TxContext">sui::tx_context::TxContext</a>): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Value&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/config.md#sui_config_read_setting">read_setting</a>&lt;Name: <b>copy</b> + drop + store, Value: <b>copy</b> + drop + store&gt;(
    <a href="../sui/config.md#sui_config">config</a>: ID,
    name: Name,
    ctx: &TxContext,
): Option&lt;Value&gt; {
    <b>use</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_Field">sui::dynamic_field::Field</a>;
    <b>let</b> config_id = <a href="../sui/config.md#sui_config">config</a>.to_address();
    <b>let</b> setting_df = field::hash_type_and_key(config_id, name);
    <a href="../sui/config.md#sui_config_read_setting_impl">read_setting_impl</a>&lt;Field&lt;Name, <a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value&gt;&gt;, <a href="../sui/config.md#sui_config_Setting">Setting</a>&lt;Value&gt;, <a href="../sui/config.md#sui_config_SettingData">SettingData</a>&lt;Value&gt;, Value&gt;(
        config_id,
        setting_df,
        ctx.epoch(),
    )
}
</code></pre>



</details>

<a name="sui_config_read_setting_impl"></a>

## Function `read_setting_impl`



<pre><code><b>fun</b> <a href="../sui/config.md#sui_config_read_setting_impl">read_setting_impl</a>&lt;FieldSettingValue: key, SettingValue: store, SettingDataValue: store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui/config.md#sui_config">config</a>: <b>address</b>, name: <b>address</b>, current_epoch: u64): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Value&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui/config.md#sui_config_read_setting_impl">read_setting_impl</a>&lt;
    FieldSettingValue: key,
    SettingValue: store,
    SettingDataValue: store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui/config.md#sui_config">config</a>: <b>address</b>,
    name: <b>address</b>,
    current_epoch: u64,
): Option&lt;Value&gt;;
</code></pre>



</details>
