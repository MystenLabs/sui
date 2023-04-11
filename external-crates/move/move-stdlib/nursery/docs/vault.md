
<a name="0x1_vault"></a>

# Module `0x1::vault`

A module which implements secure memory (called a *vault*) of some content which can only be operated
on if authorized by a signer. Authorization is managed by
[*capabilities*](https://en.wikipedia.org/wiki/Capability-based_security). The vault supports delegation
of capabilities to other signers (including revocation) as well as transfer of ownership.


<a name="@Overview_0"></a>

## Overview



<a name="@Capabilities_1"></a>

### Capabilities


Capabilities are unforgeable tokens which represent the right to perform a particular
operation on the vault. To acquire a capability instance, authentication via a signer is needed.
This signer must either be the owner of the vault, or someone the capability has been delegated to.
Once acquired, a capability can be passed to other functions to perform the operation it enables.
Specifically, those called functions do not need to have access to the original signer. This is a key
property of capability based security as it prevents granting more rights to code than needed.

Capability instances are unforgeable because they are localized to transactions. They can only be
created by functions of this module, and as they do not have the Move language <code>store</code> or <code>key</code> abilities,
they cannot leak out of a transaction.

Example:

```move
struct Content has store { ssn: u64 }
...
// Create new vault
vault::new(signer, b"My Vault", Content{ ssn: 525659745 });
...
// Obtain a read capability
let read_cap = vault::acquire_read_cap<Content>(signer);
process(&read_cap)
...
fun process(cap: &vault::ReadCap<Content>) {
let accessor = vault::read_accessor(cap);
let content = vault::borrow(accessor);
<< do something with <code>content: &Content</code> >>
vault::release_read_accessor(accessor);
}
```


<a name="@Delegation_2"></a>

### Delegation


Delegation provides the option to delegate the right to acquire a vault capability to other
signers than the owner of the vault. Delegates still need to authenticate themselves using their
signer, similar as the owner does. All security arguments for owners apply also to delegates.
Delegation can be revoked removing previously granted rights from a delegate.

Delegation can be configured to be transitive by granting the right to acquire a delegation capability
to delegates, which can then further delegate access rights.

By default, when a vault is created, it does not support delegation. The owner of the vault
needs to explicitly enable delegation. This allows to create vaults which are not intended for delegation
and one does not need to worry about its misuse.

Example:

```move
vault::new(signer, b"My Vault", Content{ ssn: 525659745 });
// Enable delegation for this vault. Only the owning signer can do this.
vault::enable_delegation<Content>(signer);
...
// Delegate read capability to some other signer.
let delegate_cap = vault::acquire_delegate_cap<Content>(signer);
vault::delegate_read_cap(&delegate_cap, other_signer);
...
// Other signer can now acquire read cap
let read_cap = vault::acquire_read_cap<Content>(other_signer);
...
// The granted capability can be revoked. There is no need to have the other signer for this.
vault::revoke_read_cap(&delegate_cap, signer::address_of(other_signer));
```


<a name="@Abilities_3"></a>

### Abilities


Currently, we require that the <code>Content</code> type of a vault has the <code>drop</code> ability in order to instantiate
a capability type like <code><a href="vault.md#0x1_vault_ReadCap">ReadCap</a>&lt;Content&gt;</code>. Without this, capabilities themselves would need to have an
explicit release function, which makes little sense as they are pure values. We expect the Move
language to have 'phantom type parameters' or similar features added, which will allows us to have
<code><a href="vault.md#0x1_vault_ReadCap">ReadCap</a>&lt;Content&gt;</code> droppable and copyable without <code>Content</code> needing the same.


-  [Overview](#@Overview_0)
    -  [Capabilities](#@Capabilities_1)
    -  [Delegation](#@Delegation_2)
    -  [Abilities](#@Abilities_3)
-  [Struct `ReadCap`](#0x1_vault_ReadCap)
-  [Struct `ModifyCap`](#0x1_vault_ModifyCap)
-  [Struct `DelegateCap`](#0x1_vault_DelegateCap)
-  [Struct `TransferCap`](#0x1_vault_TransferCap)
-  [Struct `CapType`](#0x1_vault_CapType)
-  [Struct `VaultDelegateEvent`](#0x1_vault_VaultDelegateEvent)
-  [Struct `VaultTransferEvent`](#0x1_vault_VaultTransferEvent)
-  [Resource `Vault`](#0x1_vault_Vault)
-  [Resource `VaultDelegates`](#0x1_vault_VaultDelegates)
-  [Resource `VaultEvents`](#0x1_vault_VaultEvents)
-  [Resource `VaultDelegate`](#0x1_vault_VaultDelegate)
-  [Struct `ReadAccessor`](#0x1_vault_ReadAccessor)
-  [Struct `ModifyAccessor`](#0x1_vault_ModifyAccessor)
-  [Constants](#@Constants_4)
-  [Function `read_cap_type`](#0x1_vault_read_cap_type)
-  [Function `modify_cap_type`](#0x1_vault_modify_cap_type)
-  [Function `delegate_cap_type`](#0x1_vault_delegate_cap_type)
-  [Function `transfer_cap_type`](#0x1_vault_transfer_cap_type)
-  [Function `new`](#0x1_vault_new)
-  [Function `is_delegation_enabled`](#0x1_vault_is_delegation_enabled)
-  [Function `enable_delegation`](#0x1_vault_enable_delegation)
-  [Function `enable_events`](#0x1_vault_enable_events)
-  [Function `remove_vault`](#0x1_vault_remove_vault)
-  [Function `acquire_read_cap`](#0x1_vault_acquire_read_cap)
-  [Function `acquire_modify_cap`](#0x1_vault_acquire_modify_cap)
-  [Function `acquire_delegate_cap`](#0x1_vault_acquire_delegate_cap)
-  [Function `acquire_transfer_cap`](#0x1_vault_acquire_transfer_cap)
-  [Function `validate_cap`](#0x1_vault_validate_cap)
-  [Function `read_accessor`](#0x1_vault_read_accessor)
-  [Function `borrow`](#0x1_vault_borrow)
-  [Function `release_read_accessor`](#0x1_vault_release_read_accessor)
-  [Function `modify_accessor`](#0x1_vault_modify_accessor)
-  [Function `borrow_mut`](#0x1_vault_borrow_mut)
-  [Function `release_modify_accessor`](#0x1_vault_release_modify_accessor)
-  [Function `delegate`](#0x1_vault_delegate)
-  [Function `revoke`](#0x1_vault_revoke)
-  [Function `revoke_all`](#0x1_vault_revoke_all)
-  [Function `remove_element`](#0x1_vault_remove_element)
-  [Function `add_element`](#0x1_vault_add_element)
-  [Function `emit_delegate_event`](#0x1_vault_emit_delegate_event)
-  [Function `transfer`](#0x1_vault_transfer)


<pre><code><b>use</b> <a href="">0x1::error</a>;
<b>use</b> <a href="event.md#0x1_event">0x1::event</a>;
<b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="">0x1::signer</a>;
<b>use</b> <a href="">0x1::vector</a>;
</code></pre>



<a name="0x1_vault_ReadCap"></a>

## Struct `ReadCap`

A capability to read the content of the vault. Notice that the capability cannot be
stored but can be freely copied and dropped.
TODO: remove <code>drop</code> on <code>Content</code> here and elsewhere once we have phantom type parameters.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_ReadCap">ReadCap</a>&lt;Content: drop, store&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>vault_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>authority: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x1_vault_ModifyCap"></a>

## Struct `ModifyCap`

A capability to modify the content of the vault.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_ModifyCap">ModifyCap</a>&lt;Content: drop, store&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>vault_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>authority: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x1_vault_DelegateCap"></a>

## Struct `DelegateCap`

A capability to delegate access to the vault.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_DelegateCap">DelegateCap</a>&lt;Content: drop, store&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>vault_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>authority: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x1_vault_TransferCap"></a>

## Struct `TransferCap`

A capability to transfer ownership of the vault.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_TransferCap">TransferCap</a>&lt;Content: drop, store&gt; <b>has</b> <b>copy</b>, drop
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>vault_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>authority: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x1_vault_CapType"></a>

## Struct `CapType`

A type describing a capability. This is used for functions like <code><a href="vault.md#0x1_vault_delegate">Self::delegate</a></code> where we need to
specify capability types.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_CapType">CapType</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>code: u8</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x1_vault_VaultDelegateEvent"></a>

## Struct `VaultDelegateEvent`

An event which we generate on vault access delegation or revocation if event generation is enabled.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_VaultDelegateEvent">VaultDelegateEvent</a> <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>metadata: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>vault_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>authority: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>delegate: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>cap: <a href="vault.md#0x1_vault_CapType">vault::CapType</a></code>
</dt>
<dd>

</dd>
<dt>
<code>is_revoked: bool</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x1_vault_VaultTransferEvent"></a>

## Struct `VaultTransferEvent`

An event which we generate on vault transfer if event generation is enabled.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_VaultTransferEvent">VaultTransferEvent</a> <b>has</b> drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>metadata: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
<dt>
<code>vault_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>authority: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>new_vault_address: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x1_vault_Vault"></a>

## Resource `Vault`

Private. The vault representation.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content: store&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>content: <a href="_Option">option::Option</a>&lt;Content&gt;</code>
</dt>
<dd>
 The content. If the option is empty, the content is currently moved into an
 accessor in order to work with it.
</dd>
</dl>


</details>

<a name="0x1_vault_VaultDelegates"></a>

## Resource `VaultDelegates`

Private. If the vault supports delegation, information about the delegates.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>&lt;Content: store&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>delegates: <a href="">vector</a>&lt;<b>address</b>&gt;</code>
</dt>
<dd>
 The currently authorized delegates.
</dd>
</dl>


</details>

<a name="0x1_vault_VaultEvents"></a>

## Resource `VaultEvents`

Private. If event generation is enabled, contains the event generators.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>&lt;Content: store&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>metadata: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 Metadata which identifies this vault. This information is used
 in events generated by this module.
</dd>
<dt>
<code>delegate_events: <a href="event.md#0x1_event_EventHandle">event::EventHandle</a>&lt;<a href="vault.md#0x1_vault_VaultDelegateEvent">vault::VaultDelegateEvent</a>&gt;</code>
</dt>
<dd>
 Event handle for vault delegation.
</dd>
<dt>
<code>transfer_events: <a href="event.md#0x1_event_EventHandle">event::EventHandle</a>&lt;<a href="vault.md#0x1_vault_VaultTransferEvent">vault::VaultTransferEvent</a>&gt;</code>
</dt>
<dd>
 Event handle for vault transfer.
</dd>
</dl>


</details>

<a name="0x1_vault_VaultDelegate"></a>

## Resource `VaultDelegate`

Private. A value stored at a delegates address pointing to the owner of the vault. Also
describes the capabilities granted to this delegate.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>&lt;Content: store&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>vault_address: <b>address</b></code>
</dt>
<dd>

</dd>
<dt>
<code>granted_caps: <a href="">vector</a>&lt;<a href="vault.md#0x1_vault_CapType">vault::CapType</a>&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x1_vault_ReadAccessor"></a>

## Struct `ReadAccessor`

A read accessor for the content of the vault.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_ReadAccessor">ReadAccessor</a>&lt;Content: drop, store&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>content: Content</code>
</dt>
<dd>

</dd>
<dt>
<code>vault_address: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x1_vault_ModifyAccessor"></a>

## Struct `ModifyAccessor`

A modify accessor for the content of the vault.


<pre><code><b>struct</b> <a href="vault.md#0x1_vault_ModifyAccessor">ModifyAccessor</a>&lt;Content: drop, store&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>content: Content</code>
</dt>
<dd>

</dd>
<dt>
<code>vault_address: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_4"></a>

## Constants


<a name="0x1_vault_EDELEGATE"></a>



<pre><code><b>const</b> <a href="vault.md#0x1_vault_EDELEGATE">EDELEGATE</a>: u64 = 1;
</code></pre>



<a name="0x1_vault_EACCESSOR_INCONSISTENCY"></a>



<pre><code><b>const</b> <a href="vault.md#0x1_vault_EACCESSOR_INCONSISTENCY">EACCESSOR_INCONSISTENCY</a>: u64 = 3;
</code></pre>



<a name="0x1_vault_EACCESSOR_IN_USE"></a>



<pre><code><b>const</b> <a href="vault.md#0x1_vault_EACCESSOR_IN_USE">EACCESSOR_IN_USE</a>: u64 = 2;
</code></pre>



<a name="0x1_vault_EDELEGATE_TO_SELF"></a>



<pre><code><b>const</b> <a href="vault.md#0x1_vault_EDELEGATE_TO_SELF">EDELEGATE_TO_SELF</a>: u64 = 4;
</code></pre>



<a name="0x1_vault_EDELEGATION_NOT_ENABLED"></a>



<pre><code><b>const</b> <a href="vault.md#0x1_vault_EDELEGATION_NOT_ENABLED">EDELEGATION_NOT_ENABLED</a>: u64 = 5;
</code></pre>



<a name="0x1_vault_EEVENT"></a>



<pre><code><b>const</b> <a href="vault.md#0x1_vault_EEVENT">EEVENT</a>: u64 = 6;
</code></pre>



<a name="0x1_vault_EVAULT"></a>



<pre><code><b>const</b> <a href="vault.md#0x1_vault_EVAULT">EVAULT</a>: u64 = 0;
</code></pre>



<a name="0x1_vault_read_cap_type"></a>

## Function `read_cap_type`

Creates a read capability type.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_read_cap_type">read_cap_type</a>(): <a href="vault.md#0x1_vault_CapType">vault::CapType</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_read_cap_type">read_cap_type</a>(): <a href="vault.md#0x1_vault_CapType">CapType</a> { <a href="vault.md#0x1_vault_CapType">CapType</a>{ code : 0 } }
</code></pre>



</details>

<a name="0x1_vault_modify_cap_type"></a>

## Function `modify_cap_type`

Creates a modify  capability type.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_modify_cap_type">modify_cap_type</a>(): <a href="vault.md#0x1_vault_CapType">vault::CapType</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_modify_cap_type">modify_cap_type</a>(): <a href="vault.md#0x1_vault_CapType">CapType</a> { <a href="vault.md#0x1_vault_CapType">CapType</a>{ code : 1 } }
</code></pre>



</details>

<a name="0x1_vault_delegate_cap_type"></a>

## Function `delegate_cap_type`

Creates a delegate  capability type.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_delegate_cap_type">delegate_cap_type</a>(): <a href="vault.md#0x1_vault_CapType">vault::CapType</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_delegate_cap_type">delegate_cap_type</a>(): <a href="vault.md#0x1_vault_CapType">CapType</a> { <a href="vault.md#0x1_vault_CapType">CapType</a>{ code : 2 } }
</code></pre>



</details>

<a name="0x1_vault_transfer_cap_type"></a>

## Function `transfer_cap_type`

Creates a transfer  capability type.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_transfer_cap_type">transfer_cap_type</a>(): <a href="vault.md#0x1_vault_CapType">vault::CapType</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_transfer_cap_type">transfer_cap_type</a>(): <a href="vault.md#0x1_vault_CapType">CapType</a> { <a href="vault.md#0x1_vault_CapType">CapType</a>{ code : 3 } }
</code></pre>



</details>

<a name="0x1_vault_new"></a>

## Function `new`

Creates new vault for the given signer. The vault is populated with the <code>initial_content</code>.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_new">new</a>&lt;Content: store&gt;(owner: &<a href="">signer</a>, initial_content: Content)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_new">new</a>&lt;Content: store&gt;(owner: &<a href="">signer</a>,  initial_content: Content) {
    <b>let</b> addr = <a href="_address_of">signer::address_of</a>(owner);
    <b>assert</b>!(!<b>exists</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(addr), <a href="_already_exists">error::already_exists</a>(<a href="vault.md#0x1_vault_EVAULT">EVAULT</a>));
    <b>move_to</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(
        owner,
        <a href="vault.md#0x1_vault_Vault">Vault</a>{
            content: <a href="_some">option::some</a>(initial_content)
        }
    )
}
</code></pre>



</details>

<a name="0x1_vault_is_delegation_enabled"></a>

## Function `is_delegation_enabled`

Returns <code><b>true</b></code> if the delegation functionality has been enabled.
Returns <code><b>false</b></code> otherwise.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_is_delegation_enabled">is_delegation_enabled</a>&lt;Content: store&gt;(owner: &<a href="">signer</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_is_delegation_enabled">is_delegation_enabled</a>&lt;Content: store&gt;(owner: &<a href="">signer</a>): bool {
    <b>let</b> addr = <a href="_address_of">signer::address_of</a>(owner);
    <b>assert</b>!(<b>exists</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(addr), <a href="_not_found">error::not_found</a>(<a href="vault.md#0x1_vault_EVAULT">EVAULT</a>));
    <b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>&lt;Content&gt;&gt;(addr)
}
</code></pre>



</details>

<a name="0x1_vault_enable_delegation"></a>

## Function `enable_delegation`

Enables delegation functionality for this vault. By default, vaults to not support delegation.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_enable_delegation">enable_delegation</a>&lt;Content: store&gt;(owner: &<a href="">signer</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_enable_delegation">enable_delegation</a>&lt;Content: store&gt;(owner: &<a href="">signer</a>) {
    <b>assert</b>!(!<a href="vault.md#0x1_vault_is_delegation_enabled">is_delegation_enabled</a>&lt;Content&gt;(owner), <a href="_already_exists">error::already_exists</a>(<a href="vault.md#0x1_vault_EDELEGATE">EDELEGATE</a>));
    <b>move_to</b>&lt;<a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>&lt;Content&gt;&gt;(owner, <a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>{delegates: <a href="_empty">vector::empty</a>()})
}
</code></pre>



</details>

<a name="0x1_vault_enable_events"></a>

## Function `enable_events`

Enables event generation for this vault. This passed metadata is used to identify
the vault in events.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_enable_events">enable_events</a>&lt;Content: store&gt;(owner: &<a href="">signer</a>, metadata: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_enable_events">enable_events</a>&lt;Content: store&gt;(owner: &<a href="">signer</a>, metadata: <a href="">vector</a>&lt;u8&gt;) {
    <b>let</b> addr = <a href="_address_of">signer::address_of</a>(owner);
    <b>assert</b>!(<b>exists</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(addr), <a href="_not_found">error::not_found</a>(<a href="vault.md#0x1_vault_EVAULT">EVAULT</a>));
    <b>assert</b>!(!<b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>&lt;Content&gt;&gt;(addr), <a href="_already_exists">error::already_exists</a>(<a href="vault.md#0x1_vault_EEVENT">EEVENT</a>));
    <b>move_to</b>&lt;<a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>&lt;Content&gt;&gt;(
        owner,
        <a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>{
            metadata,
            delegate_events: <a href="event.md#0x1_event_new_event_handle">event::new_event_handle</a>&lt;<a href="vault.md#0x1_vault_VaultDelegateEvent">VaultDelegateEvent</a>&gt;(owner),
            transfer_events: <a href="event.md#0x1_event_new_event_handle">event::new_event_handle</a>&lt;<a href="vault.md#0x1_vault_VaultTransferEvent">VaultTransferEvent</a>&gt;(owner),
        }
    );
}
</code></pre>



</details>

<a name="0x1_vault_remove_vault"></a>

## Function `remove_vault`

Removes a vault and all its associated data, returning the current content. In order for
this to succeed, there must be no active accessor for the vault.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_remove_vault">remove_vault</a>&lt;Content: drop, store&gt;(owner: &<a href="">signer</a>): Content
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_remove_vault">remove_vault</a>&lt;Content: store + drop&gt;(owner: &<a href="">signer</a>): Content
<b>acquires</b> <a href="vault.md#0x1_vault_Vault">Vault</a>, <a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>, <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>, <a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a> {
    <b>let</b> addr = <a href="_address_of">signer::address_of</a>(owner);
    <b>assert</b>!(<b>exists</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(addr), <a href="_not_found">error::not_found</a>(<a href="vault.md#0x1_vault_EVAULT">EVAULT</a>));
    <b>let</b> <a href="vault.md#0x1_vault_Vault">Vault</a>{content} = <b>move_from</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(addr);
    <b>assert</b>!(<a href="_is_some">option::is_some</a>(&content), <a href="_invalid_state">error::invalid_state</a>(<a href="vault.md#0x1_vault_EACCESSOR_IN_USE">EACCESSOR_IN_USE</a>));

    <b>if</b> (<b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>&lt;Content&gt;&gt;(addr)) {
        <b>let</b> delegate_cap = <a href="vault.md#0x1_vault_DelegateCap">DelegateCap</a>&lt;Content&gt;{vault_address: addr, authority: addr};
        <a href="vault.md#0x1_vault_revoke_all">revoke_all</a>(&delegate_cap);
    };
    <b>if</b> (<b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>&lt;Content&gt;&gt;(addr)) {
        <b>let</b> <a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>{metadata: _metadata, delegate_events, transfer_events} =
            <b>move_from</b>&lt;<a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>&lt;Content&gt;&gt;(addr);
        <a href="event.md#0x1_event_destroy_handle">event::destroy_handle</a>(delegate_events);
        <a href="event.md#0x1_event_destroy_handle">event::destroy_handle</a>(transfer_events);
    };

    <a href="_extract">option::extract</a>(&<b>mut</b> content)
}
</code></pre>



</details>

<a name="0x1_vault_acquire_read_cap"></a>

## Function `acquire_read_cap`

Acquires the capability to read the vault. The passed signer must either be the owner
of the vault or a delegate with appropriate access.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_acquire_read_cap">acquire_read_cap</a>&lt;Content: drop, store&gt;(requester: &<a href="">signer</a>): <a href="vault.md#0x1_vault_ReadCap">vault::ReadCap</a>&lt;Content&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_acquire_read_cap">acquire_read_cap</a>&lt;Content: store + drop&gt;(requester: &<a href="">signer</a>): <a href="vault.md#0x1_vault_ReadCap">ReadCap</a>&lt;Content&gt;
<b>acquires</b> <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a> {
    <b>let</b> (vault_address, authority) = <a href="vault.md#0x1_vault_validate_cap">validate_cap</a>&lt;Content&gt;(requester, <a href="vault.md#0x1_vault_read_cap_type">read_cap_type</a>());
    <a href="vault.md#0x1_vault_ReadCap">ReadCap</a>{ vault_address, authority }
}
</code></pre>



</details>

<a name="0x1_vault_acquire_modify_cap"></a>

## Function `acquire_modify_cap`

Acquires the capability to modify the vault. The passed signer must either be the owner
of the vault or a delegate with appropriate access.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_acquire_modify_cap">acquire_modify_cap</a>&lt;Content: drop, store&gt;(requester: &<a href="">signer</a>): <a href="vault.md#0x1_vault_ModifyCap">vault::ModifyCap</a>&lt;Content&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_acquire_modify_cap">acquire_modify_cap</a>&lt;Content: store + drop&gt;(requester: &<a href="">signer</a>): <a href="vault.md#0x1_vault_ModifyCap">ModifyCap</a>&lt;Content&gt;
<b>acquires</b> <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a> {
    <b>let</b> (vault_address, authority) = <a href="vault.md#0x1_vault_validate_cap">validate_cap</a>&lt;Content&gt;(requester, <a href="vault.md#0x1_vault_modify_cap_type">modify_cap_type</a>());
    <a href="vault.md#0x1_vault_ModifyCap">ModifyCap</a>{ vault_address, authority }
}
</code></pre>



</details>

<a name="0x1_vault_acquire_delegate_cap"></a>

## Function `acquire_delegate_cap`

Acquires the capability to delegate access to the vault. The passed signer must either be the owner
of the vault or a delegate with appropriate access.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_acquire_delegate_cap">acquire_delegate_cap</a>&lt;Content: drop, store&gt;(requester: &<a href="">signer</a>): <a href="vault.md#0x1_vault_DelegateCap">vault::DelegateCap</a>&lt;Content&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_acquire_delegate_cap">acquire_delegate_cap</a>&lt;Content: store + drop&gt;(requester: &<a href="">signer</a>): <a href="vault.md#0x1_vault_DelegateCap">DelegateCap</a>&lt;Content&gt;
<b>acquires</b> <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a> {
    <b>let</b> (vault_address, authority) = <a href="vault.md#0x1_vault_validate_cap">validate_cap</a>&lt;Content&gt;(requester, <a href="vault.md#0x1_vault_delegate_cap_type">delegate_cap_type</a>());
    <a href="vault.md#0x1_vault_DelegateCap">DelegateCap</a>{ vault_address, authority }
}
</code></pre>



</details>

<a name="0x1_vault_acquire_transfer_cap"></a>

## Function `acquire_transfer_cap`

Acquires the capability to transfer the vault. The passed signer must either be the owner
of the vault or a delegate with appropriate access.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_acquire_transfer_cap">acquire_transfer_cap</a>&lt;Content: drop, store&gt;(requester: &<a href="">signer</a>): <a href="vault.md#0x1_vault_TransferCap">vault::TransferCap</a>&lt;Content&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_acquire_transfer_cap">acquire_transfer_cap</a>&lt;Content: store + drop&gt;(requester: &<a href="">signer</a>): <a href="vault.md#0x1_vault_TransferCap">TransferCap</a>&lt;Content&gt;
<b>acquires</b> <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a> {
    <b>let</b> (vault_address, authority) = <a href="vault.md#0x1_vault_validate_cap">validate_cap</a>&lt;Content&gt;(requester, <a href="vault.md#0x1_vault_transfer_cap_type">transfer_cap_type</a>());
    <a href="vault.md#0x1_vault_TransferCap">TransferCap</a>{ vault_address, authority }
}
</code></pre>



</details>

<a name="0x1_vault_validate_cap"></a>

## Function `validate_cap`

Private. Validates whether a capability can be acquired by the given signer. Returns the
pair of the vault address and the used authority.


<pre><code><b>fun</b> <a href="vault.md#0x1_vault_validate_cap">validate_cap</a>&lt;Content: drop, store&gt;(requester: &<a href="">signer</a>, cap: <a href="vault.md#0x1_vault_CapType">vault::CapType</a>): (<b>address</b>, <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="vault.md#0x1_vault_validate_cap">validate_cap</a>&lt;Content: store + drop&gt;(requester: &<a href="">signer</a>, cap: <a href="vault.md#0x1_vault_CapType">CapType</a>): (<b>address</b>, <b>address</b>)
<b>acquires</b> <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a> {
    <b>let</b> addr = <a href="_address_of">signer::address_of</a>(requester);
    <b>if</b> (<b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>&lt;Content&gt;&gt;(addr)) {
        // The <a href="">signer</a> is a delegate. Check it's granted capabilities.
        <b>let</b> delegate = <b>borrow_global</b>&lt;<a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>&lt;Content&gt;&gt;(addr);
        <b>assert</b>!(<a href="_contains">vector::contains</a>(&delegate.granted_caps, &cap), <a href="_permission_denied">error::permission_denied</a>(<a href="vault.md#0x1_vault_EDELEGATE">EDELEGATE</a>));
        (delegate.vault_address, addr)
    } <b>else</b> {
        // If it is not a delegate, it must be the owner <b>to</b> succeed.
        <b>assert</b>!(<b>exists</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(addr), <a href="_not_found">error::not_found</a>(<a href="vault.md#0x1_vault_EVAULT">EVAULT</a>));
        (addr, addr)
    }
}
</code></pre>



</details>

<a name="0x1_vault_read_accessor"></a>

## Function `read_accessor`

Creates a read accessor for the content in the vault based on a read capability.

Only one accessor (whether read or modify) for the same vault can exist at a time, and this
function will abort if one is in use. An accessor must be explicitly released using
<code><a href="vault.md#0x1_vault_release_read_accessor">Self::release_read_accessor</a></code>.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_read_accessor">read_accessor</a>&lt;Content: drop, store&gt;(cap: &<a href="vault.md#0x1_vault_ReadCap">vault::ReadCap</a>&lt;Content&gt;): <a href="vault.md#0x1_vault_ReadAccessor">vault::ReadAccessor</a>&lt;Content&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_read_accessor">read_accessor</a>&lt;Content: store + drop&gt;(cap: &<a href="vault.md#0x1_vault_ReadCap">ReadCap</a>&lt;Content&gt;): <a href="vault.md#0x1_vault_ReadAccessor">ReadAccessor</a>&lt;Content&gt;
<b>acquires</b> <a href="vault.md#0x1_vault_Vault">Vault</a> {
    <b>let</b> content = &<b>mut</b> <b>borrow_global_mut</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(cap.vault_address).content;
    <b>assert</b>!(<a href="_is_some">option::is_some</a>(content), <a href="_invalid_state">error::invalid_state</a>(<a href="vault.md#0x1_vault_EACCESSOR_IN_USE">EACCESSOR_IN_USE</a>));
    <a href="vault.md#0x1_vault_ReadAccessor">ReadAccessor</a>{ vault_address: cap.vault_address, content: <a href="_extract">option::extract</a>(content) }
}
</code></pre>



</details>

<a name="0x1_vault_borrow"></a>

## Function `borrow`

Returns a reference to the content represented by a read accessor.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_borrow">borrow</a>&lt;Content: drop, store&gt;(accessor: &<a href="vault.md#0x1_vault_ReadAccessor">vault::ReadAccessor</a>&lt;Content&gt;): &Content
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_borrow">borrow</a>&lt;Content: store + drop&gt;(accessor: &<a href="vault.md#0x1_vault_ReadAccessor">ReadAccessor</a>&lt;Content&gt;): &Content {
    &accessor.content
}
</code></pre>



</details>

<a name="0x1_vault_release_read_accessor"></a>

## Function `release_read_accessor`

Releases read accessor.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_release_read_accessor">release_read_accessor</a>&lt;Content: drop, store&gt;(accessor: <a href="vault.md#0x1_vault_ReadAccessor">vault::ReadAccessor</a>&lt;Content&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_release_read_accessor">release_read_accessor</a>&lt;Content: store + drop&gt;(accessor: <a href="vault.md#0x1_vault_ReadAccessor">ReadAccessor</a>&lt;Content&gt;)
<b>acquires</b> <a href="vault.md#0x1_vault_Vault">Vault</a> {
    <b>let</b> <a href="vault.md#0x1_vault_ReadAccessor">ReadAccessor</a>{ content: new_content, vault_address } = accessor;
    <b>let</b> content = &<b>mut</b> <b>borrow_global_mut</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(vault_address).content;
    // We (should be/are) able <b>to</b> prove that the below cannot happen, but we leave the assertion
    // here anyway for double safety.
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(content), <a href="_internal">error::internal</a>(<a href="vault.md#0x1_vault_EACCESSOR_INCONSISTENCY">EACCESSOR_INCONSISTENCY</a>));
    <a href="_fill">option::fill</a>(content, new_content);
}
</code></pre>



</details>

<a name="0x1_vault_modify_accessor"></a>

## Function `modify_accessor`

Creates a modify accessor for the content in the vault based on a modify capability. This
is similar like <code><a href="vault.md#0x1_vault_read_accessor">Self::read_accessor</a></code> but the returned accessor will allow to mutate
the content.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_modify_accessor">modify_accessor</a>&lt;Content: drop, store&gt;(cap: &<a href="vault.md#0x1_vault_ModifyCap">vault::ModifyCap</a>&lt;Content&gt;): <a href="vault.md#0x1_vault_ModifyAccessor">vault::ModifyAccessor</a>&lt;Content&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_modify_accessor">modify_accessor</a>&lt;Content: store + drop&gt;(cap: &<a href="vault.md#0x1_vault_ModifyCap">ModifyCap</a>&lt;Content&gt;): <a href="vault.md#0x1_vault_ModifyAccessor">ModifyAccessor</a>&lt;Content&gt;
<b>acquires</b> <a href="vault.md#0x1_vault_Vault">Vault</a> {
    <b>let</b> content = &<b>mut</b> <b>borrow_global_mut</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(cap.vault_address).content;
    <b>assert</b>!(<a href="_is_some">option::is_some</a>(content), <a href="_invalid_state">error::invalid_state</a>(<a href="vault.md#0x1_vault_EACCESSOR_IN_USE">EACCESSOR_IN_USE</a>));
    <a href="vault.md#0x1_vault_ModifyAccessor">ModifyAccessor</a>{ vault_address: cap.vault_address, content: <a href="_extract">option::extract</a>(content) }
}
</code></pre>



</details>

<a name="0x1_vault_borrow_mut"></a>

## Function `borrow_mut`

Returns a mutable reference to the content represented by a modify accessor.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_borrow_mut">borrow_mut</a>&lt;Content: drop, store&gt;(accessor: &<b>mut</b> <a href="vault.md#0x1_vault_ModifyAccessor">vault::ModifyAccessor</a>&lt;Content&gt;): &<b>mut</b> Content
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_borrow_mut">borrow_mut</a>&lt;Content: store + drop&gt;(accessor: &<b>mut</b> <a href="vault.md#0x1_vault_ModifyAccessor">ModifyAccessor</a>&lt;Content&gt;): &<b>mut</b> Content {
    &<b>mut</b> accessor.content
}
</code></pre>



</details>

<a name="0x1_vault_release_modify_accessor"></a>

## Function `release_modify_accessor`

Releases a modify accessor. This will ensure that any modifications are written back
to the vault.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_release_modify_accessor">release_modify_accessor</a>&lt;Content: drop, store&gt;(accessor: <a href="vault.md#0x1_vault_ModifyAccessor">vault::ModifyAccessor</a>&lt;Content&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_release_modify_accessor">release_modify_accessor</a>&lt;Content: store + drop&gt;(accessor: <a href="vault.md#0x1_vault_ModifyAccessor">ModifyAccessor</a>&lt;Content&gt;)
<b>acquires</b> <a href="vault.md#0x1_vault_Vault">Vault</a> {
    <b>let</b> <a href="vault.md#0x1_vault_ModifyAccessor">ModifyAccessor</a>{ content: new_content, vault_address } = accessor;
    <b>let</b> content = &<b>mut</b> <b>borrow_global_mut</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(vault_address).content;
    // We (should be/are) able <b>to</b> prove that the below cannot happen, but we leave the assertion
    // here anyway for double safety.
    <b>assert</b>!(<a href="_is_none">option::is_none</a>(content), <a href="_internal">error::internal</a>(<a href="vault.md#0x1_vault_EACCESSOR_INCONSISTENCY">EACCESSOR_INCONSISTENCY</a>));
    <a href="_fill">option::fill</a>(content, new_content);
}
</code></pre>



</details>

<a name="0x1_vault_delegate"></a>

## Function `delegate`

Delegates the right to acquire a capability of the given type. Delegation must have been enabled
during vault creation for this to succeed.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_delegate">delegate</a>&lt;Content: drop, store&gt;(cap: &<a href="vault.md#0x1_vault_DelegateCap">vault::DelegateCap</a>&lt;Content&gt;, to_signer: &<a href="">signer</a>, cap_type: <a href="vault.md#0x1_vault_CapType">vault::CapType</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_delegate">delegate</a>&lt;Content: store + drop&gt;(cap: &<a href="vault.md#0x1_vault_DelegateCap">DelegateCap</a>&lt;Content&gt;, to_signer: &<a href="">signer</a>, cap_type: <a href="vault.md#0x1_vault_CapType">CapType</a>)
<b>acquires</b> <a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>, <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>, <a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a> {
    <b>assert</b>!(
        <b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>&lt;Content&gt;&gt;(cap.vault_address),
        <a href="_invalid_state">error::invalid_state</a>(<a href="vault.md#0x1_vault_EDELEGATION_NOT_ENABLED">EDELEGATION_NOT_ENABLED</a>)
    );

    <b>let</b> addr = <a href="_address_of">signer::address_of</a>(to_signer);
    <b>assert</b>!(addr != cap.vault_address, <a href="_invalid_argument">error::invalid_argument</a>(<a href="vault.md#0x1_vault_EDELEGATE_TO_SELF">EDELEGATE_TO_SELF</a>));

    <b>if</b> (!<b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>&lt;Content&gt;&gt;(addr)) {
        // Create <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a> <b>if</b> it is not yet existing.
        <b>move_to</b>&lt;<a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>&lt;Content&gt;&gt;(
            to_signer,
            <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>{vault_address: cap.vault_address, granted_caps: <a href="_empty">vector::empty</a>()}
        );
        // Add the the delegate <b>to</b> <a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>.
        <b>let</b> vault_delegates = <b>borrow_global_mut</b>&lt;<a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>&lt;Content&gt;&gt;(cap.vault_address);
        <a href="vault.md#0x1_vault_add_element">add_element</a>(&<b>mut</b> vault_delegates.delegates, addr);
    };

    // Grant the <a href="capability.md#0x1_capability">capability</a>.
    <b>let</b> delegate = <b>borrow_global_mut</b>&lt;<a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>&lt;Content&gt;&gt;(addr);
    <a href="vault.md#0x1_vault_add_element">add_element</a>(&<b>mut</b> delegate.granted_caps, *&cap_type);

    // Generate <a href="event.md#0x1_event">event</a>
    <a href="vault.md#0x1_vault_emit_delegate_event">emit_delegate_event</a>(cap, cap_type, addr, <b>false</b>);
}
</code></pre>



</details>

<a name="0x1_vault_revoke"></a>

## Function `revoke`

Revokes the delegated right to acquire a capability of given type.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_revoke">revoke</a>&lt;Content: drop, store&gt;(cap: &<a href="vault.md#0x1_vault_DelegateCap">vault::DelegateCap</a>&lt;Content&gt;, addr: <b>address</b>, cap_type: <a href="vault.md#0x1_vault_CapType">vault::CapType</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_revoke">revoke</a>&lt;Content: store + drop&gt;(cap: &<a href="vault.md#0x1_vault_DelegateCap">DelegateCap</a>&lt;Content&gt;, addr: <b>address</b>, cap_type: <a href="vault.md#0x1_vault_CapType">CapType</a>)
<b>acquires</b> <a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>, <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>, <a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a> {
    <b>assert</b>!(
        <b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>&lt;Content&gt;&gt;(cap.vault_address),
        <a href="_invalid_state">error::invalid_state</a>(<a href="vault.md#0x1_vault_EDELEGATION_NOT_ENABLED">EDELEGATION_NOT_ENABLED</a>)
    );
    <b>assert</b>!(<b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>&lt;Content&gt;&gt;(addr), <a href="_not_found">error::not_found</a>(<a href="vault.md#0x1_vault_EDELEGATE">EDELEGATE</a>));

    <b>let</b> delegate = <b>borrow_global_mut</b>&lt;<a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>&lt;Content&gt;&gt;(addr);
    <a href="vault.md#0x1_vault_remove_element">remove_element</a>(&<b>mut</b> delegate.granted_caps, &cap_type);

    // If the granted caps of this delegate drop <b>to</b> zero, remove it.
    <b>if</b> (<a href="_is_empty">vector::is_empty</a>(&delegate.granted_caps)) {
        <b>let</b> <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>{ vault_address: _owner, granted_caps: _granted_caps} =
            <b>move_from</b>&lt;<a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>&lt;Content&gt;&gt;(addr);
        <b>let</b> vault_delegates = <b>borrow_global_mut</b>&lt;<a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>&lt;Content&gt;&gt;(cap.vault_address);
        <a href="vault.md#0x1_vault_remove_element">remove_element</a>(&<b>mut</b> vault_delegates.delegates, &addr);
    };

    // Generate <a href="event.md#0x1_event">event</a>.
    <a href="vault.md#0x1_vault_emit_delegate_event">emit_delegate_event</a>(cap, cap_type, addr, <b>true</b>);
}
</code></pre>



</details>

<a name="0x1_vault_revoke_all"></a>

## Function `revoke_all`

Revokes all delegate rights for this vault.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_revoke_all">revoke_all</a>&lt;Content: drop, store&gt;(cap: &<a href="vault.md#0x1_vault_DelegateCap">vault::DelegateCap</a>&lt;Content&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_revoke_all">revoke_all</a>&lt;Content: store + drop&gt;(cap: &<a href="vault.md#0x1_vault_DelegateCap">DelegateCap</a>&lt;Content&gt;)
<b>acquires</b> <a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>, <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>, <a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a> {
    <b>assert</b>!(
        <b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>&lt;Content&gt;&gt;(cap.vault_address),
        <a href="_invalid_state">error::invalid_state</a>(<a href="vault.md#0x1_vault_EDELEGATION_NOT_ENABLED">EDELEGATION_NOT_ENABLED</a>)
    );
    <b>let</b> delegates = &<b>mut</b> <b>borrow_global_mut</b>&lt;<a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>&lt;Content&gt;&gt;(cap.vault_address).delegates;
    <b>while</b> (!<a href="_is_empty">vector::is_empty</a>(delegates)) {
        <b>let</b> addr = <a href="_pop_back">vector::pop_back</a>(delegates);
        <b>let</b> <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>{ vault_address: _vault_address, granted_caps} =
            <b>move_from</b>&lt;<a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>&lt;Content&gt;&gt;(cap.vault_address);
        <b>while</b> (!<a href="_is_empty">vector::is_empty</a>(&granted_caps)) {
            <b>let</b> cap_type = <a href="_pop_back">vector::pop_back</a>(&<b>mut</b> granted_caps);
            <a href="vault.md#0x1_vault_emit_delegate_event">emit_delegate_event</a>(cap, cap_type, addr, <b>true</b>);
        }
    }
}
</code></pre>



</details>

<a name="0x1_vault_remove_element"></a>

## Function `remove_element`

Helper to remove an element from a vector.


<pre><code><b>fun</b> <a href="vault.md#0x1_vault_remove_element">remove_element</a>&lt;E: drop&gt;(v: &<b>mut</b> <a href="">vector</a>&lt;E&gt;, x: &E)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="vault.md#0x1_vault_remove_element">remove_element</a>&lt;E: drop&gt;(v: &<b>mut</b> <a href="">vector</a>&lt;E&gt;, x: &E) {
    <b>let</b> (found, index) = <a href="_index_of">vector::index_of</a>(v, x);
    <b>if</b> (found) {
        <a href="_remove">vector::remove</a>(v, index);
    }
}
</code></pre>



</details>

<a name="0x1_vault_add_element"></a>

## Function `add_element`

Helper to add an element to a vector.


<pre><code><b>fun</b> <a href="vault.md#0x1_vault_add_element">add_element</a>&lt;E: drop&gt;(v: &<b>mut</b> <a href="">vector</a>&lt;E&gt;, x: E)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="vault.md#0x1_vault_add_element">add_element</a>&lt;E: drop&gt;(v: &<b>mut</b> <a href="">vector</a>&lt;E&gt;, x: E) {
    <b>if</b> (!<a href="_contains">vector::contains</a>(v, &x)) {
        <a href="_push_back">vector::push_back</a>(v, x)
    }
}
</code></pre>



</details>

<a name="0x1_vault_emit_delegate_event"></a>

## Function `emit_delegate_event`

Emits a delegation or revocation event if event generation is enabled.


<pre><code><b>fun</b> <a href="vault.md#0x1_vault_emit_delegate_event">emit_delegate_event</a>&lt;Content: drop, store&gt;(cap: &<a href="vault.md#0x1_vault_DelegateCap">vault::DelegateCap</a>&lt;Content&gt;, cap_type: <a href="vault.md#0x1_vault_CapType">vault::CapType</a>, delegate: <b>address</b>, is_revoked: bool)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="vault.md#0x1_vault_emit_delegate_event">emit_delegate_event</a>&lt;Content: store + drop&gt;(
       cap: &<a href="vault.md#0x1_vault_DelegateCap">DelegateCap</a>&lt;Content&gt;,
       cap_type: <a href="vault.md#0x1_vault_CapType">CapType</a>,
       delegate: <b>address</b>,
       is_revoked: bool
) <b>acquires</b> <a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a> {
    <b>if</b> (<b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>&lt;Content&gt;&gt;(cap.vault_address)) {
        <b>let</b> <a href="event.md#0x1_event">event</a> = <a href="vault.md#0x1_vault_VaultDelegateEvent">VaultDelegateEvent</a>{
            metadata: *&<b>borrow_global</b>&lt;<a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>&lt;Content&gt;&gt;(cap.vault_address).metadata,
            vault_address: cap.vault_address,
            authority: cap.authority,
            delegate,
            cap: cap_type,
            is_revoked
        };
        <a href="event.md#0x1_event_emit_event">event::emit_event</a>(&<b>mut</b> <b>borrow_global_mut</b>&lt;<a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>&lt;Content&gt;&gt;(cap.vault_address).delegate_events, <a href="event.md#0x1_event">event</a>);
    }
}
</code></pre>



</details>

<a name="0x1_vault_transfer"></a>

## Function `transfer`

Transfers ownership of the vault to a new signer. All delegations are revoked before transfer,
and the new owner must re-create delegates as needed.


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_transfer">transfer</a>&lt;Content: drop, store&gt;(cap: &<a href="vault.md#0x1_vault_TransferCap">vault::TransferCap</a>&lt;Content&gt;, to_owner: &<a href="">signer</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="vault.md#0x1_vault_transfer">transfer</a>&lt;Content: store + drop&gt;(cap: &<a href="vault.md#0x1_vault_TransferCap">TransferCap</a>&lt;Content&gt;, to_owner: &<a href="">signer</a>)
<b>acquires</b> <a href="vault.md#0x1_vault_Vault">Vault</a>, <a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>, <a href="vault.md#0x1_vault_VaultDelegate">VaultDelegate</a>, <a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a> {
    <b>let</b> new_addr = <a href="_address_of">signer::address_of</a>(to_owner);
    <b>assert</b>!(!<b>exists</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(new_addr), <a href="_already_exists">error::already_exists</a>(<a href="vault.md#0x1_vault_EVAULT">EVAULT</a>));
    <b>assert</b>!(
        <a href="_is_some">option::is_some</a>(&<b>borrow_global</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(cap.vault_address).content),
        <a href="_invalid_state">error::invalid_state</a>(<a href="vault.md#0x1_vault_EACCESSOR_IN_USE">EACCESSOR_IN_USE</a>)
    );

    // Revoke all delegates.
    <b>if</b> (<b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultDelegates">VaultDelegates</a>&lt;Content&gt;&gt;(cap.vault_address)) {
        <b>let</b> delegate_cap = <a href="vault.md#0x1_vault_DelegateCap">DelegateCap</a>&lt;Content&gt;{vault_address: cap.vault_address, authority: cap.authority };
        <a href="vault.md#0x1_vault_revoke_all">revoke_all</a>(&delegate_cap);
    };

    // Emit <a href="event.md#0x1_event">event</a> <b>if</b> <a href="event.md#0x1_event">event</a> generation is enabled. We emit the <a href="event.md#0x1_event">event</a> on the <b>old</b> <a href="vault.md#0x1_vault">vault</a> not the new one.
    <b>if</b> (<b>exists</b>&lt;<a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>&lt;Content&gt;&gt;(cap.vault_address)) {
        <b>let</b> <a href="event.md#0x1_event">event</a> = <a href="vault.md#0x1_vault_VaultTransferEvent">VaultTransferEvent</a> {
            metadata: *&<b>borrow_global</b>&lt;<a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>&lt;Content&gt;&gt;(cap.vault_address).metadata,
            vault_address: cap.vault_address,
            authority: cap.authority,
            new_vault_address: new_addr
        };
        <a href="event.md#0x1_event_emit_event">event::emit_event</a>(&<b>mut</b> <b>borrow_global_mut</b>&lt;<a href="vault.md#0x1_vault_VaultEvents">VaultEvents</a>&lt;Content&gt;&gt;(cap.vault_address).transfer_events, <a href="event.md#0x1_event">event</a>);
    };

    // Move the <a href="vault.md#0x1_vault">vault</a>.
    <b>move_to</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(to_owner, <b>move_from</b>&lt;<a href="vault.md#0x1_vault_Vault">Vault</a>&lt;Content&gt;&gt;(cap.vault_address));
}
</code></pre>



</details>
