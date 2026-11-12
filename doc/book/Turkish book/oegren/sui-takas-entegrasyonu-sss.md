# Sui Takas Entegrasyonu SSS

Sui blockchain hala geliştirme aşamasındadır, bu nedenle bu konuda sağlanan çözümlerin çoğu geçicidir. Sağlanan bilgilerle ilgili sorun yaşıyorsanız lütfen bizimle iletişime geçmekten çekinmeyin.

### Sui Geliştirici Dokümanları nerede? <a href="#where-are-the-sui-developer-docs" id="where-are-the-sui-developer-docs"></a>

* Sui Dokümentasyon Portalı: [https://docs.sui.io/](https://docs.sui.io/)
* Sui REST API'ları: [https://docs.sui.io/sui-jsonrpc](https://docs.sui.io/sui-jsonrpc)
* Bir Full Node Çalıştırın: [https://docs.sui.io/devnet/build/fullnode](https://docs.sui.io/devnet/build/fullnode)

### Mainnet'i ne zaman başlatmayı planlıyorsunuz? <a href="#when-do-you-expect-to-launch-mainnet" id="when-do-you-expect-to-launch-mainnet"></a>

Mainnet'in 2023 yılının 1. çeyreğinin sonlarında veya 2. çeyreğinin başlarında başlatılması planlanmaktadır.

### Testnet ne zaman başlayacak? <a href="#when-will-testnet-be-live" id="when-will-testnet-be-live"></a>

Testnet 1. Dalga 12/01/22 tarihinde sona ermiştir. Sonraki Testnet dalgaları hakkında bilgi mevcut olduğunda verilecektir.

### Token lansmanı için planlarınız nelerdir? <a href="#what-are-your-plans-for-the-token-launch" id="what-are-your-plans-for-the-token-launch"></a>

* OFAC tarafından kısıtlanmayan tüm yargı bölgelerinde 1. günde faaliyete geçmeyi planlıyoruz.
* SUI piyasalarının verimli bir şekilde çalışmasını sağlamak için piyasa yapıcıları şeklinde birden fazla likidite sağlayıcısı olacaktır.
* En iyi merkezi kripto borsalarında eş zamanlı olarak lansman yapmayı planlıyoruz. Sui blockchain üzerine inşa edilen merkezi olmayan borsalar da lansman sırasında yayında olacak. Daha birçok borsa, lansman sonrası sonraki günlerde / haftalarda SUI çiftlerini de başlatacak.
* Sui Tokenomics bilgileri, token lansmanından aylar önce halka açık olarak paylaşılacaktır.
* SUI tokenlerinin önemli bir kısmının topluluğun kullanımına erken sunulabilmesi için ağ yayına girdiğinde açık uçlu bir token likidite dağıtımı (genellikle IEO veya Token Launchpad olarak adlandırılır) gerçekleştirmek üzere birkaç borsa ile ortaklık kuracağız.

### Ne tür bir kurumsal stake etme ve yönetişim mevcut olacak? <a href="#what-kind-of-institutional-staking-and-governance-will-be-available" id="what-kind-of-institutional-staking-and-governance-will-be-available"></a>

Sui ağındaki farklı stake eden taraflar arasındaki tek ayrım, kendi SUI tokenlerini stake eden validatörler ile tokenleri tercih ettikleri validatör(ler)e delege eden validatör olmayan SUI token sahipleri arasındadır. Bu ayrımın ötesinde, Sui ağı, perakende ve kurumsal kuruluşlar da dahil olmak üzere SUI delegatörleri arasında hiçbir ek gereklilik getirmemektedir.

### SUI stake etme nasıl çalışacak? <a href="#how-will-sui-staking-work" id="how-will-sui-staking-work"></a>

Örnek staking uygulaması:

Staking için giriş fonksiyonları[ bu modülde](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/governance/sui\_system.move) tanımlanmıştır. İlgili fonksiyonlar şunları içerir:

* [`request_add_stake`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui\_system.md#0x2\_sui\_system\_request\_add\_stake)
* [`request_add_stake_with_locked_coin`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui\_system.md#0x2\_sui\_system\_request\_add\_stake\_with\_locked\_coin)
* [`request_withdraw_stake`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui\_system.md#0x2\_sui\_system\_request\_withdraw\_stake)
* [`request_add_delegation`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui\_system.md#0x2\_sui\_system\_request\_add\_delegation)
* [`request_add_delegation_with_locked_coin`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui\_system.md#0x2\_sui\_system\_request\_add\_delegation\_with\_locked\_coin)
* [`request_withdraw_delegation`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui\_system.md#0x2\_sui\_system\_request\_withdraw\_delegation)
* [`request_switch_delegation`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui\_system.md#0x2\_sui\_system\_request\_switch\_delegation)

İlk üç işlev validatörün kendi stake etmesi içindir, geri kalanı ise temsilci stake etmesi içindir.

#### Sui'nin genesis'inde kaç validatör olacak? <a href="#how-many-validators-will-sui-have-at-genesis" id="how-many-validators-will-sui-have-at-genesis"></a>

Bu sayı halen değerlendirilmektedir. Validatör seti sabit değildir, ancak validatörlerin validatör başvuru sürecimiz aracılığıyla başvurmaları ve ardından onaylanmaları gerekir.

**Staking için kullanılan adres, stake edilen coinlerin sahibi olan cüzdan adresi ile aynı mı?**

Evet, bir kullanıcı/validatör stake edilen coinin sahibi olan adresi kullanarak stake eder. Özel bir adres türetme yoktur

**Bir staking işleminin inşaat, imza ve yayınla ilgili tipik bir işlemden farkı nedir?**

Staking işlemleri, Sui Framework'te belirli Move işlevini çağıran Move çağrı işlemleridir. Staking işlemi paylaşılan bir nesne kullanır ve diğer paylaşılan nesne işlemlerinden farklı değildir.

**Sui, bir adresin sahip olduğu SUI'nin kısmi bir miktarını stake etmeyi destekliyor mu?**

Evet, bir adres farklı miktarlarda birden fazla coine sahip olabilir. Sui, bir adresin sahip olduğu coinleri farklı validatörlere stake etmeyi destekler. Devredilebilecek minimum stake miktarı, .000000001 SUI'ye eşit olan 1 MIST'dir.

**Birden fazla validatör ile stake etmek için tek bir hesap adresi kullanabilir miyim?**

Evet, bir adres birden fazla coine sahipse, her bir coini farklı bir validatörle stake edebilirsiniz.

**Stake süresi boyunca mevcut bir stake'in miktarını değiştirebilir miyim?**

Evet, bir validatörden stake miktarınızı ekleyebilir veya çekebilirsiniz. Stake miktarını değiştirmek için aşağıdaki yöntemleri kullanın:

&#x20;[`request_add_delegation`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui\_system.md#0x2\_sui\_system\_request\_add\_delegation) ve[`request_add_delegation_with_locked_coin`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui\_system.md#0x2\_sui\_system\_request\_add\_delegation\_with\_locked\_coin) metodları ile istediğiniz kadar stake ekleyin.

&#x20;[`request_withdraw_delegation`](https://github.com/MystenLabs/sui/blob/58229627970a6e9ff558b156c1cb193f246eaf88/crates/sui-framework/docs/sui\_system.md#0x2\_sui\_system\_request\_withdraw\_delegation) ile delegasyonun tamamını veya bir kısmını geri çekin.

**Bir coin aktif olarak stake edilmişken validatörü değiştirebilir miyim?**

Evet, bir coin stake edilirken validatörü değiştirmek için `request_switch_delegation` yöntemini kullanın. Örnekler yakında geliyor.

**Sui bir bağlanma / ısınma dönemi gerektiriyor mu?**

Evet, ancak ayrıntılar hala değerlendiriliyor. En fazla birkaç günlük bir süre bekliyoruz.

**Sui'nin bir bağlanma süresi var mı?**

Mevcut bağlanma süresi bir haftadır, bu Mainnet lansmanından önce değişebilir.

**Staking ödülleri otomatik bileşik mi?**

Evet, Sui likidite havuzlarından esinlenen bir stake havuzu yaklaşımı kullanmaktadır. Ödüller havuza eklenir ve havuz token değerinin SUI tokenlerine göre değer kazanması yoluyla otomatik olarak birleştirilir.

**Ödüller zincir üzerinde gelen/giden işlemler olarak mı görünüyor?**

Evet, ödüller dönem sınırlarında özel bir sistem işlemi aracılığıyla staking havuzuna eklenir.

**Stake ettikten sonra ilk ödülü almak ne kadar sürer? Ödüller ne sıklıkla ödenir?**

Ödüller her dönem birleştirilir ve stake miktarınızı çektiğinizde ödenir. Bir dönemin ödüllerini almak için o dönemin tüm süresi boyunca stake yapmanız gerekir.

**Minimum ve maksimum stake miktarı var mı (validasyon ve delegasyon için)?**

Gerekli minimum miktar ve izin verilen maksimum miktarın yanı sıra bir dönem içindeki stake değişikliklerine ilişkin sınırlar olacaktır.

* Validasyon: Yüksek miktarda minimum SUI gerektirir.&#x20;
* Delegasyon: Minimum .000000001 SUI olması planlanmaktadır (değişikliğe tabidir).

Belirli miktarlar mevcut olduğunda sağlanacaktır.

**Slashing nasıl çalışır ve cezaları nelerdir?**

Tahsis edilen anapara stake'i için kesinti yapılmayacaktır. Bunun yerine, validatörler, bunlar ödendiğinde daha az gelecekteki ödüllere sahip olarak cezalandırılacaktır. Halihazırda tahakkuk etmiş olan ödüller risk altında değildir.

**Sui zincir içi yönetişimi veya oylamayı destekliyor mu?**

Zincir üstü yönetişim Sui için uygulanmamaktadır. Yakın gelecekte de eklenmesi planlanmamaktadır.

### Blok detaylarını nerede bulabilirim? <a href="#where-can-i-find-block-details" id="where-can-i-find-block-details"></a>

Aşağıdaki soruların yanıtları, yüzey kaplama bloğu detaylarına ilişkin belirli ayrıntıları ele almaktadır.

**Bir Sui uç noktası kullanarak mevcut blok yüksekliğini nasıl alabilirim veya bir bloğu yüksekliğe göre nasıl sorgulayabilirim?**

Sui [DAG ](https://cointelegraph.com/explained/what-is-a-directed-acyclic-graph-in-cryptocurrency-how-does-dag-work)tabanlıdır, bu nedenle işlem geçmişinin blok tabanlı görünümü her zaman en doğrudan olanı değildir. En son işlemi almak için Transaction Query API'sini kullanın:

````
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "sui_getTransactions",
  "params": [
    "All",
    <last known transaction digest>,
    100,
    "Ascending"
  ]
}
```
````

#### Bakiye değişiklikleri için nasıl sorgulama yapabilirim? <a href="#how-do-i-query-for-balance-changes" id="how-do-i-query-for-balance-changes"></a>

Aşağıdaki çözüm ara çözümdür: Olay sorgusu API'sini kullanarak `BalanceChangeEvent`'i kullanın. `BalanceChangeEvent` Ekim 2022'de bu PR'ye eklenmiştir.

#### Blok üretimini nasıl takip edebilirim? <a href="#how-do-i-track-block-generation" id="how-do-i-track-block-generation"></a>

Sui kontrol noktaları kullanır, ancak bu hala geliştirme aşamasındadır. Kontrol noktaları, periyodik olarak (muhtemelen birkaç dakikada bir) oluşturulan bloklar gibidir, ancak yürütme için kritik yol yerine eşzamansız olarak oluşturulur. Her kontrol noktası, bir önceki kontrol noktasından bu yana onaylanan tüm işlemleri içerir.

Sui'nin performans avantajlarının önemli bir kısmı, bir işlemi gerçekten sonuçlandırmak için gereken işi, kontrol noktası oluşturma gibi defter tutma işlerinden dikkatlice ayırmasından kaynaklanmaktadır. Bir dizi farklı oluşturma aralığını deniyoruz ve trafik modellerini daha iyi anladıkça bu zaman içinde değişebilir.

Geçici çözüm Şimdilik, Kontrol Noktaları kullanılabilir hale gelene kadar işlem başına bir blok oluşturduk. Örneği [burada ](https://github.com/MystenLabs/sui/blob/91a5e988a91b41d920a082f3de3c2c7372627b00/crates/sui-rosetta/src/state.rs#L61-L74)görebilirsiniz.

````
```rust
#[async_trait]
pub trait BlockProvider {
    async fn get_block_by_index(&self, index: u64) -> Result<BlockResponse, Error>;
    async fn get_block_by_hash(&self, hash: BlockHash) -> Result<BlockResponse, Error>;
    async fn current_block(&self) -> Result<BlockResponse, Error>;
    fn genesis_block_identifier(&self) -> BlockIdentifier;
    async fn oldest_block_identifier(&self) -> Result<BlockIdentifier, Error>;
    async fn current_block_identifier(&self) -> Result<BlockIdentifier, Error>;
    async fn get_balance_at_block(
        &self,
        addr: SuiAddress,
        block_height: u64,
    ) -> Result<u128, Error>;
}
```
````

#### Bloklara dahil değillerse validatörler tarafından işlemler nasıl önerilir? Bir validatör blokları mı yoksa sadece bireysel işlemleri mi önerir? <a href="#how-are-transactions-proposed-by-validators-if-theyre-not-included-in-blocks-does-a-validator-propos" id="how-are-transactions-proposed-by-validators-if-theyre-not-included-in-blocks-does-a-validator-propos"></a>

Validatörler her işlem için bir sertifika (imza yeter sayısı) oluşturur ve ardından son kontrol noktasından bu yana sertifikalardan oluşan kontrol noktaları önerir. [Bölüm 4.3'te](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf) daha fazlasını okuyabilirsiniz.

#### Nasıl Devnet coinleri alabilirim? <a href="#how-do-i-get-devnet-coins" id="how-do-i-get-devnet-coins"></a>

* [Faucet'imizi Discord'da](https://discord.com/channels/916379725201563759/971488439931392130) bulabilirsiniz.

#### Nasıl iletişime geçebilir ve daha fazla bilgi talep edebilirim? <a href="#how-can-i-get-in-touch-and-request-more-information" id="how-can-i-get-in-touch-and-request-more-information"></a>

* Lütfen [Discord sunucumuzu](https://discord.gg/sui) ziyaret edin.
