
<a name="0x2_dynamic_field"></a>

# Module `0x2::dynamic_field`

In addition to the fields declared in its type definition, a Sui object can have dynamic fields
that can be added after the object has been constructed. Unlike ordinary field names
(which are always statically declared identifiers) a dynamic field name can be any value with
the <code><b>copy</b></code>, <code>drop</code>, and <code>store</code> abilities, e.g. an integer, a boolean, or a string.
This gives Sui programmers the flexibility to extend objects on-the-fly, and it also serves as a
building block for core collection types


-  [Resource `Field`](#0x2_dynamic_field_Field)
-  [Constants](#@Constants_0)
-  [Function `add`](#0x2_dynamic_field_add)
-  [Function `borrow`](#0x2_dynamic_field_borrow)
-  [Function `borrow_mut`](#0x2_dynamic_field_borrow_mut)
-  [Function `remove`](#0x2_dynamic_field_remove)
-  [Function `exists_with_type`](#0x2_dynamic_field_exists_with_type)
-  [Function `field_ids`](#0x2_dynamic_field_field_ids)
-  [Function `hash_type_and_key`](#0x2_dynamic_field_hash_type_and_key)
-  [Function `add_child_object`](#0x2_dynamic_field_add_child_object)
-  [Function `borrow_child_object`](#0x2_dynamic_field_borrow_child_object)
-  [Function `remove_child_object`](#0x2_dynamic_field_remove_child_object)
-  [Function `has_child_object`](#0x2_dynamic_field_has_child_object)
-  [Function `has_child_object_with_ty`](#0x2_dynamic_field_has_child_object_with_ty)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
</code></pre>



<a name="0x2_dynamic_field_Field"></a>

## Resource `Field`

Internal object used for storing the field and value


<pre><code><b>struct</b> <a href="dynamic_field.md#0x2_dynamic_field_Field">Field</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
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
<code>value: <a href="_Option">option::Option</a>&lt;Value&gt;</code>
</dt>
<dd>
 The value bound to this field
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_dynamic_field_EBCSSerializationFailure"></a>

Failed to serialize the field's name


<pre><code><b>const</b> <a href="dynamic_field.md#0x2_dynamic_field_EBCSSerializationFailure">EBCSSerializationFailure</a>: u64 = 3;
</code></pre>



<a name="0x2_dynamic_field_EFieldAlreadyExists"></a>

The object already has a dynamic field with this name (with the value and type specified)


<pre><code><b>const</b> <a href="dynamic_field.md#0x2_dynamic_field_EFieldAlreadyExists">EFieldAlreadyExists</a>: u64 = 0;
</code></pre>



<a name="0x2_dynamic_field_EFieldDoesNotExist"></a>

Cannot load dynamic field.
The object does not have a dynamic field with this name (with the value and type specified)


<pre><code><b>const</b> <a href="dynamic_field.md#0x2_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a>: u64 = 1;
</code></pre>



<a name="0x2_dynamic_field_EFieldTypeMismatch"></a>

The object has a field with that name, but the value type does not match


<pre><code><b>const</b> <a href="dynamic_field.md#0x2_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a>: u64 = 2;
</code></pre>



<a name="0x2_dynamic_field_add"></a>

## Function `add`

Adds a dynamic field to the object <code><a href="object.md#0x2_object">object</a>: &<b>mut</b> UID</code> at field specified by <code>name: Name</code>.
Aborts with <code><a href="dynamic_field.md#0x2_dynamic_field_EFieldAlreadyExists">EFieldAlreadyExists</a></code> if the object already has that field with that name.


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_add">add</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt;(<a href="object.md#0x2_object">object</a>: &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>, name: Name, value: Value)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_add">add</a>&lt;Name: <b>copy</b> + drop + store, Value: store&gt;(
    // we <b>use</b> &<b>mut</b> UID in several spots for access control
    <a href="object.md#0x2_object">object</a>: &<b>mut</b> UID,
    name: Name,
    value: Value,
) {
    <b>let</b> object_addr = <a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(<a href="object.md#0x2_object">object</a>);
    <b>let</b> <a href="hash.md#0x2_hash">hash</a> = <a href="dynamic_field.md#0x2_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>if</b> (!<a href="dynamic_field.md#0x2_dynamic_field_has_child_object">has_child_object</a>(object_addr, <a href="hash.md#0x2_hash">hash</a>)) {
        <b>let</b> field = <a href="dynamic_field.md#0x2_dynamic_field_Field">Field</a> {
            id: <a href="object.md#0x2_object_new_uid_from_hash">object::new_uid_from_hash</a>(<a href="hash.md#0x2_hash">hash</a>),
            name,
            value: <a href="_none">option::none</a>&lt;Value&gt;(),
        };
        <a href="dynamic_field.md#0x2_dynamic_field_add_child_object">add_child_object</a>(object_addr, field)
    };
    // TODO remove once we have lamport timestamps
    <b>assert</b>!(<a href="dynamic_field.md#0x2_dynamic_field_has_child_object_with_ty">has_child_object_with_ty</a>&lt;<a href="dynamic_field.md#0x2_dynamic_field_Field">Field</a>&lt;Name, Value&gt;&gt;(object_addr, <a href="hash.md#0x2_hash">hash</a>), <a href="dynamic_field.md#0x2_dynamic_field_EFieldAlreadyExists">EFieldAlreadyExists</a>);
    <b>let</b> field = <a href="dynamic_field.md#0x2_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;<a href="dynamic_field.md#0x2_dynamic_field_Field">Field</a>&lt;Name, Value&gt;&gt;(object_addr, <a href="hash.md#0x2_hash">hash</a>);
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(&field.value), <a href="dynamic_field.md#0x2_dynamic_field_EFieldAlreadyExists">EFieldAlreadyExists</a>);
    <a href="_fill">option::fill</a>(&<b>mut</b> field.value, value);
}
</code></pre>



</details>

<a name="0x2_dynamic_field_borrow"></a>

## Function `borrow`

Immutably borrows the <code><a href="object.md#0x2_object">object</a></code>s dynamic field with the name specified by <code>name: Name</code>.
Aborts with <code><a href="dynamic_field.md#0x2_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if the object does not have a field with that name.
Aborts with <code><a href="dynamic_field.md#0x2_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the field exists, but the value does not have the specified
type.


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_borrow">borrow</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt;(<a href="object.md#0x2_object">object</a>: &<a href="object.md#0x2_object_UID">object::UID</a>, name: Name): &Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_borrow">borrow</a>&lt;Name: <b>copy</b> + drop + store, Value: store&gt;(
    <a href="object.md#0x2_object">object</a>: &UID,
    name: Name,
): &Value {
    <b>let</b> object_addr = <a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(<a href="object.md#0x2_object">object</a>);
    <b>let</b> <a href="hash.md#0x2_hash">hash</a> = <a href="dynamic_field.md#0x2_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>let</b> field = <a href="dynamic_field.md#0x2_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;<a href="dynamic_field.md#0x2_dynamic_field_Field">Field</a>&lt;Name, Value&gt;&gt;(object_addr, <a href="hash.md#0x2_hash">hash</a>);
    <b>assert</b>!(<a href="_is_some">option::is_some</a>(&field.value), <a href="dynamic_field.md#0x2_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a>);
    <a href="_borrow">option::borrow</a>(&field.value)
}
</code></pre>



</details>

<a name="0x2_dynamic_field_borrow_mut"></a>

## Function `borrow_mut`

Mutably borrows the <code><a href="object.md#0x2_object">object</a></code>s dynamic field with the name specified by <code>name: Name</code>.
Aborts with <code><a href="dynamic_field.md#0x2_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if the object does not have a field with that name.
Aborts with <code><a href="dynamic_field.md#0x2_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the field exists, but the value does not have the specified
type.


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_borrow_mut">borrow_mut</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt;(<a href="object.md#0x2_object">object</a>: &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>, name: Name): &<b>mut</b> Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_borrow_mut">borrow_mut</a>&lt;Name: <b>copy</b> + drop + store, Value: store&gt;(
    <a href="object.md#0x2_object">object</a>: &<b>mut</b> UID,
    name: Name,
): &<b>mut</b> Value {
    <b>let</b> object_addr = <a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(<a href="object.md#0x2_object">object</a>);
    <b>let</b> <a href="hash.md#0x2_hash">hash</a> = <a href="dynamic_field.md#0x2_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>let</b> field = <a href="dynamic_field.md#0x2_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;<a href="dynamic_field.md#0x2_dynamic_field_Field">Field</a>&lt;Name, Value&gt;&gt;(object_addr, <a href="hash.md#0x2_hash">hash</a>);
    <b>assert</b>!(<a href="_is_some">option::is_some</a>(&field.value), <a href="dynamic_field.md#0x2_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a>);
    <a href="_borrow_mut">option::borrow_mut</a>(&<b>mut</b> field.value)
}
</code></pre>



</details>

<a name="0x2_dynamic_field_remove"></a>

## Function `remove`

Removes the <code><a href="object.md#0x2_object">object</a></code>s dynamic field with the name specified by <code>name: Name</code> and returns the
bound value.
Aborts with <code><a href="dynamic_field.md#0x2_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if the object does not have a field with that name.
Aborts with <code><a href="dynamic_field.md#0x2_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the field exists, but the value does not have the specified
type.


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_remove">remove</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt;(<a href="object.md#0x2_object">object</a>: &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>, name: Name): Value
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_remove">remove</a>&lt;Name: <b>copy</b> + drop + store, Value: store&gt;(
    <a href="object.md#0x2_object">object</a>: &<b>mut</b> UID,
    name: Name,
): Value {
    <b>let</b> object_addr = <a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(<a href="object.md#0x2_object">object</a>);
    <b>let</b> <a href="hash.md#0x2_hash">hash</a> = <a href="dynamic_field.md#0x2_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>let</b> field = <a href="dynamic_field.md#0x2_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;<a href="dynamic_field.md#0x2_dynamic_field_Field">Field</a>&lt;Name, Value&gt;&gt;(object_addr, <a href="hash.md#0x2_hash">hash</a>);
    <b>assert</b>!(<a href="_is_some">option::is_some</a>(&field.value), <a href="dynamic_field.md#0x2_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a>);
    <a href="_extract">option::extract</a>(&<b>mut</b> field.value)
}
</code></pre>



</details>

<a name="0x2_dynamic_field_exists_with_type"></a>

## Function `exists_with_type`

Returns true if and only if the <code><a href="object.md#0x2_object">object</a></code> has a dynamic field with the name specified by
<code>name: Name</code> with an assigned value of type <code>Value</code>.


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_exists_with_type">exists_with_type</a>&lt;Name: <b>copy</b>, drop, store, Value: store&gt;(<a href="object.md#0x2_object">object</a>: &<a href="object.md#0x2_object_UID">object::UID</a>, name: Name): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_exists_with_type">exists_with_type</a>&lt;Name: <b>copy</b> + drop + store, Value: store&gt;(
    <a href="object.md#0x2_object">object</a>: &UID,
    name: Name,
): bool {
    <b>let</b> object_addr = <a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(<a href="object.md#0x2_object">object</a>);
    <b>let</b> <a href="hash.md#0x2_hash">hash</a> = <a href="dynamic_field.md#0x2_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>if</b> (!<a href="dynamic_field.md#0x2_dynamic_field_has_child_object_with_ty">has_child_object_with_ty</a>&lt;<a href="dynamic_field.md#0x2_dynamic_field_Field">Field</a>&lt;Name, Value&gt;&gt;(object_addr, <a href="hash.md#0x2_hash">hash</a>)) <b>return</b> <b>false</b>;
    <b>let</b> field = <a href="dynamic_field.md#0x2_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;<a href="dynamic_field.md#0x2_dynamic_field_Field">Field</a>&lt;Name, Value&gt;&gt;(object_addr, <a href="hash.md#0x2_hash">hash</a>);
    <a href="_is_some">option::is_some</a>(&field.value)
}
</code></pre>



</details>

<a name="0x2_dynamic_field_field_ids"></a>

## Function `field_ids`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_field_ids">field_ids</a>&lt;Name: <b>copy</b>, drop, store&gt;(<a href="object.md#0x2_object">object</a>: &<a href="object.md#0x2_object_UID">object::UID</a>, name: Name): (<b>address</b>, <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_field_ids">field_ids</a>&lt;Name: <b>copy</b> + drop + store&gt;(
    <a href="object.md#0x2_object">object</a>: &UID,
    name: Name,
): (<b>address</b>, <b>address</b>) {
    <b>let</b> object_addr = <a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(<a href="object.md#0x2_object">object</a>);
    <b>let</b> <a href="hash.md#0x2_hash">hash</a> = <a href="dynamic_field.md#0x2_dynamic_field_hash_type_and_key">hash_type_and_key</a>(object_addr, name);
    <b>let</b> field = <a href="dynamic_field.md#0x2_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;<a href="dynamic_field.md#0x2_dynamic_field_Field">Field</a>&lt;Name, ID&gt;&gt;(object_addr, <a href="hash.md#0x2_hash">hash</a>);
    <b>assert</b>!(<a href="_is_some">option::is_some</a>(&field.value), <a href="dynamic_field.md#0x2_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a>);
    (<a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(&field.id), <a href="object.md#0x2_object_id_to_address">object::id_to_address</a>(&<a href="_destroy_some">option::destroy_some</a>(field.value)))
}
</code></pre>



</details>

<a name="0x2_dynamic_field_hash_type_and_key"></a>

## Function `hash_type_and_key`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_hash_type_and_key">hash_type_and_key</a>&lt;K: <b>copy</b>, drop, store&gt;(parent: <b>address</b>, k: K): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_hash_type_and_key">hash_type_and_key</a>&lt;K: <b>copy</b> + drop + store&gt;(parent: <b>address</b>, k: K): <b>address</b>;
</code></pre>



</details>

<a name="0x2_dynamic_field_add_child_object"></a>

## Function `add_child_object`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_add_child_object">add_child_object</a>&lt;Child: key&gt;(parent: <b>address</b>, child: Child)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_add_child_object">add_child_object</a>&lt;Child: key&gt;(parent: <b>address</b>, child: Child);
</code></pre>



</details>

<a name="0x2_dynamic_field_borrow_child_object"></a>

## Function `borrow_child_object`

throws <code><a href="dynamic_field.md#0x2_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if a child does not exist with that ID
or throws <code><a href="dynamic_field.md#0x2_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the type does not match


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;Child: key&gt;(parent: <b>address</b>, id: <b>address</b>): &<b>mut</b> Child
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_borrow_child_object">borrow_child_object</a>&lt;Child: key&gt;(parent: <b>address</b>, id: <b>address</b>): &<b>mut</b> Child;
</code></pre>



</details>

<a name="0x2_dynamic_field_remove_child_object"></a>

## Function `remove_child_object`

throws <code><a href="dynamic_field.md#0x2_dynamic_field_EFieldDoesNotExist">EFieldDoesNotExist</a></code> if a child does not exist with that ID
or throws <code><a href="dynamic_field.md#0x2_dynamic_field_EFieldTypeMismatch">EFieldTypeMismatch</a></code> if the type does not match


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_remove_child_object">remove_child_object</a>&lt;Child: key&gt;(parent: <b>address</b>, id: <b>address</b>): Child
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_remove_child_object">remove_child_object</a>&lt;Child: key&gt;(parent: <b>address</b>, id: <b>address</b>): Child;
</code></pre>



</details>

<a name="0x2_dynamic_field_has_child_object"></a>

## Function `has_child_object`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_has_child_object">has_child_object</a>(parent: <b>address</b>, id: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_has_child_object">has_child_object</a>(parent: <b>address</b>, id: <b>address</b>): bool;
</code></pre>



</details>

<a name="0x2_dynamic_field_has_child_object_with_ty"></a>

## Function `has_child_object_with_ty`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_has_child_object_with_ty">has_child_object_with_ty</a>&lt;Child: key&gt;(parent: <b>address</b>, id: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>native</b> <b>fun</b> <a href="dynamic_field.md#0x2_dynamic_field_has_child_object_with_ty">has_child_object_with_ty</a>&lt;Child: key&gt;(parent: <b>address</b>, id: <b>address</b>): bool;
</code></pre>



</details>
