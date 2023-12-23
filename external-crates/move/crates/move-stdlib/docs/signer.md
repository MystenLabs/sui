
<a name="0x1_signer"></a>

# Module `0x1::signer`



-  [Function `borrow_address`](#0x1_signer_borrow_address)
-  [Function `address_of`](#0x1_signer_address_of)


<pre><code></code></pre>



<a name="0x1_signer_borrow_address"></a>

## Function `borrow_address`



<pre><code><b>public</b> <b>fun</b> <a href="signer.md#0x1_signer_borrow_address">borrow_address</a>(s: &<a href="signer.md#0x1_signer">signer</a>): &<b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>public</b> <b>fun</b> <a href="signer.md#0x1_signer_borrow_address">borrow_address</a>(s: &<a href="signer.md#0x1_signer">signer</a>): &<b>address</b>;
</code></pre>



</details>

<a name="0x1_signer_address_of"></a>

## Function `address_of`



<pre><code><b>public</b> <b>fun</b> <a href="signer.md#0x1_signer_address_of">address_of</a>(s: &<a href="signer.md#0x1_signer">signer</a>): <b>address</b>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="signer.md#0x1_signer_address_of">address_of</a>(s: &<a href="signer.md#0x1_signer">signer</a>): <b>address</b> {
    *<a href="signer.md#0x1_signer_borrow_address">borrow_address</a>(s)
}
</code></pre>



</details>


[//]: # ("File containing references which can be used from documentation")
