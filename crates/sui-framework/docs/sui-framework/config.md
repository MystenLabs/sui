---
title: Module `0x2::config`
---



-  [Resource `Config`](#0x2_config_Config)
-  [Struct `Setting`](#0x2_config_Setting)
-  [Struct `SettingData`](#0x2_config_SettingData)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_config_new)
-  [Function `share`](#0x2_config_share)
-  [Function `transfer`](#0x2_config_transfer)
-  [Function `add_for_next_epoch`](#0x2_config_add_for_next_epoch)
-  [Function `remove_for_next_epoch`](#0x2_config_remove_for_next_epoch)
-  [Function `exists_with_type`](#0x2_config_exists_with_type)
-  [Function `exists_with_type_for_next_epoch`](#0x2_config_exists_with_type_for_next_epoch)
-  [Function `borrow_for_next_epoch_mut`](#0x2_config_borrow_for_next_epoch_mut)
-  [Function `read_setting_for_next_epoch`](#0x2_config_read_setting_for_next_epoch)
-  [Function `read_setting`](#0x2_config_read_setting)
-  [Function `read_setting_impl`](#0x2_config_read_setting_impl)


<pre><code><b>use</b> <a href="../move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../sui-framework/dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="../sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../sui-framework/transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="../sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_config_Config"></a>

## Resource `Config`



<pre><code><b>struct</b> <a href="../sui-framework/config.md#0x2_config_Config">Config</a>&lt;WriteCap&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui-framework/object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_config_Setting"></a>

## Struct `Setting`



<pre><code><b>struct</b> <a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value: <b>copy</b>, drop, store&gt; <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>data: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../sui-framework/config.md#0x2_config_SettingData">config::SettingData</a>&lt;Value&gt;&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_config_SettingData"></a>

## Struct `SettingData`



<pre><code><b>struct</b> <a href="../sui-framework/config.md#0x2_config_SettingData">SettingData</a>&lt;Value: <b>copy</b>, drop, store&gt; <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>newer_value_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a></code>
</dt>
<dd>

</dd>
<dt>
<code>newer_value: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Value&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>older_value_opt: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Value&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_config_EBCSSerializationFailure"></a>



<pre><code><b>const</b> <a href="../sui-framework/config.md#0x2_config_EBCSSerializationFailure">EBCSSerializationFailure</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 2;
</code></pre>



<a name="0x2_config_EAlreadySetForEpoch"></a>



<pre><code><b>const</b> <a href="../sui-framework/config.md#0x2_config_EAlreadySetForEpoch">EAlreadySetForEpoch</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 0;
</code></pre>



<a name="0x2_config_ENotSetForEpoch"></a>



<pre><code><b>const</b> <a href="../sui-framework/config.md#0x2_config_ENotSetForEpoch">ENotSetForEpoch</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 1;
</code></pre>



<a name="0x2_config_new"></a>

## Function `new`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_new">new</a>&lt;WriteCap&gt;(_cap: &<b>mut</b> WriteCap, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../sui-framework/config.md#0x2_config_Config">config::Config</a>&lt;WriteCap&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_new">new</a>&lt;WriteCap&gt;(_cap: &<b>mut</b> WriteCap, ctx: &<b>mut</b> TxContext): <a href="../sui-framework/config.md#0x2_config_Config">Config</a>&lt;WriteCap&gt; {
    <a href="../sui-framework/config.md#0x2_config_Config">Config</a>&lt;WriteCap&gt; { id: <a href="../sui-framework/object.md#0x2_object_new">object::new</a>(ctx) }
}
</code></pre>



</details>

<a name="0x2_config_share"></a>

## Function `share`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_share">share</a>&lt;WriteCap&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: <a href="../sui-framework/config.md#0x2_config_Config">config::Config</a>&lt;WriteCap&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_share">share</a>&lt;WriteCap&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: <a href="../sui-framework/config.md#0x2_config_Config">Config</a>&lt;WriteCap&gt;) {
    <a href="../sui-framework/transfer.md#0x2_transfer_share_object">transfer::share_object</a>(<a href="../sui-framework/config.md#0x2_config">config</a>)
}
</code></pre>



</details>

<a name="0x2_config_transfer"></a>

## Function `transfer`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/transfer.md#0x2_transfer">transfer</a>&lt;WriteCap&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: <a href="../sui-framework/config.md#0x2_config_Config">config::Config</a>&lt;WriteCap&gt;, owner: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/transfer.md#0x2_transfer">transfer</a>&lt;WriteCap&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: <a href="../sui-framework/config.md#0x2_config_Config">Config</a>&lt;WriteCap&gt;, owner: <b>address</b>) {
    <a href="../sui-framework/transfer.md#0x2_transfer_transfer">transfer::transfer</a>(<a href="../sui-framework/config.md#0x2_config">config</a>, owner)
}
</code></pre>



</details>

<a name="0x2_config_add_for_next_epoch"></a>

## Function `add_for_next_epoch`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_add_for_next_epoch">add_for_next_epoch</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: &<b>mut</b> <a href="../sui-framework/config.md#0x2_config_Config">config::Config</a>&lt;WriteCap&gt;, _cap: &<b>mut</b> WriteCap, name: Name, value: Value, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Value&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_add_for_next_epoch">add_for_next_epoch</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui-framework/config.md#0x2_config">config</a>: &<b>mut</b> <a href="../sui-framework/config.md#0x2_config_Config">Config</a>&lt;WriteCap&gt;,
    _cap: &<b>mut</b> WriteCap,
    name: Name,
    value: Value,
    ctx: &<b>mut</b> TxContext,
): Option&lt;Value&gt; {
    <b>let</b> epoch = ctx.epoch();
    <b>if</b> (!field::exists_(&<a href="../sui-framework/config.md#0x2_config">config</a>.id, name)) {
        <b>let</b> sobj = <a href="../sui-framework/config.md#0x2_config_Setting">Setting</a> {
            data: <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(<a href="../sui-framework/config.md#0x2_config_SettingData">SettingData</a> {
                newer_value_epoch: epoch,
                newer_value: <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(value),
                older_value_opt: <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>(),
            }),
        };
        field::add(&<b>mut</b> <a href="../sui-framework/config.md#0x2_config">config</a>.id, name, sobj);
        <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>()
    } <b>else</b> {
        <b>let</b> sobj: &<b>mut</b> <a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value&gt; = field::borrow_mut(&<b>mut</b> <a href="../sui-framework/config.md#0x2_config">config</a>.id, name);
        <b>let</b> <a href="../sui-framework/config.md#0x2_config_SettingData">SettingData</a> {
            newer_value_epoch,
            newer_value,
            older_value_opt,
        } = sobj.data.extract();
        <b>let</b> (older_value_opt, removed_value) =
            <b>if</b> (epoch &gt; newer_value_epoch) {
                // <b>if</b> the `newer_value` is for a previous epoch, <b>move</b> it <b>to</b> `older_value_opt`
                (<b>move</b> newer_value, <b>move</b> older_value_opt)
            } <b>else</b> {
                // the current epoch cannot be less than the `newer_value_epoch`
                <b>assert</b>!(epoch == newer_value_epoch);
                // <b>if</b> the `newer_value` is for the current epoch, then the <a href="../move-stdlib/option.md#0x1_option">option</a> must be `none`
                <b>assert</b>!(newer_value.is_none(), <a href="../sui-framework/config.md#0x2_config_EAlreadySetForEpoch">EAlreadySetForEpoch</a>);
                (<b>move</b> older_value_opt, <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>())
            };
        sobj.data.fill(<a href="../sui-framework/config.md#0x2_config_SettingData">SettingData</a> {
            newer_value_epoch: epoch,
            newer_value: <a href="../move-stdlib/option.md#0x1_option_some">option::some</a>(value),
            older_value_opt,
        });
        removed_value
    }
}
</code></pre>



</details>

<a name="0x2_config_remove_for_next_epoch"></a>

## Function `remove_for_next_epoch`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_remove_for_next_epoch">remove_for_next_epoch</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: &<b>mut</b> <a href="../sui-framework/config.md#0x2_config_Config">config::Config</a>&lt;WriteCap&gt;, _cap: &<b>mut</b> WriteCap, name: Name, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Value&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_remove_for_next_epoch">remove_for_next_epoch</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui-framework/config.md#0x2_config">config</a>: &<b>mut</b> <a href="../sui-framework/config.md#0x2_config_Config">Config</a>&lt;WriteCap&gt;,
    _cap: &<b>mut</b> WriteCap,
    name: Name,
    ctx: &<b>mut</b> TxContext,
): Option&lt;Value&gt; {
    <b>let</b> epoch = ctx.epoch();
    <b>if</b> (!field::exists_(&<a href="../sui-framework/config.md#0x2_config">config</a>.id, name)) <b>return</b> <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>();
    <b>let</b> sobj: &<b>mut</b> <a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value&gt; = field::borrow_mut(&<b>mut</b> <a href="../sui-framework/config.md#0x2_config">config</a>.id, name);
    <b>let</b> <a href="../sui-framework/config.md#0x2_config_SettingData">SettingData</a> {
        newer_value_epoch,
        newer_value,
        older_value_opt,
    } = sobj.data.extract();
    <b>let</b> (older_value_opt, removed_value) =
        <b>if</b> (epoch &gt; newer_value_epoch) {
            // <b>if</b> the `newer_value` is for a previous epoch, <b>move</b> it <b>to</b> `older_value_opt`
            (<b>move</b> newer_value, <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>())
        } <b>else</b> {
            // the current epoch cannot be less than the `newer_value_epoch`
            <b>assert</b>!(epoch == newer_value_epoch);
            (<b>move</b> older_value_opt, <b>move</b> newer_value)
        };
    <b>let</b> older_value_opt_is_none = older_value_opt.is_none();
    sobj.data.fill(<a href="../sui-framework/config.md#0x2_config_SettingData">SettingData</a> {
        newer_value_epoch: epoch,
        newer_value: <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>(),
        older_value_opt,
    });
    <b>if</b> (older_value_opt_is_none) {
        field::remove&lt;_, <a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value&gt;&gt;(&<b>mut</b> <a href="../sui-framework/config.md#0x2_config">config</a>.id, name);
    };
    removed_value
}
</code></pre>



</details>

<a name="0x2_config_exists_with_type"></a>

## Function `exists_with_type`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_exists_with_type">exists_with_type</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: &<a href="../sui-framework/config.md#0x2_config_Config">config::Config</a>&lt;WriteCap&gt;, name: Name): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_exists_with_type">exists_with_type</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui-framework/config.md#0x2_config">config</a>: &<a href="../sui-framework/config.md#0x2_config_Config">Config</a>&lt;WriteCap&gt;,
    name: Name,
): bool {
    field::exists_with_type&lt;_, <a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value&gt;&gt;(&<a href="../sui-framework/config.md#0x2_config">config</a>.id, name)
}
</code></pre>



</details>

<a name="0x2_config_exists_with_type_for_next_epoch"></a>

## Function `exists_with_type_for_next_epoch`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_exists_with_type_for_next_epoch">exists_with_type_for_next_epoch</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: &<a href="../sui-framework/config.md#0x2_config_Config">config::Config</a>&lt;WriteCap&gt;, name: Name, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_exists_with_type_for_next_epoch">exists_with_type_for_next_epoch</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui-framework/config.md#0x2_config">config</a>: & <a href="../sui-framework/config.md#0x2_config_Config">Config</a>&lt;WriteCap&gt;,
    name: Name,
    ctx: &TxContext,
): bool {
    field::exists_with_type&lt;_, <a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value&gt;&gt;(&<a href="../sui-framework/config.md#0x2_config">config</a>.id, name) && {
        <b>let</b> epoch = ctx.epoch();
        <b>let</b> sobj: &<a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value&gt; = field::borrow(&<a href="../sui-framework/config.md#0x2_config">config</a>.id, name);
        epoch == sobj.data.borrow().newer_value_epoch &&
        sobj.data.borrow().newer_value.is_some()
    }
}
</code></pre>



</details>

<a name="0x2_config_borrow_for_next_epoch_mut"></a>

## Function `borrow_for_next_epoch_mut`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_borrow_for_next_epoch_mut">borrow_for_next_epoch_mut</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: &<b>mut</b> <a href="../sui-framework/config.md#0x2_config_Config">config::Config</a>&lt;WriteCap&gt;, _cap: &<b>mut</b> WriteCap, name: Name, ctx: &<b>mut</b> <a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): &<b>mut</b> Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_borrow_for_next_epoch_mut">borrow_for_next_epoch_mut</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui-framework/config.md#0x2_config">config</a>: &<b>mut</b> <a href="../sui-framework/config.md#0x2_config_Config">Config</a>&lt;WriteCap&gt;,
    _cap: &<b>mut</b> WriteCap,
    name: Name,
    ctx: &<b>mut</b> TxContext,
): &<b>mut</b> Value {
    <b>let</b> epoch = ctx.epoch();
    <b>let</b> sobj: &<b>mut</b> <a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value&gt; = field::borrow_mut(&<b>mut</b> <a href="../sui-framework/config.md#0x2_config">config</a>.id, name);
    <b>let</b> data = sobj.data.borrow_mut();
    <b>assert</b>!(data.newer_value_epoch == epoch, <a href="../sui-framework/config.md#0x2_config_ENotSetForEpoch">ENotSetForEpoch</a>);
    <b>assert</b>!(data.newer_value.is_some(), <a href="../sui-framework/config.md#0x2_config_ENotSetForEpoch">ENotSetForEpoch</a>);
    data.newer_value.borrow_mut()
}
</code></pre>



</details>

<a name="0x2_config_read_setting_for_next_epoch"></a>

## Function `read_setting_for_next_epoch`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_read_setting_for_next_epoch">read_setting_for_next_epoch</a>&lt;WriteCap, Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: &<a href="../sui-framework/config.md#0x2_config_Config">config::Config</a>&lt;WriteCap&gt;, name: Name): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Value&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_read_setting_for_next_epoch">read_setting_for_next_epoch</a>&lt;
    WriteCap,
    Name: <b>copy</b> + drop + store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui-framework/config.md#0x2_config">config</a>: &<a href="../sui-framework/config.md#0x2_config_Config">Config</a>&lt;WriteCap&gt;,
    name: Name,
): Option&lt;Value&gt; {
    <b>if</b> (!field::exists_with_type&lt;_, <a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value&gt;&gt;(&<a href="../sui-framework/config.md#0x2_config">config</a>.id, name)) <b>return</b> <a href="../move-stdlib/option.md#0x1_option_none">option::none</a>();
    <b>let</b> sobj: &<a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value&gt; = field::borrow(&<a href="../sui-framework/config.md#0x2_config">config</a>.id, name);
    <b>let</b> data = sobj.data.borrow();
    data.newer_value
}
</code></pre>



</details>

<a name="0x2_config_read_setting"></a>

## Function `read_setting`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_read_setting">read_setting</a>&lt;Name: <b>copy</b>, drop, store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: <a href="../sui-framework/object.md#0x2_object_ID">object::ID</a>, name: Name, ctx: &<a href="../sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Value&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(package) <b>fun</b> <a href="../sui-framework/config.md#0x2_config_read_setting">read_setting</a>&lt;Name: <b>copy</b> + drop + store, Value: <b>copy</b> + drop + store&gt;(
    <a href="../sui-framework/config.md#0x2_config">config</a>: ID,
    name: Name,
    ctx: &TxContext,
): Option&lt;Value&gt; {
    <b>use</b> sui::dynamic_field::Field;
    <b>let</b> config_id = <a href="../sui-framework/config.md#0x2_config">config</a>.to_address();
    <b>let</b> setting_df = field::hash_type_and_key(config_id, name);
    <a href="../sui-framework/config.md#0x2_config_read_setting_impl">read_setting_impl</a>&lt;Field&lt;Name, <a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value&gt;&gt;, <a href="../sui-framework/config.md#0x2_config_Setting">Setting</a>&lt;Value&gt;, <a href="../sui-framework/config.md#0x2_config_SettingData">SettingData</a>&lt;Value&gt;, Value&gt;(
        config_id,
        setting_df,
        ctx.epoch(),
    )
}
</code></pre>



</details>

<a name="0x2_config_read_setting_impl"></a>

## Function `read_setting_impl`



<pre><code><b>fun</b> <a href="../sui-framework/config.md#0x2_config_read_setting_impl">read_setting_impl</a>&lt;FieldSettingValue: key, SettingValue: store, SettingDataValue: store, Value: <b>copy</b>, drop, store&gt;(<a href="../sui-framework/config.md#0x2_config">config</a>: <b>address</b>, name: <b>address</b>, current_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Value&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="../sui-framework/config.md#0x2_config_read_setting_impl">read_setting_impl</a>&lt;
    FieldSettingValue: key,
    SettingValue: store,
    SettingDataValue: store,
    Value: <b>copy</b> + drop + store,
&gt;(
    <a href="../sui-framework/config.md#0x2_config">config</a>: <b>address</b>,
    name: <b>address</b>,
    current_epoch: <a href="../move-stdlib/u64.md#0x1_u64">u64</a>,
): Option&lt;Value&gt;;
</code></pre>



</details>
