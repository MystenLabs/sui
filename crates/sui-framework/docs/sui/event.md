---
title: Module `sui::event`
---

Events module. Defines the <code><a href="../sui/event.md#sui_event_emit">sui::event::emit</a></code> function which
creates and sends a custom MoveEvent as a part of the effects
certificate of the transaction.

Every MoveEvent has the following properties:
- sender
- type signature (<code>T</code>)
- event data (the value of <code>T</code>)
- timestamp (local to a node)
- transaction digest

Example:
```
module my::marketplace {
use sui::event;
/* ... */
struct ItemPurchased has copy, drop {
item_id: ID, buyer: address
}
entry fun buy(/* .... */) {
/* ... */
event::emit(ItemPurchased { item_id: ..., buyer: .... })
}
}
```


-  [Function `emit`](#sui_event_emit)


<pre><code></code></pre>



<a name="sui_event_emit"></a>

## Function `emit`

Emit a custom Move event, sending the data offchain.

Used for creating custom indexes and tracking onchain
activity in a way that suits a specific application the most.

The type <code>T</code> is the main way to index the event, and can contain
phantom parameters, eg <code><a href="../sui/event.md#sui_event_emit">emit</a>(MyEvent&lt;<b>phantom</b> T&gt;)</code>.


<pre><code><b>public</b> <b>fun</b> <a href="../sui/event.md#sui_event_emit">emit</a>&lt;T: <b>copy</b>, drop&gt;(<a href="../sui/event.md#sui_event">event</a>: T)
</code></pre>



<details>
<summary>Implementation</summary>


<pre><code><b>public</b> <b>native</b> <b>fun</b> <a href="../sui/event.md#sui_event_emit">emit</a>&lt;T: <b>copy</b> + drop&gt;(<a href="../sui/event.md#sui_event">event</a>: T);
</code></pre>



</details>
