
<a name="0x2_collectible"></a>

# Module `0x2::collectible`

Small and simple implementation for the common collectible type.
Contains a basic set of fields, the only required one of which is <code>img_url</code>.

Other fields can be omitted by using an <code><a href="_none">option::none</a>()</code>.
Custom metadata can be created and passed into the <code><a href="collectible.md#0x2_collectible_Collectible">Collectible</a></code> but that would
require additional work on the creator side to set up metadata creation methods.


-  [Resource `Collectible`](#0x2_collectible_Collectible)
-  [Resource `CollectionCreatorCap`](#0x2_collectible_CollectionCreatorCap)
-  [Constants](#@Constants_0)
-  [Function `create_collection`](#0x2_collectible_create_collection)
-  [Function `mint`](#0x2_collectible_mint)
-  [Function `batch_mint`](#0x2_collectible_batch_mint)
-  [Function `uid_mut`](#0x2_collectible_uid_mut)
-  [Function `img_url`](#0x2_collectible_img_url)
-  [Function `name`](#0x2_collectible_name)
-  [Function `description`](#0x2_collectible_description)
-  [Function `creator`](#0x2_collectible_creator)
-  [Function `meta`](#0x2_collectible_meta)
-  [Function `pop_or_none`](#0x2_collectible_pop_or_none)


<pre><code><b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::string</a>;
<b>use</b> <a href="display.md#0x2_display">0x2::display</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="publisher.md#0x2_publisher">0x2::publisher</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="types.md#0x2_types">0x2::types</a>;
</code></pre>



<a name="0x2_collectible_Collectible"></a>

## Resource `Collectible`

Basic collectible - should contain only unique information (eg
if all collectibles have the same description, it should be put
into the Display to apply to all of the objects of this type, and
not in every object).


<pre><code><b>struct</b> <a href="collectible.md#0x2_collectible_Collectible">Collectible</a>&lt;T: store&gt; <b>has</b> store, key
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
<code>img_url: <a href="_String">string::String</a></code>
</dt>
<dd>
 The only required parameter for the Collectible.
 Should only contain a unique part of the URL to be used in the
 template engine in the <code>Display</code> and save gas and storage costs.
</dd>
<dt>
<code>name: <a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>description: <a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>creator: <a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>meta: <a href="_Option">option::Option</a>&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_collectible_CollectionCreatorCap"></a>

## Resource `CollectionCreatorCap`

Capability granted to the collection creator. Is guaranteed to be one
per <code>T</code> in the <code>create_collection</code> function.
Contains the cap - maximum amount of Collectibles minted.


<pre><code><b>struct</b> <a href="collectible.md#0x2_collectible_CollectionCreatorCap">CollectionCreatorCap</a>&lt;T: store&gt; <b>has</b> store, key
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
<code>max_supply: <a href="_Option">option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>minted: u64</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_collectible_ENotOneTimeWitness"></a>

For when a witness type passed is not an OTW.


<pre><code><b>const</b> <a href="collectible.md#0x2_collectible_ENotOneTimeWitness">ENotOneTimeWitness</a>: u64 = 0;
</code></pre>



<a name="0x2_collectible_ECapReached"></a>

For when maximum size of the Collection is reached - minting forbidden.


<pre><code><b>const</b> <a href="collectible.md#0x2_collectible_ECapReached">ECapReached</a>: u64 = 2;
</code></pre>



<a name="0x2_collectible_EModuleDoesNotContainT"></a>

For when the type <code>T</code> is not from the same module as the OTW.


<pre><code><b>const</b> <a href="collectible.md#0x2_collectible_EModuleDoesNotContainT">EModuleDoesNotContainT</a>: u64 = 1;
</code></pre>



<a name="0x2_collectible_EWrongCreatorsLength"></a>

For when Creators length does not match <code>img_urls</code> length


<pre><code><b>const</b> <a href="collectible.md#0x2_collectible_EWrongCreatorsLength">EWrongCreatorsLength</a>: u64 = 5;
</code></pre>



<a name="0x2_collectible_EWrongDescriptionsLength"></a>

For when Descriptions length does not match <code>img_urls</code> length


<pre><code><b>const</b> <a href="collectible.md#0x2_collectible_EWrongDescriptionsLength">EWrongDescriptionsLength</a>: u64 = 4;
</code></pre>



<a name="0x2_collectible_EWrongMetadatasLength"></a>

For when Metadatas length does not match <code>img_urls</code> length


<pre><code><b>const</b> <a href="collectible.md#0x2_collectible_EWrongMetadatasLength">EWrongMetadatasLength</a>: u64 = 6;
</code></pre>



<a name="0x2_collectible_EWrongNamesLength"></a>

For when Names length does not match <code>img_urls</code> length


<pre><code><b>const</b> <a href="collectible.md#0x2_collectible_EWrongNamesLength">EWrongNamesLength</a>: u64 = 3;
</code></pre>



<a name="0x2_collectible_create_collection"></a>

## Function `create_collection`

Create a new collection and receive <code><a href="collectible.md#0x2_collectible_CollectionCreatorCap">CollectionCreatorCap</a></code> with a <code>Publisher</code>.

To make sure that a collection is created only once, we require an OTW;
but since the collection also requires a Publisher to set up the display,
we create the Publisher object here as well.

Type parameter <code>T</code> is phantom; so we constrain it via <code>Publisher</code> to be
defined in the same module as the OTW. Aborts otherwise.


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_create_collection">create_collection</a>&lt;OTW: drop, T: store&gt;(otw: OTW, max_supply: <a href="_Option">option::Option</a>&lt;u64&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): (<a href="publisher.md#0x2_publisher_Publisher">publisher::Publisher</a>, <a href="display.md#0x2_display_Display">display::Display</a>&lt;<a href="collectible.md#0x2_collectible_Collectible">collectible::Collectible</a>&lt;T&gt;&gt;, <a href="collectible.md#0x2_collectible_CollectionCreatorCap">collectible::CollectionCreatorCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_create_collection">create_collection</a>&lt;OTW: drop, T: store&gt;(
    otw: OTW, max_supply: Option&lt;u64&gt;, ctx: &<b>mut</b> TxContext
): (
    Publisher,
    Display&lt;<a href="collectible.md#0x2_collectible_Collectible">Collectible</a>&lt;T&gt;&gt;,
    <a href="collectible.md#0x2_collectible_CollectionCreatorCap">CollectionCreatorCap</a>&lt;T&gt;
) {
    <b>assert</b>!(sui::types::is_one_time_witness(&otw), <a href="collectible.md#0x2_collectible_ENotOneTimeWitness">ENotOneTimeWitness</a>);

    <b>let</b> pub = <a href="publisher.md#0x2_publisher_claim">publisher::claim</a>(otw, ctx);
    <b>assert</b>!(<a href="publisher.md#0x2_publisher_is_module">publisher::is_module</a>&lt;T&gt;(&pub), <a href="collectible.md#0x2_collectible_EModuleDoesNotContainT">EModuleDoesNotContainT</a>);

    (
        pub,
        <a href="display.md#0x2_display_new_protected">display::new_protected</a>&lt;<a href="collectible.md#0x2_collectible_Collectible">Collectible</a>&lt;T&gt;&gt;(ctx),
        <a href="collectible.md#0x2_collectible_CollectionCreatorCap">CollectionCreatorCap</a>&lt;T&gt; {
            id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
            minted: 0,
            max_supply,
        }
    )
}
</code></pre>



</details>

<a name="0x2_collectible_mint"></a>

## Function `mint`

Mint a single Collectible specifying the fields.
Can only be performed by the owner of the <code><a href="collectible.md#0x2_collectible_CollectionCreatorCap">CollectionCreatorCap</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_mint">mint</a>&lt;T: store&gt;(cap: &<b>mut</b> <a href="collectible.md#0x2_collectible_CollectionCreatorCap">collectible::CollectionCreatorCap</a>&lt;T&gt;, img_url: <a href="_String">string::String</a>, name: <a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;, description: <a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;, creator: <a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;, meta: <a href="_Option">option::Option</a>&lt;T&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="collectible.md#0x2_collectible_Collectible">collectible::Collectible</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_mint">mint</a>&lt;T: store&gt;(
    cap: &<b>mut</b> <a href="collectible.md#0x2_collectible_CollectionCreatorCap">CollectionCreatorCap</a>&lt;T&gt;,
    img_url: String,
    name: Option&lt;String&gt;,
    description: Option&lt;String&gt;,
    creator: Option&lt;String&gt;,
    meta: Option&lt;T&gt;,
    ctx: &<b>mut</b> TxContext
): <a href="collectible.md#0x2_collectible_Collectible">Collectible</a>&lt;T&gt; {
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(&cap.max_supply) || *<a href="_borrow">option::borrow</a>(&cap.max_supply) &gt; cap.minted, <a href="collectible.md#0x2_collectible_ECapReached">ECapReached</a>);
    cap.minted = cap.minted + 1;

    <a href="collectible.md#0x2_collectible_Collectible">Collectible</a> {
        id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
        img_url,
        name,
        description,
        creator,
        meta
    }
}
</code></pre>



</details>

<a name="0x2_collectible_batch_mint"></a>

## Function `batch_mint`

Batch mint multiple Collectibles at once.
Any of the fields can be omitted by supplying a <code>none()</code>.

Field for custom metadata can be used for custom Collectibles.


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_batch_mint">batch_mint</a>&lt;T: store&gt;(cap: &<b>mut</b> <a href="collectible.md#0x2_collectible_CollectionCreatorCap">collectible::CollectionCreatorCap</a>&lt;T&gt;, img_urls: <a href="">vector</a>&lt;<a href="_String">string::String</a>&gt;, names: <a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;<a href="_String">string::String</a>&gt;&gt;, descriptions: <a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;<a href="_String">string::String</a>&gt;&gt;, creators: <a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;<a href="_String">string::String</a>&gt;&gt;, metas: <a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;T&gt;&gt;, ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>): <a href="">vector</a>&lt;<a href="collectible.md#0x2_collectible_Collectible">collectible::Collectible</a>&lt;T&gt;&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_batch_mint">batch_mint</a>&lt;T: store&gt;(
    cap: &<b>mut</b> <a href="collectible.md#0x2_collectible_CollectionCreatorCap">CollectionCreatorCap</a>&lt;T&gt;,
    img_urls: <a href="">vector</a>&lt;String&gt;,
    names: Option&lt;<a href="">vector</a>&lt;String&gt;&gt;,
    descriptions: Option&lt;<a href="">vector</a>&lt;String&gt;&gt;,
    creators: Option&lt;<a href="">vector</a>&lt;String&gt;&gt;,
    metas: Option&lt;<a href="">vector</a>&lt;T&gt;&gt;,
    ctx: &<b>mut</b> TxContext
): <a href="">vector</a>&lt;<a href="collectible.md#0x2_collectible_Collectible">Collectible</a>&lt;T&gt;&gt; {
    <b>let</b> len = vec::length(&img_urls);
    <b>let</b> res = vec::empty();

    // perform a dummy check <b>to</b> make sure collection does not overflow
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(&cap.max_supply) || cap.minted + len &lt; *<a href="_borrow">option::borrow</a>(&cap.max_supply), <a href="collectible.md#0x2_collectible_ECapReached">ECapReached</a>);
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(&names) || vec::length(<a href="_borrow">option::borrow</a>(&names)) == len, <a href="collectible.md#0x2_collectible_EWrongNamesLength">EWrongNamesLength</a>);
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(&creators) || vec::length(<a href="_borrow">option::borrow</a>(&creators)) == len, <a href="collectible.md#0x2_collectible_EWrongCreatorsLength">EWrongCreatorsLength</a>);
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(&descriptions) || vec::length(<a href="_borrow">option::borrow</a>(&descriptions)) == len, <a href="collectible.md#0x2_collectible_EWrongDescriptionsLength">EWrongDescriptionsLength</a>);
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(&metas) || vec::length(<a href="_borrow">option::borrow</a>(&metas)) == len, <a href="collectible.md#0x2_collectible_EWrongMetadatasLength">EWrongMetadatasLength</a>);

    <b>while</b> (len &gt; 0) {
        vec::push_back(&<b>mut</b> res, <a href="collectible.md#0x2_collectible_mint">mint</a>(
            cap,
            vec::pop_back(&<b>mut</b> img_urls),
            <a href="collectible.md#0x2_collectible_pop_or_none">pop_or_none</a>(&<b>mut</b> names),
            <a href="collectible.md#0x2_collectible_pop_or_none">pop_or_none</a>(&<b>mut</b> descriptions),
            <a href="collectible.md#0x2_collectible_pop_or_none">pop_or_none</a>(&<b>mut</b> creators),
            <a href="collectible.md#0x2_collectible_pop_or_none">pop_or_none</a>(&<b>mut</b> metas),
            ctx
        ));

        len = len - 1;
    };

    <b>if</b> (<a href="_is_some">option::is_some</a>(&metas)) {
        <b>let</b> metas = <a href="_destroy_some">option::destroy_some</a>(metas);
        vec::destroy_empty(metas)
    } <b>else</b> {
        <a href="_destroy_none">option::destroy_none</a>(metas);
    };

    res
}
</code></pre>



</details>

<a name="0x2_collectible_uid_mut"></a>

## Function `uid_mut`

Keeping the door open for the dynamic field extensions.


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_uid_mut">uid_mut</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="collectible.md#0x2_collectible_Collectible">collectible::Collectible</a>&lt;T&gt;): &<b>mut</b> <a href="object.md#0x2_object_UID">object::UID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_uid_mut">uid_mut</a>&lt;T: store&gt;(self: &<b>mut</b> <a href="collectible.md#0x2_collectible_Collectible">Collectible</a>&lt;T&gt;): &<b>mut</b> UID {
    &<b>mut</b> self.id
}
</code></pre>



</details>

<a name="0x2_collectible_img_url"></a>

## Function `img_url`

Read <code>img_url</code> field.


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_img_url">img_url</a>&lt;T: store&gt;(self: &<a href="collectible.md#0x2_collectible_Collectible">collectible::Collectible</a>&lt;T&gt;): &<a href="_String">string::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_img_url">img_url</a>&lt;T: store&gt;(self: &<a href="collectible.md#0x2_collectible_Collectible">Collectible</a>&lt;T&gt;): &String {
    &self.img_url
}
</code></pre>



</details>

<a name="0x2_collectible_name"></a>

## Function `name`

Read <code>name</code> field.


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_name">name</a>&lt;T: store&gt;(self: &<a href="collectible.md#0x2_collectible_Collectible">collectible::Collectible</a>&lt;T&gt;): &<a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_name">name</a>&lt;T: store&gt;(self: &<a href="collectible.md#0x2_collectible_Collectible">Collectible</a>&lt;T&gt;): &Option&lt;String&gt; {
    &self.name
}
</code></pre>



</details>

<a name="0x2_collectible_description"></a>

## Function `description`

Read <code>description</code> field.


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_description">description</a>&lt;T: store&gt;(self: &<a href="collectible.md#0x2_collectible_Collectible">collectible::Collectible</a>&lt;T&gt;): &<a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_description">description</a>&lt;T: store&gt;(self: &<a href="collectible.md#0x2_collectible_Collectible">Collectible</a>&lt;T&gt;): &Option&lt;String&gt; {
    &self.description
}
</code></pre>



</details>

<a name="0x2_collectible_creator"></a>

## Function `creator`

Read <code>creator</code> field.


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_creator">creator</a>&lt;T: store&gt;(self: &<a href="collectible.md#0x2_collectible_Collectible">collectible::Collectible</a>&lt;T&gt;): &<a href="_Option">option::Option</a>&lt;<a href="_String">string::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_creator">creator</a>&lt;T: store&gt;(self: &<a href="collectible.md#0x2_collectible_Collectible">Collectible</a>&lt;T&gt;): &Option&lt;String&gt; {
    &self.creator
}
</code></pre>



</details>

<a name="0x2_collectible_meta"></a>

## Function `meta`

Read <code>meta</code> field.


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_meta">meta</a>&lt;T: store&gt;(self: &<a href="collectible.md#0x2_collectible_Collectible">collectible::Collectible</a>&lt;T&gt;): &<a href="_Option">option::Option</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="collectible.md#0x2_collectible_meta">meta</a>&lt;T: store&gt;(self: &<a href="collectible.md#0x2_collectible_Collectible">Collectible</a>&lt;T&gt;): &Option&lt;T&gt; {
    &self.meta
}
</code></pre>



</details>

<a name="0x2_collectible_pop_or_none"></a>

## Function `pop_or_none`

Pop the value from the optional vector or return <code>none</code>.


<pre><code><b>fun</b> <a href="collectible.md#0x2_collectible_pop_or_none">pop_or_none</a>&lt;T&gt;(opt: &<b>mut</b> <a href="_Option">option::Option</a>&lt;<a href="">vector</a>&lt;T&gt;&gt;): <a href="_Option">option::Option</a>&lt;T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="collectible.md#0x2_collectible_pop_or_none">pop_or_none</a>&lt;T&gt;(opt: &<b>mut</b> Option&lt;<a href="">vector</a>&lt;T&gt;&gt;): Option&lt;T&gt; {
    <b>if</b> (<a href="_is_none">option::is_none</a>(opt)) {
        <a href="_none">option::none</a>()
    } <b>else</b> {
        <a href="_some">option::some</a>(vec::pop_back(<a href="_borrow_mut">option::borrow_mut</a>(opt)))
    }
}
</code></pre>



</details>
