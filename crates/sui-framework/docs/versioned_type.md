
<a name="0x2_versioned_type"></a>

# Module `0x2::versioned_type`



-  [Resource `Versioned`](#0x2_versioned_type_Versioned)
-  [Struct `VersionChangeCap`](#0x2_versioned_type_VersionChangeCap)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x2_versioned_type_create)
-  [Function `version`](#0x2_versioned_type_version)
-  [Function `load_value`](#0x2_versioned_type_load_value)
-  [Function `load_value_mut`](#0x2_versioned_type_load_value_mut)
-  [Function `remove_value`](#0x2_versioned_type_remove_value)
-  [Function `add_value`](#0x2_versioned_type_add_value)


<pre><code><b>use</b> <a href="dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_versioned_type_Versioned"></a>

## Resource `Versioned`

A wrapper type that supports versioning of the inner type.
The inner type is a dynamic field of the Versioned object, and is keyed using version.


<pre><code><b>struct</b> <a href="versioned_type.md#0x2_versioned_type_Versioned">Versioned</a> <b>has</b> store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
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

<a name="0x2_versioned_type_VersionChangeCap"></a>

## Struct `VersionChangeCap`

Represents a hot potato object generated when we take out the dynamic field.
This is to make sure that we always put a new value back.


<pre><code><b>struct</b> <a href="versioned_type.md#0x2_versioned_type_VersionChangeCap">VersionChangeCap</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>versioned_id: <a href="object.md#0x2_object_ID">object::ID</a></code>
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


<a name="0x2_versioned_type_EInvalidUpgrade"></a>



<pre><code><b>const</b> <a href="versioned_type.md#0x2_versioned_type_EInvalidUpgrade">EInvalidUpgrade</a>: u64 = 0;
</code></pre>



<a name="0x2_versioned_type_create"></a>

## Function `create`



<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_create">create</a>&lt;T: store&gt;(init_version: u64, init_value: T, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="versioned_type.md#0x2_versioned_type_Versioned">versioned_type::Versioned</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_create">create</a>&lt;T: store&gt;(init_version: u64, init_value: T, ctx: &<b>mut</b> TxContext): <a href="versioned_type.md#0x2_versioned_type_Versioned">Versioned</a> {
    <b>let</b> self = <a href="versioned_type.md#0x2_versioned_type_Versioned">Versioned</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        version: init_version,
    };
    <a href="dynamic_field.md#0x2_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> self.id, init_version, init_value);
    self
}
</code></pre>



</details>

<a name="0x2_versioned_type_version"></a>

## Function `version`



<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_version">version</a>(self: &<a href="versioned_type.md#0x2_versioned_type_Versioned">versioned_type::Versioned</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_version">version</a>(self: &<a href="versioned_type.md#0x2_versioned_type_Versioned">Versioned</a>): u64 {
    self.version
}
</code></pre>



</details>

<a name="0x2_versioned_type_load_value"></a>

## Function `load_value`



<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_load_value">load_value</a>&lt;T: store&gt;(self: &<a href="versioned_type.md#0x2_versioned_type_Versioned">versioned_type::Versioned</a>): &T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_load_value">load_value</a>&lt;T: store&gt;(self: &<a href="versioned_type.md#0x2_versioned_type_Versioned">Versioned</a>): &T {
    <a href="dynamic_field.md#0x2_dynamic_field_borrow">dynamic_field::borrow</a>(&self.id, self.version)
}
</code></pre>



</details>

<a name="0x2_versioned_type_load_value_mut"></a>

## Function `load_value_mut`



<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_load_value_mut">load_value_mut</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="versioned_type.md#0x2_versioned_type_Versioned">versioned_type::Versioned</a>): &<b>mut</b> T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_load_value_mut">load_value_mut</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="versioned_type.md#0x2_versioned_type_Versioned">Versioned</a>): &<b>mut</b> T {
    <a href="dynamic_field.md#0x2_dynamic_field_borrow_mut">dynamic_field::borrow_mut</a>(&<b>mut</b> self.id, self.version)
}
</code></pre>



</details>

<a name="0x2_versioned_type_remove_value"></a>

## Function `remove_value`



<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_remove_value">remove_value</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="versioned_type.md#0x2_versioned_type_Versioned">versioned_type::Versioned</a>): (T, <a href="versioned_type.md#0x2_versioned_type_VersionChangeCap">versioned_type::VersionChangeCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_remove_value">remove_value</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="versioned_type.md#0x2_versioned_type_Versioned">Versioned</a>): (T, <a href="versioned_type.md#0x2_versioned_type_VersionChangeCap">VersionChangeCap</a>) {
    (
        <a href="dynamic_field.md#0x2_dynamic_field_remove">dynamic_field::remove</a>(&<b>mut</b> self.id, self.version),
        <a href="versioned_type.md#0x2_versioned_type_VersionChangeCap">VersionChangeCap</a> {
            versioned_id: <a href="object.md#0x2_object_id">object::id</a>(self),
            old_version: self.version,
        }
    )
}
</code></pre>



</details>

<a name="0x2_versioned_type_add_value"></a>

## Function `add_value`



<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_add_value">add_value</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="versioned_type.md#0x2_versioned_type_Versioned">versioned_type::Versioned</a>, new_version: u64, new_value: T, cap: <a href="versioned_type.md#0x2_versioned_type_VersionChangeCap">versioned_type::VersionChangeCap</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="versioned_type.md#0x2_versioned_type_add_value">add_value</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="versioned_type.md#0x2_versioned_type_Versioned">Versioned</a>, new_version: u64, new_value: T, cap: <a href="versioned_type.md#0x2_versioned_type_VersionChangeCap">VersionChangeCap</a>) {
    <b>let</b> <a href="versioned_type.md#0x2_versioned_type_VersionChangeCap">VersionChangeCap</a> { versioned_id, old_version } = cap;
    <b>assert</b>!(versioned_id == <a href="object.md#0x2_object_id">object::id</a>(self), <a href="versioned_type.md#0x2_versioned_type_EInvalidUpgrade">EInvalidUpgrade</a>);
    <b>assert</b>!(old_version != new_version, <a href="versioned_type.md#0x2_versioned_type_EInvalidUpgrade">EInvalidUpgrade</a>);
    <a href="dynamic_field.md#0x2_dynamic_field_add">dynamic_field::add</a>(&<b>mut</b> self.id, new_version, new_value);
}
</code></pre>



</details>
