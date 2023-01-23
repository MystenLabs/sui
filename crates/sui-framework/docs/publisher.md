
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
-  [Function `module_name`](#0x2_publisher_module_name)
-  [Function `package`](#0x2_publisher_package)


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
<code>package: <a href="_String">ascii::String</a></code>
</dt>
<dd>

</dd>
<dt>
<code>module_name: <a href="_String">ascii::String</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_publisher_ENotOneTimeWitness"></a>

Tried to claim ownership using a type that isn't a one-time witness.


<pre><code><b>const</b> <a href="publisher.md#0x2_publisher_ENotOneTimeWitness">ENotOneTimeWitness</a>: u64 = 0;
</code></pre>



<a name="0x2_publisher_claim"></a>

## Function `claim`

Claim a Publisher object.
Requires a One-Time-Witness to prove ownership. Due to this constraint
there can be only one Publisher object per module but multiple per package (!).


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_claim">claim</a>&lt;OTW: drop&gt;(otw: OTW, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_claim">claim</a>&lt;OTW: drop&gt;(otw: OTW, ctx: &<b>mut</b> TxContext): <a href="publisher.md#0x2_publisher_Publisher">Publisher</a> {
    <b>assert</b>!(<a href="types.md#0x2_types_is_one_time_witness">types::is_one_time_witness</a>(&otw), <a href="publisher.md#0x2_publisher_ENotOneTimeWitness">ENotOneTimeWitness</a>);

    <b>let</b> type = <a href="_get">type_name::get</a>&lt;OTW&gt;();

    <a href="publisher.md#0x2_publisher_Publisher">Publisher</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        package: <a href="_get_address">type_name::get_address</a>(&type),
        module_name: <a href="_get_module">type_name::get_module</a>(&type),
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


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_burn">burn</a>(self: <a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_burn">burn</a>(self: <a href="publisher.md#0x2_publisher_Publisher">Publisher</a>) {
    <b>let</b> <a href="publisher.md#0x2_publisher_Publisher">Publisher</a> { id, package: _, module_name: _ } = self;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
}
</code></pre>



</details>

<a name="0x2_publisher_is_package"></a>

## Function `is_package`

Check whether type belongs to the same package as the publisher object.


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_is_package">is_package</a>&lt;T&gt;(self: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_is_package">is_package</a>&lt;T&gt;(self: &<a href="publisher.md#0x2_publisher_Publisher">Publisher</a>): bool {
    <b>let</b> type = <a href="_get">type_name::get</a>&lt;T&gt;();

    (<a href="_get_address">type_name::get_address</a>(&type) == self.package)
}
</code></pre>



</details>

<a name="0x2_publisher_is_module"></a>

## Function `is_module`

Check whether a type belongs to the same module as the publisher object.


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_is_module">is_module</a>&lt;T&gt;(self: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_is_module">is_module</a>&lt;T&gt;(self: &<a href="publisher.md#0x2_publisher_Publisher">Publisher</a>): bool {
    <b>let</b> type = <a href="_get">type_name::get</a>&lt;T&gt;();

    (<a href="_get_address">type_name::get_address</a>(&type) == self.package)
        && (<a href="_get_module">type_name::get_module</a>(&type) == self.module_name)
}
</code></pre>



</details>

<a name="0x2_publisher_module_name"></a>

## Function `module_name`

Read the name of the module.


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_module_name">module_name</a>(self: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>): &<a href="_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_module_name">module_name</a>(self: &<a href="publisher.md#0x2_publisher_Publisher">Publisher</a>): &String {
    &self.module_name
}
</code></pre>



</details>

<a name="0x2_publisher_package"></a>

## Function `package`

Read the package address string.


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_package">package</a>(self: &<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>): &<a href="_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="publisher.md#0x2_publisher_package">package</a>(self: &<a href="publisher.md#0x2_publisher_Publisher">Publisher</a>): &String {
    &self.package
}
</code></pre>



</details>
