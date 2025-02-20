
<a name="std_option"></a>

# Module `std::option`

This module defines the Option type and its methods to represent and handle an optional value.


-  [Struct `Option`](#std_option_Option)
-  [Constants](#@Constants_0)
-  [Function `none`](#std_option_none)
-  [Function `some`](#std_option_some)
-  [Function `is_none`](#std_option_is_none)
-  [Function `is_some`](#std_option_is_some)
-  [Function `contains`](#std_option_contains)
-  [Function `borrow`](#std_option_borrow)
-  [Function `borrow_with_default`](#std_option_borrow_with_default)
-  [Function `get_with_default`](#std_option_get_with_default)
-  [Function `fill`](#std_option_fill)
-  [Function `extract`](#std_option_extract)
-  [Function `borrow_mut`](#std_option_borrow_mut)
-  [Function `swap`](#std_option_swap)
-  [Function `swap_or_fill`](#std_option_swap_or_fill)
-  [Function `destroy_with_default`](#std_option_destroy_with_default)
-  [Function `destroy_some`](#std_option_destroy_some)
-  [Function `destroy_none`](#std_option_destroy_none)
-  [Function `to_vec`](#std_option_to_vec)
-  [Macro function `destroy`](#std_option_destroy)
-  [Macro function `do`](#std_option_do)
-  [Macro function `do_ref`](#std_option_do_ref)
-  [Macro function `do_mut`](#std_option_do_mut)
-  [Macro function `or`](#std_option_or)
-  [Macro function `and`](#std_option_and)
-  [Macro function `and_ref`](#std_option_and_ref)
-  [Macro function `map`](#std_option_map)
-  [Macro function `map_ref`](#std_option_map_ref)
-  [Macro function `filter`](#std_option_filter)
-  [Macro function `is_some_and`](#std_option_is_some_and)
-  [Macro function `destroy_or`](#std_option_destroy_or)


<pre><code><b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
</code></pre>



<a name="std_option_Option"></a>

## Struct `Option`

Abstraction of a value that may or may not be present. Implemented with a vector of size
zero or one because Move bytecode does not have ADTs.


<pre><code><b>public</b> <b>struct</b> <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt; <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>vec: <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;</code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="@Constants_0"></a>

## Constants


<a name="std_option_EOPTION_IS_SET"></a>

The <code><a href="../std/option.md#std_option_Option">Option</a></code> is in an invalid state for the operation attempted.
The <code><a href="../std/option.md#std_option_Option">Option</a></code> is <code>Some</code> while it should be <code>None</code>.


<pre><code><b>const</b> <a href="../std/option.md#std_option_EOPTION_IS_SET">EOPTION_IS_SET</a>: <a href="../std/u64.md#std_u64">u64</a> = 262144;
</code></pre>



<a name="std_option_EOPTION_NOT_SET"></a>

The <code><a href="../std/option.md#std_option_Option">Option</a></code> is in an invalid state for the operation attempted.
The <code><a href="../std/option.md#std_option_Option">Option</a></code> is <code>None</code> while it should be <code>Some</code>.


<pre><code><b>const</b> <a href="../std/option.md#std_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>: <a href="../std/u64.md#std_u64">u64</a> = 262145;
</code></pre>



<a name="std_option_none"></a>

## Function `none`

Return an empty <code><a href="../std/option.md#std_option_Option">Option</a></code>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_none">none</a>&lt;Element&gt;(): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_none">none</a>&lt;Element&gt;(): <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt; {
    <a href="../std/option.md#std_option_Option">Option</a> { vec: <a href="../std/vector.md#std_vector_empty">vector::empty</a>() }
}
</code></pre>



</details>

<a name="std_option_some"></a>

## Function `some`

Return an <code><a href="../std/option.md#std_option_Option">Option</a></code> containing <code>e</code>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_some">some</a>&lt;Element&gt;(e: Element): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_some">some</a>&lt;Element&gt;(e: Element): <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt; {
    <a href="../std/option.md#std_option_Option">Option</a> { vec: <a href="../std/vector.md#std_vector_singleton">vector::singleton</a>(e) }
}
</code></pre>



</details>

<a name="std_option_is_none"></a>

## Function `is_none`

Return true if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_is_none">is_none</a>&lt;Element&gt;(t: &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_is_none">is_none</a>&lt;Element&gt;(t: &<a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;): bool {
    t.vec.is_empty()
}
</code></pre>



</details>

<a name="std_option_is_some"></a>

## Function `is_some`

Return true if <code>t</code> holds a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_is_some">is_some</a>&lt;Element&gt;(t: &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_is_some">is_some</a>&lt;Element&gt;(t: &<a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;): bool {
    !t.vec.is_empty()
}
</code></pre>



</details>

<a name="std_option_contains"></a>

## Function `contains`

Return true if the value in <code>t</code> is equal to <code>e_ref</code>
Always returns <code><b>false</b></code> if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_contains">contains</a>&lt;Element&gt;(t: &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;, e_ref: &Element): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_contains">contains</a>&lt;Element&gt;(t: &<a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;, e_ref: &Element): bool {
    t.vec.<a href="../std/option.md#std_option_contains">contains</a>(e_ref)
}
</code></pre>



</details>

<a name="std_option_borrow"></a>

## Function `borrow`

Return an immutable reference to the value inside <code>t</code>
Aborts if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_borrow">borrow</a>&lt;Element&gt;(t: &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;): &Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_borrow">borrow</a>&lt;Element&gt;(t: &<a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;): &Element {
    <b>assert</b>!(t.<a href="../std/option.md#std_option_is_some">is_some</a>(), <a href="../std/option.md#std_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    &t.vec[0]
}
</code></pre>



</details>

<a name="std_option_borrow_with_default"></a>

## Function `borrow_with_default`

Return a reference to the value inside <code>t</code> if it holds one
Return <code>default_ref</code> if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_borrow_with_default">borrow_with_default</a>&lt;Element&gt;(t: &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;, default_ref: &Element): &Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_borrow_with_default">borrow_with_default</a>&lt;Element&gt;(t: &<a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;, default_ref: &Element): &Element {
    <b>let</b> vec_ref = &t.vec;
    <b>if</b> (vec_ref.is_empty()) default_ref
    <b>else</b> &vec_ref[0]
}
</code></pre>



</details>

<a name="std_option_get_with_default"></a>

## Function `get_with_default`

Return the value inside <code>t</code> if it holds one
Return <code>default</code> if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_get_with_default">get_with_default</a>&lt;Element: <b>copy</b>, drop&gt;(t: &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;, default: Element): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_get_with_default">get_with_default</a>&lt;Element: <b>copy</b> + drop&gt;(
    t: &<a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;,
    default: Element,
): Element {
    <b>let</b> vec_ref = &t.vec;
    <b>if</b> (vec_ref.is_empty()) default
    <b>else</b> vec_ref[0]
}
</code></pre>



</details>

<a name="std_option_fill"></a>

## Function `fill`

Convert the none option <code>t</code> to a some option by adding <code>e</code>.
Aborts if <code>t</code> already holds a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_fill">fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;, e: Element)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_fill">fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;, e: Element) {
    <b>let</b> vec_ref = &<b>mut</b> t.vec;
    <b>if</b> (vec_ref.is_empty()) vec_ref.push_back(e)
    <b>else</b> <b>abort</b> <a href="../std/option.md#std_option_EOPTION_IS_SET">EOPTION_IS_SET</a>
}
</code></pre>



</details>

<a name="std_option_extract"></a>

## Function `extract`

Convert a <code><a href="../std/option.md#std_option_some">some</a></code> option to a <code><a href="../std/option.md#std_option_none">none</a></code> by removing and returning the value stored inside <code>t</code>
Aborts if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_extract">extract</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_extract">extract</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;): Element {
    <b>assert</b>!(t.<a href="../std/option.md#std_option_is_some">is_some</a>(), <a href="../std/option.md#std_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    t.vec.pop_back()
}
</code></pre>



</details>

<a name="std_option_borrow_mut"></a>

## Function `borrow_mut`

Return a mutable reference to the value inside <code>t</code>
Aborts if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_borrow_mut">borrow_mut</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;): &<b>mut</b> Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_borrow_mut">borrow_mut</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;): &<b>mut</b> Element {
    <b>assert</b>!(t.<a href="../std/option.md#std_option_is_some">is_some</a>(), <a href="../std/option.md#std_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    &<b>mut</b> t.vec[0]
}
</code></pre>



</details>

<a name="std_option_swap"></a>

## Function `swap`

Swap the old value inside <code>t</code> with <code>e</code> and return the old value
Aborts if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_swap">swap</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;, e: Element): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_swap">swap</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;, e: Element): Element {
    <b>assert</b>!(t.<a href="../std/option.md#std_option_is_some">is_some</a>(), <a href="../std/option.md#std_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    <b>let</b> vec_ref = &<b>mut</b> t.vec;
    <b>let</b> old_value = vec_ref.pop_back();
    vec_ref.push_back(e);
    old_value
}
</code></pre>



</details>

<a name="std_option_swap_or_fill"></a>

## Function `swap_or_fill`

Swap the old value inside <code>t</code> with <code>e</code> and return the old value;
or if there is no old value, fill it with <code>e</code>.
Different from swap(), swap_or_fill() allows for <code>t</code> not holding a value.


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_swap_or_fill">swap_or_fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;, e: Element): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_swap_or_fill">swap_or_fill</a>&lt;Element&gt;(t: &<b>mut</b> <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;, e: Element): <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt; {
    <b>let</b> vec_ref = &<b>mut</b> t.vec;
    <b>let</b> old_value = <b>if</b> (vec_ref.is_empty()) <a href="../std/option.md#std_option_none">none</a>()
        <b>else</b> <a href="../std/option.md#std_option_some">some</a>(vec_ref.pop_back());
    vec_ref.push_back(e);
    old_value
}
</code></pre>



</details>

<a name="std_option_destroy_with_default"></a>

## Function `destroy_with_default`

Destroys <code>t.</code> If <code>t</code> holds a value, return it. Returns <code>default</code> otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_destroy_with_default">destroy_with_default</a>&lt;Element: drop&gt;(t: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;, default: Element): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_destroy_with_default">destroy_with_default</a>&lt;Element: drop&gt;(t: <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;, default: Element): Element {
    <b>let</b> <a href="../std/option.md#std_option_Option">Option</a> { <b>mut</b> vec } = t;
    <b>if</b> (vec.is_empty()) default
    <b>else</b> vec.pop_back()
}
</code></pre>



</details>

<a name="std_option_destroy_some"></a>

## Function `destroy_some`

Unpack <code>t</code> and return its contents
Aborts if <code>t</code> does not hold a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_destroy_some">destroy_some</a>&lt;Element&gt;(t: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;): Element
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_destroy_some">destroy_some</a>&lt;Element&gt;(t: <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;): Element {
    <b>assert</b>!(t.<a href="../std/option.md#std_option_is_some">is_some</a>(), <a href="../std/option.md#std_option_EOPTION_NOT_SET">EOPTION_NOT_SET</a>);
    <b>let</b> <a href="../std/option.md#std_option_Option">Option</a> { <b>mut</b> vec } = t;
    <b>let</b> elem = vec.pop_back();
    vec.destroy_empty();
    elem
}
</code></pre>



</details>

<a name="std_option_destroy_none"></a>

## Function `destroy_none`

Unpack <code>t</code>
Aborts if <code>t</code> holds a value


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_destroy_none">destroy_none</a>&lt;Element&gt;(t: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_destroy_none">destroy_none</a>&lt;Element&gt;(t: <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;) {
    <b>assert</b>!(t.<a href="../std/option.md#std_option_is_none">is_none</a>(), <a href="../std/option.md#std_option_EOPTION_IS_SET">EOPTION_IS_SET</a>);
    <b>let</b> <a href="../std/option.md#std_option_Option">Option</a> { vec } = t;
    vec.destroy_empty()
}
</code></pre>



</details>

<a name="std_option_to_vec"></a>

## Function `to_vec`

Convert <code>t</code> into a vector of length 1 if it is <code>Some</code>,
and an empty vector otherwise


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_to_vec">to_vec</a>&lt;Element&gt;(t: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;Element&gt;): <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../std/option.md#std_option_to_vec">to_vec</a>&lt;Element&gt;(t: <a href="../std/option.md#std_option_Option">Option</a>&lt;Element&gt;): <a href="../std/vector.md#std_vector">vector</a>&lt;Element&gt; {
    <b>let</b> <a href="../std/option.md#std_option_Option">Option</a> { vec } = t;
    vec
}
</code></pre>



</details>

<a name="std_option_destroy"></a>

## Macro function `destroy`

Destroy <code><a href="../std/option.md#std_option_Option">Option</a>&lt;T&gt;</code> and call the closure <code>f</code> on the value inside if it holds one.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_destroy">destroy</a>&lt;$T&gt;($o: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $f: |$T| -&gt; ())
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_destroy">destroy</a>&lt;$T&gt;($o: <a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $f: |$T|) {
    <b>let</b> o = $o;
    o.<a href="../std/option.md#std_option_do">do</a>!($f);
}
</code></pre>



</details>

<a name="std_option_do"></a>

## Macro function `do`

Destroy <code><a href="../std/option.md#std_option_Option">Option</a>&lt;T&gt;</code> and call the closure <code>f</code> on the value inside if it holds one.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_do">do</a>&lt;$T&gt;($o: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $f: |$T| -&gt; ())
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_do">do</a>&lt;$T&gt;($o: <a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $f: |$T|) {
    <b>let</b> o = $o;
    <b>if</b> (o.<a href="../std/option.md#std_option_is_some">is_some</a>()) {
        $f(o.<a href="../std/option.md#std_option_destroy_some">destroy_some</a>());
    }
}
</code></pre>



</details>

<a name="std_option_do_ref"></a>

## Macro function `do_ref`

Execute a closure on the value inside <code>t</code> if it holds one.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_do_ref">do_ref</a>&lt;$T&gt;($o: &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $f: |&$T| -&gt; ())
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_do_ref">do_ref</a>&lt;$T&gt;($o: &<a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $f: |&$T|) {
    <b>let</b> o = $o;
    <b>if</b> (o.<a href="../std/option.md#std_option_is_some">is_some</a>()) {
        $f(o.<a href="../std/option.md#std_option_borrow">borrow</a>());
    }
}
</code></pre>



</details>

<a name="std_option_do_mut"></a>

## Macro function `do_mut`

Execute a closure on the mutable reference to the value inside <code>t</code> if it holds one.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_do_mut">do_mut</a>&lt;$T&gt;($o: &<b>mut</b> <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $f: |&<b>mut</b> $T| -&gt; ())
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_do_mut">do_mut</a>&lt;$T&gt;($o: &<b>mut</b> <a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $f: |&<b>mut</b> $T|) {
    <b>let</b> o = $o;
    <b>if</b> (o.<a href="../std/option.md#std_option_is_some">is_some</a>()) $f(o.<a href="../std/option.md#std_option_borrow_mut">borrow_mut</a>());
}
</code></pre>



</details>

<a name="std_option_or"></a>

## Macro function `or`

Select the first <code>Some</code> value from the two options, or <code>None</code> if both are <code>None</code>.
Equivalent to Rust's <code>a.<a href="../std/option.md#std_option_or">or</a>(b)</code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_or">or</a>&lt;$T&gt;($o: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $default: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_or">or</a>&lt;$T&gt;($o: <a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $default: <a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;): <a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt; {
    <b>let</b> o = $o;
    <b>if</b> (o.<a href="../std/option.md#std_option_is_some">is_some</a>()) o
    <b>else</b> $default
}
</code></pre>



</details>

<a name="std_option_and"></a>

## Macro function `and`

If the value is <code>Some</code>, call the closure <code>f</code> on it. Otherwise, return <code>None</code>.
Equivalent to Rust's <code>t.and_then(f)</code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_and">and</a>&lt;$T, $U&gt;($o: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $f: |$T| -&gt; <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$U&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$U&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_and">and</a>&lt;$T, $U&gt;($o: <a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $f: |$T| -&gt; <a href="../std/option.md#std_option_Option">Option</a>&lt;$U&gt;): <a href="../std/option.md#std_option_Option">Option</a>&lt;$U&gt; {
    <b>let</b> o = $o;
    <b>if</b> (o.<a href="../std/option.md#std_option_is_some">is_some</a>()) $f(o.<a href="../std/option.md#std_option_extract">extract</a>())
    <b>else</b> <a href="../std/option.md#std_option_none">none</a>()
}
</code></pre>



</details>

<a name="std_option_and_ref"></a>

## Macro function `and_ref`

If the value is <code>Some</code>, call the closure <code>f</code> on it. Otherwise, return <code>None</code>.
Equivalent to Rust's <code>t.and_then(f)</code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_and_ref">and_ref</a>&lt;$T, $U&gt;($o: &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $f: |&$T| -&gt; <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$U&gt;): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$U&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_and_ref">and_ref</a>&lt;$T, $U&gt;($o: &<a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $f: |&$T| -&gt; <a href="../std/option.md#std_option_Option">Option</a>&lt;$U&gt;): <a href="../std/option.md#std_option_Option">Option</a>&lt;$U&gt; {
    <b>let</b> o = $o;
    <b>if</b> (o.<a href="../std/option.md#std_option_is_some">is_some</a>()) $f(o.<a href="../std/option.md#std_option_borrow">borrow</a>())
    <b>else</b> <a href="../std/option.md#std_option_none">none</a>()
}
</code></pre>



</details>

<a name="std_option_map"></a>

## Macro function `map`

Map an <code><a href="../std/option.md#std_option_Option">Option</a>&lt;T&gt;</code> to <code><a href="../std/option.md#std_option_Option">Option</a>&lt;U&gt;</code> by applying a function to a contained value.
Equivalent to Rust's <code>t.<a href="../std/option.md#std_option_map">map</a>(f)</code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_map">map</a>&lt;$T, $U&gt;($o: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $f: |$T| -&gt; $U): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$U&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_map">map</a>&lt;$T, $U&gt;($o: <a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $f: |$T| -&gt; $U): <a href="../std/option.md#std_option_Option">Option</a>&lt;$U&gt; {
    <b>let</b> <b>mut</b> o = $o;
    <b>if</b> (o.<a href="../std/option.md#std_option_is_some">is_some</a>()) <a href="../std/option.md#std_option_some">some</a>($f(o.<a href="../std/option.md#std_option_extract">extract</a>()))
    <b>else</b> <a href="../std/option.md#std_option_none">none</a>()
}
</code></pre>



</details>

<a name="std_option_map_ref"></a>

## Macro function `map_ref`

Map an <code><a href="../std/option.md#std_option_Option">Option</a>&lt;T&gt;</code> value to <code><a href="../std/option.md#std_option_Option">Option</a>&lt;U&gt;</code> by applying a function to a contained value by reference.
Original <code><a href="../std/option.md#std_option_Option">Option</a>&lt;T&gt;</code> is preserved.
Equivalent to Rust's <code>t.<a href="../std/option.md#std_option_map">map</a>(f)</code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_map_ref">map_ref</a>&lt;$T, $U&gt;($o: &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $f: |&$T| -&gt; $U): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$U&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_map_ref">map_ref</a>&lt;$T, $U&gt;($o: &<a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $f: |&$T| -&gt; $U): <a href="../std/option.md#std_option_Option">Option</a>&lt;$U&gt; {
    <b>let</b> o = $o;
    <b>if</b> (o.<a href="../std/option.md#std_option_is_some">is_some</a>()) <a href="../std/option.md#std_option_some">some</a>($f(o.<a href="../std/option.md#std_option_borrow">borrow</a>()))
    <b>else</b> <a href="../std/option.md#std_option_none">none</a>()
}
</code></pre>



</details>

<a name="std_option_filter"></a>

## Macro function `filter`

Return <code>None</code> if the value is <code>None</code>, otherwise return <code><a href="../std/option.md#std_option_Option">Option</a>&lt;T&gt;</code> if the predicate <code>f</code> returns true.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_filter">filter</a>&lt;$T: drop&gt;($o: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_filter">filter</a>&lt;$T: drop&gt;($o: <a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): <a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt; {
    <b>let</b> o = $o;
    <b>if</b> (o.<a href="../std/option.md#std_option_is_some">is_some</a>() && $f(o.<a href="../std/option.md#std_option_borrow">borrow</a>())) o
    <b>else</b> <a href="../std/option.md#std_option_none">none</a>()
}
</code></pre>



</details>

<a name="std_option_is_some_and"></a>

## Macro function `is_some_and`

Return <code><b>false</b></code> if the value is <code>None</code>, otherwise return the result of the predicate <code>f</code>.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_is_some_and">is_some_and</a>&lt;$T&gt;($o: &<a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): bool
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_is_some_and">is_some_and</a>&lt;$T&gt;($o: &<a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $f: |&$T| -&gt; bool): bool {
    <b>let</b> o = $o;
    o.<a href="../std/option.md#std_option_is_some">is_some</a>() && $f(o.<a href="../std/option.md#std_option_borrow">borrow</a>())
}
</code></pre>



</details>

<a name="std_option_destroy_or"></a>

## Macro function `destroy_or`

Destroy <code><a href="../std/option.md#std_option_Option">Option</a>&lt;T&gt;</code> and return the value inside if it holds one, or <code>default</code> otherwise.
Equivalent to Rust's <code>t.unwrap_or(default)</code>.

Note: this function is a more efficient version of <code><a href="../std/option.md#std_option_destroy_with_default">destroy_with_default</a></code>, as it does not
evaluate the default value unless necessary. The <code><a href="../std/option.md#std_option_destroy_with_default">destroy_with_default</a></code> function should be
deprecated in favor of this function.


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_destroy_or">destroy_or</a>&lt;$T&gt;($o: <a href="../std/option.md#std_option_Option">std::option::Option</a>&lt;$T&gt;, $default: $T): $T
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>macro</b> <b>fun</b> <a href="../std/option.md#std_option_destroy_or">destroy_or</a>&lt;$T&gt;($o: <a href="../std/option.md#std_option_Option">Option</a>&lt;$T&gt;, $default: $T): $T {
    <b>let</b> o = $o;
    <b>if</b> (o.<a href="../std/option.md#std_option_is_some">is_some</a>()) o.<a href="../std/option.md#std_option_destroy_some">destroy_some</a>()
    <b>else</b> $default
}
</code></pre>



</details>


[//]: # ("File containing references which can be used from documentation")
