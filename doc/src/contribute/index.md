---
title: Contribute to Sui
---

This page describes how to contribute to Sui, and provides additional information about participating in the Sui community.

You can find answers to common questions in our [FAQ](../contribute/faq.md).

## Join the community

To connect with the Sui community, join our [Discord](https://discord.gg/sui).

## Open issues

To report an issue with Sui, [create an issue](https://github.com/MystenLabs/sui/issues/new/choose) in the GitHub repo. Click **Get started** to open a template for the type of issue to create.

## Install Sui to contribute

To contribute to Sui source code or documentation, you need only a GitHub account. You can commit updates and then submit a PR directly from the Github website, or create a fork of the repo to your local environment and use your favorite tools to make changes. Always submit PRs to the `main` branch.

### Create a fork

First, create a fork of the Mysten Labs Sui repo in your own account so that you can work with your own copy.

**To create a fork using the website**

1. Log in to your Github account.
1. Browse to the [Sui repo](https://github.com/MystenLabs/sui) on GitHub.
1. Choose **Fork** in the top-right, then choose **Create new fork**.
1. For **Owner**, select your username.
1. For **Repository name**, you can use any name you want, but some find it easier to track if you use the same name as the source repo. 
1. Optional. To contribute you need only the main branch of the repo. To include all branches, unselect the checkbox for **Copy the `main` branch only**.
1. Click **Create fork**.

### Clone your fork

Next, clone your fork of the repo to your local workspace.

**To clone your fork to your local workspace**
1. Open the GitHub page for your fork of the repo, then click **Sync fork**.
1. Click **Code**, then click **HTTPS** and copy the web URL displayed.
1. Open a terminal session and navigate to the folder to use, then run the following command, replacing the URL with the URL you copied from the Git page:

`git clone https://github.com/github-user-name/sui.git`

The repo is automatically cloned into the `sui` folder in your workspace.
Create a branch of your fork with following command (or follow the [GitHub topic on branching](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/creating-and-deleting-branches-within-your-repository))

`git checkout -b your-branch-name`

Use the following command to set the [remote upstream repo](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/configuring-a-remote-repository-for-a-fork):

`git remote add upstream https://github.com/MystenLabs/sui.git`

You now have a fork of the Sui repo set up in your local workspace. You can make changes to the files in the workspace, add commits, then push your changes to your fork of the repo to then create a Pull Request.

## Further reading

* Read the [Sui Smart Contract Platform](../../paper/sui.pdf) white paper.
* Implementing [logging](../contribute/observability.md) in Sui to observe the behavior of your development.
* Find related [research papers](../contribute/research-papers.md).
* See and adhere to our [code of conduct](../contribute/code-of-conduct.md).
