
<a name="0x2_borrow"></a>

# Module `0x2::borrow`

A simple library that enables hot-potato-locked borrow mechanics.

With Programmable transactions, it is possible to borrow a value within
a transaction, use it and put back in the end. Hot-potato <code><a href="borrow.md#0x2_borrow_Borrow">Borrow</a></code> makes
sure the object is returned and was not swapped for another one.


-  [Struct `Referent`](#0x2_borrow_Referent)
-  [Struct `Borrow`](#0x2_borrow_Borrow)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_borrow_new)
-  [Function `borrow`](#0x2_borrow_borrow)
-  [Function `put_back`](#0x2_borrow_put_back)
-  [Function `destroy`](#0x2_borrow_destroy)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_borrow_Referent"></a>

## Struct `Referent`

An object wrapping a <code>T</code> and providing the borrow API.


<pre><code><b>struct</b> <a href="borrow.md#0x2_borrow_Referent">Referent</a>&lt;T: store, key&gt; <b>has</b> store
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
<code>value: <a href="_Option">option::Option</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_borrow_Borrow"></a>

## Struct `Borrow`

A hot potato making sure the object is put back once borrowed.


<pre><code><b>struct</b> <a href="borrow.md#0x2_borrow_Borrow">Borrow</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>ref: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>obj: <a href="object.md#0x2_object_ID">object::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_borrow_EWrongBorrow"></a>

The <code><a href="borrow.md#0x2_borrow_Borrow">Borrow</a></code> does not match the <code><a href="borrow.md#0x2_borrow_Referent">Referent</a></code>.


<pre><code><b>const</b> <a href="borrow.md#0x2_borrow_EWrongBorrow">EWrongBorrow</a>: u64 = 0;
</code></pre>



<a name="0x2_borrow_EWrongValue"></a>

An attempt to swap the <code><a href="borrow.md#0x2_borrow_Referent">Referent</a>.value</code> with another object of the same type.


<pre><code><b>const</b> <a href="borrow.md#0x2_borrow_EWrongValue">EWrongValue</a>: u64 = 1;
</code></pre>



<a name="0x2_borrow_new"></a>

## Function `new`

Create a new <code><a href="borrow.md#0x2_borrow_Referent">Referent</a></code> struct


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow_new">new</a>&lt;T: store, key&gt;(value: T, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="borrow.md#0x2_borrow_Referent">borrow::Referent</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow_new">new</a>&lt;T: key + store&gt;(value: T, ctx: &<b>mut</b> TxContext): <a href="borrow.md#0x2_borrow_Referent">Referent</a>&lt;T&gt; {
    <a href="borrow.md#0x2_borrow_Referent">Referent</a> {
        id: <a href="object.md#0x2_object_new_id">object::new_id</a>(ctx),
        value: <a href="_some">option::some</a>(value)
    }
}
</code></pre>



</details>

<a name="0x2_borrow_borrow"></a>

## Function `borrow`

Borrow the <code>T</code> from the <code><a href="borrow.md#0x2_borrow_Referent">Referent</a></code> receiving the <code>T</code> and a <code><a href="borrow.md#0x2_borrow_Borrow">Borrow</a></code>
hot potato.


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow">borrow</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="borrow.md#0x2_borrow_Referent">borrow::Referent</a>&lt;T&gt;): (T, <a href="borrow.md#0x2_borrow_Borrow">borrow::Borrow</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow">borrow</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="borrow.md#0x2_borrow_Referent">Referent</a>&lt;T&gt;): (T, <a href="borrow.md#0x2_borrow_Borrow">Borrow</a>) {
    <b>let</b> value = <a href="_extract">option::extract</a>(&<b>mut</b> self.value);
    <b>let</b> id = <a href="object.md#0x2_object_id">object::id</a>(&value);

    (value, <a href="borrow.md#0x2_borrow_Borrow">Borrow</a> {
        ref: self.id,
        obj: id
    })
}
</code></pre>



</details>

<a name="0x2_borrow_put_back"></a>

## Function `put_back`

Put an object and the <code><a href="borrow.md#0x2_borrow_Borrow">Borrow</a></code> hot potato back.


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow_put_back">put_back</a>&lt;T: store, key&gt;(self: &<b>mut</b> <a href="borrow.md#0x2_borrow_Referent">borrow::Referent</a>&lt;T&gt;, value: T, <a href="borrow.md#0x2_borrow">borrow</a>: <a href="borrow.md#0x2_borrow_Borrow">borrow::Borrow</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow_put_back">put_back</a>&lt;T: key + store&gt;(self: &<b>mut</b> <a href="borrow.md#0x2_borrow_Referent">Referent</a>&lt;T&gt;, value: T, <a href="borrow.md#0x2_borrow">borrow</a>: <a href="borrow.md#0x2_borrow_Borrow">Borrow</a>) {
    <b>let</b> <a href="borrow.md#0x2_borrow_Borrow">Borrow</a> { ref, obj } = <a href="borrow.md#0x2_borrow">borrow</a>;

    <b>assert</b>!(<a href="object.md#0x2_object_id">object::id</a>(&value) == obj, <a href="borrow.md#0x2_borrow_EWrongValue">EWrongValue</a>);
    <b>assert</b>!(self.id == ref, <a href="borrow.md#0x2_borrow_EWrongBorrow">EWrongBorrow</a>);
    <a href="_fill">option::fill</a>(&<b>mut</b> self.value, value);
}
</code></pre>



</details>

<a name="0x2_borrow_destroy"></a>

## Function `destroy`

Unpack the <code><a href="borrow.md#0x2_borrow_Referent">Referent</a></code> struct and return the value.


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow_destroy">destroy</a>&lt;T: store, key&gt;(self: <a href="borrow.md#0x2_borrow_Referent">borrow::Referent</a>&lt;T&gt;): T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="borrow.md#0x2_borrow_destroy">destroy</a>&lt;T: key + store&gt;(self: <a href="borrow.md#0x2_borrow_Referent">Referent</a>&lt;T&gt;): T {
    <b>let</b> <a href="borrow.md#0x2_borrow_Referent">Referent</a> { id: _, value } = self;
    <a href="_destroy_some">option::destroy_some</a>(value)
}
</code></pre>



</details>
