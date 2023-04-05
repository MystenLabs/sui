
<a name="0x1_acl"></a>

# Module `0x1::acl`

Access control list (ACL) module. An ACL is a list of account addresses who
have the access permission to a certain object.
This module uses a <code><a href="">vector</a></code> to represent the list, but can be refactored to
use a "set" instead when it's available in the language in the future.


-  [Struct `ACL`](#0x1_acl_ACL)
-  [Constants](#@Constants_0)
-  [Function `empty`](#0x1_acl_empty)
-  [Function `add`](#0x1_acl_add)
-  [Function `remove`](#0x1_acl_remove)
-  [Function `contains`](#0x1_acl_contains)
-  [Function `assert_contains`](#0x1_acl_assert_contains)


<pre><code><b>use</b> <a href="errors.md#0x1_errors">0x1::errors</a>;
<b>use</b> <a href="">0x1::vector</a>;
</code></pre>



<a name="0x1_acl_ACL"></a>

## Struct `ACL`



<pre><code><b>struct</b> <a href="acl.md#0x1_acl_ACL">ACL</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>list: <a href="">vector</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x1_acl_ECONTAIN"></a>

The ACL already contains the address.


<pre><code><b>const</b> <a href="acl.md#0x1_acl_ECONTAIN">ECONTAIN</a>: u64 = 0;
</code></pre>



<a name="0x1_acl_ENOT_CONTAIN"></a>

The ACL does not contain the address.


<pre><code><b>const</b> <a href="acl.md#0x1_acl_ENOT_CONTAIN">ENOT_CONTAIN</a>: u64 = 1;
</code></pre>



<a name="0x1_acl_empty"></a>

## Function `empty`

Return an empty ACL.


<pre><code><b>public</b> <b>fun</b> <a href="acl.md#0x1_acl_empty">empty</a>(): <a href="acl.md#0x1_acl_ACL">acl::ACL</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="acl.md#0x1_acl_empty">empty</a>(): <a href="acl.md#0x1_acl_ACL">ACL</a> {
    <a href="acl.md#0x1_acl_ACL">ACL</a>{ list: <a href="_empty">vector::empty</a>&lt;<b>address</b>&gt;() }
}
</code></pre>



</details>

<a name="0x1_acl_add"></a>

## Function `add`

Add the address to the ACL.


<pre><code><b>public</b> <b>fun</b> <a href="acl.md#0x1_acl_add">add</a>(<a href="acl.md#0x1_acl">acl</a>: &<b>mut</b> <a href="acl.md#0x1_acl_ACL">acl::ACL</a>, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="acl.md#0x1_acl_add">add</a>(<a href="acl.md#0x1_acl">acl</a>: &<b>mut</b> <a href="acl.md#0x1_acl_ACL">ACL</a>, addr: <b>address</b>) {
    <b>assert</b>!(!<a href="_contains">vector::contains</a>(&<b>mut</b> <a href="acl.md#0x1_acl">acl</a>.list, &addr), <a href="errors.md#0x1_errors_invalid_argument">errors::invalid_argument</a>(<a href="acl.md#0x1_acl_ECONTAIN">ECONTAIN</a>));
    <a href="_push_back">vector::push_back</a>(&<b>mut</b> <a href="acl.md#0x1_acl">acl</a>.list, addr);
}
</code></pre>



</details>

<a name="0x1_acl_remove"></a>

## Function `remove`

Remove the address from the ACL.


<pre><code><b>public</b> <b>fun</b> <a href="acl.md#0x1_acl_remove">remove</a>(<a href="acl.md#0x1_acl">acl</a>: &<b>mut</b> <a href="acl.md#0x1_acl_ACL">acl::ACL</a>, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="acl.md#0x1_acl_remove">remove</a>(<a href="acl.md#0x1_acl">acl</a>: &<b>mut</b> <a href="acl.md#0x1_acl_ACL">ACL</a>, addr: <b>address</b>) {
    <b>let</b> (found, index) = <a href="_index_of">vector::index_of</a>(&<b>mut</b> <a href="acl.md#0x1_acl">acl</a>.list, &addr);
    <b>assert</b>!(found, <a href="errors.md#0x1_errors_invalid_argument">errors::invalid_argument</a>(<a href="acl.md#0x1_acl_ENOT_CONTAIN">ENOT_CONTAIN</a>));
    <a href="_remove">vector::remove</a>(&<b>mut</b> <a href="acl.md#0x1_acl">acl</a>.list, index);
}
</code></pre>



</details>

<a name="0x1_acl_contains"></a>

## Function `contains`

Return true iff the ACL contains the address.


<pre><code><b>public</b> <b>fun</b> <a href="acl.md#0x1_acl_contains">contains</a>(<a href="acl.md#0x1_acl">acl</a>: &<a href="acl.md#0x1_acl_ACL">acl::ACL</a>, addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="acl.md#0x1_acl_contains">contains</a>(<a href="acl.md#0x1_acl">acl</a>: &<a href="acl.md#0x1_acl_ACL">ACL</a>, addr: <b>address</b>): bool {
    <a href="_contains">vector::contains</a>(&<a href="acl.md#0x1_acl">acl</a>.list, &addr)
}
</code></pre>



</details>

<a name="0x1_acl_assert_contains"></a>

## Function `assert_contains`

assert! that the ACL has the address.


<pre><code><b>public</b> <b>fun</b> <a href="acl.md#0x1_acl_assert_contains">assert_contains</a>(<a href="acl.md#0x1_acl">acl</a>: &<a href="acl.md#0x1_acl_ACL">acl::ACL</a>, addr: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="acl.md#0x1_acl_assert_contains">assert_contains</a>(<a href="acl.md#0x1_acl">acl</a>: &<a href="acl.md#0x1_acl_ACL">ACL</a>, addr: <b>address</b>) {
    <b>assert</b>!(<a href="acl.md#0x1_acl_contains">contains</a>(<a href="acl.md#0x1_acl">acl</a>, addr), <a href="errors.md#0x1_errors_invalid_argument">errors::invalid_argument</a>(<a href="acl.md#0x1_acl_ENOT_CONTAIN">ENOT_CONTAIN</a>));
}
</code></pre>



</details>
