
<a name="0x2_group_ops"></a>

# Module `0x2::group_ops`

Generic Move and native functions for group operations.


-  [Struct `Element`](#0x2_group_ops_Element)
-  [Constants](#@Constants_0)
-  [Function `bytes`](#0x2_group_ops_bytes)
-  [Function `equal`](#0x2_group_ops_equal)
-  [Function `from_bytes`](#0x2_group_ops_from_bytes)
-  [Function `add`](#0x2_group_ops_add)
-  [Function `sub`](#0x2_group_ops_sub)
-  [Function `mul`](#0x2_group_ops_mul)
-  [Function `div`](#0x2_group_ops_div)
-  [Function `hash_to`](#0x2_group_ops_hash_to)
-  [Function `multi_scalar_multiplication`](#0x2_group_ops_multi_scalar_multiplication)
-  [Function `pairing`](#0x2_group_ops_pairing)
-  [Function `internal_validate`](#0x2_group_ops_internal_validate)
-  [Function `internal_add`](#0x2_group_ops_internal_add)
-  [Function `internal_sub`](#0x2_group_ops_internal_sub)
-  [Function `internal_mul`](#0x2_group_ops_internal_mul)
-  [Function `internal_div`](#0x2_group_ops_internal_div)
-  [Function `internal_hash_to`](#0x2_group_ops_internal_hash_to)
-  [Function `internal_multi_scalar_mul`](#0x2_group_ops_internal_multi_scalar_mul)
-  [Function `internal_pairing`](#0x2_group_ops_internal_pairing)
-  [Function `set_as_prefix`](#0x2_group_ops_set_as_prefix)


<pre><code><b>use</b> <a href="">0x1::debug</a>;
<b>use</b> <a href="">0x1::vector</a>;
</code></pre>



<a name="0x2_group_ops_Element"></a>

## Struct `Element`



<pre><code><b>struct</b> <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;T&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>bytes: <a href="">vector</a>&lt;u8&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x2_group_ops_EInvalidBufferLength"></a>



<pre><code><b>const</b> <a href="group_ops.md#0x2_group_ops_EInvalidBufferLength">EInvalidBufferLength</a>: u64 = 1;
</code></pre>



<a name="0x2_group_ops_EInvalidInput"></a>



<pre><code><b>const</b> <a href="group_ops.md#0x2_group_ops_EInvalidInput">EInvalidInput</a>: u64 = 0;
</code></pre>



<a name="0x2_group_ops_bytes"></a>

## Function `bytes`



<pre><code><b>public</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_bytes">bytes</a>&lt;G&gt;(e: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): &<a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_bytes">bytes</a>&lt;G&gt;(e: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): &<a href="">vector</a>&lt;u8&gt; {
    &e.bytes
}
</code></pre>



</details>

<a name="0x2_group_ops_equal"></a>

## Function `equal`



<pre><code><b>public</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_equal">equal</a>&lt;G&gt;(e1: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_equal">equal</a>&lt;G&gt;(e1: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): bool {
    // TODO: Remove before merging
    <a href="_print">debug::print</a>(&e1.bytes);
    <a href="_print">debug::print</a>(&e2.bytes);
    e1.bytes == e2.bytes
}
</code></pre>



</details>

<a name="0x2_group_ops_from_bytes"></a>

## Function `from_bytes`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_from_bytes">from_bytes</a>&lt;G&gt;(type: u8, bytes: &<a href="">vector</a>&lt;u8&gt;, is_trusted: bool): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_from_bytes">from_bytes</a>&lt;G&gt;(type: u8, bytes: &<a href="">vector</a>&lt;u8&gt;, is_trusted: bool): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <b>assert</b>!(is_trusted || <a href="group_ops.md#0x2_group_ops_internal_validate">internal_validate</a>(type, bytes), <a href="group_ops.md#0x2_group_ops_EInvalidInput">EInvalidInput</a>);
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: *bytes }
}
</code></pre>



</details>

<a name="0x2_group_ops_add"></a>

## Function `add`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_add">add</a>&lt;G&gt;(type: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_add">add</a>&lt;G&gt;(type: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_add">internal_add</a>(type, &e1.bytes, &e2.bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_sub"></a>

## Function `sub`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_sub">sub</a>&lt;G&gt;(type: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_sub">sub</a>&lt;G&gt;(type: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_sub">internal_sub</a>(type, &e1.bytes, &e2.bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_mul"></a>

## Function `mul`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_mul">mul</a>&lt;S, G&gt;(type: u8, scalar: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;S&gt;, e: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_mul">mul</a>&lt;S, G&gt;(type: u8, scalar: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;S&gt;, e: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_mul">internal_mul</a>(type, &scalar.bytes, &e.bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_div"></a>

## Function `div`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_div">div</a>&lt;S, G&gt;(type: u8, scalar: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;S&gt;, e: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_div">div</a>&lt;S, G&gt;(type: u8, scalar: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;S&gt;, e: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_div">internal_div</a>(type, &scalar.bytes, &e.bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_hash_to"></a>

## Function `hash_to`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_hash_to">hash_to</a>&lt;G&gt;(type: u8, m: &<a href="">vector</a>&lt;u8&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_hash_to">hash_to</a>&lt;G&gt;(type: u8, m: &<a href="">vector</a>&lt;u8&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_hash_to">internal_hash_to</a>(type, m) }
}
</code></pre>



</details>

<a name="0x2_group_ops_multi_scalar_multiplication"></a>

## Function `multi_scalar_multiplication`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_multi_scalar_multiplication">multi_scalar_multiplication</a>&lt;S, G&gt;(type: u8, scalars: &<a href="">vector</a>&lt;<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;S&gt;&gt;, elements: &<a href="">vector</a>&lt;<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_multi_scalar_multiplication">multi_scalar_multiplication</a>&lt;S, G&gt;(type: u8, scalars: &<a href="">vector</a>&lt;<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;S&gt;&gt;, elements: &<a href="">vector</a>&lt;<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt;&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; {
    <b>assert</b>!(<a href="_length">vector::length</a>(scalars) == <a href="_length">vector::length</a>(elements), <a href="group_ops.md#0x2_group_ops_EInvalidInput">EInvalidInput</a>);
    <b>assert</b>!(<a href="_length">vector::length</a>(scalars) &gt; 0, <a href="group_ops.md#0x2_group_ops_EInvalidInput">EInvalidInput</a>);
    <b>assert</b>!(<a href="_length">vector::length</a>(elements) &lt;= 32, <a href="group_ops.md#0x2_group_ops_EInvalidInput">EInvalidInput</a>); // TODO: other limit?

    <b>let</b> scalars_bytes = <a href="_empty">vector::empty</a>&lt;u8&gt;();
    <b>let</b> elements_bytes = <a href="_empty">vector::empty</a>&lt;u8&gt;();
    <b>let</b> i = 0;
    <b>while</b> (i &lt; <a href="_length">vector::length</a>(scalars)) {
        <b>let</b> scalar_vec = *<a href="_borrow">vector::borrow</a>(scalars, i);
        <a href="_append">vector::append</a>(&<b>mut</b> scalars_bytes, scalar_vec.bytes);
        <b>let</b> element_vec = *<a href="_borrow">vector::borrow</a>(elements, i);
        <a href="_append">vector::append</a>(&<b>mut</b> elements_bytes, element_vec.bytes);
        i = i + 1;
    };
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_multi_scalar_mul">internal_multi_scalar_mul</a>(type, &scalars_bytes, &elements_bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_pairing"></a>

## Function `pairing`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_pairing">pairing</a>&lt;G1, G2, G3&gt;(type: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G1&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G2&gt;): <a href="group_ops.md#0x2_group_ops_Element">group_ops::Element</a>&lt;G3&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_pairing">pairing</a>&lt;G1, G2, G3&gt;(type: u8, e1: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G1&gt;, e2: &<a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G2&gt;): <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G3&gt; {
    <a href="group_ops.md#0x2_group_ops_Element">Element</a>&lt;G3&gt; { bytes: <a href="group_ops.md#0x2_group_ops_internal_pairing">internal_pairing</a>(type, &e1.bytes, &e2.bytes) }
}
</code></pre>



</details>

<a name="0x2_group_ops_internal_validate"></a>

## Function `internal_validate`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_validate">internal_validate</a>(type: u8, bytes: &<a href="">vector</a>&lt;u8&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_validate">internal_validate</a>(type: u8, bytes: &<a href="">vector</a>&lt;u8&gt;): bool;
</code></pre>



</details>

<a name="0x2_group_ops_internal_add"></a>

## Function `internal_add`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_add">internal_add</a>(type: u8, e1: &<a href="">vector</a>&lt;u8&gt;, e2: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_add">internal_add</a>(type: u8, e1: &<a href="">vector</a>&lt;u8&gt;, e2: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_sub"></a>

## Function `internal_sub`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_sub">internal_sub</a>(type: u8, e1: &<a href="">vector</a>&lt;u8&gt;, e2: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_sub">internal_sub</a>(type: u8, e1: &<a href="">vector</a>&lt;u8&gt;, e2: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_mul"></a>

## Function `internal_mul`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_mul">internal_mul</a>(type: u8, e1: &<a href="">vector</a>&lt;u8&gt;, e2: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_mul">internal_mul</a>(type: u8, e1: &<a href="">vector</a>&lt;u8&gt;, e2: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_div"></a>

## Function `internal_div`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_div">internal_div</a>(type: u8, e1: &<a href="">vector</a>&lt;u8&gt;, e2: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_div">internal_div</a>(type: u8, e1: &<a href="">vector</a>&lt;u8&gt;, e2: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_hash_to"></a>

## Function `internal_hash_to`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_hash_to">internal_hash_to</a>(type: u8, m: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_hash_to">internal_hash_to</a>(type: u8, m: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_multi_scalar_mul"></a>

## Function `internal_multi_scalar_mul`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_multi_scalar_mul">internal_multi_scalar_mul</a>(type: u8, scalars: &<a href="">vector</a>&lt;u8&gt;, elements: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_multi_scalar_mul">internal_multi_scalar_mul</a>(type: u8, scalars: &<a href="">vector</a>&lt;u8&gt;, elements: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_internal_pairing"></a>

## Function `internal_pairing`



<pre><code><b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_pairing">internal_pairing</a>(type: u8, e1: &<a href="">vector</a>&lt;u8&gt;, e2: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="group_ops.md#0x2_group_ops_internal_pairing">internal_pairing</a>(type:u8, e1: &<a href="">vector</a>&lt;u8&gt;, e2: &<a href="">vector</a>&lt;u8&gt;): <a href="">vector</a>&lt;u8&gt;;
</code></pre>



</details>

<a name="0x2_group_ops_set_as_prefix"></a>

## Function `set_as_prefix`



<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_set_as_prefix">set_as_prefix</a>(x: u64, big_endian: bool, buffer: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b>(<b>friend</b>) <b>fun</b> <a href="group_ops.md#0x2_group_ops_set_as_prefix">set_as_prefix</a>(x: u64, big_endian: bool, buffer: &<b>mut</b> <a href="">vector</a>&lt;u8&gt;) {
    <b>let</b> buffer_len = <a href="_length">vector::length</a>(buffer);
    <b>assert</b>!(buffer_len &gt; 7, <a href="group_ops.md#0x2_group_ops_EInvalidBufferLength">EInvalidBufferLength</a>);
    <b>let</b> i = 0;
    <b>while</b> (i &lt; 8) {
        <b>let</b> curr_byte = x % 0x100;
        <b>let</b> position = <b>if</b> (big_endian) { buffer_len - i - 1 } <b>else</b> { i };
        <b>let</b> curr_element = <a href="_borrow_mut">vector::borrow_mut</a>(buffer, position);
        *curr_element = (curr_byte <b>as</b> u8);
        x = x &gt;&gt; 8;
        i = i + 1;
    };
}
</code></pre>



</details>
