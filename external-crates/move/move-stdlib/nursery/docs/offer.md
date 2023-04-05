
<a name="0x1_offer"></a>

# Module `0x1::offer`

Provides a way to transfer structs from one account to another in two transactions.
Unlike many languages, Move cannot move data from one account to another with
single-signer transactions. As of this writing, ordinary transactions can only have
a single signer, and Move code can only store to an address (via <code><b>move_to</b></code>) if it
can supply a reference to a signer for the destination address (there are special case
exceptions in Genesis and DiemAccount where there can temporarily be multiple signers).

Offer solves this problem by providing an <code><a href="offer.md#0x1_offer_Offer">Offer</a></code> resource.  To move a struct <code>T</code> from
account A to B, account A first publishes an <code><a href="offer.md#0x1_offer_Offer">Offer</a>&lt;T&gt;</code> resource at <code><a href="offer.md#0x1_offer_address_of">address_of</a>(A)</code>,
using the <code><a href="offer.md#0x1_offer_create">offer::create</a></code> function.
Then account B, in a separate transaction, can move the struct <code>T</code> from the <code><a href="offer.md#0x1_offer_Offer">Offer</a></code> at
A's address to the desired destination. B accesses the resource using the <code>redeem</code> function,
which aborts unless the <code>for</code> field is B's address (preventing other addresses from
accessing the <code>T</code> that is intended only for B). A can also redeem the <code>T</code> value if B hasn't
redeemed it.


-  [Resource `Offer`](#0x1_offer_Offer)
-  [Constants](#@Constants_0)
-  [Function `create`](#0x1_offer_create)
-  [Function `redeem`](#0x1_offer_redeem)
-  [Function `exists_at`](#0x1_offer_exists_at)
-  [Function `address_of`](#0x1_offer_address_of)
-  [Module Specification](#@Module_Specification_1)
    -  [Access Control](#@Access_Control_2)
        -  [Creation of Offers](#@Creation_of_Offers_3)
        -  [Removal of Offers](#@Removal_of_Offers_4)
    -  [Helper Functions](#@Helper_Functions_5)


<pre><code><b>use</b> <a href="">0x1::error</a>;
<b>use</b> <a href="">0x1::signer</a>;
</code></pre>



<a name="0x1_offer_Offer"></a>

## Resource `Offer`

A wrapper around value <code>offered</code> that can be claimed by the address stored in <code>for</code>.


<pre><code><b>struct</b> <a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt; <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>offered: Offered</code>
</dt>
<dd>

</dd>
<dt>
<code>for: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x1_offer_EOFFER_ALREADY_CREATED"></a>

Address already has an offer of this type.


<pre><code><b>const</b> <a href="offer.md#0x1_offer_EOFFER_ALREADY_CREATED">EOFFER_ALREADY_CREATED</a>: u64 = 1;
</code></pre>



<a name="0x1_offer_EOFFER_DNE_FOR_ACCOUNT"></a>

An offer of the specified type for the account does not exist


<pre><code><b>const</b> <a href="offer.md#0x1_offer_EOFFER_DNE_FOR_ACCOUNT">EOFFER_DNE_FOR_ACCOUNT</a>: u64 = 0;
</code></pre>



<a name="0x1_offer_EOFFER_DOES_NOT_EXIST"></a>

Address does not have an offer of this type to redeem.


<pre><code><b>const</b> <a href="offer.md#0x1_offer_EOFFER_DOES_NOT_EXIST">EOFFER_DOES_NOT_EXIST</a>: u64 = 2;
</code></pre>



<a name="0x1_offer_create"></a>

## Function `create`

Publish a value of type <code>Offered</code> under the sender's account. The value can be claimed by
either the <code>for</code> address or the transaction sender.


<pre><code><b>public</b> <b>fun</b> <a href="offer.md#0x1_offer_create">create</a>&lt;Offered: store&gt;(account: &<a href="">signer</a>, offered: Offered, for: <b>address</b>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="offer.md#0x1_offer_create">create</a>&lt;Offered: store&gt;(account: &<a href="">signer</a>, offered: Offered, for: <b>address</b>) {
  <b>assert</b>!(!<b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(<a href="_address_of">signer::address_of</a>(account)), <a href="_already_exists">error::already_exists</a>(<a href="offer.md#0x1_offer_EOFFER_ALREADY_CREATED">EOFFER_ALREADY_CREATED</a>));
  <b>move_to</b>(account, <a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt; { offered, for });
}
</code></pre>



</details>

<details>
<summary>Specification</summary>


Offer a struct to the account under address <code>for</code> by
placing the offer under the signer's address


<pre><code><b>aborts_if</b> <b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(<a href="_address_of">signer::address_of</a>(account));
<b>ensures</b> <b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(<a href="_address_of">signer::address_of</a>(account));
<b>ensures</b> <b>global</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(<a href="_address_of">signer::address_of</a>(account)) == <a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt; { offered: offered, for: for };
</code></pre>



</details>

<a name="0x1_offer_redeem"></a>

## Function `redeem`

Claim the value of type <code>Offered</code> published at <code>offer_address</code>.
Only succeeds if the sender is the intended recipient stored in <code>for</code> or the original
publisher <code>offer_address</code>.
Also fails if there is no <code><a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;</code> published.


<pre><code><b>public</b> <b>fun</b> <a href="offer.md#0x1_offer_redeem">redeem</a>&lt;Offered: store&gt;(account: &<a href="">signer</a>, offer_address: <b>address</b>): Offered
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="offer.md#0x1_offer_redeem">redeem</a>&lt;Offered: store&gt;(account: &<a href="">signer</a>, offer_address: <b>address</b>): Offered <b>acquires</b> <a href="offer.md#0x1_offer_Offer">Offer</a> {
  <b>assert</b>!(<b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_address), <a href="_not_found">error::not_found</a>(<a href="offer.md#0x1_offer_EOFFER_DOES_NOT_EXIST">EOFFER_DOES_NOT_EXIST</a>));
  <b>let</b> <a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt; { offered, for } = <b>move_from</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_address);
  <b>let</b> sender = <a href="_address_of">signer::address_of</a>(account);
  <b>assert</b>!(sender == for || sender == offer_address, <a href="_invalid_argument">error::invalid_argument</a>(<a href="offer.md#0x1_offer_EOFFER_DNE_FOR_ACCOUNT">EOFFER_DNE_FOR_ACCOUNT</a>));
  offered
}
</code></pre>



</details>

<details>
<summary>Specification</summary>


Aborts if there is no offer under <code>offer_address</code> or if the account
cannot redeem the offer.
Ensures that the offered struct under <code>offer_address</code> is removed.


<pre><code><b>aborts_if</b> !<b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_address);
<b>aborts_if</b> !<a href="offer.md#0x1_offer_is_allowed_recipient">is_allowed_recipient</a>&lt;Offered&gt;(offer_address, <a href="_address_of">signer::address_of</a>(account));
<b>ensures</b> !<b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_address);
<b>ensures</b> result == <b>old</b>(<b>global</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_address).offered);
</code></pre>



</details>

<a name="0x1_offer_exists_at"></a>

## Function `exists_at`



<pre><code><b>public</b> <b>fun</b> <a href="offer.md#0x1_offer_exists_at">exists_at</a>&lt;Offered: store&gt;(offer_address: <b>address</b>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="offer.md#0x1_offer_exists_at">exists_at</a>&lt;Offered: store&gt;(offer_address: <b>address</b>): bool {
  <b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_address)
}
</code></pre>



</details>

<details>
<summary>Specification</summary>



<pre><code><b>aborts_if</b> <b>false</b>;
</code></pre>


Returns whether or not an <code><a href="offer.md#0x1_offer_Offer">Offer</a></code> resource is under the given address <code>offer_address</code>.


<pre><code><b>ensures</b> result == <b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_address);
</code></pre>



</details>

<a name="0x1_offer_address_of"></a>

## Function `address_of`



<pre><code><b>public</b> <b>fun</b> <a href="offer.md#0x1_offer_address_of">address_of</a>&lt;Offered: store&gt;(offer_address: <b>address</b>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="offer.md#0x1_offer_address_of">address_of</a>&lt;Offered: store&gt;(offer_address: <b>address</b>): <b>address</b> <b>acquires</b> <a href="offer.md#0x1_offer_Offer">Offer</a> {
  <b>assert</b>!(<b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_address), <a href="_not_found">error::not_found</a>(<a href="offer.md#0x1_offer_EOFFER_DOES_NOT_EXIST">EOFFER_DOES_NOT_EXIST</a>));
  <b>borrow_global</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_address).for
}
</code></pre>



</details>

<details>
<summary>Specification</summary>


Aborts is there is no offer resource <code><a href="offer.md#0x1_offer_Offer">Offer</a></code> at the <code>offer_address</code>.
Returns the address of the intended recipient of the Offer
under the <code>offer_address</code>.


<pre><code><b>aborts_if</b> !<b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_address);
<b>ensures</b> result == <b>global</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_address).for;
</code></pre>



</details>

<a name="@Module_Specification_1"></a>

## Module Specification



<a name="@Access_Control_2"></a>

### Access Control


<a name="@Creation_of_Offers_3"></a>

#### Creation of Offers



<a name="0x1_offer_NoOfferCreated"></a>

Says no offer is created for any address. Later, it is applied to all functions
except <code>create</code>


<pre><code><b>schema</b> <a href="offer.md#0x1_offer_NoOfferCreated">NoOfferCreated</a>&lt;Offered&gt; {
    <b>ensures</b> <b>forall</b> addr: <b>address</b> <b>where</b> !<b>old</b>(<b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(addr)) : !<b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(addr);
}
</code></pre>



Apply OnlyCreateCanCreateOffer to every function except <code>create</code>


<pre><code><b>apply</b> <a href="offer.md#0x1_offer_NoOfferCreated">NoOfferCreated</a>&lt;Offered&gt; <b>to</b> *&lt;Offered&gt;, * <b>except</b> create;
</code></pre>



<a name="@Removal_of_Offers_4"></a>

#### Removal of Offers



<a name="0x1_offer_NoOfferRemoved"></a>

Says no offer is removed for any address. Applied below to everything except <code>redeem</code>


<pre><code><b>schema</b> <a href="offer.md#0x1_offer_NoOfferRemoved">NoOfferRemoved</a>&lt;Offered&gt; {
    <b>ensures</b> <b>forall</b> addr: <b>address</b> <b>where</b> <b>old</b>(<b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(addr)) :
              (<b>exists</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(addr) && <b>global</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(addr) == <b>old</b>(<b>global</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(addr)));
}
</code></pre>



Only <code>redeem</code> can remove an offer from the global store.


<pre><code><b>apply</b> <a href="offer.md#0x1_offer_NoOfferRemoved">NoOfferRemoved</a>&lt;Offered&gt; <b>to</b> *&lt;Offered&gt;, * <b>except</b> redeem;
</code></pre>



<a name="@Helper_Functions_5"></a>

### Helper Functions


Returns true if the recipient is allowed to redeem <code><a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;</code> at <code>offer_address</code>
and false otherwise.


<a name="0x1_offer_is_allowed_recipient"></a>


<pre><code><b>fun</b> <a href="offer.md#0x1_offer_is_allowed_recipient">is_allowed_recipient</a>&lt;Offered&gt;(offer_addr: <b>address</b>, recipient: <b>address</b>): bool {
  recipient == <b>global</b>&lt;<a href="offer.md#0x1_offer_Offer">Offer</a>&lt;Offered&gt;&gt;(offer_addr).for || recipient == offer_addr
}
</code></pre>
