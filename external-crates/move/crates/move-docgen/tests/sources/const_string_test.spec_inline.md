
<a name="0x42_m"></a>

# Module `0x42::m`



-  [Constants](#@Constants_0)


<pre><code></code></pre>



<a name="@Constants_0"></a>

## Constants


<a name="0x42_m_AString"></a>

This is a doc comment above an error constant that should be rendered as a string


<pre><code>#[error]
<b>const</b> <a href="const_string_test.md#0x42_m_AString">AString</a>: vector&lt;u8&gt; = b"Hello, world  🦀   ";
</code></pre>



<a name="0x42_m_AStringNotError"></a>



<pre><code><b>const</b> <a href="const_string_test.md#0x42_m_AStringNotError">AStringNotError</a>: vector&lt;u8&gt; = [72, 101, 108, 108, 111, 44, 32, 119, 111, 114, 108, 100, 32, 32, 240, 159, 166, 128, 32, 32, 32];
</code></pre>



<a name="0x42_m_ErrorNotString"></a>

This is a doc comment above an error constant that should not be rendered as a string


<pre><code>#[error]
<b>const</b> <a href="const_string_test.md#0x42_m_ErrorNotString">ErrorNotString</a>: u64 = 10;
</code></pre>



<a name="0x42_m_NotAString"></a>



<pre><code><b>const</b> <a href="const_string_test.md#0x42_m_NotAString">NotAString</a>: vector&lt;u8&gt; = [1, 2, 3];
</code></pre>
