# Sui Hakkında

Sui üreticilerin ve geliştiricilerin web3'deki gelecek milyonlarca kullanıcının ihtiyaçlarına yönelik deneyimler geliştirmelerini sağlayan sıfırdan dizaynlanmış ilk izinsiz Layer 1 blockchaindir. Sui, geniş bir uygulama yelpazesinin eşi görülmemiş hızda ve düşük maliyetle geliştirilmesini desteklemek için yatay olarak ölçeklenebilirdir.

### Sui Nedir? <a href="#what-sui-is" id="what-sui-is"></a>

Sui, başka blockchainlerdeki validatörlere ve madencilere benzer bir role sahip bir takım izinsiz validatör tarafından sürdürülen bir akıllı kontrat platformudur.

Sui basit kullanım senaryoları için ölçeklenebilirlik ve benzeri görülmemiş derecede düşük gecikme sunar. Sui çoğu işlemin paralel şekilde sürdürülebilmesini sağlar. Bu yöntem işleme kaynaklarını daha verimli kullanır ve daha fazla kaynak ekleyerek verimi arttırma seçeneği sunar.&#x20;

Sui, ödeme işlemleri ve varlık transferi gibi basit kullanım durumlarında daha basit ve daha düşük gecikmeli primitive'leri kullanmak hakkındaki consensus'dan vazgeçer. Bu, blockchain dünyasında eşi benzeri görülmemiş bir durum ve oyun oynamaktan fiziksel satış noktalarında perakende ödemeye kadar birçok yeni gecikmeye-duyarlı dağıtılmış uygulamayı mümkün kılıyor.

Sui, [Rust](https://www.rust-lang.org/)'ta yazılmıştır ve sahibi olabilecek varlıkları tanımlamak için [Move programlama dilinde](https://golden.com/wiki/Move\_\(programming\_language\)-MNA4DZ6) yazılmış akıllı kontratları da destekler. Move programları, operasyonları şunlar dahil olacak şekilde tanımlar; yaratmak için özel kurallar, bu varlıkların yeni sahiplerine transferi ve varlıkları değiştiren operasyonlar. Temel Move ile Sui Move arasındaki farkları öğrenmek için, [buraya](https://docs.sui.io/devnet/learn/sui-move-diffs) göz atın.&#x20;

#### Sui tokenleri ve validatörler <a href="#sui-tokens-and-validators" id="sui-tokens-and-validators"></a>

Sui has a native token called SUI, with a fixed supply. Sui, sabit bir arzı olan kendine ait SUI adlı bir tokene sahiptir. SUI tokeni gas ödemeleri için ve ayrıca bir epoch içerisinde [validatörler üzerinde yetkilendirilmiş stake](https://learn.bybit.com/blockchain/delegated-proof-of-stake-dpos/) olarak kullanılır.  Bu epoch içinde validatörlerin oy gücü bu yetkilendirimiş stake'in bir fonksiyonudur. Validatörler onlara yetkileri verilen stakelere göre periyodik olaran yeniden ayarlanır. Herhangi bir epoch'da validatörler "[Bizans Hata Toleranslı](https://pmg.csail.mit.edu/papers/osdi99.pdf)" 'tır. Epoch'un sonunda tüm gerçekleşen işlemlerden toplanan fee'ler validatörlere sistemin işleyişine sağladıkları katkıya göre dağıtılır. Validatörler bunun karşılığında onlara stake yetkisi veren kullanıcılara ödül olarak fee'lerin bir kısmını dağıtabilir.

Sui son teknoloji hakem denetimli çalışmalar tarafından ve yılların açık kaynaklı geliştirme süreci tarafından desteklenir.

#### İşlemler <a href="#transactions" id="transactions"></a>

Sui'de bir işlem, blockzincirde bir değişiklik demektir. Bu, tek sahibi olan, tek adresi olan objeleri etkileyen örneğin bir NFT'yi yayınlama veya bir NFT'yi ya da başka bir tokeni transfer etmek gibi _basit bir işlem_ olabilir. Bu _basit işlemler_ Sui'deki consensus protokolünü atlayabilir.

Varlık yönetimi ve diğer DeFi kullanım durumları gibi birden fazla adres tarafından paylaşılan veya sahip olunan nesneleri etkileyen daha _karmaşık işlemler_, [Narwhal ve Bullshark](https://github.com/MystenLabs/narwhal) DAG tabanlı mempool'dan ve verimli Bizans Hata Toleransı (BFT) consensus'undan geçer.

### Paralel anlaşma - sistem dizaynında bir devrim <a href="#parallel-agreement---a-breakthrough-in-system-design" id="parallel-agreement---a-breakthrough-in-system-design"></a>

Sui, işlem başına son derece düşük işletme maliyetlerini korurken uygulama talebini karşılamak için üst sınır olmaksızın yatay olarak ölçeklenir. Sistem tasarımındaki devrim, mevcut blockchainlerdeki kritik bir darboğazı ortadan kaldırıyor: toplam sıralı işlem listesi üzerinde küresel consensus sağlama ihtiyacı. Birçok işlemin aynı kaynak için diğer işlemlere karşı mücadele etmediği göz önüne alındığında bu hesaplama israftır.

Sui, nedensel olarak bağımsız işlemler üzerinde paralel consensus sağlayarak ölçeklenebilirlikte önemli bir adım atmaktadır. Sui validatörleri, Bizans Tutarlı Yayın (BCB, Byzantine Consistent Broadcast) kullanarak bu tür işlemleri gerçekleştirir ve güvenlik ve canlılık garantilerinden ödün vermeden küresel consensus'un ek yükünü ortadan kaldırır.

Bu devrim ancak Sui'nin yeni veri modeli ile mümkündür. Nesne merkezli görünümü ve Move'un güçlü sahiplik türleri sayesinde bağımlılıklar açıkça kodlanmıştır. Sonuç olarak Sui, birçok nesne üzerindeki işlemleri paralel olarak hem kabul eder hem de yürütür. Bu arada, paylaşılan durumu etkileyen işlemler Bizans Hata Toleransı consensus'u ile sıralanır ve paralel olarak yürütülür.

#### Sui'de Öne Çıkanlar <a href="#sui-highlights" id="sui-highlights"></a>

* Eşsiz ölçeklenebilirlik, anında ödeme
* Ana akım geliştiricilerin erişebileceği güvenli bir akıllı kontrat dili
* Zengin ve birleştirilebilir zincir üzeri varlıklar tanımlayabilme
* Web3 uygulamaları için daha iyi kullanıcı deneyimi
* Narwhal and Bullshark DAG tabanlı mempool ve efficient Bizans Hata Toleransı (BFT) consensus'u

### Benzersiz ölçeklenebilirlik, anında ödeme <a href="#parallel-agreement---a-breakthrough-in-system-design" id="parallel-agreement---a-breakthrough-in-system-design"></a>

Sui, günümüzde sektör lideri performans, maliyet, programlanabilirlik ve kullanılabilirlik elde ederken web3'ün büyümesiyle ölçeklenebilen tek blockchainidir. Mainnet lansmanına doğru ilerlerken, hem geleneksel hem de blockchain gibi yerleşik sistemlerin işlem işleme kapasitelerinin ötesinde bir kapasite sergileyeceğiz. Sui'yi, web3 için temel bir katman olan ilk internet ölçeğinde programlanabilir blok zinciri platformu olarak görüyoruz.

Günümüzde mevcut blok zincirlerinin kullanıcıları, sınırlı verim nedeniyle ağ kullanımı arttıkça önemli bir vergi ödemektedir. Buna ek olarak, yüksek gecikme süresi uygulamaların yanıt verebilirliğini sınırlamaktadır. Bu faktörler, web3'te çok yaygın olan kötü kullanıcı deneyimlerine katkıda bulunmaktadır:

* Oyunlar yavaş ve oynaması çok pahalı
* Yatırımcılar, Merkeziyetsizleştirilmiş Finans'ta (DeFi) teminat altına alınmamış kredileri nakde çeviremediklerinde fon kaybederler
* Mikro ödemeler ve kuponlar gibi yüksek hacimli, düşük değerli, işlem başına kitlesel pazar hizmetleri ağ dışında fiyatlandırılır
* Yüksek gas fiyatları nedeniyle varlıklar üzerinde yapay yüksek taban fiyatları

Sui, uygulamaların taleplerini karşılamak için yatay olarak ölçeklenir. Ağ kapasitesi, işçi ekleyerek Sui doğrulayıcılarının işlem gücündeki artışla orantılı olarak büyür ve bu da yüksek ağ trafiği sırasında bile düşük gas ücretleriyle sonuçlanır. Bu ölçeklenebilirlik özelliği, katı darboğazlara sahip diğer blockchainlerle keskin bir tezat oluşturmaktadır.

Tasarım gereği, Sui validatörleri (node'ları), kurucuların ve yaratıcıların talebini karşılamak için ağ verimini etkili bir şekilde sonsuza kadar ölçeklendirebilir. Geniş bant internetin web2 için yaptığını Sui'nin web3 için yapabileceğine inanıyoruz.

> **Not:** 19 Mart 2022 itibariyle, 8 çekirdekli bir M1 Macbook Pro üzerinde çalışan optimize edilmemiş tek işçili bir Sui validatör, saniyede 120.000 token aktarım işlemi (TPS) gerçekleştirebilir ve taahhüt edebilir. Verim, çekirdek sayısı ile doğrusal olarak ölçeklenir; aynı makine tek çekirdekli bir yapılandırmada 25.000 TPS işler.

Bu deneyde, her bir istemcinin tek bir imza ile 100 işlemden (100 farklı alıcıya transfer gibi) oluşan bir toplu gönderim yaptığı bir yapılandırma kullanılmıştır. Bu yapılandırma, yüksek düzeyde ölçeklenebilir bir blockchainin beklenen kullanım modelini yansıtmaktadır - örneğin, büyük ölçekte çalışan bir emanet cüzdanı veya oyun sunucusunun saniyede yüzlerce veya binlerce zincir içi işlem göndermesi gerekecektir. Aynı makinede çalışan bir validatör, 1 parti boyutu ile 8 çekirdekle 20.000 TPS işleyebilir ve daha fazla çekirdek eklendikçe verimde aynı doğrusal büyümeyi sergiler.

Testnet'imiz yayınlandığında çeşitli konfigürasyonlarda optimize edilmiş Sui ağları için tam bir performans raporu yayınlayacağız.

### Ana akım geliştiricilerin erişebileceği güvenli bir akıllı kontrat dili <a href="#a-safe-smart-contract-language-accessible-to-mainstream-developers" id="a-safe-smart-contract-language-accessible-to-mainstream-developers"></a>

Move akıllı kontratları Sui uygulamalarına güç verir. Move, başlangıçta Facebook'ta güvenli akıllı kontratlar yazmak için geliştirilmiş bir programlama dilidir. blockchainler arasında paylaşılan kütüphaneler, araçlar ve geliştirici toplulukları sağlayan platformdan bağımsız bir dildir.

Move'un tasarımı, saldırganların diğer platformlarda milyonları çalmak için yararlandıkları [reentrancy güvenlik açıkları](https://en.wikipedia.org/wiki/Reentrancy\_\(computing\)), [zehirli tokenler](https://www.theblock.co/post/112339/creative-attacker-steals-76000-in-rune-by-giving-out-free-tokens) ve [sahte token onayları](https://www.theverge.com/2022/2/20/22943228/opensea-phishing-hack-smart-contract-bug-stolen-nft) gibi sorunları önler. Güvenlik ve ifade edilebilirliğe verdiği önem, geliştiricilerin altta yatan altyapının inceliklerini anlamadan web2'den web3'e geçişini kolaylaştırır.

Move'un yalnızca Sui için değil, tüm yeni nesil akıllı kontrat platformları için de-facto yürütme ortamı haline gelmek üzere iyi bir konuma sahip olduğundan eminiz.

### Zengin ve birleştirilebilir zincir üzeri varlıklar tanımlayabilme <a href="#ability-to-define-rich-and-composable-on-chain-assets" id="ability-to-define-rich-and-composable-on-chain-assets"></a>

Sui'nin ölçeklenebilirliği işlem işleme ile sınırlı değildir. Depolama da düşük maliyetli ve yatay olarak ölçeklenebilir. Bu, geliştiricilerin gas ücretlerinden tasarruf etmek için zincir dışı depolamaya dolaylı katmanlar eklemek yerine doğrudan zincir üzerinde yaşayan zengin niteliklere sahip karmaşık varlıklar tanımlamasına olanak tanır. Bu niteliklerin zincir üzerinde taşınması, bu nitelikleri akıllı kontratlarda kullanan uygulama mantığı becerisinin kilidini açarak uygulamalar için birleştirilebilirliği ve şeffaflığı artırır.

Zengin zincir içi varlıklar, yalnızca yapay kıtlığa dayanmadan faydaya dayalı yeni uygulamaları ve ekonomileri mümkün kılacaktır. Geliştiriciler, oyuna bağlı olarak avatarlarda ve özelleştirilebilir öğelerde yapılan değişiklikler gibi uygulamaya özgü bir şekilde yükseltilebilen, paketlenebilen ve gruplandırılabilen dinamik NFT'ler uygulayabilir. Bu özellik, NFT davranışı zincire tam olarak yansıdığı için daha güçlü oyun içi ekonomiler sunar, NFT'leri daha değerli hale getirir ve daha ilgi çekici geribildirim döngüleri sağlar.

### Web3 uygulamaları için daha iyi kullanıcı deneyimi <a href="#better-user-experience-for-web3-apps" id="better-user-experience-for-web3-apps"></a>

Sui'yi en erişilebilir akıllı kontrat platformu haline getirerek, geliştiricilerin web3'te harika kullanıcı deneyimleri yaratmalarını sağlamak istiyoruz. Bir sonraki milyar kullanıcıya ulaşmak için, geliştiricileri Sui blockchainini gücünden yararlanmaları için çeşitli araçlarla güçlendireceğiz. Sui Geliştirme Kiti (SDK), geliştiricilerin sınırsız bir şekilde inşa etmelerini sağlayacaktır.

### Havalı şeyler inşa edin <a href="#build-cool-stuff" id="build-cool-stuff"></a>

İşte şimdi yapabileceğiniz bazı harika şeyler ve önümüzdeki birkaç hafta ve ay içinde mümkün olacak bazı uygulamalar. Sui, geliştiricilerin bunları tanımlamasına ve oluşturmasına olanak tanır:

* Zincir içi DeFi ve Geleneksel Finans (TradFi) primitive'leri: gerçek zamanlı, düşük gecikmeli zincir içi ticareti mümkün kılmak.
* Ödül ve sadakat programları: düşük maliyetli işlemlerle milyonlarca kişiye ulaşan toplu airdrop'ların hayata geçirilmesi.
* Karmaşık oyunlar ve iş mantığı: zincir üzerinde mantığı şeffaf bir şekilde uygulamak, varlıkların işlevselliğini genişletmek ve saf kıtlığın ötesinde değer sunmak
* Varlık tokenleştirme hizmetleri: mülk tapularından koleksiyonlara, tıbbi ve eğitim kayıtlarına kadar her şeyin sahipliğinin büyük ölçekte sorunsuz bir şekilde gerçekleştirilmesi
* Merkeziyetsiz sosyal medya ağları: gizlilik ve birlikte çalışabilirlik göz önünde bulundurularak içerik yaratıcısına ait medyanın, gönderilerin, beğenilerin ve ağların güçlendirilmesi
