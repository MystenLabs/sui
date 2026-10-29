# Turnip Town

Turnip Town is an end-to-end demo of a simulation game implemented as
a Kiosk App where players can interact with each other's assets. It
supports tradeable NFTs that can be bought and sold through Kiosks,
(where the game publisher receives a royalty on each sale).

## Highlights

The demo illustrates how to:

- **share ownership** between players, while preserving a player's
  sense of ownership over their assets (only they can perform certain
  privileged actions).
- allow a game to interact with a player's Kiosk, to **place** items in
  there for sale.
- define a transfer policy for in-game assets, to enforce **royalties**.
- architect the game's front-end to take advantage of Sui's
  **parallelism** and programmable transaction blocks to update the
  simulations across multiple fields, concurrently.
- **query Sui's RPC** to get the necessary information to visualize on
  the front-end.

## Premise

In Turnip Town, players plant fields with turnips and must return to
them daily to ensure they have enough water to grow. Other players can
visit your field, and help you take care of your turnips (growing them
or harvesting them for you).

Each player has a well from which they get the same amount of water
every epoch. If that water is not used, it is wasted (it does not
accumulate). They are free to use this water wherever they like (in
their own field, or in another player's).

Players must balance how much water each turnip gets: Too little
water, and turnips will dry up, too much and they will get
water-logged and rot, becoming less fresh in both cases. With just the
right amount of water, turnips will keep growing, but watch out,
because the bigger a turnip grows, the more water it needs to stay
fresh.

## Deploying

TODO

## Possible Extensions

To keep the initial version of the demonstration focussed, the
following possible extensions have not been implemented, but are left
as an exercise for the reader!

 - Defining **Display** for `Turnip` and `Field` to visualize them in
   Explorers.
 - Extending the `Turnip`'s transfer policy to reward players who
   contributed to the turnip every time it is sold.
 - Implementing weather using **on-chain randomness** to control how
   much rain and sun a field gets.
 - Integrating with a **weather oracle** and augmenting `Field`s with
   a location, so that the `Field` benefits from the real-world
   weather at its location.
 - **Genetics** to add variety to how each turnip responds to the
   weather, and to offer the possibility for cross-pollination and
   breeding.
 - Generalising `Field` so the game can support growing different
   kinds of produce, including "seasonal" produce (only possible to
   grow at certain, restricted times).
