
<a name="0x42_m"></a>

# Module `0x42::m`

This is a doc comment above an annotation.


-  [Struct `X`](#0x42_m_X)
-  [Struct `Y`](#0x42_m_Y)
-  [Struct `XG`](#0x42_m_XG)
-  [Struct `YG`](#0x42_m_YG)
-  [Struct `XGG`](#0x42_m_XGG)
-  [Struct `YGG`](#0x42_m_YGG)
-  [Enum `Enum`](#0x42_m_Enum)
-  [Enum `GenericEnum`](#0x42_m_GenericEnum)


<pre><code></code></pre>



<a name="0x42_m_X"></a>

## Struct `X`



<pre><code><b>struct</b> <a href="enums_test.md#0x42_m_X">X</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>x: m::Enum</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x42_m_Y"></a>

## Struct `Y`



<pre><code><b>struct</b> <a href="enums_test.md#0x42_m_Y">Y</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pos0: m::Enum</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x42_m_XG"></a>

## Struct `XG`



<pre><code><b>struct</b> <a href="enums_test.md#0x42_m_XG">XG</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>x: m::GenericEnum&lt;m::Enum&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x42_m_YG"></a>

## Struct `YG`



<pre><code><b>struct</b> <a href="enums_test.md#0x42_m_YG">YG</a>
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pos0: m::GenericEnum&lt;m::Enum&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x42_m_XGG"></a>

## Struct `XGG`



<pre><code><b>struct</b> <a href="enums_test.md#0x42_m_XGG">XGG</a>&lt;T&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>x: m::GenericEnum&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x42_m_YGG"></a>

## Struct `YGG`



<pre><code><b>struct</b> <a href="enums_test.md#0x42_m_YGG">YGG</a>&lt;T&gt;
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>pos0: m::GenericEnum&lt;T&gt;</code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x42_m_Enum"></a>

## Enum `Enum`

This is a doc comment above an enum


<pre><code><b>public</b> enum Enum
</code></pre>



<details>
<summary>Variants</summary>


<dl>
<dt>
Variant <code>A</code>
</dt>
<dd>
 This is a doc comment above a variant
</dd>
<dt>
Variant <code>B</code>
</dt>
<dd>

</dd>
<dt>
Variant <code>C</code>
</dt>
<dd>

</dd>

<dl>
<dt>
<code>pos0: u64</code>
</dt>
<dd>

</dd>
</dl>

<dt>
Variant <code>D</code>
</dt>
<dd>
 Another doc comment
</dd>

<dl>
<dt>
<code>x: u64</code>
</dt>
<dd>
 Doc text on variant field
</dd>
</dl>

<dt>
Variant <code>E</code>
</dt>
<dd>

</dd>

<dl>
<dt>
<code>x: u64</code>
</dt>
<dd>

</dd>
</dl>


<dl>
<dt>
<code>y: u64</code>
</dt>
<dd>

</dd>
</dl>

</dl>


</details>

<a name="0x42_m_GenericEnum"></a>

## Enum `GenericEnum`



<pre><code><b>public</b> enum GenericEnum&lt;T&gt;
</code></pre>



<details>
<summary>Variants</summary>


<dl>
<dt>
Variant <code>A</code>
</dt>
<dd>

</dd>

<dl>
<dt>
<code>pos0: T</code>
</dt>
<dd>

</dd>
</dl>

<dt>
Variant <code>B</code>
</dt>
<dd>

</dd>
</dl>


</details>
