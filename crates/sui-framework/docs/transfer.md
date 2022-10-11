
<a name="0x2_transfer"></a>

# Module `0x2::transfer`



-  [Function `transfer`](#0x2_transfer_transfer)
-  [Function `transfer_to_object`](#0x2_transfer_transfer_to_object)
-  [Function `transfer_to_object_id`](#0x2_transfer_transfer_to_object_id)
-  [Function `freeze_object`](#0x2_transfer_freeze_object)
-  [Function `share_object`](#0x2_transfer_share_object)
-  [Function `transfer_internal`](#0x2_transfer_transfer_internal)


<pre><code><b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
</code></pre>



<a name="0x2_transfer_transfer"></a>

## Function `transfer`

Transfer ownership of <code>obj</code> to <code>recipient</code>. <code>obj</code> must have the
<code>key</code> attribute, which (in turn) ensures that <code>obj</code> has a globally
unique ID.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer">transfer</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>) {
    // TODO: emit <a href="event.md#0x2_event">event</a>
    <a href="transfer.md#0x2_transfer_transfer_internal">transfer_internal</a>(obj, recipient, <b>false</b>)
}
</code></pre>



</details>

<a name="0x2_transfer_transfer_to_object"></a>

## Function `transfer_to_object`

Transfer ownership of <code>obj</code> to another object <code>owner</code>.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_transfer_to_object">transfer_to_object</a>&lt;T: key, R: key&gt;(obj: T, owner: &<b>mut</b> R)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_transfer_to_object">transfer_to_object</a>&lt;T: key, R: key&gt;(obj: T, owner: &<b>mut</b> R) {
    <b>let</b> owner_id = <a href="object.md#0x2_object_id_address">object::id_address</a>(owner);
    <a href="transfer.md#0x2_transfer_transfer_internal">transfer_internal</a>(obj, owner_id, <b>true</b>);
}
</code></pre>



</details>

<a name="0x2_transfer_transfer_to_object_id"></a>

## Function `transfer_to_object_id`

Similar to transfer_to_object where we want to transfer an object to another object.
However, in the case when we haven't yet created the parent object (typically during
parent object construction), and all we have is just a parent object ID, we could
use this function to transfer an object to the parent object identified by its id.
Additionally, this API is useful for transfering to objects, outside of that object's
module. The object's module can expose a function that returns a reference to the object's
UID, <code>&<b>mut</b> UID</code>, which can then be used with this function. The mutable <code>&<b>mut</b> UID</code> reference
prevents child objects from being added to immutable objects (immutable objects cannot have
child objects).
The child object is specified in <code>obj</code>, and the parent object id is specified in <code>owner_id</code>.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_transfer_to_object_id">transfer_to_object_id</a>&lt;T: key&gt;(obj: T, owner_id: &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_transfer_to_object_id">transfer_to_object_id</a>&lt;T: key&gt;(obj: T, owner_id: &<b>mut</b> UID) {
    <a href="transfer.md#0x2_transfer_transfer_internal">transfer_internal</a>(obj, <a href="object.md#0x2_object_uid_to_address">object::uid_to_address</a>(owner_id), <b>true</b>);
}
</code></pre>



</details>

<a name="0x2_transfer_freeze_object"></a>

## Function `freeze_object`

Freeze <code>obj</code>. After freezing <code>obj</code> becomes immutable and can no
longer be transferred or mutated.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_freeze_object">freeze_object</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="transfer.md#0x2_transfer_freeze_object">freeze_object</a>&lt;T: key&gt;(obj: T);
</code></pre>



</details>

<a name="0x2_transfer_share_object"></a>

## Function `share_object`

Turn the given object into a mutable shared object that everyone
can access and mutate. This is irreversible, i.e. once an object
is shared, it will stay shared forever.
Shared mutable object is not yet fully supported in Sui, which is being
actively worked on and should be supported very soon.
https://github.com/MystenLabs/sui/issues/633
https://github.com/MystenLabs/sui/issues/681
This API is exposed to demonstrate how we may be able to use it to program
Move contracts that use shared objects.


<pre><code><b>public</b> <b>fun</b> <a href="transfer.md#0x2_transfer_share_object">share_object</a>&lt;T: key&gt;(obj: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="transfer.md#0x2_transfer_share_object">share_object</a>&lt;T: key&gt;(obj: T);
</code></pre>



</details>

<a name="0x2_transfer_transfer_internal"></a>

## Function `transfer_internal`



<pre><code><b>fun</b> <a href="transfer.md#0x2_transfer_transfer_internal">transfer_internal</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>, to_object: bool)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="transfer.md#0x2_transfer_transfer_internal">transfer_internal</a>&lt;T: key&gt;(obj: T, recipient: <b>address</b>, to_object: bool);
</code></pre>



</details>
