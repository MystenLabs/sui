All icons in [svgs](./svgs/) folder will be used to generate our custom font icons.

Run `npm run make-font-icons` to update the font when needed.

Under [output](./output/) directory, a demo page [index.html](./output/index.html) will be created with a preview of the font icons.

# Svg Icon Source

It will be useful to also document here the source of each icon.

-   sui-logo-icon.svg - created from the original logo given by design
-   sui-logo-txt.svg - created from the original logo given by design

# Troubleshooting

-   Sometimes the svg icon will not work properly when converted to font. An easy way to fix it is use [IcoMoon](https://icomoon.io/app) and [svgfont2svgicons](https://github.com/nfroidure/svgfont2svgicons)
    -   Upload the svg icon to `IcoMoon`
    -   Generate the font (check if the icon looks good - if not probably it will not work)
    -   Download the new font
    -   Unzip the font
    -   Use `svgfont2svgicons path/to/svgfont path/to/output` to extract the svg icon
    -   Use the extracted svg icon instead of the original one
