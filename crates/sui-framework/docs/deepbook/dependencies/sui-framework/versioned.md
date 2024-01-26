
<a name="0x2_versioned"></a>

# Module `0x2::versioned`



-  [Resource `Versioned`](#0x2_versioned_Versioned)
-  [Struct `VersionChangeCap`](#0x2_versioned_VersionChangeCap)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x2_versioned_create)
-  [Function `version`](#0x2_versioned_version)
-  [Function `load_value`](#0x2_versioned_load_value)
-  [Function `load_value_mut`](#0x2_versioned_load_value_mut)
-  [Function `remove_value_for_upgrade`](#0x2_versioned_remove_value_for_upgrade)
-  [Function `upgrade`](#0x2_versioned_upgrade)
-  [Function `destroy`](#0x2_versioned_destroy)


<pre><code><b>use</b> <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="../../dependencies/sui-framework/object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_versioned_Versioned"></a>

## Resource `Versioned`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">Versioned</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a></code>
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

<a name="0x2_versioned_VersionChangeCap"></a>

## Struct `VersionChangeCap`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_VersionChangeCap">VersionChangeCap</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>versioned_id: <a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>old_version: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_versioned_EInvalidUpgrade"></a>



<pre><code><b>const</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_EInvalidUpgrade">EInvalidUpgrade</a>: u64 = 0;
</code></pre>



<a name="0x2_versioned_create"></a>

## Function `create`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_create">create</a>&lt;T: store&gt;(init_version: u64, init_value: T, ctx: &<b>mut</b> <a href="../../dependencies/sui-framework/tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">versioned::Versioned</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_create">create</a>&lt;T: store&gt;(init_version: u64, init_value: T, ctx: &<b>mut</b> TxContext): <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">Versioned</a> {
    <b>let</b> self = <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">Versioned</a> {
        id: <a href="../../dependencies/sui-framework/object.md#0x2_object_new">object::new</a>(ctx),
        version: init_version,
    };
    <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> self.id, init_version, init_value);
    self
}
</code></pre>



</details>

<a name="0x2_versioned_version"></a>

## Function `version`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_version">version</a>(self: &<a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">versioned::Versioned</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_version">version</a>(self: &<a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">Versioned</a>): u64 {
    self.version
}
</code></pre>



</details>

<a name="0x2_versioned_load_value"></a>

## Function `load_value`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_load_value">load_value</a>&lt;T: store&gt;(self: &<a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">versioned::Versioned</a>): &T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_load_value">load_value</a>&lt;T: store&gt;(self: &<a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">Versioned</a>): &T {
    <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field_borrow">dynamic_field::borrow</a>(&self.id, self.version)
}
</code></pre>



</details>

<a name="0x2_versioned_load_value_mut"></a>

## Function `load_value_mut`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_load_value_mut">load_value_mut</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">versioned::Versioned</a>): &<b>mut</b> T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_load_value_mut">load_value_mut</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">Versioned</a>): &<b>mut</b> T {
    <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>(&<b>mut</b> self.id, self.version)
}
</code></pre>



</details>

<a name="0x2_versioned_remove_value_for_upgrade"></a>

## Function `remove_value_for_upgrade`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_remove_value_for_upgrade">remove_value_for_upgrade</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">versioned::Versioned</a>): (T, <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_VersionChangeCap">versioned::VersionChangeCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_remove_value_for_upgrade">remove_value_for_upgrade</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">Versioned</a>): (T, <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_VersionChangeCap">VersionChangeCap</a>) {
    (
        <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field_remove">dynamic_field::remove</a>(&<b>mut</b> self.id, self.version),
        <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_VersionChangeCap">VersionChangeCap</a> {
            versioned_id: <a href="../../dependencies/sui-framework/object.md#0x2_object_id">object::id</a>(self),
            old_version: self.version,
        }
    )
}
</code></pre>



</details>

<a name="0x2_versioned_upgrade"></a>

## Function `upgrade`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_upgrade">upgrade</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">versioned::Versioned</a>, new_version: u64, new_value: T, cap: <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_VersionChangeCap">versioned::VersionChangeCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_upgrade">upgrade</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">Versioned</a>, new_version: u64, new_value: T, cap: <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_VersionChangeCap">VersionChangeCap</a>) {
    <b>let</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_VersionChangeCap">VersionChangeCap</a> { versioned_id, old_version } = cap;
    <b>assert</b>!(versioned_id == <a href="../../dependencies/sui-framework/object.md#0x2_object_id">object::id</a>(self), <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_EInvalidUpgrade">EInvalidUpgrade</a>);
    <b>assert</b>!(old_version &lt; new_version, <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_EInvalidUpgrade">EInvalidUpgrade</a>);
    <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> self.id, new_version, new_value);
    self.version = new_version;
}
</code></pre>



</details>

<a name="0x2_versioned_destroy"></a>

## Function `destroy`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_destroy">destroy</a>&lt;T: store&gt;(self: <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">versioned::Versioned</a>): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_destroy">destroy</a>&lt;T: store&gt;(self: <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">Versioned</a>): T {
    <b>let</b> <a href="../../dependencies/sui-framework/versioned.md#0x2_versioned_Versioned">Versioned</a> { id, version } = self;
    <b>let</b> ret = <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field_remove">dynamic_field::remove</a>(&<b>mut</b> id, version);
    <a href="../../dependencies/sui-framework/object.md#0x2_object_delete">object::delete</a>(id);
    ret
}
</code></pre>



</details>
