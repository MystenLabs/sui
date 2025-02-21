---
title: Module `sui::url`
---

URL: standard Uniform Resource Locator string


-  [Struct `Url`](#sui_url_Url)
-  [Function `new_unsafe`](#sui_url_new_unsafe)
-  [Function `new_unsafe_from_bytes`](#sui_url_new_unsafe_from_bytes)
-  [Function `inner_url`](#sui_url_inner_url)
-  [Function `update`](#sui_url_update)


<pre><code><b>use</b> <a href="../std/ascii.md#std_ascii">std::ascii</a>;
<b>use</b> <a href="../std/option.md#std_option">std::option</a>;
<b>use</b> <a href="../std/vector.md#std_vector">std::vector</a>;
</code></pre>



<a name="sui_url_Url"></a>

## Struct `Url`

Standard Uniform Resource Locator (URL) string.


<pre><code><b>public</b> <b>struct</b> <a href="../sui/url.md#sui_url_Url">Url</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../sui/url.md#sui_url">url</a>: <a href="../std/ascii.md#std_ascii_String">std::ascii::String</a></code>
</dt>
<dd>
</dd>
</dl>


</details>

<a name="sui_url_new_unsafe"></a>

## Function `new_unsafe`

Create a <code><a href="../sui/url.md#sui_url_Url">Url</a></code>, with no validation


<pre><code><b>public</b> <b>fun</b> <a href="../sui/url.md#sui_url_new_unsafe">new_unsafe</a>(<a href="../sui/url.md#sui_url">url</a>: <a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>): <a href="../sui/url.md#sui_url_Url">sui::url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/url.md#sui_url_new_unsafe">new_unsafe</a>(<a href="../sui/url.md#sui_url">url</a>: String): <a href="../sui/url.md#sui_url_Url">Url</a> {
    <a href="../sui/url.md#sui_url_Url">Url</a> { <a href="../sui/url.md#sui_url">url</a> }
}
</code></pre>



</details>

<a name="sui_url_new_unsafe_from_bytes"></a>

## Function `new_unsafe_from_bytes`

Create a <code><a href="../sui/url.md#sui_url_Url">Url</a></code> with no validation from bytes
Note: this will abort if <code>bytes</code> is not valid ASCII


<pre><code><b>public</b> <b>fun</b> <a href="../sui/url.md#sui_url_new_unsafe_from_bytes">new_unsafe_from_bytes</a>(bytes: vector&lt;u8&gt;): <a href="../sui/url.md#sui_url_Url">sui::url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/url.md#sui_url_new_unsafe_from_bytes">new_unsafe_from_bytes</a>(bytes: vector&lt;u8&gt;): <a href="../sui/url.md#sui_url_Url">Url</a> {
    <b>let</b> <a href="../sui/url.md#sui_url">url</a> = bytes.to_ascii_string();
    <a href="../sui/url.md#sui_url_Url">Url</a> { <a href="../sui/url.md#sui_url">url</a> }
}
</code></pre>



</details>

<a name="sui_url_inner_url"></a>

## Function `inner_url`

Get inner URL


<pre><code><b>public</b> <b>fun</b> <a href="../sui/url.md#sui_url_inner_url">inner_url</a>(self: &<a href="../sui/url.md#sui_url_Url">sui::url::Url</a>): <a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/url.md#sui_url_inner_url">inner_url</a>(self: &<a href="../sui/url.md#sui_url_Url">Url</a>): String {
    self.<a href="../sui/url.md#sui_url">url</a>
}
</code></pre>



</details>

<a name="sui_url_update"></a>

## Function `update`

Update the inner URL


<pre><code><b>public</b> <b>fun</b> <a href="../sui/url.md#sui_url_update">update</a>(self: &<b>mut</b> <a href="../sui/url.md#sui_url_Url">sui::url::Url</a>, <a href="../sui/url.md#sui_url">url</a>: <a href="../std/ascii.md#std_ascii_String">std::ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../sui/url.md#sui_url_update">update</a>(self: &<b>mut</b> <a href="../sui/url.md#sui_url_Url">Url</a>, <a href="../sui/url.md#sui_url">url</a>: String) {
    self.<a href="../sui/url.md#sui_url">url</a> = <a href="../sui/url.md#sui_url">url</a>;
}
</code></pre>



</details>
