
<a name="0x1_guid"></a>

# Module `0x1::guid`

A module for generating globally unique identifiers


-  [Resource `Generator`](#0x1_guid_Generator)
-  [Struct `GUID`](#0x1_guid_GUID)
-  [Struct `ID`](#0x1_guid_ID)
-  [Resource `CreateCapability`](#0x1_guid_CreateCapability)
-  [Constants](#@Constants_0)
-  [Function `gen_create_capability`](#0x1_guid_gen_create_capability)
-  [Function `create_id`](#0x1_guid_create_id)
-  [Function `create_with_capability`](#0x1_guid_create_with_capability)
-  [Function `create`](#0x1_guid_create)
-  [Function `create_impl`](#0x1_guid_create_impl)
-  [Function `publish_generator`](#0x1_guid_publish_generator)
-  [Function `id`](#0x1_guid_id)
-  [Function `creator_address`](#0x1_guid_creator_address)
-  [Function `id_creator_address`](#0x1_guid_id_creator_address)
-  [Function `creation_num`](#0x1_guid_creation_num)
-  [Function `id_creation_num`](#0x1_guid_id_creation_num)
-  [Function `eq_id`](#0x1_guid_eq_id)
-  [Function `get_next_creation_num`](#0x1_guid_get_next_creation_num)


<pre><code><b>use</b> <a href="">0x1::signer</a>;
</code></pre>



<a name="0x1_guid_Generator"></a>

## Resource `Generator`

A generator for new GUIDs.


<pre><code><b>struct</b> <a href="guid.md#0x1_guid_Generator">Generator</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>counter: u64</code>
</dt>
<dd>
 A monotonically increasing counter
</dd>
</dl>


</details>

<a name="0x1_guid_GUID"></a>

## Struct `GUID`

A globally unique identifier derived from the sender's address and a counter


<pre><code><b>struct</b> <a href="guid.md#0x1_guid_GUID">GUID</a> <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="guid.md#0x1_guid_ID">guid::ID</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x1_guid_ID"></a>

## Struct `ID`

A non-privileged identifier that can be freely created by anyone. Useful for looking up GUID's.


<pre><code><b>struct</b> <a href="guid.md#0x1_guid_ID">ID</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>creation_num: u64</code>
</dt>
<dd>
 If creation_num is <code>i</code>, this is the <code>i+1</code>th GUID created by <code>addr</code>
</dd>
<dt>
<code>addr: <b>address</b></code>
</dt>
<dd>
 Address that created the GUID
</dd>
</dl>


</details>

<a name="0x1_guid_CreateCapability"></a>

## Resource `CreateCapability`

A capability to create a privileged identifier on behalf of the given address


<pre><code><b>struct</b> <a href="guid.md#0x1_guid_CreateCapability">CreateCapability</a> <b>has</b> drop, store, key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>addr: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x1_guid_EGUID_GENERATOR_NOT_PUBLISHED"></a>

GUID generator must be published ahead of first usage of <code>create_with_capability</code> function.


<pre><code><b>const</b> <a href="guid.md#0x1_guid_EGUID_GENERATOR_NOT_PUBLISHED">EGUID_GENERATOR_NOT_PUBLISHED</a>: u64 = 0;
</code></pre>



<a name="0x1_guid_gen_create_capability"></a>

## Function `gen_create_capability`

Generates a capability to create the privileged GUID on behalf of the signer


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_gen_create_capability">gen_create_capability</a>(account: &<a href="">signer</a>): <a href="guid.md#0x1_guid_CreateCapability">guid::CreateCapability</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_gen_create_capability">gen_create_capability</a>(account: &<a href="">signer</a>): <a href="guid.md#0x1_guid_CreateCapability">CreateCapability</a> {
    <b>let</b> addr = <a href="_address_of">signer::address_of</a>(account);
    <b>if</b> (!<b>exists</b>&lt;<a href="guid.md#0x1_guid_Generator">Generator</a>&gt;(addr)) {
        <b>move_to</b>(account, <a href="guid.md#0x1_guid_Generator">Generator</a> { counter: 0 })
    };
    <a href="guid.md#0x1_guid_CreateCapability">CreateCapability</a> { addr }
}
</code></pre>



</details>

<a name="0x1_guid_create_id"></a>

## Function `create_id`

Create a non-privileged id from <code>addr</code> and <code>creation_num</code>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_create_id">create_id</a>(addr: <b>address</b>, creation_num: u64): <a href="guid.md#0x1_guid_ID">guid::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_create_id">create_id</a>(addr: <b>address</b>, creation_num: u64): <a href="guid.md#0x1_guid_ID">ID</a> {
    <a href="guid.md#0x1_guid_ID">ID</a> { creation_num, addr }
}
</code></pre>



</details>

<a name="0x1_guid_create_with_capability"></a>

## Function `create_with_capability`



<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_create_with_capability">create_with_capability</a>(addr: <b>address</b>, _cap: &<a href="guid.md#0x1_guid_CreateCapability">guid::CreateCapability</a>): <a href="guid.md#0x1_guid_GUID">guid::GUID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_create_with_capability">create_with_capability</a>(addr: <b>address</b>, _cap: &<a href="guid.md#0x1_guid_CreateCapability">CreateCapability</a>): <a href="guid.md#0x1_guid_GUID">GUID</a> <b>acquires</b> <a href="guid.md#0x1_guid_Generator">Generator</a> {
    <b>assert</b>!(<b>exists</b>&lt;<a href="guid.md#0x1_guid_Generator">Generator</a>&gt;(addr), <a href="guid.md#0x1_guid_EGUID_GENERATOR_NOT_PUBLISHED">EGUID_GENERATOR_NOT_PUBLISHED</a>);
    <a href="guid.md#0x1_guid_create_impl">create_impl</a>(addr)
}
</code></pre>



</details>

<a name="0x1_guid_create"></a>

## Function `create`

Create and return a new GUID. Creates a <code><a href="guid.md#0x1_guid_Generator">Generator</a></code> under <code>account</code>
if it does not already have one


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_create">create</a>(account: &<a href="">signer</a>): <a href="guid.md#0x1_guid_GUID">guid::GUID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_create">create</a>(account: &<a href="">signer</a>): <a href="guid.md#0x1_guid_GUID">GUID</a> <b>acquires</b> <a href="guid.md#0x1_guid_Generator">Generator</a> {
    <b>let</b> addr = <a href="_address_of">signer::address_of</a>(account);
    <b>if</b> (!<b>exists</b>&lt;<a href="guid.md#0x1_guid_Generator">Generator</a>&gt;(addr)) {
        <b>move_to</b>(account, <a href="guid.md#0x1_guid_Generator">Generator</a> { counter: 0 })
    };
    <a href="guid.md#0x1_guid_create_impl">create_impl</a>(addr)
}
</code></pre>



</details>

<a name="0x1_guid_create_impl"></a>

## Function `create_impl`



<pre><code><b>fun</b> <a href="guid.md#0x1_guid_create_impl">create_impl</a>(addr: <b>address</b>): <a href="guid.md#0x1_guid_GUID">guid::GUID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="guid.md#0x1_guid_create_impl">create_impl</a>(addr: <b>address</b>): <a href="guid.md#0x1_guid_GUID">GUID</a> <b>acquires</b> <a href="guid.md#0x1_guid_Generator">Generator</a> {
    <b>let</b> generator = <b>borrow_global_mut</b>&lt;<a href="guid.md#0x1_guid_Generator">Generator</a>&gt;(addr);
    <b>let</b> creation_num = generator.counter;
    generator.counter = creation_num + 1;
    <a href="guid.md#0x1_guid_GUID">GUID</a> { id: <a href="guid.md#0x1_guid_ID">ID</a> { creation_num, addr } }
}
</code></pre>



</details>

<a name="0x1_guid_publish_generator"></a>

## Function `publish_generator`

Publish a Generator resource under <code>account</code>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_publish_generator">publish_generator</a>(account: &<a href="">signer</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_publish_generator">publish_generator</a>(account: &<a href="">signer</a>) {
    <b>move_to</b>(account, <a href="guid.md#0x1_guid_Generator">Generator</a> { counter: 0 })
}
</code></pre>



</details>

<a name="0x1_guid_id"></a>

## Function `id`

Get the non-privileged ID associated with a GUID


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_id">id</a>(<a href="guid.md#0x1_guid">guid</a>: &<a href="guid.md#0x1_guid_GUID">guid::GUID</a>): <a href="guid.md#0x1_guid_ID">guid::ID</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_id">id</a>(<a href="guid.md#0x1_guid">guid</a>: &<a href="guid.md#0x1_guid_GUID">GUID</a>): <a href="guid.md#0x1_guid_ID">ID</a> {
    *&<a href="guid.md#0x1_guid">guid</a>.id
}
</code></pre>



</details>

<a name="0x1_guid_creator_address"></a>

## Function `creator_address`

Return the account address that created the GUID


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_creator_address">creator_address</a>(<a href="guid.md#0x1_guid">guid</a>: &<a href="guid.md#0x1_guid_GUID">guid::GUID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_creator_address">creator_address</a>(<a href="guid.md#0x1_guid">guid</a>: &<a href="guid.md#0x1_guid_GUID">GUID</a>): <b>address</b> {
    <a href="guid.md#0x1_guid">guid</a>.id.addr
}
</code></pre>



</details>

<a name="0x1_guid_id_creator_address"></a>

## Function `id_creator_address`

Return the account address that created the guid::ID


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_id_creator_address">id_creator_address</a>(id: &<a href="guid.md#0x1_guid_ID">guid::ID</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_id_creator_address">id_creator_address</a>(id: &<a href="guid.md#0x1_guid_ID">ID</a>): <b>address</b> {
    id.addr
}
</code></pre>



</details>

<a name="0x1_guid_creation_num"></a>

## Function `creation_num`

Return the creation number associated with the GUID


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_creation_num">creation_num</a>(<a href="guid.md#0x1_guid">guid</a>: &<a href="guid.md#0x1_guid_GUID">guid::GUID</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_creation_num">creation_num</a>(<a href="guid.md#0x1_guid">guid</a>: &<a href="guid.md#0x1_guid_GUID">GUID</a>): u64 {
    <a href="guid.md#0x1_guid">guid</a>.id.creation_num
}
</code></pre>



</details>

<a name="0x1_guid_id_creation_num"></a>

## Function `id_creation_num`

Return the creation number associated with the guid::ID


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_id_creation_num">id_creation_num</a>(id: &<a href="guid.md#0x1_guid_ID">guid::ID</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_id_creation_num">id_creation_num</a>(id: &<a href="guid.md#0x1_guid_ID">ID</a>): u64 {
    id.creation_num
}
</code></pre>



</details>

<a name="0x1_guid_eq_id"></a>

## Function `eq_id`

Return true if the GUID's ID is <code>id</code>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_eq_id">eq_id</a>(<a href="guid.md#0x1_guid">guid</a>: &<a href="guid.md#0x1_guid_GUID">guid::GUID</a>, id: &<a href="guid.md#0x1_guid_ID">guid::ID</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_eq_id">eq_id</a>(<a href="guid.md#0x1_guid">guid</a>: &<a href="guid.md#0x1_guid_GUID">GUID</a>, id: &<a href="guid.md#0x1_guid_ID">ID</a>): bool {
    &<a href="guid.md#0x1_guid">guid</a>.id == id
}
</code></pre>



</details>

<a name="0x1_guid_get_next_creation_num"></a>

## Function `get_next_creation_num`

Return the number of the next GUID to be created by <code>addr</code>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_get_next_creation_num">get_next_creation_num</a>(addr: <b>address</b>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="guid.md#0x1_guid_get_next_creation_num">get_next_creation_num</a>(addr: <b>address</b>): u64 <b>acquires</b> <a href="guid.md#0x1_guid_Generator">Generator</a> {
    <b>if</b> (!<b>exists</b>&lt;<a href="guid.md#0x1_guid_Generator">Generator</a>&gt;(addr)) {
        0
    } <b>else</b> {
        <b>borrow_global</b>&lt;<a href="guid.md#0x1_guid_Generator">Generator</a>&gt;(addr).counter
    }
}
</code></pre>



</details>
