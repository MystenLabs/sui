
<a name="0x2_publisher"></a>

# Module `0x2::publisher`

Module that allows creation and proof of publishing.
Based on the type name reflection; requires an OTW to claim
in the module initializer.


-  [Resource `Publisher`](#0x2_publisher_Publisher)
-  [Constants](#@Constants_0)
-  [Function `claim`](#0x2_publisher_claim)
-  [Function `claim_and_keep`](#0x2_publisher_claim_and_keep)
-  [Function `burn`](#0x2_publisher_burn)
-  [Function `is_package`](#0x2_publisher_is_package)
-  [Function `is_module`](#0x2_publisher_is_module)


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
<b>use</b> <a href="">0x1::type_name</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="types.md#0x2_types">0x2::types</a>;
</code></pre>



<a name="0x2_publisher_Publisher"></a>

## Resource `Publisher`

This type can only be created in the transaction that
generates a module, by consuming its one-time witness, so it
can be used to identify the address that published the package
a type originated from.


<pre><code><b>struct</b> <a href="publisher.md#0x2_publisher_Publisher">Publisher</a> <b>has</b> store, key
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
<code>type: <a href="_String">ascii::String</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_publisher_ASCII_COLON"></a>

ASCII character code for the <code>:</code> (colon) symbol.


<pre><code><b>const</b> <a href="publisher.md#0x2_publisher_ASCII_COLON">ASCII_COLON</a>: u8 = 58;
</code></pre>



<a name="0x2_publisher_ENotOneTimeWitness"></a>

Tried to claim ownership using a type that isn't a one-time witness.


<pre><code><b>const</b> <a href="publisher.md#0x2_publisher_ENotOneTimeWitness">ENotOneTimeWitness</a>: u64 = 0;
</code></pre>



<a name="0x2_publisher_claim"></a>

## Function `claim`

Claim a Publisher object.
Requires a One-Time-Witness to


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_claim">claim</a>&lt;OTW: drop&gt;(otw: OTW, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_claim">claim</a>&lt;OTW: drop&gt;(otw: OTW, ctx: &<b>mut</b> TxContext): <a href="publisher.md#0x2_publisher_Publisher">Publisher</a> {
    <b>assert</b>!(<a href="types.md#0x2_types_is_one_time_witness">types::is_one_time_witness</a>(&otw), <a href="publisher.md#0x2_publisher_ENotOneTimeWitness">ENotOneTimeWitness</a>);

    <a href="publisher.md#0x2_publisher_Publisher">Publisher</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        type: <a href="_into_string">type_name::into_string</a>(<a href="_get">type_name::get</a>&lt;OTW&gt;()),
    }
}
</code></pre>



</details>

<a name="0x2_publisher_claim_and_keep"></a>

## Function `claim_and_keep`

Claim a Publisher object and send it to transaction sender.
Since this function can only be called in the module initializer,
the sender is the publisher.


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_claim_and_keep">claim_and_keep</a>&lt;OTW: drop&gt;(otw: OTW, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_claim_and_keep">claim_and_keep</a>&lt;OTW: drop&gt;(otw: OTW, ctx: &<b>mut</b> TxContext) {
    sui::transfer::transfer(<a href="publisher.md#0x2_publisher_claim">claim</a>(otw, ctx), sender(ctx))
}
</code></pre>



</details>

<a name="0x2_publisher_burn"></a>

## Function `burn`

Destroy a Publisher object effectively removing all privileges
associated with it.


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_burn">burn</a>(<a href="publisher.md#0x2_publisher">publisher</a>: <a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_burn">burn</a>(<a href="publisher.md#0x2_publisher">publisher</a>: <a href="publisher.md#0x2_publisher_Publisher">Publisher</a>) {
    <b>let</b> <a href="publisher.md#0x2_publisher_Publisher">Publisher</a> { id, type: _ } = <a href="publisher.md#0x2_publisher">publisher</a>;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
}
</code></pre>



</details>

<a name="0x2_publisher_is_package"></a>

## Function `is_package`

Check whether type belongs to the same package as the publisher object.


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_is_package">is_package</a>&lt;T&gt;(<a href="publisher.md#0x2_publisher">publisher</a>: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_is_package">is_package</a>&lt;T&gt;(<a href="publisher.md#0x2_publisher">publisher</a>: &<a href="publisher.md#0x2_publisher_Publisher">Publisher</a>): bool {
    <b>let</b> this = <a href="_as_bytes">ascii::as_bytes</a>(&<a href="publisher.md#0x2_publisher">publisher</a>.type);
    <b>let</b> their = <a href="_as_bytes">ascii::as_bytes</a>(<a href="_borrow_string">type_name::borrow_string</a>(&<a href="_get">type_name::get</a>&lt;T&gt;()));

    <b>let</b> i = 0;

    // 40 bytes =&gt; length of the HEX encoded <a href="">string</a>
    <b>while</b> (i &lt; 40) {
        <b>if</b> (<a href="_borrow">vector::borrow</a>&lt;u8&gt;(this, i) != <a href="_borrow">vector::borrow</a>&lt;u8&gt;(their, i)) {
            <b>return</b> <b>false</b>
        };

        i = i + 1;
    };

    <b>true</b>
}
</code></pre>



</details>

<a name="0x2_publisher_is_module"></a>

## Function `is_module`

Check whether a type belogs to the same module as the publisher object.


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_is_module">is_module</a>&lt;T&gt;(<a href="publisher.md#0x2_publisher">publisher</a>: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_is_module">is_module</a>&lt;T&gt;(<a href="publisher.md#0x2_publisher">publisher</a>: &<a href="publisher.md#0x2_publisher_Publisher">Publisher</a>): bool {
    <b>if</b> (!<a href="publisher.md#0x2_publisher_is_package">is_package</a>&lt;T&gt;(<a href="publisher.md#0x2_publisher">publisher</a>)) {
        <b>return</b> <b>false</b>
    };

    <b>let</b> this = <a href="_as_bytes">ascii::as_bytes</a>(&<a href="publisher.md#0x2_publisher">publisher</a>.type);
    <b>let</b> their = <a href="_as_bytes">ascii::as_bytes</a>(<a href="_borrow_string">type_name::borrow_string</a>(&<a href="_get">type_name::get</a>&lt;T&gt;()));

    // 42 bytes =&gt; length of the HEX encoded <a href="">string</a> + :: (double colon)
    <b>let</b> i = 42;
    <b>loop</b> {
        <b>let</b> left = <a href="_borrow">vector::borrow</a>&lt;u8&gt;(this, i);
        <b>let</b> right = <a href="_borrow">vector::borrow</a>&lt;u8&gt;(their, i);

        <b>if</b> (left == &<a href="publisher.md#0x2_publisher_ASCII_COLON">ASCII_COLON</a> && right == &<a href="publisher.md#0x2_publisher_ASCII_COLON">ASCII_COLON</a>) {
            <b>return</b> <b>true</b>
        };

        <b>if</b> (left != right) {
            <b>return</b> <b>false</b>
        };

        i = i + 1;
    }
}
</code></pre>



</details>
