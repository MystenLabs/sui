All icons in [svgs](./svgs/) folder will be used to generate our custom font icons.

Run `pnpm make-font-icons` to update the font when needed.

Under [output](./output/) directory, a demo page [index.html](./output/index.html) will be created with a preview of the font icons.

# Svg Icon Source

It will be useful to also document here the source of each icon.

-   [sui-logo-icon.svg](./svgs/sui-logo-icon.svg) - created from the original logo given by design
-   [sui-logo-txt.svg](./svgs/sui-logo-txt.svg) - created from the original logo given by design
-   [tokens.svg](./svgs/tokens.svg) - exported from [figma](https://www.figma.com/file/rkFrheddol8YO7HQaHgIfF/Sui-Systematize?node-id=3547%3A3433)
-   [nfts.svg](./svgs/nfts.svg) - exported from [figma](https://www.figma.com/file/rkFrheddol8YO7HQaHgIfF/Sui-Systematize?node-id=3547%3A3433)
-   [history.svg](./svgs/history.svg) - exported from [figma](https://www.figma.com/file/rkFrheddol8YO7HQaHgIfF/Sui-Systematize?node-id=3547%3A3433)
-   [apps.svg](./svgs/apps.svg) - exported from [figma](https://www.figma.com/file/rkFrheddol8YO7HQaHgIfF/Sui-Systematize?node-id=3547%3A3433)
-   [globe.svg](./svgs/globe.svg) - exported from [figma](https://www.figma.com/file/OzLaRFzevjxdQAbybWEZk0/Sui-Visualize?node-id=1607%3A18842)
-   [person.svg](./svgs/person.svg) - exported from [figma](https://www.figma.com/file/OzLaRFzevjxdQAbybWEZk0/Sui-Visualize?node-id=1607%3A18842)
-   [arrow-left.svg](./svgs/arrow-left.svg) - exported from [figma](https://www.figma.com/file/OzLaRFzevjxdQAbybWEZk0/Sui-Visualize?node-id=1609%3A19253)
-   [clipboard.svg](./svgs/clipboard.svg) - exported from [figma](https://www.figma.com/file/OzLaRFzevjxdQAbybWEZk0/Sui-Visualize?node-id=1609%3A19253)
-   [logout.svg](./svgs/logout.svg) - exported from [figma](https://www.figma.com/file/OzLaRFzevjxdQAbybWEZk0/Sui-Visualize?node-id=1609%3A19253)
-   [sui-chevron-right.svg](./svgs/sui-chevron-right.svg) - exported from [figma](https://www.figma.com/file/OzLaRFzevjxdQAbybWEZk0/Sui-Visualize?node-id=1607%3A18842)
-   [coins.svg](./svgs/coins.svg) - exported from [figma](https://www.figma.com/file/OzLaRFzevjxdQAbybWEZk0/Sui-Visualize?node-id=2251%3A47447)
-   [hand-coins.svg](./svgs/hand-coins.svg) - exported from [figma](https://www.figma.com/file/OzLaRFzevjxdQAbybWEZk0/Sui-Visualize?node-id=2251%3A47447)
-   [percentage-polygon.svg](./svgs/percentage-polygon.svg) - exported from [figma](https://www.figma.com/file/OzLaRFzevjxdQAbybWEZk0/Sui-Visualize?node-id=2251%3A47447)
-   [close.svg](./svgs/close.svg) - exported from [figma](https://www.figma.com/file/rkFrheddol8YO7HQaHgIfF/Sui-Systematize?node-id=3421%3A2392)
-   [arrow-right.svg](./svgs/arrow-right.svg) - exported from [figma](https://www.figma.com/file/rkFrheddol8YO7HQaHgIfF/Sui-Systematize?node-id=3421%3A2392)
-   [info.svg](./svgs/info.svg) - figma
-   [copy.svg](./svgs/copy.svg) - figma
-   [check-fill.svg](./svgs/check-fill.svg) - figma
-   [lock.svg](./svgs/lock.svg) - figma
-   [unlocked.svg](./svgs/unlocked.svg) - figma

# Troubleshooting

-   Sometimes the svg icon will not work properly when converted to font (it might be a good idea to do this process for all svgs - to take advantage of any optimizations). An easy way to fix it is use [IcoMoon](https://icomoon.io/app) and [svgfont2svgicons](https://github.com/nfroidure/svgfont2svgicons)
    -   Upload the svg icon to `IcoMoon`
    -   Generate the font (check if the icon looks good - if not probably it will not work)
    -   Download the new font
    -   Unzip the font
    -   Use `svgfont2svgicons path/to/svgfont path/to/output` to extract the svg icon
    -   Use the extracted svg icon instead of the original one
    -   The new svg usually starts with something like `<?xml version="1.0" encoding="UTF-8" standalone="no"?>` but this doesn't work properly when running `make-font-icons`. Removing it will fix the error.
