
<a name="0x2_display"></a>

# Module `0x2::display`

Defines a Display struct which defines the way an Object
should be displayed. The intention is to keep data as independent
from its display as possible, protecting the development process
and keeping it separate from the ecosystem agreements.

Each of the fields of the Display object should allow for pattern
substitution and filling-in the pieces using the data from the object T.

More entry functions might be added in the future depending on the use cases.


-  [Resource `Display`](#0x2_display_Display)
-  [Struct `DisplayCreated`](#0x2_display_DisplayCreated)
-  [Struct `VersionUpdated`](#0x2_display_VersionUpdated)
-  [Constants](#@Constants_0)
-  [Function `add_owned`](#0x2_display_add_owned)
-  [Function `new`](#0x2_display_new)
-  [Function `share`](#0x2_display_share)
-  [Function `transfer`](#0x2_display_transfer)
-  [Function `new_protected`](#0x2_display_new_protected)
-  [Function `create_and_share`](#0x2_display_create_and_share)
-  [Function `create_and_keep`](#0x2_display_create_and_keep)
-  [Function `new_with_fields`](#0x2_display_new_with_fields)
-  [Function `update_version`](#0x2_display_update_version)
-  [Function `add`](#0x2_display_add)
-  [Function `add_multiple`](#0x2_display_add_multiple)
-  [Function `edit`](#0x2_display_edit)
-  [Function `remove`](#0x2_display_remove)
-  [Function `version`](#0x2_display_version)
-  [Function `fields`](#0x2_display_fields)
-  [Function `create_internal`](#0x2_display_create_internal)
-  [Function `add_internal`](#0x2_display_add_internal)


<pre><code><b>use</b> <a href="">0x1::string</a>;
<b>use</b> <a href="event.md#0x2_event">0x2::event</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="publisher.md#0x2_publisher">0x2::publisher</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="vec_map.md#0x2_vec_map">0x2::vec_map</a>;
</code></pre>



<a name="0x2_display_Display"></a>

## Resource `Display`

The Display<T> object. Defines the way a T instance should be
displayed. Display object can only be created and modified with
a PublisherCap, making sure that the rules are set by the owner
of the type.

Each of the display properties should support patterns outside
of the system, making it simpler to customize Display based
on the property values of an Object.
```
// Example of a display object
Display<0x...::capy::Capy> {
fields:
<name, "Capy { genes }">
<link, "https://capy.art/capy/{ id }">
<image, "https://api.capy.art/capy/{ id }/svg">
<description, "Lovely Capy, one of many">
}
```

Uses only String type due to external-facing nature of the object,
the property names have a priority over their types.


<pre><code><b>struct</b> <a href="display.md#0x2_display_Display">Display</a>&lt;T: key&gt; <b>has</b> key
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
<code>fields: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="_String">string::String</a>, <a href="_String">string::String</a>&gt;</code>
</dt>
<dd>
 Contains fields for display. Currently supported
 fields are: name, link, image and description.
</dd>
<dt>
<code>version: u16</code>
</dt>
<dd>
 Version that can only be updated manually by the Publisher.
</dd>
</dl>


</details>

<a name="0x2_display_DisplayCreated"></a>

## Struct `DisplayCreated`

Event: emitted when a new Display object has been created for type T.
Type signature of the event corresponds to the type while id serves for
the discovery.

Since Sui RPC supports querying events by type, finding a Display for the T
would be as simple as looking for the first event with <code><a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;</code>.


<pre><code><b>struct</b> <a href="display.md#0x2_display_DisplayCreated">DisplayCreated</a>&lt;T: key&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_display_VersionUpdated"></a>

## Struct `VersionUpdated`

Version of Display got updated -


<pre><code><b>struct</b> <a href="display.md#0x2_display_VersionUpdated">VersionUpdated</a>&lt;T: key&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>version: u16</code>
</dt>
<dd>

</dd>
<dt>
<code>fields: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="_String">string::String</a>, <a href="_String">string::String</a>&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_display_ENotOwner"></a>

For when T does not belong to the package <code>Publisher</code>.


<pre><code><b>const</b> <a href="display.md#0x2_display_ENotOwner">ENotOwner</a>: u64 = 0;
</code></pre>



<a name="0x2_display_EVecLengthMismatch"></a>

For when vectors passed into one of the multiple insert functions
don't match in their lengths.


<pre><code><b>const</b> <a href="display.md#0x2_display_EVecLengthMismatch">EVecLengthMismatch</a>: u64 = 1;
</code></pre>



<a name="0x2_display_add_owned"></a>

## Function `add_owned`

Since the only way to own a Display is before it has been published,
we don't need to perform an authorization check.

Also, the only place it can be used is the function where the Display
object was created; hence values and names are likely to be hardcoded and
vector<u8> is the best type for that purpose.


<pre><code><b>public</b> <b>fun</b> <a href="display.md#0x2_display_add_owned">add_owned</a>&lt;T: key&gt;(d: <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;, name: <a href="">vector</a>&lt;u8&gt;, value: <a href="">vector</a>&lt;u8&gt;): <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="display.md#0x2_display_add_owned">add_owned</a>&lt;T: key&gt;(d: <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;, name: <a href="">vector</a>&lt;u8&gt;, value: <a href="">vector</a>&lt;u8&gt;): <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt; {
    <a href="display.md#0x2_display_add_internal">add_internal</a>(&<b>mut</b> d, utf8(name), utf8(value));
    d
}
</code></pre>



</details>

<a name="0x2_display_new"></a>

## Function `new`

Create an empty Display object. It can either be shared empty or filled
with data right away via cheaper <code>set_owned</code> method.


<pre><code><b>public</b> <b>fun</b> <a href="display.md#0x2_display_new">new</a>&lt;T: key&gt;(pub: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="display.md#0x2_display_new">new</a>&lt;T: key&gt;(pub: &Publisher, ctx: &<b>mut</b> TxContext): <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt; {
    <b>assert</b>!(is_package&lt;T&gt;(pub), <a href="display.md#0x2_display_ENotOwner">ENotOwner</a>);
    <a href="display.md#0x2_display_create_internal">create_internal</a>(ctx)
}
</code></pre>



</details>

<a name="0x2_display_share"></a>

## Function `share`

Share an object after the initialization is complete.


<pre><code><b>public</b> <b>fun</b> <a href="display.md#0x2_display_share">share</a>&lt;T: key&gt;(d: <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="display.md#0x2_display_share">share</a>&lt;T: key&gt;(d: <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;) {
    <a href="transfer.md#0x2_transfer_share_object">transfer::share_object</a>(d)
}
</code></pre>



</details>

<a name="0x2_display_transfer"></a>

## Function `transfer`

Transfer an object to an address to have it single owner.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>&lt;T: key&gt;(d: <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;, receiver: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>&lt;T: key&gt;(d: <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;, receiver: <b>address</b>) {
    <a href="transfer.md#0x2_transfer_transfer">transfer::transfer</a>(d, receiver)
}
</code></pre>



</details>

<a name="0x2_display_new_protected"></a>

## Function `new_protected`

Protected method to create an empty Display for the <code>Collectible&lt;T&gt;</code>.
Similar result can be achieved by freezing the Publisher for the
Container package.


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="display.md#0x2_display_new_protected">new_protected</a>&lt;T: key&gt;(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="display.md#0x2_display_new_protected">new_protected</a>&lt;T: key&gt;(ctx: &<b>mut</b> TxContext): <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt; {
    <a href="display.md#0x2_display_create_internal">create_internal</a>(ctx)
}
</code></pre>



</details>

<a name="0x2_display_create_and_share"></a>

## Function `create_and_share`

Create a new empty Display<T> object and share it.


<pre><code><b>public</b> entry <b>fun</b> <a href="display.md#0x2_display_create_and_share">create_and_share</a>&lt;T: key&gt;(pub: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>public</b> <b>fun</b> <a href="display.md#0x2_display_create_and_share">create_and_share</a>&lt;T: key&gt;(pub: &Publisher, ctx: &<b>mut</b> TxContext) {
    <a href="display.md#0x2_display_share">share</a>(<a href="display.md#0x2_display_new">new</a>&lt;T&gt;(pub, ctx))
}
</code></pre>



</details>

<a name="0x2_display_create_and_keep"></a>

## Function `create_and_keep`

Create a new empty Display<T> object and keep it.


<pre><code><b>public</b> entry <b>fun</b> <a href="display.md#0x2_display_create_and_keep">create_and_keep</a>&lt;T: key&gt;(pub: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>public</b> <b>fun</b> <a href="display.md#0x2_display_create_and_keep">create_and_keep</a>&lt;T: key&gt;(pub: &Publisher, ctx: &<b>mut</b> TxContext) {
    <a href="transfer.md#0x2_transfer">transfer</a>(<a href="display.md#0x2_display_new">new</a>&lt;T&gt;(pub, ctx), sender(ctx))
}
</code></pre>



</details>

<a name="0x2_display_new_with_fields"></a>

## Function `new_with_fields`

Create a new Display<T> object with a set of fields.


<pre><code><b>public</b> entry <b>fun</b> <a href="display.md#0x2_display_new_with_fields">new_with_fields</a>&lt;T: key&gt;(pub: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>, fields: <a href="">vector</a>&lt;<a href="_String">string::String</a>&gt;, values: <a href="">vector</a>&lt;<a href="_String">string::String</a>&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>public</b> <b>fun</b> <a href="display.md#0x2_display_new_with_fields">new_with_fields</a>&lt;T: key&gt;(
    pub: &Publisher, fields: <a href="">vector</a>&lt;String&gt;, values: <a href="">vector</a>&lt;String&gt;, ctx: &<b>mut</b> TxContext
) {
    <b>let</b> len = <a href="_length">vector::length</a>(&fields);
    <b>assert</b>!(len == <a href="_length">vector::length</a>(&values), <a href="display.md#0x2_display_EVecLengthMismatch">EVecLengthMismatch</a>);

    <b>let</b> i = 0;
    <b>let</b> <a href="display.md#0x2_display">display</a> = <a href="display.md#0x2_display_new">new</a>&lt;T&gt;(pub, ctx);
    <b>while</b> (i &lt; len) {
        <a href="display.md#0x2_display_add_internal">add_internal</a>(&<b>mut</b> <a href="display.md#0x2_display">display</a>, *<a href="_borrow">vector::borrow</a>(&fields, i), *<a href="_borrow">vector::borrow</a>(&values, i));
        i = i + 1;
    };

    <a href="display.md#0x2_display_share">share</a>(<a href="display.md#0x2_display">display</a>)
}
</code></pre>



</details>

<a name="0x2_display_update_version"></a>

## Function `update_version`

Manually bump the version and emit an event with the updated version's contents.


<pre><code><b>public</b> entry <b>fun</b> <a href="display.md#0x2_display_update_version">update_version</a>&lt;T: key&gt;(pub: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>, d: &<b>mut</b> <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>public</b> <b>fun</b> <a href="display.md#0x2_display_update_version">update_version</a>&lt;T: key&gt;(
    pub: &Publisher, d: &<b>mut</b> <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;
) {
    <b>assert</b>!(is_package&lt;T&gt;(pub), <a href="display.md#0x2_display_ENotOwner">ENotOwner</a>);
    d.version = d.version + 1;
    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="display.md#0x2_display_VersionUpdated">VersionUpdated</a>&lt;T&gt; {
        version: d.version,
        fields: *&d.fields,
        id: <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&d.id),
    })
}
</code></pre>



</details>

<a name="0x2_display_add"></a>

## Function `add`

Sets a custom <code>name</code> field with the <code>value</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="display.md#0x2_display_add">add</a>&lt;T: key&gt;(pub: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>, d: &<b>mut</b> <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;, name: <a href="_String">string::String</a>, value: <a href="_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>public</b> <b>fun</b> <a href="display.md#0x2_display_add">add</a>&lt;T: key&gt;(pub: &Publisher, d: &<b>mut</b> <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;, name: String, value: String) {
    <b>assert</b>!(is_package&lt;T&gt;(pub), <a href="display.md#0x2_display_ENotOwner">ENotOwner</a>);
    <a href="display.md#0x2_display_add_internal">add_internal</a>(d, name, value)
}
</code></pre>



</details>

<a name="0x2_display_add_multiple"></a>

## Function `add_multiple`

Sets multiple <code>fields</code> with <code>values</code>.


<pre><code><b>public</b> entry <b>fun</b> <a href="display.md#0x2_display_add_multiple">add_multiple</a>&lt;T: key&gt;(pub: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>, d: &<b>mut</b> <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;, fields: <a href="">vector</a>&lt;<a href="_String">string::String</a>&gt;, values: <a href="">vector</a>&lt;<a href="_String">string::String</a>&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>public</b> <b>fun</b> <a href="display.md#0x2_display_add_multiple">add_multiple</a>&lt;T: key&gt;(
    pub: &Publisher, d: &<b>mut</b> <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;, fields: <a href="">vector</a>&lt;String&gt;, values: <a href="">vector</a>&lt;String&gt;
) {
    <b>let</b> len = <a href="_length">vector::length</a>(&fields);
    <b>assert</b>!(is_package&lt;T&gt;(pub), <a href="display.md#0x2_display_ENotOwner">ENotOwner</a>);
    <b>assert</b>!(len == <a href="_length">vector::length</a>(&values), <a href="display.md#0x2_display_EVecLengthMismatch">EVecLengthMismatch</a>);

    <b>let</b> i = 0;
    <b>while</b> (i &lt; 0) {
        <a href="display.md#0x2_display_add_internal">add_internal</a>(d, *<a href="_borrow">vector::borrow</a>(&fields, i), *<a href="_borrow">vector::borrow</a>(&values, i));
        i = i + 1;
    };
}
</code></pre>



</details>

<a name="0x2_display_edit"></a>

## Function `edit`

Change the value of the field.
TODO (long run): version changes;


<pre><code><b>public</b> entry <b>fun</b> <a href="display.md#0x2_display_edit">edit</a>&lt;T: key&gt;(pub: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>, d: &<b>mut</b> <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;, name: <a href="_String">string::String</a>, value: <a href="_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>public</b> <b>fun</b> <a href="display.md#0x2_display_edit">edit</a>&lt;T: key&gt;(pub: &Publisher, d: &<b>mut</b> <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;, name: String, value: String) {
    <b>assert</b>!(is_package&lt;T&gt;(pub), <a href="display.md#0x2_display_ENotOwner">ENotOwner</a>);
    <b>let</b> (_k, _v) = <a href="vec_map.md#0x2_vec_map_remove">vec_map::remove</a>(&<b>mut</b> d.fields, &name);
    <a href="display.md#0x2_display_add_internal">add_internal</a>(d, name, value)
}
</code></pre>



</details>

<a name="0x2_display_remove"></a>

## Function `remove`

Remove the key from the Display.


<pre><code><b>public</b> entry <b>fun</b> <a href="display.md#0x2_display_remove">remove</a>&lt;T: key&gt;(pub: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>, d: &<b>mut</b> <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;, name: <a href="_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code>entry <b>public</b> <b>fun</b> <a href="display.md#0x2_display_remove">remove</a>&lt;T: key&gt;(pub: &Publisher, d: &<b>mut</b> <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;, name: String) {
    <b>assert</b>!(is_package&lt;T&gt;(pub), <a href="display.md#0x2_display_ENotOwner">ENotOwner</a>);
    <a href="vec_map.md#0x2_vec_map_remove">vec_map::remove</a>(&<b>mut</b> d.fields, &name);
}
</code></pre>



</details>

<a name="0x2_display_version"></a>

## Function `version`

Read the <code>version</code> field.


<pre><code><b>public</b> <b>fun</b> <a href="display.md#0x2_display_version">version</a>&lt;T: key&gt;(d: &<a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;): u16
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="display.md#0x2_display_version">version</a>&lt;T: key&gt;(d: &<a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;): u16 {
    d.version
}
</code></pre>



</details>

<a name="0x2_display_fields"></a>

## Function `fields`

Read the <code>fields</code> field.


<pre><code><b>public</b> <b>fun</b> <a href="display.md#0x2_display_fields">fields</a>&lt;T: key&gt;(d: &<a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;): &<a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="_String">string::String</a>, <a href="_String">string::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="display.md#0x2_display_fields">fields</a>&lt;T: key&gt;(d: &<a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;): &VecMap&lt;String, String&gt; {
    &d.fields
}
</code></pre>



</details>

<a name="0x2_display_create_internal"></a>

## Function `create_internal`

Internal function to create a new <code><a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;</code>.


<pre><code><b>fun</b> <a href="display.md#0x2_display_create_internal">create_internal</a>&lt;T: key&gt;(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="display.md#0x2_display_create_internal">create_internal</a>&lt;T: key&gt;(ctx: &<b>mut</b> TxContext): <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt; {
    <b>let</b> uid = <a href="object.md#0x2_object_new">object::new</a>(ctx);

    <a href="event.md#0x2_event_emit">event::emit</a>(<a href="display.md#0x2_display_DisplayCreated">DisplayCreated</a>&lt;T&gt; {
        id: <a href="object.md#0x2_object_uid_to_inner">object::uid_to_inner</a>(&uid)
    });

    <a href="display.md#0x2_display_Display">Display</a> {
        id: uid,
        fields: <a href="vec_map.md#0x2_vec_map_empty">vec_map::empty</a>(),
        version: 0,
    }
}
</code></pre>



</details>

<a name="0x2_display_add_internal"></a>

## Function `add_internal`

Private method for inserting fields without security checks.


<pre><code><b>fun</b> <a href="display.md#0x2_display_add_internal">add_internal</a>&lt;T: key&gt;(d: &<b>mut</b> <a href="display.md#0x2_display_Display">display::Display</a>&lt;T&gt;, name: <a href="_String">string::String</a>, value: <a href="_String">string::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="display.md#0x2_display_add_internal">add_internal</a>&lt;T: key&gt;(d: &<b>mut</b> <a href="display.md#0x2_display_Display">Display</a>&lt;T&gt;, name: String, value: String) {
    <a href="vec_map.md#0x2_vec_map_insert">vec_map::insert</a>(&<b>mut</b> d.fields, name, value)
}
</code></pre>



</details>
