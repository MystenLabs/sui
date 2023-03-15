
<a name="0x2_dynamic_object_field"></a>

# Module `0x2::dynamic_object_field`

Similar to <code>sui::dynamic_field</code>, this module allows for the access of dynamic fields. But
unlike, <code>sui::dynamic_field</code> the values bound to these dynamic fields _must_ be objects
themselves. This allows for the objects to still exist within in storage, which may be important
for external tools. The difference is otherwise not observable from within Move.


-  [Struct `Wrapper`](#0x2_dynamic_object_field_Wrapper)
-  [Function `add`](#0x2_dynamic_object_field_add)
-  [Function `borrow`](#0x2_dynamic_object_field_borrow)
-  [Function `borrow_mut`](#0x2_dynamic_object_field_borrow_mut)
-  [Function `remove`](#0x2_dynamic_object_field_remove)
-  [Function `exists_`](#0x2_dynamic_object_field_exists_)
-  [Function `exists_with_type`](#0x2_dynamic_object_field_exists_with_type)
-  [Function `id`](#0x2_dynamic_object_field_id)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="dynamic_field.md#0x2_dynamic_field">0x2::dynamic_field</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
</code></pre>



<a name="0x2_dynamic_object_field_Wrapper"></a>

## Struct `Wrapper`



<pre><code><b>struct</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt; <b>has</b> <b>copy</b>, drop, store
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

Adds a dynamic object field to the object <code><a href="object.md#0x2_object">object</a>: &<b>mut</b> UID</code> at field specified by <code>name: Name</code>.
Aborts with <code>EFieldAlreadyExists</code> if the object already has that field with that name.


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_add">add</a>&lt;Name: <b>copy</b>, drop, store, Value: store, key&gt;(<a href="object.md#0x2_object">object</a>: &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>, name: Name, value: Value)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_add">add</a>&lt;Name: <b>copy</b> + drop + store, Value: key + store&gt;(
    // we <b>use</b> &<b>mut</b> UID in several spots for access control
    <a href="object.md#0x2_object">object</a>: &<b>mut</b> UID,
    name: Name,
    value: Value,
) {
    <b>let</b> key = <a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>let</b> id = <a href="object.md#0x2_object_id">object::id</a>(&value);
    field::add(<a href="object.md#0x2_object">object</a>, key, id);
    <b>let</b> (field, _) = field::field_info&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="object.md#0x2_object">object</a>, key);
    add_child_object(<a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(field), value);
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_borrow"></a>

## Function `borrow`

Immutably borrows the <code><a href="object.md#0x2_object">object</a></code>s dynamic object field with the name specified by <code>name: Name</code>.
Aborts with <code>EFieldDoesNotExist</code> if the object does not have a field with that name.
Aborts with <code>EFieldTypeMismatch</code> if the field exists, but the value object does not have the
specified type.


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_borrow">borrow</a>&lt;Name: <b>copy</b>, drop, store, Value: store, key&gt;(<a href="object.md#0x2_object">object</a>: &<a href="object.md#0x2_object_UID">object::UID</a>, name: Name): &Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_borrow">borrow</a>&lt;Name: <b>copy</b> + drop + store, Value: key + store&gt;(
    <a href="object.md#0x2_object">object</a>: &UID,
    name: Name,
): &Value {
    <b>let</b> key = <a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>let</b> (field, value_id) = field::field_info&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="object.md#0x2_object">object</a>, key);
    borrow_child_object&lt;Value&gt;(field, value_id)
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_borrow_mut"></a>

## Function `borrow_mut`

Mutably borrows the <code><a href="object.md#0x2_object">object</a></code>s dynamic object field with the name specified by <code>name: Name</code>.
Aborts with <code>EFieldDoesNotExist</code> if the object does not have a field with that name.
Aborts with <code>EFieldTypeMismatch</code> if the field exists, but the value object does not have the
specified type.


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_borrow_mut">borrow_mut</a>&lt;Name: <b>copy</b>, drop, store, Value: store, key&gt;(<a href="object.md#0x2_object">object</a>: &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>, name: Name): &<b>mut</b> Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_borrow_mut">borrow_mut</a>&lt;Name: <b>copy</b> + drop + store, Value: key + store&gt;(
    <a href="object.md#0x2_object">object</a>: &<b>mut</b> UID,
    name: Name,
): &<b>mut</b> Value {
    <b>let</b> key = <a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>let</b> (field, value_id) = field::field_info_mut&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="object.md#0x2_object">object</a>, key);
    borrow_child_object_mut&lt;Value&gt;(field, value_id)
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_remove"></a>

## Function `remove`

Removes the <code><a href="object.md#0x2_object">object</a></code>s dynamic object field with the name specified by <code>name: Name</code> and returns
the bound object.
Aborts with <code>EFieldDoesNotExist</code> if the object does not have a field with that name.
Aborts with <code>EFieldTypeMismatch</code> if the field exists, but the value object does not have the
specified type.


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_remove">remove</a>&lt;Name: <b>copy</b>, drop, store, Value: store, key&gt;(<a href="object.md#0x2_object">object</a>: &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>, name: Name): Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_remove">remove</a>&lt;Name: <b>copy</b> + drop + store, Value: key + store&gt;(
    <a href="object.md#0x2_object">object</a>: &<b>mut</b> UID,
    name: Name,
): Value {
    <b>let</b> key = <a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>let</b> (field, value_id) = field::field_info&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="object.md#0x2_object">object</a>, key);
    <b>let</b> value = remove_child_object&lt;Value&gt;(<a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(field), value_id);
    field::remove&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;, ID&gt;(<a href="object.md#0x2_object">object</a>, key);
    value
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_exists_"></a>

## Function `exists_`

Returns true if and only if the <code><a href="object.md#0x2_object">object</a></code> has a dynamic object field with the name specified by
<code>name: Name</code>.


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_exists_">exists_</a>&lt;Name: <b>copy</b>, drop, store&gt;(<a href="object.md#0x2_object">object</a>: &<a href="object.md#0x2_object_UID">object::UID</a>, name: Name): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_exists_">exists_</a>&lt;Name: <b>copy</b> + drop + store&gt;(
    <a href="object.md#0x2_object">object</a>: &UID,
    name: Name,
): bool {
    <b>let</b> key = <a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    field::exists_with_type&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;, ID&gt;(<a href="object.md#0x2_object">object</a>, key)
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_exists_with_type"></a>

## Function `exists_with_type`

Returns true if and only if the <code><a href="object.md#0x2_object">object</a></code> has a dynamic field with the name specified by
<code>name: Name</code> with an assigned value of type <code>Value</code>.


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_exists_with_type">exists_with_type</a>&lt;Name: <b>copy</b>, drop, store, Value: store, key&gt;(<a href="object.md#0x2_object">object</a>: &<a href="object.md#0x2_object_UID">object::UID</a>, name: Name): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_exists_with_type">exists_with_type</a>&lt;Name: <b>copy</b> + drop + store, Value: key + store&gt;(
    <a href="object.md#0x2_object">object</a>: &UID,
    name: Name,
): bool {
    <b>let</b> key = <a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>if</b> (!field::exists_with_type&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;, ID&gt;(<a href="object.md#0x2_object">object</a>, key)) <b>return</b> <b>false</b>;
    <b>let</b> (field, value_id) = field::field_info&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="object.md#0x2_object">object</a>, key);
    field::has_child_object_with_ty&lt;Value&gt;(<a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(field), value_id)
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_id"></a>

## Function `id`

Returns the ID of the object associated with the dynamic object field
Returns none otherwise


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_id">id</a>&lt;Name: <b>copy</b>, drop, store&gt;(<a href="object.md#0x2_object">object</a>: &<a href="object.md#0x2_object_UID">object::UID</a>, name: Name): <a href="_Option">option::Option</a>&lt;<a href="object.md#0x2_object_ID">object::ID</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_id">id</a>&lt;Name: <b>copy</b> + drop + store&gt;(
    <a href="object.md#0x2_object">object</a>: &UID,
    name: Name,
): Option&lt;ID&gt; {
    <b>let</b> key = <a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a> { name };
    <b>if</b> (!field::exists_with_type&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;, ID&gt;(<a href="object.md#0x2_object">object</a>, key)) <b>return</b> <a href="_none">option::none</a>();
    <b>let</b> (_field, value_id) = field::field_info&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="object.md#0x2_object">object</a>, key);
    <a href="_some">option::some</a>(<a href="object.md#0x2_object_id_from_address">object::id_from_address</a>(value_id))
}
</code></pre>



</details>
