
<a name="0x2_package"></a>

# Module `0x2::package`

Functions for operating on Move packages from within Move:
- Creating proof-of-publish objects from one-time witnesses
- Administering package upgrades through upgrade policies.


-  [Resource `Publisher`](#0x2_package_Publisher)
-  [Constants](#@Constants_0)
-  [Function `claim`](#0x2_package_claim)
-  [Function `claim_and_keep`](#0x2_package_claim_and_keep)
-  [Function `burn_publisher`](#0x2_package_burn_publisher)
-  [Function `from_package`](#0x2_package_from_package)
-  [Function `from_module`](#0x2_package_from_module)
-  [Function `published_module`](#0x2_package_published_module)
-  [Function `published_package`](#0x2_package_published_package)


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
<b>use</b> <a href="">0x1::type_name</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="types.md#0x2_types">0x2::types</a>;
</code></pre>



<a name="0x2_package_Publisher"></a>

## Resource `Publisher`

This type can only be created in the transaction that
generates a module, by consuming its one-time witness, so it
can be used to identify the address that published the package
a type originated from.


<pre><code><b>struct</b> <a href="package.md#0x2_package_Publisher">Publisher</a> <b>has</b> store, key
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
<code><a href="package.md#0x2_package">package</a>: <a href="_String">ascii::String</a></code>
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


<a name="0x2_package_ENotOneTimeWitness"></a>

Tried to create a <code><a href="package.md#0x2_package_Publisher">Publisher</a></code> using a type that isn't a
one-time witness.


<pre><code><b>const</b> <a href="package.md#0x2_package_ENotOneTimeWitness">ENotOneTimeWitness</a>: u64 = 0;
</code></pre>



<a name="0x2_package_claim"></a>

## Function `claim`

Claim a Publisher object.
Requires a One-Time-Witness to prove ownership. Due to this
constraint there can be only one Publisher object per module
but multiple per package (!).


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_claim">claim</a>&lt;OTW: drop&gt;(otw: OTW, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="package.md#0x2_package_Publisher">package::Publisher</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_claim">claim</a>&lt;OTW: drop&gt;(otw: OTW, ctx: &<b>mut</b> TxContext): <a href="package.md#0x2_package_Publisher">Publisher</a> {
    <b>assert</b>!(<a href="types.md#0x2_types_is_one_time_witness">types::is_one_time_witness</a>(&otw), <a href="package.md#0x2_package_ENotOneTimeWitness">ENotOneTimeWitness</a>);

    <b>let</b> type = <a href="_get">type_name::get</a>&lt;OTW&gt;();

    <a href="package.md#0x2_package_Publisher">Publisher</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        <a href="package.md#0x2_package">package</a>: <a href="_get_address">type_name::get_address</a>(&type),
        module_name: <a href="_get_module">type_name::get_module</a>(&type),
    }
}
</code></pre>



</details>

<a name="0x2_package_claim_and_keep"></a>

## Function `claim_and_keep`

Claim a Publisher object and send it to transaction sender.
Since this function can only be called in the module initializer,
the sender is the publisher.


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_claim_and_keep">claim_and_keep</a>&lt;OTW: drop&gt;(otw: OTW, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_claim_and_keep">claim_and_keep</a>&lt;OTW: drop&gt;(otw: OTW, ctx: &<b>mut</b> TxContext) {
    sui::transfer::transfer(<a href="package.md#0x2_package_claim">claim</a>(otw, ctx), sender(ctx))
}
</code></pre>



</details>

<a name="0x2_package_burn_publisher"></a>

## Function `burn_publisher`

Destroy a Publisher object effectively removing all privileges
associated with it.


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_burn_publisher">burn_publisher</a>(self: <a href="package.md#0x2_package_Publisher">package::Publisher</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_burn_publisher">burn_publisher</a>(self: <a href="package.md#0x2_package_Publisher">Publisher</a>) {
    <b>let</b> <a href="package.md#0x2_package_Publisher">Publisher</a> { id, <a href="package.md#0x2_package">package</a>: _, module_name: _ } = self;
    <a href="object.md#0x2_object_delete">object::delete</a>(id);
}
</code></pre>



</details>

<a name="0x2_package_from_package"></a>

## Function `from_package`

Check whether type belongs to the same package as the publisher object.


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_from_package">from_package</a>&lt;T&gt;(self: &<a href="package.md#0x2_package_Publisher">package::Publisher</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_from_package">from_package</a>&lt;T&gt;(self: &<a href="package.md#0x2_package_Publisher">Publisher</a>): bool {
    <b>let</b> type = <a href="_get">type_name::get</a>&lt;T&gt;();

    (<a href="_get_address">type_name::get_address</a>(&type) == self.<a href="package.md#0x2_package">package</a>)
}
</code></pre>



</details>

<a name="0x2_package_from_module"></a>

## Function `from_module`

Check whether a type belongs to the same module as the publisher object.


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_from_module">from_module</a>&lt;T&gt;(self: &<a href="package.md#0x2_package_Publisher">package::Publisher</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_from_module">from_module</a>&lt;T&gt;(self: &<a href="package.md#0x2_package_Publisher">Publisher</a>): bool {
    <b>let</b> type = <a href="_get">type_name::get</a>&lt;T&gt;();

    (<a href="_get_address">type_name::get_address</a>(&type) == self.<a href="package.md#0x2_package">package</a>)
        && (<a href="_get_module">type_name::get_module</a>(&type) == self.module_name)
}
</code></pre>



</details>

<a name="0x2_package_published_module"></a>

## Function `published_module`

Read the name of the module.


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_published_module">published_module</a>(self: &<a href="package.md#0x2_package_Publisher">package::Publisher</a>): &<a href="_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_published_module">published_module</a>(self: &<a href="package.md#0x2_package_Publisher">Publisher</a>): &String {
    &self.module_name
}
</code></pre>



</details>

<a name="0x2_package_published_package"></a>

## Function `published_package`

Read the package address string.


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_published_package">published_package</a>(self: &<a href="package.md#0x2_package_Publisher">package::Publisher</a>): &<a href="_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="package.md#0x2_package_published_package">published_package</a>(self: &<a href="package.md#0x2_package_Publisher">Publisher</a>): &String {
    &self.<a href="package.md#0x2_package">package</a>
}
</code></pre>



</details>
