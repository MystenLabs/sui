# RPC guidance for partners

The motive of this doc is to ensure that projects building on Sui use reliable infrastructure for their services-

- Please use dedicated nodes/ shared services rather than public endpoints for production apps. For context, the public endpoints maintained by Mysten are rate-limited and only support ~100 requests per 30 seconds. Public endpoints in general should NOT be used by production applications that face high traffic.
- You can either run your own full nodes or outsource this to a professional infrastructure provider (preferred for apps that have high traffic). For reference, please refer to the Sui dev portal- [https://sui.io/developers](https://sui.io/developers?tools=RPC)  to find a list of reliable RPC endpoint providers that service Sui.

> **NOTE-** If you want to be put in touch with a provider urgently, please let us know and we can make introductions.
> 

### Guidance for projects while provisioning for RPCs

- **SLA and 24-hour support-** with your RPC provider.
- **Onboarding call-** Always do an onboarding call with your selected provider to inform them about your needs. If you have a high-traffic event like a mint coming up, please notify your RPC provider regarding the updated traffic influx at least 48 hours in advance.
- **Redundancy**- It is very important for high-traffic and time-sensitive apps like NFT marketplaces/ DeFi protocols to ensure they don't rely on just one provider for RPCs. Most folks default to just using a single provider, but that's extremely risky and you should ensure there is redundancy through other providers.
- **Traffic estimate-** You should have a good idea about the kind of traffic you expect and communicate that well in advance with your RPC provider. During high-traffic events (e.g.- NFT mints) please speak with your RPC provider to increase the capacity of your node in advance.
- **Bot mitigation-** As Sui matures, we will see tons of bots emerging. Sui dApps need to think about bot mitigation at the infra level. This depends heavily on use-cases- For NFT minting, bots are undesirable, however, for certain DeFi use-cases, bots are necessary. Again, these app devs need to think about this and prepare accordingly to get that infra stack ready.
- **Provisioning heads up-** Please make RPC provisioning requests at least a week in advance to give these operators a heads-up to arrange for the servers. If there’s a sudden demand, please reach out to us and we will help set you up with providers that have capacity handy for urgent situations.
