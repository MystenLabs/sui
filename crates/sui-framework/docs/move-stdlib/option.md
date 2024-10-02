---
title: Module `0x1::option`
---

This module defines the Option type and its methods to represent and handle an optional value.


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


<pre><code><b>use</b> <a href="../move-stdlib/vector.md#0x1_vector">0x1::vector</a>;
</code></pre>



<a name="0x1_option_Option"></a>

## Struct `Option`

Abstraction of a value that may or may not be present. Implemented with a vector of size
zero or one because Move bytecode does not have ADTs.


<pre><code><b>struct</b> <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>vec: <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Element&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="0x1_option_EOPTION_IS_SET"></a>

The <code><a href="../move-stdlib/option.md#0x1_option_Option">Option</a></code> is in an invalid state for the operation attempted.
The <code><a href="../move-stdlib/option.md#0x1_option_Option">Option</a></code> is <code>Some</code> while it should be <code>None</code>.


<pre><code><b>const</b> <a href="../move-stdlib/option.md#0x1_option_EOPTION_IS_SET">EOPTION_IS_SET</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 262144;
</code></pre>



<a name="0x1_option_EOPTION_NOT_SET"></a>

The <code><a href="../move-stdlib/option.md#0x1_option_Option">Option</a></code> is in an invalid state for the operation attempted.
The <code><a href="../move-stdlib/option.md#0x1_option_Option">Option</a></code> is <code>None</code> while it should be <code>Some</code>.


<pre><code><b>const</b> <a href="../move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>: <a href="../move-stdlib/u64.md#0x1_u64">u64</a> = 262145;
</code></pre>



<a name="0x1_option_none"></a>

## Function `none`

Return an empty <code><a href="../move-stdlib/option.md#0x1_option_Option">Option</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_none">none</a>&lt;Element&gt;(): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_none">none</a>&lt;Element&gt;(): <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt; {
    <a href="../move-stdlib/option.md#0x1_option_Option">Option</a> { vec: <a href="../move-stdlib/vector.md#0x1_vector_empty">vector::empty</a>() }
}
</code></pre>



</details>

<a name="0x1_option_some"></a>

## Function `some`

Return an <code><a href="../move-stdlib/option.md#0x1_option_Option">Option</a></code> containing <code>e</code>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_some">some</a>&lt;Element&gt;(e: Element): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_some">some</a>&lt;Element&gt;(e: Element): <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt; {
    <a href="../move-stdlib/option.md#0x1_option_Option">Option</a> { vec: <a href="../move-stdlib/vector.md#0x1_vector_singleton">vector::singleton</a>(e) }
}
</code></pre>



</details>

<a name="0x1_option_is_none"></a>

## Function `is_none`

Return true if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_is_none">is_none</a>&lt;Element&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_is_none">is_none</a>&lt;Element&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): bool {
    t.vec.is_empty()
}
</code></pre>



</details>

<a name="0x1_option_is_some"></a>

## Function `is_some`

Return true if <code>t</code> holds a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_is_some">is_some</a>&lt;Element&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_is_some">is_some</a>&lt;Element&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): bool {
    !t.vec.is_empty()
}
</code></pre>



</details>

<a name="0x1_option_contains"></a>

## Function `contains`

Return true if the value in <code>t</code> is equal to <code>e_ref</code>
Always returns <code><b>false</b></code> if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_contains">contains</a>&lt;Element&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, e_ref: &Element): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_contains">contains</a>&lt;Element&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, e_ref: &Element): bool {
    t.vec.<a href="../move-stdlib/option.md#0x1_option_contains">contains</a>(e_ref)
}
</code></pre>



</details>

<a name="0x1_option_borrow"></a>

## Function `borrow`

Return an immutable reference to the value inside <code>t</code>
Aborts if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_borrow">borrow</a>&lt;Element&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): &Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_borrow">borrow</a>&lt;Element&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): &Element {
    <b>assert</b>!(t.<a href="../move-stdlib/option.md#0x1_option_is_some">is_some</a>(), <a href="../move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    &t.vec[0]
}
</code></pre>



</details>

<a name="0x1_option_borrow_with_default"></a>

## Function `borrow_with_default`

Return a reference to the value inside <code>t</code> if it holds one
Return <code>default_ref</code> if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_borrow_with_default">borrow_with_default</a>&lt;Element&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, default_ref: &Element): &Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_borrow_with_default">borrow_with_default</a>&lt;Element&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, default_ref: &Element): &Element {
    <b>let</b> vec_ref = &t.vec;
    <b>if</b> (vec_ref.is_empty()) default_ref
    <b>else</b> &vec_ref[0]
}
</code></pre>



</details>

<a name="0x1_option_get_with_default"></a>

## Function `get_with_default`

Return the value inside <code>t</code> if it holds one
Return <code>default</code> if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_get_with_default">get_with_default</a>&lt;Element: <b>copy</b>, drop&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, default: Element): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_get_with_default">get_with_default</a>&lt;Element: <b>copy</b> + drop&gt;(t: &<a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, default: Element): Element {
    <b>let</b> vec_ref = &t.vec;
    <b>if</b> (vec_ref.is_empty()) default
    <b>else</b> vec_ref[0]
}
</code></pre>



</details>

<a name="0x1_option_fill"></a>

## Function `fill`

Convert the none option <code>t</code> to a some option by adding <code>e</code>.
Aborts if <code>t</code> already holds a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_fill">fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, e: Element)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_fill">fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, e: Element) {
    <b>let</b> vec_ref = &<b>mut</b> t.vec;
    <b>if</b> (vec_ref.is_empty()) vec_ref.push_back(e)
    <b>else</b> <b>abort</b> <a href="../move-stdlib/option.md#0x1_option_EOPTION_IS_SET">EOPTION_IS_SET</a>
}
</code></pre>



</details>

<a name="0x1_option_extract"></a>

## Function `extract`

Convert a <code>some</code> option to a <code>none</code> by removing and returning the value stored inside <code>t</code>
Aborts if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_extract">extract</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_extract">extract</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): Element {
    <b>assert</b>!(t.<a href="../move-stdlib/option.md#0x1_option_is_some">is_some</a>(), <a href="../move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    t.vec.pop_back()
}
</code></pre>



</details>

<a name="0x1_option_borrow_mut"></a>

## Function `borrow_mut`

Return a mutable reference to the value inside <code>t</code>
Aborts if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_borrow_mut">borrow_mut</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): &<b>mut</b> Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_borrow_mut">borrow_mut</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): &<b>mut</b> Element {
    <b>assert</b>!(t.<a href="../move-stdlib/option.md#0x1_option_is_some">is_some</a>(), <a href="../move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    &<b>mut</b> t.vec[0]
}
</code></pre>



</details>

<a name="0x1_option_swap"></a>

## Function `swap`

Swap the old value inside <code>t</code> with <code>e</code> and return the old value
Aborts if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_swap">swap</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, e: Element): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_swap">swap</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, e: Element): Element {
    <b>assert</b>!(t.<a href="../move-stdlib/option.md#0x1_option_is_some">is_some</a>(), <a href="../move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    <b>let</b> vec_ref = &<b>mut</b> t.vec;
    <b>let</b> old_value = vec_ref.pop_back();
    vec_ref.push_back(e);
    old_value
}
</code></pre>



</details>

<a name="0x1_option_swap_or_fill"></a>

## Function `swap_or_fill`

Swap the old value inside <code>t</code> with <code>e</code> and return the old value;
or if there is no old value, fill it with <code>e</code>.
Different from swap(), swap_or_fill() allows for <code>t</code> not holding a value.


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_swap_or_fill">swap_or_fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, e: Element): <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_swap_or_fill">swap_or_fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, e: Element): <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt; {
    <b>let</b> vec_ref = &<b>mut</b> t.vec;
    <b>let</b> old_value = <b>if</b> (vec_ref.is_empty()) <a href="../move-stdlib/option.md#0x1_option_none">none</a>()
    <b>else</b> <a href="../move-stdlib/option.md#0x1_option_some">some</a>(vec_ref.pop_back());
    vec_ref.push_back(e);
    old_value
}
</code></pre>



</details>

<a name="0x1_option_destroy_with_default"></a>

## Function `destroy_with_default`

Destroys <code>t.</code> If <code>t</code> holds a value, return it. Returns <code>default</code> otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_destroy_with_default">destroy_with_default</a>&lt;Element: drop&gt;(t: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;, default: Element): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_destroy_with_default">destroy_with_default</a>&lt;Element: drop&gt;(t: <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;, default: Element): Element {
    <b>let</b> <a href="../move-stdlib/option.md#0x1_option_Option">Option</a> { <b>mut</b> vec } = t;
    <b>if</b> (vec.is_empty()) default
    <b>else</b> vec.pop_back()
}
</code></pre>



</details>

<a name="0x1_option_destroy_some"></a>

## Function `destroy_some`

Unpack <code>t</code> and return its contents
Aborts if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_destroy_some">destroy_some</a>&lt;Element&gt;(t: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_destroy_some">destroy_some</a>&lt;Element&gt;(t: <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): Element {
    <b>assert</b>!(t.<a href="../move-stdlib/option.md#0x1_option_is_some">is_some</a>(), <a href="../move-stdlib/option.md#0x1_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    <b>let</b> <a href="../move-stdlib/option.md#0x1_option_Option">Option</a> { <b>mut</b> vec } = t;
    <b>let</b> elem = vec.pop_back();
    vec.destroy_empty();
    elem
}
</code></pre>



</details>

<a name="0x1_option_destroy_none"></a>

## Function `destroy_none`

Unpack <code>t</code>
Aborts if <code>t</code> holds a value


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_destroy_none">destroy_none</a>&lt;Element&gt;(t: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_destroy_none">destroy_none</a>&lt;Element&gt;(t: <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;) {
    <b>assert</b>!(t.<a href="../move-stdlib/option.md#0x1_option_is_none">is_none</a>(), <a href="../move-stdlib/option.md#0x1_option_EOPTION_IS_SET">EOPTION_IS_SET</a>);
    <b>let</b> <a href="../move-stdlib/option.md#0x1_option_Option">Option</a> { vec } = t;
    vec.destroy_empty()
}
</code></pre>



</details>

<a name="0x1_option_to_vec"></a>

## Function `to_vec`

Convert <code>t</code> into a vector of length 1 if it is <code>Some</code>,
and an empty vector otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_to_vec">to_vec</a>&lt;Element&gt;(t: <a href="../move-stdlib/option.md#0x1_option_Option">option::Option</a>&lt;Element&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../move-stdlib/option.md#0x1_option_to_vec">to_vec</a>&lt;Element&gt;(t: <a href="../move-stdlib/option.md#0x1_option_Option">Option</a>&lt;Element&gt;): <a href="../move-stdlib/vector.md#0x1_vector">vector</a>&lt;Element&gt; {
    <b>let</b> <a href="../move-stdlib/option.md#0x1_option_Option">Option</a> { vec } = t;
    vec
}
</code></pre>



</details>
