---
title: Module `sui::dynamic_field`
---

In addition to the fields declared in its type definition, a Sui object can have dynamic fields
that can be added after the object has been constructed. Unlike ordinary field names
(which are always statically declared identifiers) a dynamic field name can be any value with
the <code><b>copy</b></code>, <code>drop</code>, and <code>store</code> abilities, e.g. an integer, a boolean, or a string.
This gives Sui programmers the flexibility to extend objects on-the-fly, and it also serves as a
building block for core collection types


-  [Struct `Field`](#sui_dynamic_field_Field)
-  [Constants](#@Constants_0)
-  [Function `add`](#sui_dynamic_field_add)
-  [Function `borrow`](#sui_dynamic_field_borrow)
-  [Function `borrow_mut`](#sui_dynamic_field_borrow_mut)
-  [Function `remove`](#sui_dynamic_field_remove)
-  [Function `exists_`](#sui_dynamic_field_exists_)
-  [Function `remove_if_exists`](#sui_dynamic_field_remove_if_exists)
-  [Function `exists_with_type`](#sui_dynamic_field_exists_with_type)
-  [Function `field_info`](#sui_dynamic_field_field_info)
-  [Function `field_info_mut`](#sui_dynamic_field_field_info_mut)
-  [Function `hash_type_and_key`](#sui_dynamic_field_hash_type_and_key)
-  [Function `add_child_object`](#sui_dynamic_field_add_child_object)
-  [Function `borrow_child_object`](#sui_dynamic_field_borrow_child_object)
-  [Function `borrow_child_object_mut`](#sui_dynamic_field_borrow_child_object_mut)
-  [Function `remove_child_object`](#sui_dynamic_field_remove_child_object)
-  [Function `has_child_object`](#sui_dynamic_field_has_child_object)
-  [Function `has_child_object_with_ty`](#sui_dynamic_field_has_child_object_with_ty)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/bcs.md#std_bcs">std::bcs</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/string.md#std_string">std::string</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
<b>use</b> <a href="../sui/address.md#sui_address">sui::address</a>;
<b>use</b> <a href="../sui/hex.md#sui_hex">sui::hex</a>;
<b>use</b> <a href="../sui/object.md#sui_object">sui::object</a>;
<b>use</b> <a href="../sui/tx_context.md#sui_tx_context">sui::tx_context</a>;
</code></pre>



<a name="sui_dynamic_field_Field"></a>

## Struct `Field`

Internal object used for storing the field and value


<pre><code><b>public</b> <b>struct</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_Field">Field</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="../sui/object.md#sui_object_UID">sui::object::UID</a></code>
</dt>
<dd>
 Determined by the hash of the object ID, the field name value and it's type,
 i.e. hash(parent.id || name || Name)
</dd>
<dt>
<code>name: Name</code>
</dt>
<dd>
 The value for the name of this field
</dd>
<dt>
<code>value: Value</code>
</dt>
<dd>
 The value bound to this field
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="sui_dynamic_field_EBCSSerializationFailure"></a>

Failed to serialize the field's name


<pre><code><b>const</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_EBCSSerializationFailure">EBCSSerializationFailure</a>: u64 = 3;
</code></pre>



<a name="sui_dynamic_field_EFieldAlreadyExists"></a>

The object already has a dynamic field with this name (with the value and type specified)


<pre><code><b>const</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldAlreadyExists">EFieldAlreadyExists</a>: u64 = 0;
</code></pre>



<a name="sui_dynamic_field_EFieldDoesNotExist"></a>

Cannot load dynamic field.
The object does not have a dynamic field with this name (with the value and type specified)


<pre><code><b>const</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a>: u64 = 1;
</code></pre>



<a name="sui_dynamic_field_EFieldTypeMismatch"></a>

The object has a field with that name, but the value type does not match


<pre><code><b>const</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a>: u64 = 2;
</code></pre>



<a name="sui_dynamic_field_ESharedObjectOperationNotSupported"></a>

The object added as a dynamic field was previously a shared object


<pre><code><b>const</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_ESharedObjectOperationNotSupported">ESharedObjectOperationNotSupported</a>: u64 = 4;
</code></pre>



<a name="sui_dynamic_field_add"></a>

## Function `add`

Adds a dynamic field to the object <code><a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> UID</code> at field specified by <code>name: Name</code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldAlreadyExists">EFieldAlreadyExists</a></code> if the object already has that field with that name.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_add">add</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt;(<a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, name: Name, value: Value)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_add">add</a>&lt;Name: <b>copy</b> + drop + store, Value: store&gt;(
    // we <b>use</b> &<b>mut</b> UID in several spots <b>for</b> access control
    <a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> UID,
    name: Name,
    value: Value,
) {
    <b>let</b> object_addr = <a href="../sui/object.md#sui_object">object</a>.to_address();
    <b>let</b> <a href="../sui/hash.md#sui_hash">hash</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>assert</b>!(!<a href="../sui/dynamic_field.md#sui_dynamic_field_has_child_object">has_child_object</a>(object_addr, <a href="../sui/hash.md#sui_hash">hash</a>), <a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldAlreadyExists">EFieldAlreadyExists</a>);
    <b>let</b> field = <a href="../sui/dynamic_field.md#sui_dynamic_field_Field">Field</a> {
        id: <a href="../sui/object.md#sui_object_new_uid_from_hash">object::new_uid_from_hash</a>(<a href="../sui/hash.md#sui_hash">hash</a>),
        name,
        value,
    };
    <a href="../sui/dynamic_field.md#sui_dynamic_field_add_child_object">add_child_object</a>(object_addr, field)
}
</code></pre>



</details>

<a name="sui_dynamic_field_borrow"></a>

## Function `borrow`

Immutably borrows the <code><a href="../sui/object.md#sui_object">object</a></code>s dynamic field with the name specified by <code>name: Name</code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if the object does not have a field with that name.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the field exists, but the value does not have the specified
type.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/borrow.md#sui_borrow">borrow</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt;(<a href="../sui/object.md#sui_object">object</a>: &<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, name: Name): &Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/borrow.md#sui_borrow">borrow</a>&lt;Name: <b>copy</b> + drop + store, Value: store&gt;(<a href="../sui/object.md#sui_object">object</a>: &UID, name: Name): &Value {
    <b>let</b> object_addr = <a href="../sui/object.md#sui_object">object</a>.to_address();
    <b>let</b> <a href="../sui/hash.md#sui_hash">hash</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>let</b> field = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;<a href="../sui/dynamic_field.md#sui_dynamic_field_Field">Field</a>&lt;Name, Value&gt;&gt;(<a href="../sui/object.md#sui_object">object</a>, <a href="../sui/hash.md#sui_hash">hash</a>);
    &field.value
}
</code></pre>



</details>

<a name="sui_dynamic_field_borrow_mut"></a>

## Function `borrow_mut`

Mutably borrows the <code><a href="../sui/object.md#sui_object">object</a></code>s dynamic field with the name specified by <code>name: Name</code>.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if the object does not have a field with that name.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the field exists, but the value does not have the specified
type.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">borrow_mut</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt;(<a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, name: Name): &<b>mut</b> Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_mut">borrow_mut</a>&lt;Name: <b>copy</b> + drop + store, Value: store&gt;(
    <a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> UID,
    name: Name,
): &<b>mut</b> Value {
    <b>let</b> object_addr = <a href="../sui/object.md#sui_object">object</a>.to_address();
    <b>let</b> <a href="../sui/hash.md#sui_hash">hash</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>let</b> field = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_child_object_mut">borrow_child_object_mut</a>&lt;<a href="../sui/dynamic_field.md#sui_dynamic_field_Field">Field</a>&lt;Name, Value&gt;&gt;(<a href="../sui/object.md#sui_object">object</a>, <a href="../sui/hash.md#sui_hash">hash</a>);
    &<b>mut</b> field.value
}
</code></pre>



</details>

<a name="sui_dynamic_field_remove"></a>

## Function `remove`

Removes the <code><a href="../sui/object.md#sui_object">object</a></code>s dynamic field with the name specified by <code>name: Name</code> and returns the
bound value.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if the object does not have a field with that name.
Aborts with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the field exists, but the value does not have the specified
type.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_remove">remove</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt;(<a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, name: Name): Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_remove">remove</a>&lt;Name: <b>copy</b> + drop + store, Value: store&gt;(<a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> UID, name: Name): Value {
    <b>let</b> object_addr = <a href="../sui/object.md#sui_object">object</a>.to_address();
    <b>let</b> <a href="../sui/hash.md#sui_hash">hash</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>let</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_Field">Field</a> { id, name: _, value } = <a href="../sui/dynamic_field.md#sui_dynamic_field_remove_child_object">remove_child_object</a>&lt;<a href="../sui/dynamic_field.md#sui_dynamic_field_Field">Field</a>&lt;Name, Value&gt;&gt;(object_addr, <a href="../sui/hash.md#sui_hash">hash</a>);
    id.delete();
    value
}
</code></pre>



</details>

<a name="sui_dynamic_field_exists_"></a>

## Function `exists_`

Returns true if and only if the <code><a href="../sui/object.md#sui_object">object</a></code> has a dynamic field with the name specified by
<code>name: Name</code> but without specifying the <code>Value</code> type


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_exists_">exists_</a>&lt;Name: <b>copy</b>, drop, store&gt;(<a href="../sui/object.md#sui_object">object</a>: &<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, name: Name): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_exists_">exists_</a>&lt;Name: <b>copy</b> + drop + store&gt;(<a href="../sui/object.md#sui_object">object</a>: &UID, name: Name): bool {
    <b>let</b> object_addr = <a href="../sui/object.md#sui_object">object</a>.to_address();
    <b>let</b> <a href="../sui/hash.md#sui_hash">hash</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <a href="../sui/dynamic_field.md#sui_dynamic_field_has_child_object">has_child_object</a>(object_addr, <a href="../sui/hash.md#sui_hash">hash</a>)
}
</code></pre>



</details>

<a name="sui_dynamic_field_remove_if_exists"></a>

## Function `remove_if_exists`

Removes the dynamic field if it exists. Returns the <code>some(Value)</code> if it exists or none otherwise.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_remove_if_exists">remove_if_exists</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt;(<a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, name: Name): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Value&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_remove_if_exists">remove_if_exists</a>&lt;Name: <b>copy</b> + drop + store, Value: store&gt;(
    <a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> UID,
    name: Name,
): Option&lt;Value&gt; {
    <b>if</b> (<a href="../sui/dynamic_field.md#sui_dynamic_field_exists_">exists_</a>&lt;Name&gt;(<a href="../sui/object.md#sui_object">object</a>, name)) {
        option::some(<a href="../sui/dynamic_field.md#sui_dynamic_field_remove">remove</a>(<a href="../sui/object.md#sui_object">object</a>, name))
    } <b>else</b> {
        option::none()
    }
}
</code></pre>



</details>

<a name="sui_dynamic_field_exists_with_type"></a>

## Function `exists_with_type`

Returns true if and only if the <code><a href="../sui/object.md#sui_object">object</a></code> has a dynamic field with the name specified by
<code>name: Name</code> with an assigned value of type <code>Value</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_exists_with_type">exists_with_type</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt;(<a href="../sui/object.md#sui_object">object</a>: &<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, name: Name): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_exists_with_type">exists_with_type</a>&lt;Name: <b>copy</b> + drop + store, Value: store&gt;(
    <a href="../sui/object.md#sui_object">object</a>: &UID,
    name: Name,
): bool {
    <b>let</b> object_addr = <a href="../sui/object.md#sui_object">object</a>.to_address();
    <b>let</b> <a href="../sui/hash.md#sui_hash">hash</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <a href="../sui/dynamic_field.md#sui_dynamic_field_has_child_object_with_ty">has_child_object_with_ty</a>&lt;<a href="../sui/dynamic_field.md#sui_dynamic_field_Field">Field</a>&lt;Name, Value&gt;&gt;(object_addr, <a href="../sui/hash.md#sui_hash">hash</a>)
}
</code></pre>



</details>

<a name="sui_dynamic_field_field_info"></a>

## Function `field_info`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_field_info">field_info</a>&lt;Name: <b>copy</b>, drop, store&gt;(<a href="../sui/object.md#sui_object">object</a>: &<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, name: Name): (&<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_field_info">field_info</a>&lt;Name: <b>copy</b> + drop + store&gt;(
    <a href="../sui/object.md#sui_object">object</a>: &UID,
    name: Name,
): (&UID, <b>address</b>) {
    <b>let</b> object_addr = <a href="../sui/object.md#sui_object">object</a>.to_address();
    <b>let</b> <a href="../sui/hash.md#sui_hash">hash</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>let</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_Field">Field</a> { id, name: _, value } = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;<a href="../sui/dynamic_field.md#sui_dynamic_field_Field">Field</a>&lt;Name, ID&gt;&gt;(<a href="../sui/object.md#sui_object">object</a>, <a href="../sui/hash.md#sui_hash">hash</a>);
    (id, value.to_address())
}
</code></pre>



</details>

<a name="sui_dynamic_field_field_info_mut"></a>

## Function `field_info_mut`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_field_info_mut">field_info_mut</a>&lt;Name: <b>copy</b>, drop, store&gt;(<a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, name: Name): (&<b>mut</b> <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_field_info_mut">field_info_mut</a>&lt;Name: <b>copy</b> + drop + store&gt;(
    <a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> UID,
    name: Name,
): (&<b>mut</b> UID, <b>address</b>) {
    <b>let</b> object_addr = <a href="../sui/object.md#sui_object">object</a>.to_address();
    <b>let</b> <a href="../sui/hash.md#sui_hash">hash</a> = <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>let</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_Field">Field</a> { id, name: _, value } = <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_child_object_mut">borrow_child_object_mut</a>&lt;<a href="../sui/dynamic_field.md#sui_dynamic_field_Field">Field</a>&lt;Name, ID&gt;&gt;(<a href="../sui/object.md#sui_object">object</a>, <a href="../sui/hash.md#sui_hash">hash</a>);
    (id, value.to_address())
}
</code></pre>



</details>

<a name="sui_dynamic_field_hash_type_and_key"></a>

## Function `hash_type_and_key`

May abort with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EBCSSerializationFailure">EBCSSerializationFailure</a></code>.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">hash_type_and_key</a>&lt;K: <b>copy</b>, drop, store&gt;(parent: <b>address</b>, k: K): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_hash_type_and_key">hash_type_and_key</a>&lt;K: <b>copy</b> + drop + store&gt;(
    parent: <b>address</b>,
    k: K,
): <b>address</b>;
</code></pre>



</details>

<a name="sui_dynamic_field_add_child_object"></a>

## Function `add_child_object`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_add_child_object">add_child_object</a>&lt;Child: key&gt;(parent: <b>address</b>, child: Child)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_add_child_object">add_child_object</a>&lt;Child: key&gt;(parent: <b>address</b>, child: Child);
</code></pre>



</details>

<a name="sui_dynamic_field_borrow_child_object"></a>

## Function `borrow_child_object`

throws <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if a child does not exist with that ID
or throws <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the type does not match,
and may also abort with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EBCSSerializationFailure">EBCSSerializationFailure</a></code>
we need two versions to return a reference or a mutable reference


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;Child: key&gt;(<a href="../sui/object.md#sui_object">object</a>: &<a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, id: <b>address</b>): &Child
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;Child: key&gt;(<a href="../sui/object.md#sui_object">object</a>: &UID, id: <b>address</b>): &Child;
</code></pre>



</details>

<a name="sui_dynamic_field_borrow_child_object_mut"></a>

## Function `borrow_child_object_mut`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_child_object_mut">borrow_child_object_mut</a>&lt;Child: key&gt;(<a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> <a href="../sui/object.md#sui_object_UID">sui::object::UID</a>, id: <b>address</b>): &<b>mut</b> Child
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_borrow_child_object_mut">borrow_child_object_mut</a>&lt;Child: key&gt;(
    <a href="../sui/object.md#sui_object">object</a>: &<b>mut</b> UID,
    id: <b>address</b>,
): &<b>mut</b> Child;
</code></pre>



</details>

<a name="sui_dynamic_field_remove_child_object"></a>

## Function `remove_child_object`

throws <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if a child does not exist with that ID
or throws <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the type does not match,
and may also abort with <code><a href="../sui/dynamic_field.md#sui_dynamic_field_EBCSSerializationFailure">EBCSSerializationFailure</a></code>.


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_remove_child_object">remove_child_object</a>&lt;Child: key&gt;(parent: <b>address</b>, id: <b>address</b>): Child
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_remove_child_object">remove_child_object</a>&lt;Child: key&gt;(parent: <b>address</b>, id: <b>address</b>): Child;
</code></pre>



</details>

<a name="sui_dynamic_field_has_child_object"></a>

## Function `has_child_object`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_has_child_object">has_child_object</a>(parent: <b>address</b>, id: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_has_child_object">has_child_object</a>(parent: <b>address</b>, id: <b>address</b>): bool;
</code></pre>



</details>

<a name="sui_dynamic_field_has_child_object_with_ty"></a>

## Function `has_child_object_with_ty`



<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_has_child_object_with_ty">has_child_object_with_ty</a>&lt;Child: key&gt;(parent: <b>address</b>, id: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<a href="../sui/package.md#sui_package">package</a>) <b>native</b> <b>fun</b> <a href="../sui/dynamic_field.md#sui_dynamic_field_has_child_object_with_ty">has_child_object_with_ty</a>&lt;Child: key&gt;(parent: <b>address</b>, id: <b>address</b>): bool;
</code></pre>



</details>
