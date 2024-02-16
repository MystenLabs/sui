
<a name="0x2_url"></a>

# Module `0x2::url`



-  [Struct `Url`](#0x2_url_Url)
-  [Function `new_unsafe`](#0x2_url_new_unsafe)
-  [Function `new_unsafe_from_bytes`](#0x2_url_new_unsafe_from_bytes)
-  [Function `inner_url`](#0x2_url_inner_url)
-  [Function `update`](#0x2_url_update)


<pre><code><b>use</b> <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii">0x1::ascii</a>;
</code></pre>



<a name="0x2_url_Url"></a>

## Struct `Url`



<pre><code><b>struct</b> <a href="../../dependencies/sui-framework/url.md#0x2_url_Url">Url</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code><a href="../../dependencies/sui-framework/url.md#0x2_url">url</a>: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a></code>
</dt>
<dd>

</dd>
</dl>


</details>

<a name="0x2_url_new_unsafe"></a>

## Function `new_unsafe`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/url.md#0x2_url_new_unsafe">new_unsafe</a>(<a href="../../dependencies/sui-framework/url.md#0x2_url">url</a>: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>): <a href="../../dependencies/sui-framework/url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/url.md#0x2_url_new_unsafe">new_unsafe</a>(<a href="../../dependencies/sui-framework/url.md#0x2_url">url</a>: String): <a href="../../dependencies/sui-framework/url.md#0x2_url_Url">Url</a> {
    <a href="../../dependencies/sui-framework/url.md#0x2_url_Url">Url</a> { <a href="../../dependencies/sui-framework/url.md#0x2_url">url</a> }
}
</code></pre>



</details>

<a name="0x2_url_new_unsafe_from_bytes"></a>

## Function `new_unsafe_from_bytes`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/url.md#0x2_url_new_unsafe_from_bytes">new_unsafe_from_bytes</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/sui-framework/url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/url.md#0x2_url_new_unsafe_from_bytes">new_unsafe_from_bytes</a>(bytes: <a href="../../dependencies/move-stdlib/vector.md#0x1_vector">vector</a>&lt;u8&gt;): <a href="../../dependencies/sui-framework/url.md#0x2_url_Url">Url</a> {
    <b>let</b> <a href="../../dependencies/sui-framework/url.md#0x2_url">url</a> = <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_string">ascii::string</a>(bytes);
    <a href="../../dependencies/sui-framework/url.md#0x2_url_Url">Url</a> { <a href="../../dependencies/sui-framework/url.md#0x2_url">url</a> }
}
</code></pre>



</details>

<a name="0x2_url_inner_url"></a>

## Function `inner_url`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/url.md#0x2_url_inner_url">inner_url</a>(self: &<a href="../../dependencies/sui-framework/url.md#0x2_url_Url">url::Url</a>): <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/url.md#0x2_url_inner_url">inner_url</a>(self: &<a href="../../dependencies/sui-framework/url.md#0x2_url_Url">Url</a>): String{
    self.<a href="../../dependencies/sui-framework/url.md#0x2_url">url</a>
}
</code></pre>



</details>

<a name="0x2_url_update"></a>

## Function `update`



<pre><code><b>public</b> <b>fun</b> <b>update</b>(self: &<b>mut</b> <a href="../../dependencies/sui-framework/url.md#0x2_url_Url">url::Url</a>, <a href="../../dependencies/sui-framework/url.md#0x2_url">url</a>: <a href="../../dependencies/move-stdlib/ascii.md#0x1_ascii_String">ascii::String</a>)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <b>update</b>(self: &<b>mut</b> <a href="../../dependencies/sui-framework/url.md#0x2_url_Url">Url</a>, <a href="../../dependencies/sui-framework/url.md#0x2_url">url</a>: String) {
    self.<a href="../../dependencies/sui-framework/url.md#0x2_url">url</a> = <a href="../../dependencies/sui-framework/url.md#0x2_url">url</a>;
}
</code></pre>



</details>
