
<a name="0x2_dynamic_object_field"></a>

# Module `0x2::dynamic_object_field`



-  [Struct `Wrapper`](#0x2_dynamic_object_field_Wrapper)
-  [Function `add`](#0x2_dynamic_object_field_add)
-  [Function `borrow`](#0x2_dynamic_object_field_borrow)
-  [Function `borrow_mut`](#0x2_dynamic_object_field_borrow_mut)
-  [Function `remove`](#0x2_dynamic_object_field_remove)
-  [Function `exists_`](#0x2_dynamic_object_field_exists_)
-  [Function `exists_with_type`](#0x2_dynamic_object_field_exists_with_type)
-  [Function `id`](#0x2_dynamic_object_field_id)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option">0x1::option</a>;
<b>use</b> <a href="../../dependencies/sui-framework/dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="../../dependencies/sui-framework/object.md#0x2_object">0x2::object</a>;
</code></pre>



<a name="0x2_dynamic_object_field_Wrapper"></a>

## Struct `Wrapper`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>name: Name</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_dynamic_object_field_add"></a>

## Function `add`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_add">add</a>&lt;Name: <b>copy</b>, drop, store, Value: store, key&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>, name: Name, value: Value)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_add">add</a>&lt;Name: <b>copy</b> + drop + store, Value: key + store&gt;(
    // we <b>use</b> &<b>mut</b> UID in several spots for access control
    <a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &<b>mut</b> UID,
    name: Name,
    value: Value,
) {
    <b>let</b> key = <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>let</b> id = <a href="../../dependencies/sui-framework/object.md#0x2_object_id">object::id</a>(&value);
    field::add(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>, key, id);
    <b>let</b> (field, _) = field::field_info&lt;<a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>, key);
    add_child_object(<a href="../../dependencies/sui-framework/object.md#0x2_object_uid_to_address">object::uid_to_address</a>(field), value);
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_borrow"></a>

## Function `borrow`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_borrow">borrow</a>&lt;Name: <b>copy</b>, drop, store, Value: store, key&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>, name: Name): &Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_borrow">borrow</a>&lt;Name: <b>copy</b> + drop + store, Value: key + store&gt;(
    <a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &UID,
    name: Name,
): &Value {
    <b>let</b> key = <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>let</b> (field, value_id) = field::field_info&lt;<a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>, key);
    borrow_child_object&lt;Value&gt;(field, value_id)
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_borrow_mut"></a>

## Function `borrow_mut`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_borrow_mut">borrow_mut</a>&lt;Name: <b>copy</b>, drop, store, Value: store, key&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>, name: Name): &<b>mut</b> Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_borrow_mut">borrow_mut</a>&lt;Name: <b>copy</b> + drop + store, Value: key + store&gt;(
    <a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &<b>mut</b> UID,
    name: Name,
): &<b>mut</b> Value {
    <b>let</b> key = <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>let</b> (field, value_id) = field::field_info_mut&lt;<a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>, key);
    borrow_child_object_mut&lt;Value&gt;(field, value_id)
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_remove"></a>

## Function `remove`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_remove">remove</a>&lt;Name: <b>copy</b>, drop, store, Value: store, key&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &<b>mut</b> <a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>, name: Name): Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_remove">remove</a>&lt;Name: <b>copy</b> + drop + store, Value: key + store&gt;(
    <a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &<b>mut</b> UID,
    name: Name,
): Value {
    <b>let</b> key = <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>let</b> (field, value_id) = field::field_info&lt;<a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>, key);
    <b>let</b> value = remove_child_object&lt;Value&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object_uid_to_address">object::uid_to_address</a>(field), value_id);
    field::remove&lt;<a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;, ID&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>, key);
    value
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_exists_"></a>

## Function `exists_`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_exists_">exists_</a>&lt;Name: <b>copy</b>, drop, store&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>, name: Name): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_exists_">exists_</a>&lt;Name: <b>copy</b> + drop + store&gt;(
    <a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &UID,
    name: Name,
): bool {
    <b>let</b> key = <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    field::exists_with_type&lt;<a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;, ID&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>, key)
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_exists_with_type"></a>

## Function `exists_with_type`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_exists_with_type">exists_with_type</a>&lt;Name: <b>copy</b>, drop, store, Value: store, key&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>, name: Name): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_exists_with_type">exists_with_type</a>&lt;Name: <b>copy</b> + drop + store, Value: key + store&gt;(
    <a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &UID,
    name: Name,
): bool {
    <b>let</b> key = <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>if</b> (!field::exists_with_type&lt;<a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;, ID&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>, key)) <b>return</b> <b>false</b>;
    <b>let</b> (field, value_id) = field::field_info&lt;<a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>, key);
    field::has_child_object_with_ty&lt;Value&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object_uid_to_address">object::uid_to_address</a>(field), value_id)
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_id"></a>

## Function `id`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_id">id</a>&lt;Name: <b>copy</b>, drop, store&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &<a href="../../dependencies/sui-framework/object.md#0x2_object_UID">object::UID</a>, name: Name): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;<a href="../../dependencies/sui-framework/object.md#0x2_object_ID">object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_id">id</a>&lt;Name: <b>copy</b> + drop + store&gt;(
    <a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>: &UID,
    name: Name,
): Option&lt;ID&gt; {
    <b>let</b> key = <a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>if</b> (!field::exists_with_type&lt;<a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;, ID&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>, key)) <b>return</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">option::none</a>();
    <b>let</b> (_field, value_id) = field::field_info&lt;<a href="../../dependencies/sui-framework/dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="../../dependencies/sui-framework/object.md#0x2_object">object</a>, key);
    <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">option::some</a>(<a href="../../dependencies/sui-framework/object.md#0x2_object_id_from_address">object::id_from_address</a>(value_id))
}
</code></pre>



</details>
