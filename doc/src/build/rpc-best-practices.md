---
title: RPC Best Practice for Sui
---

The topic provides best practices for configuring your RPC settings to ensure a reliable infrastructure for your projects and services built on Sui.

- Use dedicated nodes / shared services rather than public endpoints for production apps. The public endpoints maintained by Mysten Labs are rate-limited, and support only approximately 100 requests per 30 seconds. You should not use public endpointsin production applications with high traffic volume.
- You can either run your own Full nodes, or outsource this to a professional infrastructure provider (preferred for apps that have high traffic). You can find a list of reliable RPC endpoint providers for Sui on the [Sui Dev Portal](https://sui.io/developers?tools=RPC).

## RPC provisioning guidance

Consider the following when working with a provider:
- **SLA and 24-hour support** - Choose a provider that offers a SLA that meets your needs and 24-hour support.
- **Onboarding call** - Always do an onboarding call with the provider you select to ensure they can provide service that meets your needs. If you have a high-traffic event, such as an NFT mint coming up, notify your RPC provider with the expected traffic influx at least 48 hours in advance.
- **Redundancy** - It is very important for high-traffic and time-sensitive apps, like NFT marketplaces and DeFi protocols, to ensure they don't rely on just one provider for RPCs. Many projects default to just using a single provider, but that's extremely risky and you should ensure that there is redundancy by also using other providers.
- **Traffic estimate** - You should have a good idea about the amount and type of traffic you expect, and you should communicate that information in advance with your RPC provider. During high-traffic events (such as NFT mints), request increased capacity from your RPC provider in advance.
- **Bot mitigation** - As Sui matures, a lot of bots will emerge on the network. Sui dApp builders should think about bot mitigation at the infrastructure level. This depends heavily on use-cases. For NFT minting, bots are undesirable. However, for certain DeFi use-cases, bots are necessary. You need to think about the implications and prepare accordingly to prepare your infrastructure.
- **Provisioning notice** - Make RPC provisioning requests at least one week in advance. This gives operators and providers advance notice so they can arrange for the configure hardware / servers as necessary. If there’s a sudden, unexpected demand, please reach out to us and we will help set you up with providers that have capacity handy for urgent situations.
