
<a name="0x2_url"></a>

# Module `0x2::url`

URL: standard Uniform Resource Locator string


-  [Struct `Url`](#0x2_url_Url)
-  [Struct `ParsedUrl`](#0x2_url_ParsedUrl)
-  [Function `new`](#0x2_url_new)
-  [Function `new_from_bytes`](#0x2_url_new_from_bytes)
-  [Function `new_unsafe`](#0x2_url_new_unsafe)
-  [Function `new_unsafe_from_bytes`](#0x2_url_new_unsafe_from_bytes)
-  [Function `inner_url`](#0x2_url_inner_url)
-  [Function `update`](#0x2_url_update)
-  [Function `parse_url`](#0x2_url_parse_url)
-  [Function `parsed_scheme`](#0x2_url_parsed_scheme)
-  [Function `parsed_host`](#0x2_url_parsed_host)
-  [Function `parsed_path`](#0x2_url_parsed_path)
-  [Function `parsed_port`](#0x2_url_parsed_port)
-  [Function `parsed_params`](#0x2_url_parsed_params)
-  [Function `validate_url`](#0x2_url_validate_url)
-  [Function `parse_url_internal`](#0x2_url_parse_url_internal)


<pre><code><b>use</b> <a href="">0x1::ascii</a>;
<b>use</b> <a href="">0x1::option</a>;
<b>use</b> <a href="vec_map.md#0x2_vec_map">0x2::vec_map</a>;
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

<a name="0x2_url_ParsedUrl"></a>

## Struct `ParsedUrl`

Parsed URL. URL split into it's component parts


<pre><code><b>struct</b> <a href="url.md#0x2_url_ParsedUrl">ParsedUrl</a> <b>has</b> <b>copy</b>, drop, store
</code></pre>



<details>
<summary>Fields</summary>


<dl>
<dt>
<code>scheme: <a href="_String">ascii::String</a></code>
</dt>
<dd>
 The scheme of the URL (e.g https, http)
</dd>
<dt>
<code>host: <a href="_Option">option::Option</a>&lt;<a href="_String">ascii::String</a>&gt;</code>
</dt>
<dd>
 The hostname of the URL, empty if URL is a data url
</dd>
<dt>
<code>path: <a href="_String">ascii::String</a></code>
</dt>
<dd>
 The path of the URL
</dd>
<dt>
<code>port: <a href="_Option">option::Option</a>&lt;u64&gt;</code>
</dt>
<dd>
 The port of the URL, empty if it's not available in the URL string
</dd>
<dt>
<code>params: <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="_String">ascii::String</a>, <a href="_String">ascii::String</a>&gt;</code>
</dt>
<dd>
 The URL query parameters
</dd>
</dl>


</details>

<a name="0x2_url_new"></a>

## Function `new`

Create a <code><a href="url.md#0x2_url_Url">Url</a></code>, with validation


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new">new</a>(<a href="url.md#0x2_url">url</a>: <a href="_String">ascii::String</a>): <a href="url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new">new</a>(<a href="url.md#0x2_url">url</a>: String): <a href="url.md#0x2_url_Url">Url</a> {
    <a href="url.md#0x2_url_new_from_bytes">new_from_bytes</a>(<a href="_into_bytes">ascii::into_bytes</a>(<a href="url.md#0x2_url">url</a>))
}
</code></pre>



</details>

<a name="0x2_url_new_from_bytes"></a>

## Function `new_from_bytes`

Create a <code><a href="url.md#0x2_url_Url">Url</a></code> with validation from bytes


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_from_bytes">new_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="url.md#0x2_url_Url">url::Url</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_new_from_bytes">new_from_bytes</a>(bytes: <a href="">vector</a>&lt;u8&gt;): <a href="url.md#0x2_url_Url">Url</a> {
   <a href="url.md#0x2_url_validate_url">validate_url</a>(bytes);
   <a href="url.md#0x2_url_Url">Url</a> { <a href="url.md#0x2_url">url</a>: <a href="_string">ascii::string</a>(bytes) }
}
</code></pre>



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


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_inner_url">inner_url</a>(self: &<a href="url.md#0x2_url_Url">Url</a>): String {
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
    <a href="url.md#0x2_url_validate_url">validate_url</a>(<a href="_into_bytes">ascii::into_bytes</a>(<a href="url.md#0x2_url">url</a>));
    self.<a href="url.md#0x2_url">url</a> = <a href="url.md#0x2_url">url</a>;
}
</code></pre>



</details>

<a name="0x2_url_parse_url"></a>

## Function `parse_url`

Parse URL, split a URL into it's components


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parse_url">parse_url</a>(self: &<a href="url.md#0x2_url_Url">url::Url</a>): <a href="url.md#0x2_url_ParsedUrl">url::ParsedUrl</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parse_url">parse_url</a>(self: &<a href="url.md#0x2_url_Url">Url</a>): <a href="url.md#0x2_url_ParsedUrl">ParsedUrl</a> {
   <a href="url.md#0x2_url_parse_url_internal">parse_url_internal</a>(<a href="_into_bytes">ascii::into_bytes</a>(self.<a href="url.md#0x2_url">url</a>))
}
</code></pre>



</details>

<a name="0x2_url_parsed_scheme"></a>

## Function `parsed_scheme`

Returns the <code>scheme</code> of a parsed URL


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parsed_scheme">parsed_scheme</a>(parsed_url: &<a href="url.md#0x2_url_ParsedUrl">url::ParsedUrl</a>): <a href="_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parsed_scheme">parsed_scheme</a>(parsed_url: &<a href="url.md#0x2_url_ParsedUrl">ParsedUrl</a>): String {
    parsed_url.scheme
}
</code></pre>



</details>

<a name="0x2_url_parsed_host"></a>

## Function `parsed_host`

Returns the <code>host</code> of a parsed URL


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parsed_host">parsed_host</a>(parsed_url: &<a href="url.md#0x2_url_ParsedUrl">url::ParsedUrl</a>): <a href="_Option">option::Option</a>&lt;<a href="_String">ascii::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parsed_host">parsed_host</a>(parsed_url: &<a href="url.md#0x2_url_ParsedUrl">ParsedUrl</a>): Option&lt;String&gt; {
    parsed_url.host
}
</code></pre>



</details>

<a name="0x2_url_parsed_path"></a>

## Function `parsed_path`

Returns the <code>path</code> of a parsed URL


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parsed_path">parsed_path</a>(parsed_url: &<a href="url.md#0x2_url_ParsedUrl">url::ParsedUrl</a>): <a href="_String">ascii::String</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parsed_path">parsed_path</a>(parsed_url: &<a href="url.md#0x2_url_ParsedUrl">ParsedUrl</a>): String {
    parsed_url.path
}
</code></pre>



</details>

<a name="0x2_url_parsed_port"></a>

## Function `parsed_port`

Returns the <code>port</code> of a parsed URL


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parsed_port">parsed_port</a>(parsed_url: &<a href="url.md#0x2_url_ParsedUrl">url::ParsedUrl</a>): <a href="_Option">option::Option</a>&lt;u64&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parsed_port">parsed_port</a>(parsed_url: &<a href="url.md#0x2_url_ParsedUrl">ParsedUrl</a>): Option&lt;u64&gt; {
    parsed_url.port
}
</code></pre>



</details>

<a name="0x2_url_parsed_params"></a>

## Function `parsed_params`

Returns the <code>params</code> (query parameters) of a parsed URL


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parsed_params">parsed_params</a>(parsed_url: &<a href="url.md#0x2_url_ParsedUrl">url::ParsedUrl</a>): <a href="vec_map.md#0x2_vec_map_VecMap">vec_map::VecMap</a>&lt;<a href="_String">ascii::String</a>, <a href="_String">ascii::String</a>&gt;
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>fun</b> <a href="url.md#0x2_url_parsed_params">parsed_params</a>(parsed_url: &<a href="url.md#0x2_url_ParsedUrl">ParsedUrl</a>): VecMap&lt;String, String&gt; {
    parsed_url.params
}
</code></pre>



</details>

<a name="0x2_url_validate_url"></a>

## Function `validate_url`

Validates a URL, aborts if the URL invalid


<pre><code><b>fun</b> <a href="url.md#0x2_url_validate_url">validate_url</a>(<a href="url.md#0x2_url">url</a>: <a href="">vector</a>&lt;u8&gt;)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="url.md#0x2_url_validate_url">validate_url</a>(<a href="url.md#0x2_url">url</a>: <a href="">vector</a>&lt;u8&gt;);
</code></pre>



</details>

<a name="0x2_url_parse_url_internal"></a>

## Function `parse_url_internal`

Parses a URL into it's components


<pre><code><b>fun</b> <a href="url.md#0x2_url_parse_url_internal">parse_url_internal</a>(<a href="url.md#0x2_url">url</a>: <a href="">vector</a>&lt;u8&gt;): <a href="url.md#0x2_url_ParsedUrl">url::ParsedUrl</a>
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>native</b> <b>fun</b> <a href="url.md#0x2_url_parse_url_internal">parse_url_internal</a>(<a href="url.md#0x2_url">url</a>: <a href="">vector</a>&lt;u8&gt;): <a href="url.md#0x2_url_ParsedUrl">ParsedUrl</a>;
</code></pre>



</details>
