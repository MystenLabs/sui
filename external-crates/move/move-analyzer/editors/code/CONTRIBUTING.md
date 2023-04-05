# How to contribute to the move-analyzer Visual Studio Code extension

## Installing dependencies

To begin contributing to the VS Code extension for Move, you'll need to have some prerequisite technologies installed:

1. Visual Studio Code itself. To install, follow the directions on [the VS Code website](https://code.visualstudio.com).
2. Node.js and its package manager, NPM. To install, follow the directions on [the Node.js website](https://nodejs.org/en/).
3. The Node module dependencies this extension relies upon. To install, make sure you have Node.js and NPM installed, then open your favorite terminal app. Navigate to the extension's directory and install its dependencies using `npm install`. In short: `cd language/move-analyzer/editors/code && npm install`.

> If you'll be using [VS Code Remote Development](https://code.visualstudio.com/docs/remote/remote-overview) to connect to a computer over SSH, install Node.js on the computer you'll be connecting to.

## Running tests on the command line using `package.json` scripts

The best way to confirm that your dependency installation succeeded is by running the tests:

```sh
npm run pretest
npm run test
```

This executes the `"pretest"` and `"test"` "scripts" from thr `package.json` in the extension's directory. This `package.json` file defines not only information about the extension, such as its name and a description, but also "scripts": aliases for commands that can be run with `npm run`.

Currently, the scripts focus on compiling the project's [TypeScript](https://www.typescriptlang.org) into JavaScript (VS Code extensions run in a JavaScript environment).

## Launching VS Code with the extension installed from source

Running the extension's tests from the command-line is cumbersome because the test environment insists no other instance of VS Code is running while the tests run. Instead, to launch VS Code with the extension installed from source:

1. Open this repository in VS Code.
2. Open the "Run and Debug" view: open the command palette (keyboard shortcut `⇧⌘P` on macOS) and type in "View: Show Run and Debug".
3. From the pull-down menu in the Run and Debug view, select "Launch with Extension" and click the green arrow button. This will launch a new VS Code window that has the `move-analyzer` extension installed. If you open a `.move` file, it will be highlighted according to the any changes you've made in your local checkout of this repository.
4. Once you've launched the extension, in the original VS Code window you'll see a small box with pause, refresh, and stop buttons. Clicking these allows you to quickly refresh the extension window with your latest changes, or quit the window.

To test the Move TextMate grammar, open a file with a `.move` file extension and use the command palette to run the "Developer: Inspect Editor Tokens and Scopes" command. This displays a large window that shows what token is detected below the typing cursor in the VS Code window.

## Running the tests from within VS Code

You can run the extensions tests from within the "Run and Debug" view:

1. Open this repository in VS Code.
2. Open the "Run and Debug" view: open the command palette (keyboard shortcut `⇧⌘P` on macOS) and type in "View: Show Run and Debug".
3. Toward the bottom of the Run and Debug view, you should see an option to enable breakpoints on "Uncaught Exceptions." Clicking the check-box to enable breakpoints will make test failures easier to debug.
4. From the pull-down menu in the Run and Debug view, select "VS Code Tokenizer Tests" and click the green arrow button to run the tests. Should they fail, you will see a stack trace.

## Releasing new versions of the extension

To add new versions of the move-analyzer Visual Studio Code extension, you will need to:

1. Request to be added to the `move` publisher team.
2. Package and upload a new extension version.

### 1: Become a member of the `move` publisher team

As [Visual Studio Code's documentation](https://code.visualstudio.com/api/working-with-extensions/publishing-extension) explains, the Visual Studio Marketplace  uses [Azure DevOps](https://azure.microsoft.com/services/devops/) to authenticate users who are allowed to manage extensions. To register with Azure DevOps, you need a Microsoft account.

> **NOTE:** If you work for a company that uses Microsoft Outlook to serve your corporate email, then you already have a Microsoft account associated with that email address.

To be added to the group of maintainers allowed to release new versions of the move-analyzer Visual Studio Code extension:

1. Open https://marketplace.visualstudio.com/vscode in your browser and click on "Sign in" on the upper-right of the page.
2. Sign in with your Microsoft account's email address. If using a corporate email address, you'll go through an authentication flow specific to your organization (two-factor authentication, for example).
3. Once you've signed in, contact someone who is already in the existing group of maintainers, and ask them to [please add you to this list of members](https://marketplace.visualstudio.com/manage/publishers/move). If you do not know anyone in this group personally, run `git log -- language/move-analyzer/editors/code` and send an email to one or more of the people who have committed to this directory.

Once you've been added, confirm that you are able to access [the `move` publisher page](https://marketplace.visualstudio.com/manage/publishers/move). If you can see yourself listed in the "Members" tab, then you have successfully been added to the publisher team.

### 2: Publish a new version

To publish a new version of the extension, you'll need to:

1. Create a personal access token (PAT) that allows your command line to authenticate with the Visual Studio Marketplace.
2. Publish and merge a pull request to update the version number of the extension.
3. Upload a new version of the extension to the Visual Studio Code Marketplace.

#### 2.1: Create a personal access token

Follow the instructions in [the Visual Studio Code documentation on creating a personal access token](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#get-a-personal-access-token). To summarize:

1. From the [`move` publisher page](https://marketplace.visualstudio.com/manage/publishers/move), click on your username in the upper-right of the page. This will load [your Azure DevOps page](https://aex.dev.azure.com/me).
2. On that page, click on the `dev.azure.com/<name>` link below "Azure DevOps Organizations." If you have more than one organization listed there, choose the one you feel is best. This will load the `https://dev.azure.com/<name>` organization page.
3. On that page, click on the "User Settings" icon in the upper-right, and select "Personal access tokens" from the drop-down menu. This will load the "Personal Access Tokens" page.
4. On that page, click "New Token." Name it whatever you like but make sure to select an expiration of 30 days, select "All accessible organizations" under the "Organization" drop-down menu, and click the "Custom defined" button under "Scopes" followed by setting (only) "Marketplace" scope on the list of custom scopes to "Manage". Click "Create," and copy the token that is presented on the following page to your clipboard.
5. Using your command line, enter the directory containing this `CONTRIBUTING.md` file, make sure you've installed the project's dependencies using `npm install`, and then run `npx vsce login move`. You will be prompted to enter the token you just copied in the previous step. Do so, and you'll have successfully authenticated on the command line.

#### 2.2: Update the version number of the extension

The `package.json` file in this directory specifies the extension's version number. The Visual Studio Code Marketplace will only allow an extension with a greater version number to be published.

Update the version number, commit that change in Git, and submit a pull request to update the version number. Once the version number update pull request is merged, proceed to the next step.

#### 2.3: Upload a new version of the extension

You can publish a new version with `npm run publish`. This will lint the source code, run the extension's tests and, if they succeed, validate the extension and publish it to the Visual Studio Code Marketplace.

The extension package may fail to validate, in which case it will print an error message that explains the problem. Please fix the problems, submit a pull request, and only once that pull request is merged, attempt once again to publish the extension update.
