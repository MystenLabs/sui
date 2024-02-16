
<a name="0x2_event"></a>

# Module `0x2::event`



-  [Function `emit`](#0x2_event_emit)


<pre><code></code></pre>



<a name="0x2_event_emit"></a>

## Function `emit`



<pre><code><b>public</b> <b>fun</b> <a href="../../dependencies/sui-framework/event.md#0x2_event_emit">emit</a>&lt;T: <b>copy</b>, drop&gt;(<a href="../../dependencies/sui-framework/event.md#0x2_event">event</a>: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../../dependencies/sui-framework/event.md#0x2_event_emit">emit</a>&lt;T: <b>copy</b> + drop&gt;(<a href="../../dependencies/sui-framework/event.md#0x2_event">event</a>: T);
</code></pre>



</details>
