
<a name="0x1_option"></a>

# Module `0x1::option`



-  [Struct `Option`](#0x1_option_Option)
-  [Constants](#@Constants_0)
-  [Function `none`](#0x1_option_none)
-  [Function `some`](#0x1_option_some)
-  [Function `is_none`](#0x1_option_is_none)
-  [Function `is_some`](#0x1_option_is_some)
-  [Function `contains`](#0x1_option_contains)
-  [Function `borrow`](#0x1_option_borrow)
-  [Function `borrow_with_default`](#0x1_option_borrow_with_default)
-  [Function `get_with_default`](#0x1_option_get_with_default)
-  [Function `fill`](#0x1_option_fill)
-  [Function `extract`](#0x1_option_extract)
-  [Function `borrow_mut`](#0x1_option_borrow_mut)
-  [Function `swap`](#0x1_option_swap)
-  [Function `swap_or_fill`](#0x1_option_swap_or_fill)
-  [Function `destroy_with_default`](#0x1_option_destroy_with_default)
-  [Function `destroy_some`](#0x1_option_destroy_some)
-  [Function `destroy_none`](#0x1_option_destroy_none)
-  [Function `to_vec`](#0x1_option_to_vec)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="0x1_option_Option"></a>

## Struct `Option`



<pre><code><b>struct</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>vec: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Element&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x1_option_EOPTION_IS_SET"></a>



<pre><code><b>const</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_EOPTION_IS_SET">EOPTION_IS_SET</a>: u64 = 262144;
</code></pre>



<a name="0x1_option_EOPTION_NOT_SET"></a>



<pre><code><b>const</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>: u64 = 262145;
</code></pre>



<a name="0x1_option_none"></a>

## Function `none`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">none</a>&lt;Element&gt;(): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">none</a>&lt;Element&gt;(): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt; {
    <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a> { vec: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_empty">vector::empty</a>() }
}
</code></pre>



</details>

<a name="0x1_option_some"></a>

## Function `some`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">some</a>&lt;Element&gt;(e: Element): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">some</a>&lt;Element&gt;(e: Element): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt; {
    <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a> { vec: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_singleton">vector::singleton</a>(e) }
}
</code></pre>



</details>

<a name="0x1_option_is_none"></a>

## Function `is_none`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_is_none">is_none</a>&lt;Element&gt;(t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_is_none">is_none</a>&lt;Element&gt;(t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): bool {
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&t.vec)
}
</code></pre>



</details>

<a name="0x1_option_is_some"></a>

## Function `is_some`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">is_some</a>&lt;Element&gt;(t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">is_some</a>&lt;Element&gt;(t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): bool {
    !<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&t.vec)
}
</code></pre>



</details>

<a name="0x1_option_contains"></a>

## Function `contains`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_contains">contains</a>&lt;Element&gt;(t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, e_ref: &Element): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_contains">contains</a>&lt;Element&gt;(t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, e_ref: &Element): bool {
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_contains">vector::contains</a>(&t.vec, e_ref)
}
</code></pre>



</details>

<a name="0x1_option_borrow"></a>

## Function `borrow`



<pre><code><b>public</b> <b>fun</b> <a href="../../borrow.md#0x2_borrow">borrow</a>&lt;Element&gt;(t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): &Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../borrow.md#0x2_borrow">borrow</a>&lt;Element&gt;(t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): &Element {
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">is_some</a>(t), <a href="../../dependencies/move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(&t.vec, 0)
}
</code></pre>



</details>

<a name="0x1_option_borrow_with_default"></a>

## Function `borrow_with_default`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_borrow_with_default">borrow_with_default</a>&lt;Element&gt;(t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, default_ref: &Element): &Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_borrow_with_default">borrow_with_default</a>&lt;Element&gt;(t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, default_ref: &Element): &Element {
    <b>let</b> vec_ref = &t.vec;
    <b>if</b> (<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(vec_ref)) default_ref
    <b>else</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(vec_ref, 0)
}
</code></pre>



</details>

<a name="0x1_option_get_with_default"></a>

## Function `get_with_default`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_get_with_default">get_with_default</a>&lt;Element: <b>copy</b>, drop&gt;(t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, default: Element): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_get_with_default">get_with_default</a>&lt;Element: <b>copy</b> + drop&gt;(
    t: &<a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;,
    default: Element,
): Element {
    <b>let</b> vec_ref = &t.vec;
    <b>if</b> (<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(vec_ref)) default
    <b>else</b> *<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow">vector::borrow</a>(vec_ref, 0)
}
</code></pre>



</details>

<a name="0x1_option_fill"></a>

## Function `fill`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_fill">fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, e: Element)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_fill">fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, e: Element) {
    <b>let</b> vec_ref = &<b>mut</b> t.vec;
    <b>if</b> (<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(vec_ref)) <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(vec_ref, e)
    <b>else</b> <b>abort</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_EOPTION_IS_SET">EOPTION_IS_SET</a>
}
</code></pre>



</details>

<a name="0x1_option_extract"></a>

## Function `extract`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_extract">extract</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_extract">extract</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): Element {
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">is_some</a>(t), <a href="../../dependencies/move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> t.vec)
}
</code></pre>



</details>

<a name="0x1_option_borrow_mut"></a>

## Function `borrow_mut`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_borrow_mut">borrow_mut</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): &<b>mut</b> Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_borrow_mut">borrow_mut</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): &<b>mut</b> Element {
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">is_some</a>(t), <a href="../../dependencies/move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_borrow_mut">vector::borrow_mut</a>(&<b>mut</b> t.vec, 0)
}
</code></pre>



</details>

<a name="0x1_option_swap"></a>

## Function `swap`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_swap">swap</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, e: Element): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_swap">swap</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, e: Element): Element {
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">is_some</a>(t), <a href="../../dependencies/move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    <b>let</b> vec_ref = &<b>mut</b> t.vec;
    <b>let</b> old_value = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(vec_ref);
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(vec_ref, e);
    old_value
}
</code></pre>



</details>

<a name="0x1_option_swap_or_fill"></a>

## Function `swap_or_fill`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_swap_or_fill">swap_or_fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, e: Element): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_swap_or_fill">swap_or_fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, e: Element): <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt; {
    <b>let</b> vec_ref = &<b>mut</b> t.vec;
    <b>let</b> old_value = <b>if</b> (<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(vec_ref)) <a href="../../dependencies/move-stdlib/option.md#0x1_option_none">none</a>()
        <b>else</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_some">some</a>(<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(vec_ref));
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_push_back">vector::push_back</a>(vec_ref, e);
    old_value
}
</code></pre>



</details>

<a name="0x1_option_destroy_with_default"></a>

## Function `destroy_with_default`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_destroy_with_default">destroy_with_default</a>&lt;Element: drop&gt;(t: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, default: Element): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_destroy_with_default">destroy_with_default</a>&lt;Element: drop&gt;(t: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, default: Element): Element {
    <b>let</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a> { vec } = t;
    <b>if</b> (<a href="../../dependencies/move-stdlib/vector.md#0x1_vector_is_empty">vector::is_empty</a>(&vec)) default
    <b>else</b> <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> vec)
}
</code></pre>



</details>

<a name="0x1_option_destroy_some"></a>

## Function `destroy_some`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_destroy_some">destroy_some</a>&lt;Element&gt;(t: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_destroy_some">destroy_some</a>&lt;Element&gt;(t: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): Element {
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_some">is_some</a>(&t), <a href="../../dependencies/move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    <b>let</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a> { vec } = t;
    <b>let</b> elem = <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_pop_back">vector::pop_back</a>(&<b>mut</b> vec);
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_destroy_empty">vector::destroy_empty</a>(vec);
    elem
}
</code></pre>



</details>

<a name="0x1_option_destroy_none"></a>

## Function `destroy_none`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_destroy_none">destroy_none</a>&lt;Element&gt;(t: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_destroy_none">destroy_none</a>&lt;Element&gt;(t: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;) {
    <b>assert</b>!(<a href="../../dependencies/move-stdlib/option.md#0x1_option_is_none">is_none</a>(&t), <a href="../../dependencies/move-stdlib/option.md#0x1_option_EOPTION_IS_SET">EOPTION_IS_SET</a>);
    <b>let</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a> { vec } = t;
    <a href="../../dependencies/move-stdlib/vector.md#0x1_vector_destroy_empty">vector::destroy_empty</a>(vec)
}
</code></pre>



</details>

<a name="0x1_option_to_vec"></a>

## Function `to_vec`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_to_vec">to_vec</a>&lt;Element&gt;(t: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_to_vec">to_vec</a>&lt;Element&gt;(t: <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;Element&gt; {
    <b>let</b> <a href="../../dependencies/move-stdlib/option.md#0x1_option_Option">Option</a> { vec } = t;
    vec
}
</code></pre>



</details>
