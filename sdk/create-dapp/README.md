# @mysten/create-dapp

`@mysten/create-dapp` is a CLI tool that helps you to create a new dApp project.

You can get started quickly by running the following command:

```bash
pnpm create @mysten/dapp
```

This will prompt you through creating a new dApp project. It will ask you for the name/directory and
ask you to select from one of the provided templates.

## Templates

The following templates are available:

- `react-client-dapp`: A basic React dApp that fetches a list of objects owned by the connected
  wallet
- `react-e2e-counter`: An end to end Example with move code and UI for a simple counter app

The examples are based off the Vite TypeScript starter project, and pre-configure a few things for
you including:

- [React](https://react.dev/)
- [TypeScript](https://www.typescriptlang.org/)
- [Vite](https://vitejs.dev/)
- [Radix UI](https://www.radix-ui.com/)
- [ESLint](https://eslint.org/)
- [`@mysten/dapp-kit`](https://sdk.mystenlabs.com/dapp-kit)

These templates are still new, and would love to get feedback and suggestions for improvements or
future templates. Please open an issue on GitHub if you have any feedback.
