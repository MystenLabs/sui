
<a name="0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some"></a>

# Module `0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff::some`



-  [Function `some`](#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some_some)


<pre><code></code></pre>



<a name="0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some_some"></a>

## Function `some`

This script does really nothing but just aborts.


<pre><code><b>public</b> entry <b>fun</b> <a href="some_script.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some">some</a>&lt;T&gt;(_account: signer)
</code></pre>



##### Implementation


<pre><code><b>fun</b> <a href="some_script.md#0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_some">some</a>&lt;T&gt;(_account: signer) {
    <b>abort</b> 1
}
</code></pre>
