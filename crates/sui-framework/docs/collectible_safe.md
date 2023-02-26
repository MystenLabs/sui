
<a name="0x2_collectible_safe"></a>

# Module `0x2::collectible_safe`



-  [Struct `TransferRequest`](#0x2_collectible_safe_TransferRequest)


<pre><code></code></pre>



<a name="0x2_collectible_safe_TransferRequest"></a>

## Struct `TransferRequest`

A Hot Potato making sure the buyer gets an authorization
from the owner of the T to perform a transfer after a purchase.

Contains the amount paid for the <code>T</code> so the commission could be
calculated; <code>from</code> field contains the seller of the <code>T</code>.


<pre><code><b>struct</b> <a href="collectible_safe.md#0x2_collectible_safe_TransferRequest">TransferRequest</a>&lt;T: store, key&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>inner: T</code>
</dt>
<dd>

</dd>
<dt>
<code>paid: u64</code>
</dt>
<dd>

</dd>
<dt>
<code>from: <b>address</b></code>
</dt>
<dd>

</dd>
</dl>


</details>
