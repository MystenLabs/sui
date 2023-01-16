
<a name="0x2_unsecure"></a>

# Module `0x2::unsecure`



-  [Function `unsecure_verify`](#0x2_unsecure_unsecure_verify)
-  [Function `unsecure_verify_with_domain`](#0x2_unsecure_unsecure_verify_with_domain)


<pre><code><b>use</b> <a href="">0x1::vector</a>;
</code></pre>



<a name="0x2_unsecure_unsecure_verify"></a>

## Function `unsecure_verify`



<pre><code><b>public</b> <b>fun</b> <a href="unsecure.md#0x2_unsecure_unsecure_verify">unsecure_verify</a>(signature: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, msg: &<a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="unsecure.md#0x2_unsecure_unsecure_verify">unsecure_verify</a>(signature: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, msg: &<a href="">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>

<a name="0x2_unsecure_unsecure_verify_with_domain"></a>

## Function `unsecure_verify_with_domain`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="unsecure.md#0x2_unsecure_unsecure_verify_with_domain">unsecure_verify_with_domain</a>(signature: &<a href="">vector</a>&lt;u8&gt;, public_key: &<a href="">vector</a>&lt;u8&gt;, msg: <a href="">vector</a>&lt;u8&gt;, domain: <a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="unsecure.md#0x2_unsecure_unsecure_verify_with_domain">unsecure_verify_with_domain</a>(
    signature: &<a href="">vector</a>&lt;u8&gt;,
    public_key: &<a href="">vector</a>&lt;u8&gt;,
    msg: <a href="">vector</a>&lt;u8&gt;,
    domain: <a href="">vector</a>&lt;u8&gt;
): bool {
    std::vector::append(&<b>mut</b> domain, msg);
    <a href="unsecure.md#0x2_unsecure_unsecure_verify">unsecure_verify</a>(signature, public_key, &domain)
}
</code></pre>



</details>
