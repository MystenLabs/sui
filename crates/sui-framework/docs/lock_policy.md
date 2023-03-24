
<a name="0x2_witness_policy"></a>

# Module `0x2::witness_policy`

Requires a Witness on every transfer. Witness needs to be generated
in some way and presented to the <code>prove</code> method for the TransferRequest
to receive a matching receipt.

One important use case for this policy is the ability to lock something
in the <code>Kiosk</code>. When an item is placed into the Kiosk, a <code>PlacedWitness</code>
struct is created which can be used to prove that the <code>T</code> was placed
to the <code>Kiosk</code>.


-  [Struct `Rule`](#0x2_witness_policy_Rule)
-  [Constants](#@Constants_0)
-  [Function `set`](#0x2_witness_policy_set)
-  [Function `prove`](#0x2_witness_policy_prove)


<pre><code><b>use</b> <a href="transfer_policy.md#0x2_transfer_policy">0x2::transfer_policy</a>;
</code></pre>



<a name="0x2_witness_policy_Rule"></a>

## Struct `Rule`

Custom witness-key for the "proof policy".


<pre><code><b>struct</b> <a href="lock_policy.md#0x2_witness_policy_Rule">Rule</a>&lt;Proof: drop&gt; <b>has</b> drop
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


<a name="0x2_witness_policy_ERuleNotFound"></a>

When a Proof does not find its Rule<Proof>.


<pre><code><b>const</b> <a href="lock_policy.md#0x2_witness_policy_ERuleNotFound">ERuleNotFound</a>: u64 = 0;
</code></pre>



<a name="0x2_witness_policy_set"></a>

## Function `set`

Creator action: adds the Rule.
Requires a "Proof" witness confirmation on every transfer.


<pre><code><b>public</b> <b>fun</b> <a href="lock_policy.md#0x2_witness_policy_set">set</a>&lt;T: store, key, Proof: drop&gt;(policy: &<b>mut</b> <a href="transfer_policy.md#0x2_transfer_policy_TransferPolicy">transfer_policy::TransferPolicy</a>&lt;T&gt;, cap: &<a href="transfer_policy.md#0x2_transfer_policy_TransferPolicyCap">transfer_policy::TransferPolicyCap</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="lock_policy.md#0x2_witness_policy_set">set</a>&lt;T: key + store, Proof: drop&gt;(
    policy: &<b>mut</b> TransferPolicy&lt;T&gt;,
    cap: &TransferPolicyCap&lt;T&gt;
) {
    policy::add_rule(<a href="lock_policy.md#0x2_witness_policy_Rule">Rule</a>&lt;Proof&gt; {}, policy, cap, <b>true</b>);
}
</code></pre>



</details>

<a name="0x2_witness_policy_prove"></a>

## Function `prove`

Buyer action: follow the policy.
Present the required "Proof" instance to get a receipt.


<pre><code><b>public</b> <b>fun</b> <a href="lock_policy.md#0x2_witness_policy_prove">prove</a>&lt;T: store, key, Proof: drop&gt;(_proof: Proof, policy: &<a href="transfer_policy.md#0x2_transfer_policy_TransferPolicy">transfer_policy::TransferPolicy</a>&lt;T&gt;, request: &<b>mut</b> <a href="transfer_policy.md#0x2_transfer_policy_TransferRequest">transfer_policy::TransferRequest</a>&lt;T&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="lock_policy.md#0x2_witness_policy_prove">prove</a>&lt;T: key + store, Proof: drop&gt;(
    _proof: Proof,
    policy: &TransferPolicy&lt;T&gt;,
    request: &<b>mut</b> TransferRequest&lt;T&gt;
) {
    <b>assert</b>!(policy::has_rule&lt;T, <a href="lock_policy.md#0x2_witness_policy_Rule">Rule</a>&lt;Proof&gt;&gt;(policy), <a href="lock_policy.md#0x2_witness_policy_ERuleNotFound">ERuleNotFound</a>);
    policy::add_receipt(<a href="lock_policy.md#0x2_witness_policy_Rule">Rule</a>&lt;Proof&gt; {}, request)
}
</code></pre>



</details>
