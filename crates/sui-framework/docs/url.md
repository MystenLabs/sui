
<a name="0x2_url"></a>

# Module `0x2::url`

URL: standard Uniform Resource Locator string


-  [Struct `Url`](#0x2_url_Url)
-  [Function `new_unsafe`](#0x2_url_new_unsafe)
-  [Function `new_unsafe_from_bytes`](#0x2_url_new_unsafe_from_bytes)
-  [Function `inner_url`](#0x2_url_inner_url)
-  [Function `update`](#0x2_url_update)


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
</code></pre>



<a name="0x2_url_Url"></a>

## Struct `Url`

Standard Uniform Resource Locator (URL) string.


<pre><code><b>struct</b> <a href="url.md#0x2_url_Url">Url</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="url.md#0x2_url">url</a>: <a href="_String">ascii::String</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_url_new_unsafe"></a>

## Function `new_unsafe`

Create a <code><a href="url.md#0x2_url_Url">Url</a></code>, with no validation


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_unsafe">new_unsafe</a>(<a href="url.md#0x2_url">url</a>: <a href="_String">ascii::String</a>): <a href="url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_unsafe">new_unsafe</a>(<a href="url.md#0x2_url">url</a>: String): <a href="url.md#0x2_url_Url">Url</a> {
    <a href="url.md#0x2_url_Url">Url</a> { <a href="url.md#0x2_url">url</a> }
}
</code></pre>



</details>

<a name="0x2_url_new_unsafe_from_bytes"></a>

## Function `new_unsafe_from_bytes`

Create a <code><a href="url.md#0x2_url_Url">Url</a></code> with no validation from bytes
Note: this will abort if <code>bytes</code> is not valid ASCII


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_unsafe_from_bytes">new_unsafe_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_unsafe_from_bytes">new_unsafe_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="url.md#0x2_url_Url">Url</a> {
    <b>let</b> <a href="url.md#0x2_url">url</a> = <a href="_string">ascii::string</a>(bytes);
    <a href="url.md#0x2_url_Url">Url</a> { <a href="url.md#0x2_url">url</a> }
}
</code></pre>



</details>

<a name="0x2_url_inner_url"></a>

## Function `inner_url`

Get inner URL


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_inner_url">inner_url</a>(self: &<a href="url.md#0x2_url_Url">url::Url</a>): <a href="_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_inner_url">inner_url</a>(self: &<a href="url.md#0x2_url_Url">Url</a>): String{
    self.<a href="url.md#0x2_url">url</a>
}
</code></pre>



</details>

<a name="0x2_url_update"></a>

## Function `update`

Update the inner URL


<pre><code><b>public</b> <b>fun</b> <b>update</b>(self: &<b>mut</b> <a href="url.md#0x2_url_Url">url::Url</a>, <a href="url.md#0x2_url">url</a>: <a href="_String">ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <b>update</b>(self: &<b>mut</b> <a href="url.md#0x2_url_Url">Url</a>, <a href="url.md#0x2_url">url</a>: String) {
    self.<a href="url.md#0x2_url">url</a> = <a href="url.md#0x2_url">url</a>;
}
</code></pre>



</details>
