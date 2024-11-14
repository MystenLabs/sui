# Sui Sözlüğü

Sui'de kullanılan terimleri aşağıda bulabilirsiniz. Mümkün olduğunda, kanonik bir tanım veriyoruz ve Sui'nin terimi kullanımına odaklanıyoruz.

#### Nedensel geçmiş (Causal history) <a href="#causal-history" id="causal-history"></a>

Nedensel geçmiş, Sui'deki bir nesne ile onun doğrudan öncülleri ve ardılları arasındaki ilişkidir. Bu geçmiş, Sui'nin işlemleri işlemek için kullandığı nedensel düzen için esastır. Buna karşılık, diğer blockchainler her işlem için dünyalarının tüm durumunu okuyarak gecikmeye neden olur.

#### Nedensel düzen (Causal order) <a href="#causal-order" id="causal-order"></a>

[Nedensel düzen](https://www.scattered-thoughts.net/writing/causal-ordering/), işlemler ve ürettikleri nesneler arasındaki ilişkinin bağımlılıklar olarak ortaya konan bir temsilidir. Validatörler, tamamlanmamış önceki bir işlem tarafından oluşturulan nesnelere bağlı bir işlemi yürütemez. Sui, toplam düzen yerine nedensel düzen (kısmi bir düzen) kullanır.

Daha fazla bilgi için [Nedensel düzen vs toplam düzen](https://docs.sui.io/devnet/learn/sui-compared#causal-order-vs-total-order) bölümüne bakınız.

#### Sertifika (Certificate) <a href="#certificate" id="certificate"></a>

Sertifika, bir işlemin onaylandığını veya tasdik edildiğini kanıtlayan mekanizmadır. Doğrulayıcılar işlemleri oylar ve toplayıcılar bu oyların Bizans dirençli çoğunluğunu bir sertifikada toplar ve tüm Sui doğrulayıcılarına yayınlar, böylece kesinliği sağlar.

#### Epoch (Dönem) <a href="#epoch" id="epoch"></a>

Sui ağının çalışması zamansal olarak çakışmayan, sabit süreli epoch'lara ayrılmıştır. Belirli bir epoch boyunca, ağa katılan validatörler sabittir.

Daha fazla bilgi için [Epoch](https://docs.sui.io/devnet/learn/architecture/validators#epochs)[lar](https://docs.sui.io/devnet/learn/architecture/validators#epochs) bölümüne bakınız.

#### Equivocation (Kaçamak söz) <a href="#equivocation" id="equivocation"></a>

Blokzincirlerinde equivocation, dürüst olmayan kişilerin aynı mesaj için tutarsız veya kopya oylama gibi çelişkili bilgiler vermesi gibi kötü niyetli bir eylemdir.

#### Nihai istikrar (Eventual consistency) <a href="#eventual-consistency" id="eventual-consistency"></a>

[Nihai istikrar](https://en.wikipedia.org/wiki/Eventual\_consistency) Sui tarafından kullanılan consensus modelidir; eğer dürüst bir validatör işlemi onaylarsa, diğer tüm dürüst validatörler de eninde sonunda onaylayacaktır.

#### Finality (Kesinlik) <a href="#finality" id="finality"></a>

[Kesinlik](https://medium.com/mechanism-labs/finality-in-blockchain-consensus-d1f83c120a9a), bir işlemin iptal edilmeyeceğinin güvencesidir. Bu aşama bir borsa veya başka bir blokzinciri işlemi için kapanış olarak kabul edilir.

#### Gas  <a href="#gas" id="gas"></a>

[Gas](https://ethereum.org/en/developers/docs/gas/), Sui ağındaki işlemleri yürütmek için gereken hesaplama çabasını ifade eder. Sui'de gaz, ağın yerel para birimi SUI ile ödenir. SUI birimlerinde bir işlem gerçekleştirmenin maliyeti işlem ücreti olarak adlandırılır.

#### Genesis <a href="#genesis" id="genesis"></a>

Genesis, bir Sui ağı için hesaplar ve gas nesneleri oluşturmanın ilk eylemidir. Sui, kullanıcıların ağın çalışması için genesis nesnesini oluşturmasına ve incelemesine olanak tanıyan bir `genesis` komutu sağlar.

Daha fazla bilgi için [Genesis](https://docs.sui.io/devnet/build/sui-local-network#genesis) bölümüne bakınız.

#### Multi-writer objects (Multi-writer nesneler) <a href="#multi-writer-objects" id="multi-writer-objects"></a>

Multi-writer nesneler, birden fazla adresin sahibi olduğu nesnelerdir. Multi-writer nesneleri etkileyen işlemler Sui'de consensus gerektirir. Bu durum, yalnızca sahibinin adres içeriğinin onaylanmasını gerektiren Single-writer nesneleri etkileyen işlemlerle tezat oluşturur.

#### Nesne (Object) <a href="#object" id="object"></a>

Sui'deki temel depolama birimi nesnedir. Depolamanın adres merkezli olduğu ve her adresin bir anahtar-değer deposu içerdiği diğer birçok blok zincirinin aksine, Sui'nin depolaması nesneler merkezlidir. Sui nesneleri aşağıdaki birincil durumlardan birine sahiptir:

* Değişmez (Immutable) - nesne değiştirilemez.
* Değiştirilebilir (Mutable) - nesne değiştirilebilir.

Ayrıca, değiştirilebilir nesneler bu kategorilere ayrılır:

* Sahipli - nesne yalnızca sahibi tarafından değiştirilebilir.
* Paylaşılan - nesne herkes tarafından değiştirilebilir.

Sahibi olmadığı için değişmez nesnelerin bu ayrıma ihtiyacı yoktur.

Daha fazla bilgi için [Sui Nesneleri](https://docs.sui.io/devnet/learn/objects) bölümüne bakınız.

#### Proof-of-stake (Stake kanıtı)  <a href="#proof-of-stake" id="proof-of-stake"></a>

[Proof-of-stake](https://en.wikipedia.org/wiki/Proof\_of\_stake), validatörlerin oylama ağırlıklarının ağın yerel para biriminin bağlı bir miktarı (ağdaki hisseleri olarak adlandırılır) ile orantılı olduğu bir blokzinciri mutabakat mekanizmasıdır. Bu, kötü aktörleri önce blokzincirinde büyük bir pay elde etmeye zorlayarak [Sybil saldırılarını](https://en.wikipedia.org/wiki/Sybil\_attack) azaltır.

#### Single-writer objects (Single-writer nesneler) <a href="#single-writer-objects" id="single-writer-objects"></a>

Single-writer nesneler tek bir adrese aittir. Sui'de, yalnızca aynı adrese ait tek yazarlı nesneleri etkileyen işlemler, yalnızca gönderenin adresinin doğrulanmasıyla devam edebilir ve işlem sürelerini büyük ölçüde hızlandırır. Bunları basit işlemler olarak adlandırıyoruz. Bu basit işlem modelinin örnek uygulamaları için [Single-writer Uygulamalar](https://docs.sui.io/devnet/learn/single-writer-apps) bölümüne bakınız.

#### Akıllı Kontrat (Smart contract) <a href="#smart-contract" id="smart-contract"></a>

[Akıllı kontrat](https://en.wikipedia.org/wiki/Smart\_contract), bir blokzincirinde işlem gerçekleştirme protokolüne dayanan bir anlaşmadır. Sui'de akıllı kontratlar [Move](https://github.com/MystenLabs/awesome-move) programlama dilinde yazılır.

#### Sui/SUI <a href="#suisui" id="suisui"></a>

Sui, Sui blok zincirini, SUI para birimini ve bir bütün olarak [Sui açık kaynak projesini](https://github.com/MystenLabs/sui/) ifade eder.

#### Toplam düzen (Total order) <a href="#total-order" id="total-order"></a>

[Toplam düzen](https://en.wikipedia.org/wiki/Total\_order), geleneksel bir blokzinciri tarafından belirli bir zamana kadar işlenen tüm işlemlerin geçmişinin sıralı sunumunu ifade eder. Bu, işlemleri işlemenin tek yolu olarak birçok blokzinciri sistemi tarafından sürdürülmektedir. Buna karşılık Sui, mümkün ve güvenli olan her yerde nedensel (kısmi) bir düzen kullanır.

Daha fazla bilgi için [Nedensel düzen vs toplam düzen](https://docs.sui.io/devnet/learn/sui-compared#causal-order-vs-total-order) bölümüne bakınız.

#### İşlem (Transaction) <a href="#transaction" id="transaction"></a>

Sui'de bir işlem, blokzincirinde yapılan bir değişikliktir. Bu, bir NFT yaratmak veya bir NFT ya da başka bir token aktarmak gibi yalnızca tek yazarlı, tek adresli nesneleri etkileyen basit bir işlem olabilir. Bu işlemler Sui'deki consensus protokolünü baypas edebilir.

Varlık yönetimi ve diğer DeFi kullanım durumları gibi birden fazla adres tarafından paylaşılan veya sahip olunan nesneleri etkileyen daha _karmaşık işlemler_, [Narwhal ve Bullshark](https://github.com/MystenLabs/narwhal) DAG tabanlı mempool ve verimli Bizans Hata Toleransı (BFT) consensus'undan geçer.

#### Transfer <a href="#transfer" id="transfer"></a>

Transfer, Sui'deki bir komut aracılığıyla bir tokenin sahip adresinin yenisiyle değiştirilmesidir. Bu, [Sui CLI istemci](https://docs.sui.io/devnet/build/cli-client) komut satırı arayüzü aracılığıyla gerçekleştirilir. CLI istemcisinde bulunan birçok komutun en yaygın olanlarından biridir.

Daha fazla bilgi için [Nesneleri transfer etme](https://docs.sui.io/devnet/build/cli-client#transferring-objects) bölümüne bakın.

#### Validatör (Validator) <a href="#validator" id="validator"></a>

Sui'deki bir validatör, diğer blokzincirlerindeki validatörlerin ve madencilerin daha aktif rolüne benzer şekilde pasif bir rol oynar. Sui'de validatörler consensus protokolüne sürekli olarak katılmazlar, ancak yalnızca bir işlem veya sertifika aldıklarında harekete geçmeye çağrılırlar.

Daha fazla bilgi için [Validatörler](https://docs.sui.io/devnet/learn/architecture/validators) bölümüne bakın.
