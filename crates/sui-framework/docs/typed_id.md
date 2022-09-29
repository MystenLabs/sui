
<a name="0x2_typed_id"></a>

# Module `0x2::typed_id`

Typed wrappers around Sui object IDs
While not always necessary, this is helpful for indicating the type of an object, particularly
when storing its ID in another object.
Additionally, it can be helpful for disambiguating between different IDs in an object.
For example
```
struct MyObject has key {
id: VersionedID,
child1: TypedID<A>,
child2: TypedID<B>,
}
```
We then know that <code>child1</code> is an ID for an object of type <code>A</code> and that <code>child2</code> is an <code>ID</code>
of an object of type <code>B</code>


-  [Struct `TypedID`](#0x2_typed_id_TypedID)
-  [Function `new`](#0x2_typed_id_new)
-  [Function `as_id`](#0x2_typed_id_as_id)
-  [Function `to_id`](#0x2_typed_id_to_id)
-  [Function `equals_object`](#0x2_typed_id_equals_object)


<pre><code><b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
</code></pre>



<a name="0x2_typed_id_TypedID"></a>

## Struct `TypedID`

An ID of an of type <code>T</code>. See <code>ID</code> for more details
By construction, it is guaranteed that the <code>ID</code> represents an object of type <code>T</code>


<pre><code><b>struct</b> <a href="typed_id.md#0x2_typed_id_TypedID">TypedID</a>&lt;T: key&gt; <b>has</b> <b>copy</b>, drop, store
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

<a name="0x2_typed_id_new"></a>

## Function `new`

Get the underlying <code>ID</code> of <code>obj</code>, and remember the type


<pre><code><b>public</b> <b>fun</b> <a href="typed_id.md#0x2_typed_id_new">new</a>&lt;T: key&gt;(obj: &T): <a href="typed_id.md#0x2_typed_id_TypedID">typed_id::TypedID</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="typed_id.md#0x2_typed_id_new">new</a>&lt;T: key&gt;(obj: &T): <a href="typed_id.md#0x2_typed_id_TypedID">TypedID</a>&lt;T&gt; {
    <a href="typed_id.md#0x2_typed_id_TypedID">TypedID</a> { id: <a href="object.md#0x2_object_id">object::id</a>(obj) }
}
</code></pre>



</details>

<a name="0x2_typed_id_as_id"></a>

## Function `as_id`

Borrow the inner <code>ID</code> of <code><a href="typed_id.md#0x2_typed_id">typed_id</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="typed_id.md#0x2_typed_id_as_id">as_id</a>&lt;T: key&gt;(<a href="typed_id.md#0x2_typed_id">typed_id</a>: &<a href="typed_id.md#0x2_typed_id_TypedID">typed_id::TypedID</a>&lt;T&gt;): &<a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="typed_id.md#0x2_typed_id_as_id">as_id</a>&lt;T: key&gt;(<a href="typed_id.md#0x2_typed_id">typed_id</a>: &<a href="typed_id.md#0x2_typed_id_TypedID">TypedID</a>&lt;T&gt;): &ID {
    &<a href="typed_id.md#0x2_typed_id">typed_id</a>.id
}
</code></pre>



</details>

<a name="0x2_typed_id_to_id"></a>

## Function `to_id`

Get the inner <code>ID</code> of <code><a href="typed_id.md#0x2_typed_id">typed_id</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="typed_id.md#0x2_typed_id_to_id">to_id</a>&lt;T: key&gt;(<a href="typed_id.md#0x2_typed_id">typed_id</a>: <a href="typed_id.md#0x2_typed_id_TypedID">typed_id::TypedID</a>&lt;T&gt;): <a href="object.md#0x2_object_ID">object::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="typed_id.md#0x2_typed_id_to_id">to_id</a>&lt;T: key&gt;(<a href="typed_id.md#0x2_typed_id">typed_id</a>: <a href="typed_id.md#0x2_typed_id_TypedID">TypedID</a>&lt;T&gt;): ID {
    <b>let</b> <a href="typed_id.md#0x2_typed_id_TypedID">TypedID</a> { id } = <a href="typed_id.md#0x2_typed_id">typed_id</a>;
    id
}
</code></pre>



</details>

<a name="0x2_typed_id_equals_object"></a>

## Function `equals_object`

Check that underlying <code>ID</code> in the <code><a href="typed_id.md#0x2_typed_id">typed_id</a></code> equals the objects ID


<pre><code><b>public</b> <b>fun</b> <a href="typed_id.md#0x2_typed_id_equals_object">equals_object</a>&lt;T: key&gt;(<a href="typed_id.md#0x2_typed_id">typed_id</a>: &<a href="typed_id.md#0x2_typed_id_TypedID">typed_id::TypedID</a>&lt;T&gt;, obj: &T): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="typed_id.md#0x2_typed_id_equals_object">equals_object</a>&lt;T: key&gt;(<a href="typed_id.md#0x2_typed_id">typed_id</a>: &<a href="typed_id.md#0x2_typed_id_TypedID">TypedID</a>&lt;T&gt;, obj: &T): bool {
    <a href="typed_id.md#0x2_typed_id">typed_id</a>.id == <a href="object.md#0x2_object_id">object::id</a>(obj)
}
</code></pre>



</details>
