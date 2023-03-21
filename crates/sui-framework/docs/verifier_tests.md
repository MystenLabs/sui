
<a name="0x2_verifier_tests"></a>

# Module `0x2::verifier_tests`

Tests if normally illegal (in terms of Sui bytecode verification) code is allowed in tests.


-  [Struct `VERIFIER_TESTS`](#0x2_verifier_tests_VERIFIER_TESTS)
-  [Function `init`](#0x2_verifier_tests_init)
-  [Function `is_otw`](#0x2_verifier_tests_is_otw)


<pre><code><b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
<b>use</b> <a href="types.md#0x2_types">0x2::types</a>;
</code></pre>



<a name="0x2_verifier_tests_VERIFIER_TESTS"></a>

## Struct `VERIFIER_TESTS`



<pre><code><b>struct</b> <a href="verifier_tests.md#0x2_verifier_tests_VERIFIER_TESTS">VERIFIER_TESTS</a> <b>has</b> drop
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

<a name="0x2_verifier_tests_init"></a>

## Function `init`



<pre><code><b>fun</b> <a href="verifier_tests.md#0x2_verifier_tests_init">init</a>(otw: <a href="verifier_tests.md#0x2_verifier_tests_VERIFIER_TESTS">verifier_tests::VERIFIER_TESTS</a>, _: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="verifier_tests.md#0x2_verifier_tests_init">init</a>(otw: <a href="verifier_tests.md#0x2_verifier_tests_VERIFIER_TESTS">VERIFIER_TESTS</a>, _: &<b>mut</b> sui::tx_context::TxContext) {
    <b>assert</b>!(sui::types::is_one_time_witness(&otw), 0);
}
</code></pre>



</details>

<a name="0x2_verifier_tests_is_otw"></a>

## Function `is_otw`



<pre><code><b>fun</b> <a href="verifier_tests.md#0x2_verifier_tests_is_otw">is_otw</a>(witness: <a href="verifier_tests.md#0x2_verifier_tests_VERIFIER_TESTS">verifier_tests::VERIFIER_TESTS</a>): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="verifier_tests.md#0x2_verifier_tests_is_otw">is_otw</a>(witness: <a href="verifier_tests.md#0x2_verifier_tests_VERIFIER_TESTS">VERIFIER_TESTS</a>): bool {
    sui::types::is_one_time_witness(&witness)
}
</code></pre>



</details>
