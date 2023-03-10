
<a name="0x2_recipient"></a>

# Module `0x2::recipient`



-  [Struct `Recipient`](#0x2_recipient_Recipient)
-  [Constants](#@Constants_0)
-  [Function `new`](#0x2_recipient_new)
-  [Function `destroy`](#0x2_recipient_destroy)


<pre><code></code></pre>



<a name="0x2_recipient_Recipient"></a>

## Struct `Recipient`

The recipient of a transfer


<pre><code><b>struct</b> <a href="recipient.md#0x2_recipient_Recipient">Recipient</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>kind: u8</code>
</dt>
<dd>
 The kind of recipient, currently only an address recipient is supported,
 but object recipients will be supported in the future
</dd>
<dt>
<code>value: <b>address</b></code>
</dt>
<dd>
 The underlying value for the recipient, ID or address
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_recipient_ADDRESS_RECIPIENT_KIND"></a>

The recipient is an address


<pre><code><b>const</b> <a href="recipient.md#0x2_recipient_ADDRESS_RECIPIENT_KIND">ADDRESS_RECIPIENT_KIND</a>: u8 = 0;
</code></pre>



<a name="0x2_recipient_ENotAnAddress"></a>

The recipient is not an address


<pre><code><b>const</b> <a href="recipient.md#0x2_recipient_ENotAnAddress">ENotAnAddress</a>: u64 = 0;
</code></pre>



<a name="0x2_recipient_ENotAnObject"></a>

Currently unused. The recipient is not an object


<pre><code><b>const</b> <a href="recipient.md#0x2_recipient_ENotAnObject">ENotAnObject</a>: u64 = 1;
</code></pre>



<a name="0x2_recipient_OBJECT_RECIPIENT_KIND"></a>

Currently unused. The recipient is an object


<pre><code><b>const</b> <a href="recipient.md#0x2_recipient_OBJECT_RECIPIENT_KIND">OBJECT_RECIPIENT_KIND</a>: u8 = 1;
</code></pre>



<a name="0x2_recipient_new"></a>

## Function `new`

internal construction of a Recipient


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="recipient.md#0x2_recipient_new">new</a>(kind: u8, value: <b>address</b>): <a href="recipient.md#0x2_recipient_Recipient">recipient::Recipient</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="recipient.md#0x2_recipient_new">new</a>(kind: u8, value: <b>address</b>): <a href="recipient.md#0x2_recipient_Recipient">Recipient</a> {
    <a href="recipient.md#0x2_recipient_Recipient">Recipient</a> { kind, value }
}
</code></pre>



</details>

<a name="0x2_recipient_destroy"></a>

## Function `destroy`

internal deconstruction of a Recipient


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="recipient.md#0x2_recipient_destroy">destroy</a>(<a href="recipient.md#0x2_recipient">recipient</a>: <a href="recipient.md#0x2_recipient_Recipient">recipient::Recipient</a>): (u8, <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="recipient.md#0x2_recipient_destroy">destroy</a>(<a href="recipient.md#0x2_recipient">recipient</a>: <a href="recipient.md#0x2_recipient_Recipient">Recipient</a>): (u8, <b>address</b>) {
    <b>let</b> <a href="recipient.md#0x2_recipient_Recipient">Recipient</a> { kind, value } = <a href="recipient.md#0x2_recipient">recipient</a>;
    (kind, value)
}
</code></pre>



</details>
