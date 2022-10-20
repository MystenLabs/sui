
<a name="0x2_dynamic_object_field"></a>

# Module `0x2::dynamic_object_field`

Similar to <code>sui::dynamic_field</code>, this module allows for the access of dynamic fields. But
unlike, <code>sui::dynamic_field</code> the values bound to these dynamic fields _must_ be objects
themselves. This allows for the objects to still exist within in storage, which may be important
for external tools. The difference is otherwise not observable from within Move.


-  [Struct `Wrapper`](#0x2_dynamic_object_field_Wrapper)
-  [Constants](#@Constants_0)
-  [Function `add`](#0x2_dynamic_object_field_add)
-  [Function `borrow`](#0x2_dynamic_object_field_borrow)
-  [Function `borrow_mut`](#0x2_dynamic_object_field_borrow_mut)
-  [Function `remove`](#0x2_dynamic_object_field_remove)
-  [Function `exists_`](#0x2_dynamic_object_field_exists_)
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

<a name="@Constants_0"></a>

## Constants


<a name="0x2_dynamic_object_field_EBCSSerializationFailure"></a>

Failed to serialize the field's name


<pre><code><b>const</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_EBCSSerializationFailure">EBCSSerializationFailure</a>: u64 = 3;
</code></pre>



<a name="0x2_dynamic_object_field_EFieldAlreadyExists"></a>

The object already has a dynamic field with this name (with the value and type specified)


<pre><code><b>const</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_EFieldAlreadyExists">EFieldAlreadyExists</a>: u64 = 0;
</code></pre>



<a name="0x2_dynamic_object_field_EFieldDoesNotExist"></a>

Cannot load dynamic field.
The object does not have a dynamic field with this name (with the value and type specified)


<pre><code><b>const</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_EFieldDoesNotExist">EFieldDoesNotExist</a>: u64 = 1;
</code></pre>



<a name="0x2_dynamic_object_field_EFieldTypeMismatch"></a>

The object has a field with that name, but the value type does not match


<pre><code><b>const</b> <a href="dynamic_object_field.md#0x2_dynamic_object_field_EFieldTypeMismatch">EFieldTypeMismatch</a>: u64 = 2;
</code></pre>



<a name="0x2_dynamic_object_field_add"></a>

## Function `add`

Adds a dynamic object field to the object <code><a href="object.md#0x2_object">object</a>: &<b>mut</b> UID</code> at field specified by <code>name: Name</code>.
Aborts with <code><a href="dynamic_object_field.md#0x2_dynamic_object_field_EFieldAlreadyExists">EFieldAlreadyExists</a></code> if the object already has that field with that name.


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
    df::add(<a href="object.md#0x2_object">object</a>, key, id);
    <b>let</b> (field_id, _) = df::field_ids&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="object.md#0x2_object">object</a>, key);
    add_child_object(<a href="object.md#0x2_object_id_to_address">object::id_to_address</a>(&field_id), value);
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_borrow"></a>

## Function `borrow`

Immutably borrows the <code><a href="object.md#0x2_object">object</a></code>s dynamic object field with the name specified by <code>name: Name</code>.
Aborts with <code><a href="dynamic_object_field.md#0x2_dynamic_object_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if the object does not have a field with that name.
Aborts with <code><a href="dynamic_object_field.md#0x2_dynamic_object_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the field exists, but the value object does not have the
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
    <b>let</b> (field_id, value_id) = df::field_ids&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="object.md#0x2_object">object</a>, key);
    borrow_child_object&lt;Value&gt;(<a href="object.md#0x2_object_id_to_address">object::id_to_address</a>(&field_id), <a href="object.md#0x2_object_id_to_address">object::id_to_address</a>(&value_id))
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_borrow_mut"></a>

## Function `borrow_mut`

Mutably borrows the <code><a href="object.md#0x2_object">object</a></code>s dynamic object field with the name specified by <code>name: Name</code>.
Aborts with <code><a href="dynamic_object_field.md#0x2_dynamic_object_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if the object does not have a field with that name.
Aborts with <code><a href="dynamic_object_field.md#0x2_dynamic_object_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the field exists, but the value object does not have the
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
    <b>let</b> (field_id, value_id) = df::field_ids&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="object.md#0x2_object">object</a>, key);
    borrow_child_object&lt;Value&gt;(<a href="object.md#0x2_object_id_to_address">object::id_to_address</a>(&field_id), <a href="object.md#0x2_object_id_to_address">object::id_to_address</a>(&value_id))
}
</code></pre>



</details>

<a name="0x2_dynamic_object_field_remove"></a>

## Function `remove`

Removes the <code><a href="object.md#0x2_object">object</a></code>s dynamic object field with the name specified by <code>name: Name</code> and returns
the bound object.
Aborts with <code><a href="dynamic_object_field.md#0x2_dynamic_object_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if the object does not have a field with that name.
Aborts with <code><a href="dynamic_object_field.md#0x2_dynamic_object_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the field exists, but the value object does not have the
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
    <b>let</b> (field_id, value_id) = df::field_ids&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="object.md#0x2_object">object</a>, key);
    <b>let</b> value = remove_child_object&lt;Value&gt;(
        <a href="object.md#0x2_object_id_to_address">object::id_to_address</a>(&field_id),
        <a href="object.md#0x2_object_id_to_address">object::id_to_address</a>(&value_id),
    );
    df::remove&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;, ID&gt;(<a href="object.md#0x2_object">object</a>, key);
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
    df::exists_with_type&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;, ID&gt;(<a href="object.md#0x2_object">object</a>, key)
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
    <b>if</b> (!df::exists_with_type&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;, ID&gt;(<a href="object.md#0x2_object">object</a>, key)) <b>return</b> <a href="_none">option::none</a>();
    <b>let</b> (_field_id, value_id) = df::field_ids&lt;<a href="dynamic_object_field.md#0x2_dynamic_object_field_Wrapper">Wrapper</a>&lt;Name&gt;&gt;(<a href="object.md#0x2_object">object</a>, key);
    <a href="_some">option::some</a>(value_id)
}
</code></pre>



</details>
