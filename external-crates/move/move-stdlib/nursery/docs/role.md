
<a name="0x1_role"></a>

# Module `0x1::role`

A generic module for role-based access control (RBAC).


-  [Resource `Role`](#0x1_role_Role)
-  [Constants](#@Constants_0)
-  [Function `assign_role`](#0x1_role_assign_role)
-  [Function `revoke_role`](#0x1_role_revoke_role)
-  [Function `has_role`](#0x1_role_has_role)
-  [Function `assert_has_role`](#0x1_role_assert_has_role)


<pre><code><b>use</b> <a href="">0x1::error</a>;
<b>use</b> <a href="">0x1::signer</a>;
</code></pre>



<a name="0x1_role_Role"></a>

## Resource `Role`



<pre><code><b>struct</b> <a href="role.md#0x1_role_Role">Role</a>&lt;Type&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>dummy_field: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x1_role_EROLE"></a>



<pre><code><b>const</b> <a href="role.md#0x1_role_EROLE">EROLE</a>: u64 = 0;
</code></pre>



<a name="0x1_role_assign_role"></a>

## Function `assign_role`

Assign the role to the account. The caller must pass a witness, so is
expected to be a function of the module that defines <code>Type</code>.


<pre><code><b>public</b> <b>fun</b> <a href="role.md#0x1_role_assign_role">assign_role</a>&lt;Type&gt;(<b>to</b>: &<a href="">signer</a>, _witness: &Type)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="role.md#0x1_role_assign_role">assign_role</a>&lt;Type&gt;(<b>to</b>: &<a href="">signer</a>, _witness: &Type) {
    <b>assert</b>!(!<a href="role.md#0x1_role_has_role">has_role</a>&lt;Type&gt;(<a href="_address_of">signer::address_of</a>(<b>to</b>)), <a href="_already_exists">error::already_exists</a>(<a href="role.md#0x1_role_EROLE">EROLE</a>));
    <b>move_to</b>&lt;<a href="role.md#0x1_role_Role">Role</a>&lt;Type&gt;&gt;(<b>to</b>, <a href="role.md#0x1_role_Role">Role</a>&lt;Type&gt;{});
}
</code></pre>



</details>

<a name="0x1_role_revoke_role"></a>

## Function `revoke_role`

Revoke the role from the account. The caller must pass a witness, so is
expected to be a function of the module that defines <code>Type</code>.


<pre><code><b>public</b> <b>fun</b> <a href="role.md#0x1_role_revoke_role">revoke_role</a>&lt;Type&gt;(from: &<a href="">signer</a>, _witness: &Type)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="role.md#0x1_role_revoke_role">revoke_role</a>&lt;Type&gt;(from: &<a href="">signer</a>, _witness: &Type) <b>acquires</b> <a href="role.md#0x1_role_Role">Role</a> {
    <b>assert</b>!(<a href="role.md#0x1_role_has_role">has_role</a>&lt;Type&gt;(<a href="_address_of">signer::address_of</a>(from)), <a href="_not_found">error::not_found</a>(<a href="role.md#0x1_role_EROLE">EROLE</a>));
    <b>let</b> <a href="role.md#0x1_role_Role">Role</a>&lt;Type&gt;{} = <b>move_from</b>&lt;<a href="role.md#0x1_role_Role">Role</a>&lt;Type&gt;&gt;(<a href="_address_of">signer::address_of</a>(from));
}
</code></pre>



</details>

<a name="0x1_role_has_role"></a>

## Function `has_role`

Return true iff the address has the role.


<pre><code><b>public</b> <b>fun</b> <a href="role.md#0x1_role_has_role">has_role</a>&lt;Type&gt;(addr: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="role.md#0x1_role_has_role">has_role</a>&lt;Type&gt;(addr: <b>address</b>): bool {
    <b>exists</b>&lt;<a href="role.md#0x1_role_Role">Role</a>&lt;Type&gt;&gt;(addr)
}
</code></pre>



</details>

<a name="0x1_role_assert_has_role"></a>

## Function `assert_has_role`

assert! that the account has the role.


<pre><code><b>public</b> <b>fun</b> <a href="role.md#0x1_role_assert_has_role">assert_has_role</a>&lt;Type&gt;(account: &<a href="">signer</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="role.md#0x1_role_assert_has_role">assert_has_role</a>&lt;Type&gt;(account: &<a href="">signer</a>) {
    <b>assert</b>!(<a href="role.md#0x1_role_has_role">has_role</a>&lt;Type&gt;(<a href="_address_of">signer::address_of</a>(account)), <a href="_not_found">error::not_found</a>(<a href="role.md#0x1_role_EROLE">EROLE</a>));
}
</code></pre>



</details>
