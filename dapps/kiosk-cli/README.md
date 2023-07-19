# Kiosk CLI

A simple CLI application written in NodeJS to showcase Kiosk usage and basic applications.

## Requirements

- nodejs v19
- pnpm (`npm i -g pnpm`)
- testnet account

## Install and use

Export a mnemonic phrase

```
export MNEMONIC="..."
```

Create a Kiosk

```
pnpm cli new
```

Mint a TestItem

```
pnpm cli mint-to-kiosk
```

View Kiosk contents

```
pnpm cli contents

# search contents of a specific Kiosk
# pnpm cli contents --id <kiosk_id>

# search Kiosk at user address
# pnpm cli contents --address <address>
```

List an item in the Kiosk

```
# list an item in the Kiosk
pnpm list <item_id> <price>
```

## License

Apache 2.0
