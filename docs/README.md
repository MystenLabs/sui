## Overview

This directory contains the assets required to build and update the [Sui documentation](https://docs.sui.io). The directory is split between `content` and `site`. To run the docs.sui.io site locally, open the `site` folder in a terminal or console. Use a package manager to install the required modules:

```shell
pnpm install
```

In the same folder, use the following command to deploy the site to `localhost:3000`:

```shell
pnpm start
```

The deploy watches for updates to files in the `content` folder (and site source files), updating the UI to match any saves you make. 

You can also build the site locally using `pnpm build`. This builds the static site and places the files in `site\build`. The build fails on errors like bad internal links, displaying the cause of the error to the console.

Sui Foundation is not able to provide support for building the documentation site locally. If you run into trouble, consult the [Docusaurus](https://docusaurus.io/) documentation.

## Pull requests

Sui uses Vercel to host its documentation site. Vercel builds a preview of the documentation for every pull request submitted to the Sui repo. You can find a link to this preview in the PR comment section from the Vercel bot. Click the **Visit Preview** link for the **sui-core** project to verify your changes behave as you expect.  


## Contributing

Sui is for the community. Contribute for the benefit of all.

- [Docs contributing guidelines](https://docs.sui.io/references/contribute/contribution-process)
- [Repo contributing guidelines](https://docs.sui.io/contribute-to-sui-repos)
- [Style guide](https://docs.sui.io/style-guide)
- [Localization](https://docs.sui.io/localize-sui-docs)
- [Code of conduct](https://docs.sui.io/contribute/code-of-conduct)

## License

The Sui Documentation is distributed under the [LICENSE](CC BY 4.0 license).
