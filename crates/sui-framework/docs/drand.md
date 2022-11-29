
<a name="0x2_drand"></a>

# Module `0x2::drand`

Module for working with drand (https://drand.love/), a distributed randomness beacon.
Hardcoded to work with the main drand chain 8990e7a9aaed2ffed73dbd7092123d6f289930540d7651336225dc172e51b2ce
WARNING: Using this module places the drand committee (and all Drand committees of the past) on
a) the shared secret's secrecy (no collusion)
b) producing new rounds every PERIOD seconds (only relevant for using <code><a href="drand.md#0x2_drand_unsafe_unix_time_seconds">drand::unsafe_unix_time_seconds</a></code> as a time reference


-  [Resource `Drand`](#0x2_drand_Drand)
-  [Constants](#@Constants_0)
-  [Function `init`](#0x2_drand_init)
-  [Function `advance`](#0x2_drand_advance)
-  [Function `jump`](#0x2_drand_jump)
-  [Function `round`](#0x2_drand_round)
-  [Function `sig`](#0x2_drand_sig)
-  [Function `unsafe_unix_time_seconds`](#0x2_drand_unsafe_unix_time_seconds)
-  [Function `unsafe_timestamp_has_passed`](#0x2_drand_unsafe_timestamp_has_passed)
-  [Function `verify_time_has_passed`](#0x2_drand_verify_time_has_passed)
-  [Function `verify_drand_signature`](#0x2_drand_verify_drand_signature)
-  [Function `derive_randomness`](#0x2_drand_derive_randomness)
-  [Function `safe_selection`](#0x2_drand_safe_selection)
-  [Function `chain`](#0x2_drand_chain)
-  [Function `public_key`](#0x2_drand_public_key)
-  [Function `period`](#0x2_drand_period)


<pre><code><b>use</b> <a href="">0x1::hash</a>;
<b>use</b> <a href="">0x1::vector</a>;
<b>use</b> <a href="bls12381.md#0x2_bls12381">0x2::bls12381</a>;
<b>use</b> <a href="object.md#0x2_object">0x2::object</a>;
<b>use</b> <a href="transfer.md#0x2_transfer">0x2::transfer</a>;
<b>use</b> <a href="tx_context.md#0x2_tx_context">0x2::tx_context</a>;
</code></pre>



<a name="0x2_drand_Drand"></a>

## Resource `Drand`

Shared object recording drand progress. Anyone can advance the round by providing a valid signature


<pre><code><b>struct</b> <a href="drand.md#0x2_drand_Drand">Drand</a> <b>has</b> key
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>id: <a href="object.md#0x2_object_UID">object::UID</a></code>
</dt>
<dd>

</dd>
<dt>
<code>previous_round: u64</code>
</dt>
<dd>
 Most recent drand round we have seen
</dd>
<dt>
<code>previous_sig: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>
 Signature from the last drand round
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_drand_CHAIN"></a>



<pre><code><b>const</b> <a href="drand.md#0x2_drand_CHAIN">CHAIN</a>: <a href="">vector</a>&lt;u8&gt; = [137, 144, 231, 169, 170, 237, 47, 254, 215, 61, 189, 112, 146, 18, 61, 111, 40, 153, 48, 84, 13, 118, 81, 51, 98, 37, 220, 23, 46, 81, 178, 206];
</code></pre>



<a name="0x2_drand_DRAND_PK"></a>

The public key of CHAIN


<pre><code><b>const</b> <a href="drand.md#0x2_drand_DRAND_PK">DRAND_PK</a>: <a href="">vector</a>&lt;u8&gt; = [134, 143, 0, 94, 184, 230, 228, 202, 10, 71, 200, 167, 124, 234, 165, 48, 154, 71, 151, 138, 124, 113, 188, 92, 206, 150, 54, 107, 93, 122, 86, 153, 55, 197, 41, 238, 218, 102, 199, 41, 55, 132, 169, 64, 40, 1, 175, 49];
</code></pre>



<a name="0x2_drand_EInvalidProof"></a>

Could not verify signature on Drand


<pre><code><b>const</b> <a href="drand.md#0x2_drand_EInvalidProof">EInvalidProof</a>: u64 = 1;
</code></pre>



<a name="0x2_drand_EInvalidRndLength"></a>

Expected a 16 byte RND, but got a different length


<pre><code><b>const</b> <a href="drand.md#0x2_drand_EInvalidRndLength">EInvalidRndLength</a>: u64 = 0;
</code></pre>



<a name="0x2_drand_ERoundAlreadyPassed"></a>

Trying to advance to a round that has already passed


<pre><code><b>const</b> <a href="drand.md#0x2_drand_ERoundAlreadyPassed">ERoundAlreadyPassed</a>: u64 = 2;
</code></pre>



<a name="0x2_drand_GENESIS"></a>

The genesis time of CHAIN


<pre><code><b>const</b> <a href="drand.md#0x2_drand_GENESIS">GENESIS</a>: u64 = 1595431050;
</code></pre>



<a name="0x2_drand_PERIOD"></a>

Time in seconds between randomness beacon rounds for CHAIN


<pre><code><b>const</b> <a href="drand.md#0x2_drand_PERIOD">PERIOD</a>: u64 = 30;
</code></pre>



<a name="0x2_drand_init"></a>

## Function `init`



<pre><code><b>fun</b> <a href="drand.md#0x2_drand_init">init</a>(ctx: &<b>mut</b> <a href="tx_context.md#0x2_tx_context_TxContext">tx_context::TxContext</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="drand.md#0x2_drand_init">init</a>(ctx: &<b>mut</b> TxContext) {
    // initialize at a fairly recent round
    <a href="transfer.md#0x2_transfer_share_object">transfer::share_object</a>(
        <a href="drand.md#0x2_drand_Drand">Drand</a> {
            id: <a href="object.md#0x2_object_new">object::new</a>(ctx),
            previous_round: 2475343,
            previous_sig: x"880c20aa83669a234f7b633917e43b7afb4e72421f0f520f7f570559356ed72e15867a77513346fbd37401709fc2192c1830936f23a751c228d091527290b304f2c6fa93851ca92c5ae7c3f7d7e0d06c65b561a6d6afeabce080f442f68d3845"
        }
    )
}
</code></pre>



</details>

<a name="0x2_drand_advance"></a>

## Function `advance`

Advance <code><a href="drand.md#0x2_drand">drand</a></code> by one round


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_advance">advance</a>(<a href="drand.md#0x2_drand">drand</a>: &<b>mut</b> <a href="drand.md#0x2_drand_Drand">drand::Drand</a>, sig: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_advance">advance</a>(<a href="drand.md#0x2_drand">drand</a>: &<b>mut</b> <a href="drand.md#0x2_drand_Drand">Drand</a>, sig: <a href="">vector</a>&lt;u8&gt;) {
    <b>let</b> round = <a href="drand.md#0x2_drand">drand</a>.previous_round + 1;
    <a href="drand.md#0x2_drand_verify_drand_signature">verify_drand_signature</a>(sig, <a href="drand.md#0x2_drand">drand</a>.previous_sig, round);
    <a href="drand.md#0x2_drand">drand</a>.previous_sig = sig;
    <a href="drand.md#0x2_drand">drand</a>.previous_round = round;
}
</code></pre>



</details>

<a name="0x2_drand_jump"></a>

## Function `jump`

Advance <code><a href="drand.md#0x2_drand">drand</a></code> by an arbitrary number of rounds.
Aborts if <code>round</code> is smaler than the most recent round <code><a href="drand.md#0x2_drand">drand</a></code> has seen


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_jump">jump</a>(<a href="drand.md#0x2_drand">drand</a>: &<b>mut</b> <a href="drand.md#0x2_drand_Drand">drand::Drand</a>, sig: <a href="">vector</a>&lt;u8&gt;, previous_sig: <a href="">vector</a>&lt;u8&gt;, round: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_jump">jump</a>(<a href="drand.md#0x2_drand">drand</a>: &<b>mut</b> <a href="drand.md#0x2_drand_Drand">Drand</a>, sig: <a href="">vector</a>&lt;u8&gt;, previous_sig: <a href="">vector</a>&lt;u8&gt;, round: u64) {
    <b>assert</b>!(round &gt; <a href="drand.md#0x2_drand">drand</a>.previous_round, <a href="drand.md#0x2_drand_ERoundAlreadyPassed">ERoundAlreadyPassed</a>);
    <a href="drand.md#0x2_drand_verify_drand_signature">verify_drand_signature</a>(sig, previous_sig, round);
    <a href="drand.md#0x2_drand">drand</a>.previous_sig = sig;
    <a href="drand.md#0x2_drand">drand</a>.previous_round = round;
}
</code></pre>



</details>

<a name="0x2_drand_round"></a>

## Function `round`

Return the most recent round we have seen


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_round">round</a>(<a href="drand.md#0x2_drand">drand</a>: &<a href="drand.md#0x2_drand_Drand">drand::Drand</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_round">round</a>(<a href="drand.md#0x2_drand">drand</a>: &<a href="drand.md#0x2_drand_Drand">Drand</a>): u64 {
    <a href="drand.md#0x2_drand">drand</a>.previous_round
}
</code></pre>



</details>

<a name="0x2_drand_sig"></a>

## Function `sig`

Return the most recent signature we have seen


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_sig">sig</a>(<a href="drand.md#0x2_drand">drand</a>: &<a href="drand.md#0x2_drand_Drand">drand::Drand</a>): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_sig">sig</a>(<a href="drand.md#0x2_drand">drand</a>: &<a href="drand.md#0x2_drand_Drand">Drand</a>): <a href="">vector</a>&lt;u8&gt; {
    <a href="drand.md#0x2_drand">drand</a>.previous_sig
}
</code></pre>



</details>

<a name="0x2_drand_unsafe_unix_time_seconds"></a>

## Function `unsafe_unix_time_seconds`

Return the most recent Unix time we have seen in seconds, according to drand.
This time is coarse-grained--it will only advance by increments of PERIOD (30 seconds).
*WARNING*: Although drand is supposed to produce a new round every 30 seconds
and always has since GENESIS, it is possible for the gaps between rounds to be > or < 30 seconds, and future.
versions of the protocol may change PERIOD.
Thus, this function should only be used as an approximation of Unix time for non-production use-cases
--for anything security-critical, please use either Sui epoch time, and orcale, or the forthcoming
protocol time feature https://github.com/MystenLabs/sui/issues/226.
Finally, note that this is not an up-to-date time unless <code><a href="drand.md#0x2_drand">drand</a></code> has been updated recently
This function will likely be removed before Sui mainnet.


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_unsafe_unix_time_seconds">unsafe_unix_time_seconds</a>(<a href="drand.md#0x2_drand">drand</a>: &<a href="drand.md#0x2_drand_Drand">drand::Drand</a>): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_unsafe_unix_time_seconds">unsafe_unix_time_seconds</a>(<a href="drand.md#0x2_drand">drand</a>: &<a href="drand.md#0x2_drand_Drand">Drand</a>): u64 {
    <a href="drand.md#0x2_drand_GENESIS">GENESIS</a> + (<a href="drand.md#0x2_drand_PERIOD">PERIOD</a> * <a href="drand.md#0x2_drand">drand</a>.previous_round - 1)
}
</code></pre>



</details>

<a name="0x2_drand_unsafe_timestamp_has_passed"></a>

## Function `unsafe_timestamp_has_passed`

Return <code><b>true</b></code> if <code>unix_timestamp</code> is confirmed to be in the past based on drand updates.
Note that a return value of <code><b>false</b></code> does *not* necesarily mean that <code>unix_timestamp</code> is in
in the future--<code><a href="drand.md#0x2_drand">drand</a></code> might just be out-of-date.
*WARNING*: Although drand is supposed to produce a new round every 30 seconds
and always has since GENESIS, it is possible for the gaps between rounds to be > or < 30 seconds, and future.
versions of the protocol may change PERIOD.
Thus, this function should only be used as an approximation of Unix time for non-production use-cases
--for anything security-critical, please use either Sui epoch time, and orcale, or the forthcoming
protocol time feature https://github.com/MystenLabs/sui/issues/226.
Finally, note that this is not an up-to-date time unless <code><a href="drand.md#0x2_drand">drand</a></code> has been updated recently
This function will likely be removed before Sui mainnet.


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_unsafe_timestamp_has_passed">unsafe_timestamp_has_passed</a>(<a href="drand.md#0x2_drand">drand</a>: &<a href="drand.md#0x2_drand_Drand">drand::Drand</a>, unix_timestamp: u64): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_unsafe_timestamp_has_passed">unsafe_timestamp_has_passed</a>(<a href="drand.md#0x2_drand">drand</a>: &<a href="drand.md#0x2_drand_Drand">Drand</a>, unix_timestamp: u64): bool {
    unix_timestamp &lt; <a href="drand.md#0x2_drand_unsafe_unix_time_seconds">unsafe_unix_time_seconds</a>(<a href="drand.md#0x2_drand">drand</a>)
}
</code></pre>



</details>

<a name="0x2_drand_verify_time_has_passed"></a>

## Function `verify_time_has_passed`

Check that a given epoch time has passed by verifying a drand signature from a later time.
round must be at least (epoch_time - GENESIS) / PERIOD + 1).


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_verify_time_has_passed">verify_time_has_passed</a>(epoch_time: u64, sig: <a href="">vector</a>&lt;u8&gt;, prev_sig: <a href="">vector</a>&lt;u8&gt;, round: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_verify_time_has_passed">verify_time_has_passed</a>(epoch_time: u64, sig: <a href="">vector</a>&lt;u8&gt;, prev_sig: <a href="">vector</a>&lt;u8&gt;, round: u64) {
    <b>assert</b>!(epoch_time &lt;= <a href="drand.md#0x2_drand_GENESIS">GENESIS</a> + <a href="drand.md#0x2_drand_PERIOD">PERIOD</a> * (round - 1), <a href="drand.md#0x2_drand_ERoundAlreadyPassed">ERoundAlreadyPassed</a>);
    <a href="drand.md#0x2_drand_verify_drand_signature">verify_drand_signature</a>(sig, prev_sig, round);
}
</code></pre>



</details>

<a name="0x2_drand_verify_drand_signature"></a>

## Function `verify_drand_signature`

Check a drand output.


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_verify_drand_signature">verify_drand_signature</a>(sig: <a href="">vector</a>&lt;u8&gt;, prev_sig: <a href="">vector</a>&lt;u8&gt;, round: u64)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_verify_drand_signature">verify_drand_signature</a>(sig: <a href="">vector</a>&lt;u8&gt;, prev_sig: <a href="">vector</a>&lt;u8&gt;, round: u64) {
    // Convert round <b>to</b> a byte array in big-endian order.
    <b>let</b> round_bytes: <a href="">vector</a>&lt;u8&gt; = <a href="">vector</a>[0, 0, 0, 0, 0, 0, 0, 0];
    <b>let</b> i = 7;
    <b>while</b> (i &gt; 0) {
        <b>let</b> curr_byte = round % 0x100;
        <b>let</b> curr_element = <a href="_borrow_mut">vector::borrow_mut</a>(&<b>mut</b> round_bytes, i);
        *curr_element = (curr_byte <b>as</b> u8);
        round = round &gt;&gt; 8;
        i = i - 1;
    };

    // Compute sha256(prev_sig, round_bytes).
    <a href="_append">vector::append</a>(&<b>mut</b> prev_sig, round_bytes);
    <b>let</b> <a href="digest.md#0x2_digest">digest</a> = sha2_256(prev_sig);
    // Verify the signature on the <a href="">hash</a>.
    <b>assert</b>!(<a href="bls12381.md#0x2_bls12381_bls12381_min_pk_verify">bls12381::bls12381_min_pk_verify</a>(&sig, &<a href="drand.md#0x2_drand_DRAND_PK">DRAND_PK</a>, &<a href="digest.md#0x2_digest">digest</a>), <a href="drand.md#0x2_drand_EInvalidProof">EInvalidProof</a>);
}
</code></pre>



</details>

<a name="0x2_drand_derive_randomness"></a>

## Function `derive_randomness`

Derive a uniform vector from a drand signature.


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_derive_randomness">derive_randomness</a>(drand_sig: <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_derive_randomness">derive_randomness</a>(drand_sig: <a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt; {
    sha2_256(drand_sig)
}
</code></pre>



</details>

<a name="0x2_drand_safe_selection"></a>

## Function `safe_selection`

Converts the first 16 bytes of rnd to a u128 number and outputs its modulo with input n.
Since n is u64, the output is at most 2^{-64} biased assuming rnd is uniformly random.


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_safe_selection">safe_selection</a>(n: u64, rnd: <a href="">vector</a>&lt;u8&gt;): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_safe_selection">safe_selection</a>(n: u64, rnd: <a href="">vector</a>&lt;u8&gt;): u64 {
    <b>assert</b>!(<a href="_length">vector::length</a>(&rnd) &gt;= 16, <a href="drand.md#0x2_drand_EInvalidRndLength">EInvalidRndLength</a>);
    <b>let</b> m: u128 = 0;
    <b>let</b> i = 0;
    <b>while</b> (i &lt; 16) {
        m = m &lt;&lt; 8;
        <b>let</b> curr_byte = *<a href="_borrow">vector::borrow</a>(&rnd, i);
        m = m + (curr_byte <b>as</b> u128);
        i = i + 1;
    };
    <b>let</b> n_128 = (n <b>as</b> u128);
    <b>let</b> module_128  = m % n_128;
    <b>let</b> res = (module_128 <b>as</b> u64);
    res
}
</code></pre>



</details>

<a name="0x2_drand_chain"></a>

## Function `chain`

Return the chain ID of the drand instance used by this module


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_chain">chain</a>(): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_chain">chain</a>(): <a href="">vector</a>&lt;u8&gt; {
    <a href="drand.md#0x2_drand_CHAIN">CHAIN</a>
}
</code></pre>



</details>

<a name="0x2_drand_public_key"></a>

## Function `public_key`

Return the public key of the drand instance used by this module


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_public_key">public_key</a>(): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_public_key">public_key</a>(): <a href="">vector</a>&lt;u8&gt; {
    <a href="drand.md#0x2_drand_DRAND_PK">DRAND_PK</a>
}
</code></pre>



</details>

<a name="0x2_drand_period"></a>

## Function `period`

Return the period of the drand instance used by this module


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_period">period</a>(): u64
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="drand.md#0x2_drand_period">period</a>(): u64 {
    <a href="drand.md#0x2_drand_PERIOD">PERIOD</a>
}
</code></pre>



</details>
