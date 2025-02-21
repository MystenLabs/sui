# Kiosk Demo Dapp

A demo for Kiosk's functionality.

## Installation

1. Install dependencies (you can use any package manager)

```sh
pnpm install
```

2. Run the development server

```sh
pnpm turbo dev
```

## Kiosk Management

An interactive demo for Kiosk, giving the following flows for a Kiosk owner.

1. Create a Kiosk if the account doesn't have one.
2. View the Kiosk details (profits, items count, address), the items that are included, the listings and the locked status.
3. (Place / list for sale) of owned objects from the connected wallet's address to the Kiosk.
4. (Delist / list for sale / take from Kiosk) For items in the Kiosk.
5. Withdraw Kiosk profits.

## Purchase Flow

Apart from the management flows, there's also the `purchase flow.`

You can type a kiosk's address on the search bar and view the contents of it.

If there are items listed for sale, you can purchase them directly. When you purchase an item, it gets placed into your Kiosk.
If the connected address doesn't own a Kiosk (missing kioskOwnerCap), the purchase will fail.

### Transfer Policy Rules supported

Currently, the demo supports the following Transfer Policy cases:
(based on the [`@mysten/kiosk`](https://github.com/MystenLabs/ts-sdks/tree/main/packages/kiosk) SDK)

1. No rules
2. Royalty rule (soft royalties)
3. Kiosk Lock Rule
4. Combination of (3 + 4) (strong royalties enforcement)
