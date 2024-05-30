
<a name="0x42_m"></a>

# Module `0x42::m`

This is a doc comment above an annotation.


-  [Constants](#@Constants_0)
-  [Function `test`](#0x42_m_test)
-  [Function `test1`](#0x42_m_test1)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x42_m_Cool"></a>

This is the top doc comment
This is the middle doc comment


<pre><code><b>const</b> <a href="annotation_test.md#0x42_m_Cool">Cool</a>: u32 = 0;
</code></pre>



<a name="0x42_m_Error"></a>

This is a doc comment on a constant with an annotation. Below the annotation.


<pre><code><b>const</b> <a href="annotation_test.md#0x42_m_Error">Error</a>: u32 = 0;
</code></pre>



<a name="0x42_m_OtherError"></a>

This is a doc comment on a constant with an annotation. Above the annotation.


<pre><code><b>const</b> <a href="annotation_test.md#0x42_m_OtherError">OtherError</a>: u32 = 0;
</code></pre>



<a name="0x42_m_Woah"></a>

This is the top doc comment
This is the middle doc comment
This is the bottom doc comment


<pre><code><b>const</b> <a href="annotation_test.md#0x42_m_Woah">Woah</a>: u32 = 0;
</code></pre>



<a name="0x42_m_test"></a>

## Function `test`

This is a doc comment above a function with an annotation. Above the annotation.


<pre><code><b>fun</b> <a href="annotation_test.md#0x42_m_test">test</a>()
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="annotation_test.md#0x42_m_test">test</a>() { }
</code></pre>



</details>

<a name="0x42_m_test1"></a>

## Function `test1`

This is a doc comment above a function with an annotation. Below the annotation.


<pre><code><b>fun</b> <a href="annotation_test.md#0x42_m_test1">test1</a>()
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>fun</b> <a href="annotation_test.md#0x42_m_test1">test1</a>() { }
</code></pre>



</details>
warning: unused function
   ┌─ tests/sources/annotation_test.move:27:9
   │
27 │     fun test() { }
   │         ^^^^ The non-'public', non-'entry' function 'test' is never called. Consider removing it.
   │
   = This warning can be suppressed with '#[allow(unused_function)]' applied to the 'module' or module member ('const', 'fun', or 'struct')

warning: unused function
   ┌─ tests/sources/annotation_test.move:31:9
   │
31 │     fun test1() { }
   │         ^^^^^ The non-'public', non-'entry' function 'test1' is never called. Consider removing it.
   │
   = This warning can be suppressed with '#[allow(unused_function)]' applied to the 'module' or module member ('const', 'fun', or 'struct')
