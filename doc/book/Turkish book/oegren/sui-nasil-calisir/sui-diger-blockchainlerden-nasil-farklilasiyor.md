# Sui Diğer Blockchainlerden Nasıl Farklılaşıyor?

Bu sayfa, Sui'nin mevcut blockchainlerle nasıl karşılaştırıldığını özetler ve Sui'yi potansiyel olarak benimseyenlerin kullanım durumlarına uyup uymadığına karar vermeleri için hazırlanmıştır. Sui mimarisine giriş için Sui Nasıl Çalışır bölümüne bakınız.

İşte Sui'nin temel özellikleri:

* [Nedensel düzene karşı toplam düzen](https://docs.sui.io/devnet/learn/sui-compared#causal-order-vs-total-order) ([Causal order vs. total order](https://docs.sui.io/devnet/learn/sui-compared#causal-order-vs-total-order)) büyük ölçüde paralel yürütmeye olanak sağlar.
* Sui'nin Move varyantı ve nesne merkezli veri modeli, birleştirilebilir nesneleri/NFT'leri mümkün kılıyor.
* Blockchain odaklı [Move programlama dili](https://github.com/MystenLabs/awesome-move) geliştirici deneyimini iyileştiriyor.

### Geleneksel Blockchainler <a href="#traditional-blockchains" id="traditional-blockchains"></a>

Geleneksel blockchain validatörleri toplu olarak ortak bir akümülatör oluşturur: blockchain durumunun bir temsili, zaman içinde blok adı verilen artışlar ekledikleri bir zincir. Deterministik kesinlik sunan blok zincirlerinde, validatörler blockchain'e her artımlı ekleme yapmak istediklerinde, yani bir blok önerisinde bulunduklarında, öneriyi sıraya koyarlar. Bu protokol, zincirin mevcut durumu, önerilen artışın geçerli olup olmadığı ve yeni eklemeden sonra zincirin durumunun ne olacağı konusunda bir anlaşma oluşturmalarını sağlar.

Zaman içinde ortak durumu korumaya yönelik bu yöntem, Bizans Hata Toleranslı (Byzantine Fault Tolerant, BFT) dağıtık sistemler alanındaki son 50 yıllık araştırmalardan elde edilen zengin teoriyi kullanarak son 14 yıl içinde pratik bir başarı elde etmiştir.

Yine de doğası gereği sıralıdır: zincirdeki artışlar, bir dizideki inciler gibi her seferinde bir tane eklenir. Uygulamada bu yaklaşım, mevcut blok değerlendirilirken işlem akışını (genellikle bir "mempool "da saklanır) duraklatır.

### Sui'nin yeni işlemleri doğrulama yaklaşımı <a href="#suis-approach-to-validating-new-transactions" id="suis-approach-to-validating-new-transactions"></a>

Pek çok işlemin blockchain durumunun diğer, keyfi kısımlarıyla karmaşık karşılıklı bağımlılıkları yoktur. Genellikle finansal kullanıcılar sadece bir alıcıya bir varlık göndermek ister ve bu basit işlemin kabul edilebilir olup olmadığını ölçmek için gereken tek veri gönderenin adresinin yeni bir görünümüdür. Bu nedenle Sui, tüm zincir yerine yalnızca ilgili veri parçası için - bu durumda, bir seferde yalnızca bir işlem gönderebilen göndericinin adresi - bir kilit alma - ya da "dünyayı durdurma" yaklaşımını benimser.

Sui bu yaklaşımı, bir [nesne modeli](https://docs.sui.io/devnet/learn/objects) kullanarak ve [Move](https://docs.sui.io/devnet/build/move)'un güçlü sahiplik modelinden yararlanarak, göndericinin kontrolü altındaki birden fazla öğeye açıkça bağlı olabilecek daha karmaşık işlemlere doğru genişletir. Bağımlılıkların açık olmasını gerektiren Sui, işlem doğrulamasına "çok şeritli" bir yaklaşım uygulayarak bu bağımsız işlem akışlarının diğerlerinden etkilenmeden ilerleyebilmesini sağlar.

Bu, bir platform olarak Sui'nin işlemleri asla birbirlerine göre sıralamadığı veya sahiplerin yalnızca sahip oldukları nesnelerin mikrokozmosunu etkilemelerine izin verdiği anlamına gelmez. Sui ayrıca bazı paylaşılan durumlar üzerinde etkisi olan işlemleri de titiz, fikir birliği ile düzenlenmiş bir şekilde işler. Bunlar sadece varsayılan kullanım durumu değildir. [Narwhal ve Bullshark konsensüs motoru](https://docs.sui.io/devnet/learn/architecture/consensus) hakkında ayrıntılar için [Son teknoloji konsensüs](https://docs.sui.io/devnet/learn/sui-compared#state-of-the-art-consensus) bölümüne bakın.

### İşlem sunumuna yönelik işbirlikçi bir yaklaşım <a href="#a-collaborative-approach-to-transaction-submission" id="a-collaborative-approach-to-transaction-submission"></a>

Sui, işlemleri geleneksel bloklar halinde gruplamak yerine tek tek valide etmektedir. Bu yaklaşımın en önemli avantajı düşük gecikme süresidir; her başarılı işlem hızlı bir şekilde, işlemin Sui ağı tarafından işleneceğini herkese kanıtlayan bir kesinlik sertifikası alır.

Ancak bir işlem gönderme süreci biraz daha karmaşıktır. Bu biraz daha fazla iş ağ üzerinde gerçekleşir. (Bant genişliğinin ucuzlamasıyla birlikte bu durum daha az endişe yaratmaktadır.) Normal bir blok zinciri aynı yazardan gelen bir grup işlemi fire-and-forget modunda kabul edebilirken, Sui işlem gönderimi şu adımları takip eder:

1. Gönderici bir işlemi tüm Sui validatörlerine yayınlar.
2. Sui validatörleri bu işlem için gönderene ayrı ayrı oy gönderir.
3. Her oy belirli bir ağırlığa sahiptir çünkü her doğrulayıcı [Proof of Stake](https://en.wikipedia.org/wiki/Proof\_of\_work) kurallarına göre bir ağırlığa sahiptir.
4. Gönderici, bu oyların Bizans-dirençli çoğunluğunu bir _sertifikada_ toplar ve bunu tüm Sui validatörlerine yayınlar, böylece _kesinliği_ veya işlemin geri çekilmeyeceğinin (iptal edilmeyeceğinin) güvencesini sağlar.
5. İsteğe bağlı olarak, gönderici işlemin etkilerini detaylandıran bir sertifika toplar.

Bu adımlar göndericiden daha fazlasını talep etse de, bunları verimli bir şekilde gerçekleştirmek yine de minimum gecikmeyle kriptografik bir kesinlik kanıtı sağlayabilir. Orijinal işlemin kendisinin hazırlanmasının yanı sıra, bir işlem için oturum yönetimi herhangi bir özel anahtara erişim gerektirmez ve üçüncü bir tarafa devredilebilir.

### State'e Farklı Bir Yaklaşım <a href="#a-different-approach-to-state" id="a-different-approach-to-state"></a>

Sui, tek bir durum toplamı yerine belirli nesneleri yönetmeye odaklandığından, bunları benzersiz bir şekilde raporlar:

* Sui'deki her nesnenin benzersiz bir sürüm numarası vardır.
* Her yeni sürüm, kendileri de sürümlü nesneler olan çeşitli bağımlılıkları içerebilen bir işlemden oluşturulur.

Sonuç olarak, bir Sui validatörü - ya da durumun bir kopyasına sahip herhangi bir validatör - bir nesnenin oluşumundan bu yana geçmişini gösteren nedensel geçmişini sergileyebilir. Sui, birçok durumda, bu nedensel geçmişin başka bir nesnenin nedensel geçmişiyle sıralanmasının önemsiz olduğu iddiasını açıkça ortaya koyar; ve bu bilginin önemli olduğu birkaç durumda, Sui bu ilişkiyi verilerde açık hale getirir.

### Nedensel düzen vs. toplam düzen <a href="#causal-order-vs-total-order" id="causal-order-vs-total-order"></a>

Mevcut blockchain sistemlerinin çoğundan farklı olarak (ve okuyucunun yukarıdaki yazma talepleri açıklamasından tahmin edebileceği gibi) Sui, paylaşılan nesneler istisna olmak üzere, istemciler tarafından gönderilen işlemlere her zaman toplam bir sıra uygulamaz. Bunun yerine, birçok işlem _nedensel_ olarak sıralanır - eğer bir `T1` işlemi, bir `T2` işleminde girdi nesneleri olarak kullanılan `O1` çıktı nesnelerini üretiyorsa, bir validatör `T2`'yi yürütmeden önce `T1`'i yürütmelidir. Nedensel bir ilişkinin var olması için `T2`'nin bu nesneleri doğrudan kullanması gerekmediğini unutmayın - örneğin, `T1` daha sonra `T3` tarafından kullanılan çıktı nesneleri üretebilir ve `T2` de `T3`'ün çıktı nesnelerini kullanabilir. Ancak, nedensel ilişkisi olmayan işlemler Sui validatörleri tarafından herhangi bir sırada işlenebilir.

### Son Teknoloji Konsensüs <a href="#state-of-the-art-consensus" id="state-of-the-art-consensus"></a>

[Narwhal ve Bullshark](https://docs.sui.io/devnet/learn/architecture/consensus), üretim kriptografisi, kalıcı depolama ve ölçeklendirilmiş bir birincil çalışan mimarisi ile bir WAN üzerinde saniyede 130.000'den fazla işleme ulaşan çoklu-proposer, yüksek verimli konsensüs algoritmaları üzerinde onlarca yıllık çalışmanın en son varyantını temsil etmektedir.

[Narwhal mempool](https://github.com/MystenLabs/narwhal), disk I/O ve ağ gereksinimlerini birkaç çalışana bölen yüksek verimli bir veri kullanılabilirliği motoru ve ölçeklendirilmiş bir mimari sunar. Bullshark ise grafik çaprazlamalarından yararlanan sıfır mesaj ek yükü konsensüs algoritmasıdır.

### Sui'nin üstün olduğu noktalar <a href="#where-sui-excels" id="where-sui-excels"></a>

Bu bölümde Sui'nin geleneksel blockchainlere kıyasla başlıca avantajları özetlenmektedir.

#### Yüksek Performans <a href="#high-performance" id="high-performance"></a>

Sui'nin ana satış noktası eşi benzeri görülmemiş performansıdır. Aşağıdaki maddeler Sui'nin geleneksel blockchainlere kıyasla başlıca performans avantajlarını özetlemektedir:

* Sui birçok işlem için konsensüsten vazgeçerken, diğer blockchainler bunları her zaman tamamen sıralar. İşlemlerin nedensel olarak sıralanması, Sui'nin birçok işlemin yürütülmesini büyük ölçüde paralelleştirmesine olanak tanır; bu gecikmeyi azaltır ve doğrulayıcıların tüm CPU çekirdeklerinden yararlanmasına olanak tanır.
* Sui karmaşıklığı kenarlara iter: istemci bir dizi protokol adımına dahil olur. Bu, validatörler arasındaki etkileşimleri en aza indirir ve kodlarını daha basit ve daha verimli tutar. Sui, daha iyi bir kullanıcı deneyimi için her zaman müşterinin iş yükünün çoğunu bir Sui Gateway hizmetine devretme imkanı sunar. Bunun aksine, geleneksel blockchainler, müşterilerin işlem gönderimlerinin başarısını değerlendirmek için blockchain durumunu izledikleri bir ateşle ve unut (fire-and-forget) modelini takip eder.
* Sui, protokol adımları arasında sistem zaman aşımlarını beklemeden ağ hızında çalışır. Bu, ağ iyi olduğunda ve saldırı altında olmadığında gecikmeyi önemli ölçüde azaltır. Buna karşılık, bir dizi geleneksel blockchainin (iş kanıtı tabanlı blockchainlerin çoğu dahil) güvenliği, işlem yapmadan önce önceden tanımlanmış zaman aşımlarını beklemeye ihtiyaç duyar.
* Sui, performansını artırmak için validatör başına daha fazla makineden faydalanabilir. Geleneksel blockchainler genellikle validatör başına tek bir makinede (hatta tek bir CPU'da) çalışacak şekilde tasarlanmıştır.

#### Arıza durumlarında performans <a href="#performance-under-faults" id="performance-under-faults"></a>

Sui, basit işlemleri (yani yalnızca sahip olunan nesneleri içeren) işlemek için lidersiz bir protokol çalıştırır. Sonuç olarak, hatalı validatörler performansı önemli bir şekilde etkilemez. Paylaşılan nesneleri içeren işlemler için Sui, [görünüm değiştirme alt protokolü](https://pmg.csail.mit.edu/papers/osdi99.pdf) gerektirmeyen son teknoloji ürünü bir konsensüs protokolü kullanır ve bu nedenle yalnızca hafif performans düşüşleri yaşar. Buna karşılık, tek bir validatörün bile çökmesine maruz kalan lider tabanlı blockchainlerin çoğunda verim düşmekte ve gecikme süreleri artmaktadır (genellikle bir büyüklük sırasından daha fazla).

#### Güvenlik varsayımları <a href="#security-assumptions" id="security-assumptions"></a>

Birçok geleneksel blockchainin aksine, Sui ağ üzerinde güçlü senkronizasyon varsayımları yapmaz. Bu da Sui'nin kötü ağ koşulları (hatta aşırı kötü), ağ bölünmeleri/bölünmeleri ve hatta validatörleri hedef alan güçlü DoS saldırıları altında bile güvenlik özelliklerini koruduğu anlamına gelir. Eşzamanlı blockchainlere (yani çoğu iş kanıtı tabanlı blockchain) yönelik sürekli ağ saldırıları, kaynakların çifte harcanmasına ve kilitlenmelere yol açabilir.

#### Verimli yerel okuma işlemleri <a href="#efficient-local-read-operations" id="efficient-local-read-operations"></a>

Sui'nin okuma süreci diğer blockchainlerden büyük ölçüde farklıdır. Yalnızca bir avuç nesne ve bunların geçmişiyle ilgilenen kullanıcılar, düşük ayrıntı düzeyinde ve düşük gecikme süresinde kimlik doğrulamalı okumalar gerçekleştirir. Sui, oluşumdan başlayarak nesnelerin dar bir soy ağacını oluşturur ve yalnızca işlemin göndericisine bağlı nesneleri okumasına olanak tanır. Sistemin genel bir görünümüne ihtiyaç duyan kullanıcılar (örneğin, sistemi denetlemek için) performansı artırmak için kontrol noktalarından yararlanabilir.

Geleneksel blockchainlerde, işlemleri tamamen düzenlemek için aileler birbirlerine göre sıralanır. Bu da ihtiyaç duyulan kesin bilgiler için devasa bir blob'un sorgulanmasını gerektirir. Disk I/O böylece bir performans darboğazı haline gelir ve sonuç olarak bazı blockchainler artık validatörlerinde SSD sürücülere ihtiyaç duymaktadır.

#### Daha kolay geliştirici deneyimi <a href="#easier-developer-experience" id="easier-developer-experience"></a>

Sui, geliştiricilere bu avantajları sağlar:

* Hareket ve nesne merkezli veri modeli (birleştirilebilir nesneler/NFT'ler sağlar)&#x20;
* Varlık merkezli programlama modeli&#x20;
* Daha kolay geliştirici deneyimi

### Mühendislik açısından dezavantajlar <a href="#engineering-trade-offs" id="engineering-trade-offs"></a>

Bu bölümde Sui'nin geleneksel blockchainlere kıyasla başlıca dezavantajları sunulmaktadır.

#### Karmaşık Dizayn <a href="#design-complexity" id="design-complexity"></a>

Geleneksel blockchainler yalnızca tek bir konsensüs protokolünün uygulanmasını gerektirirken, Sui iki protokol gerektirir: (i) basit işlemleri ele almak için Bizans Tutarlı Yayınına dayalı bir protokol ve (ii) paylaşılan nesnelerle işlemleri ele almak için bir konsensüs protokolü. Bu, Sui ekibinin çok daha büyük bir kod tabanını sürdürmesi gerektiği anlamına gelir.

Paylaşılan nesneleri içeren işlemler, konsensüs protokolüne gönderilmeden önce biraz ek yük gerektirir (Sui Ağ Geçidi hizmeti kullanan iyi bağlanmış istemciler için 200 ms'lik iki ekstra gidiş dönüş ekleyerek). Bu ek yük, yukarıda açıklanan iki protokolü güvenli bir şekilde oluşturmak için gereklidir. Diğer blockchainler bunun yerine işlemi doğrudan konsensüs protokolüne gönderebilir. Paylaşılan nesne işlemleri için kesinliğin bu ek yüke rağmen hala 2-3 saniye aralığında olduğunu unutmayın.

Sui'de verimli bir senkronizatör oluşturmak geleneksel blockchainlere kıyasla daha zordur. Eşzamanlayıcı alt protokolü, validatörlerin veri paylaşarak birbirlerini güncellemelerine ve yavaş validatörlerin birbirlerini yakalamalarına olanak tanır. Geleneksel blockchainler için verimli bir senkronizatör oluşturmak kolay bir iş değildir, ancak yine de Sui'dekinden daha basittir.

#### Basit durumda sıralı yazmalar <a href="#sequential-writes-in-the-simple-case" id="sequential-writes-in-the-simple-case"></a>

Geleneksel blockchainler tüm müşteri işlemlerini birbirlerine göre tamamen sıralar. Bu tasarım, validatörler arasında konsensüs sağlanmasını gerektirir ki bu da etkili ancak yavaştır.

Önceki bölümlerde belirtildiği gibi, Sui gecikme sürelerini azaltmak için birçok işlem için konsensüsten vazgeçmektedir. Bu şekilde, Sui çok şeritli işleme olanak sağlar ve hat başı engellemeyi ortadan kaldırır. Diğer tüm işlemlerin artık tek bir şeritte ilk işlemin artışının tamamlanmasını beklemesi gerekmez. Sui, her işlem için uygun genişlikte bir şerit sağlar. Basit işlemler yalnızca gönderen adresinin görüntülenmesini gerektirir, bu da sistemin kapasitesini büyük ölçüde artırır.

Bu basit işlemler için göndericide hat başı engellemeye izin vermenin dezavantajı, göndericinin bir seferde yalnızca bir işlem gönderebilmesidir. Sonuç olarak, işlemlerin hızlı bir şekilde sonuçlandırılması zorunludur.

#### Karmaşık toplam sorgular <a href="#complex-total-queries" id="complex-total-queries"></a>

Sui, toplam sorguları geleneksel blockchainlere göre daha zor hale getirebilir çünkü her zaman işlemlerin toplam sırasını dayatmaz. Toplam sorgular yerel okumalara göre oldukça nadirdir (yukarıya bakın) ancak bazı senaryolarda faydalıdır. Örneğin, ağa yeni bir validatör katılır ve toplam durumu diske indirmesi gerekir ya da bir denetçi tüm blockchaini denetlemek ister.

Sui bunu kontrol noktaları ile hafifletir. Onaylı bir işlem sonucunda blockchaine her artış eklendiğinde bir kontrol noktası oluşturulur. Kontrol noktaları, bir programın tam olarak yürütülmesinden önce durumu saklayan bir [ileriye yazma günlüğü](https://en.wikipedia.org/wiki/Write-ahead\_logging) gibi çalışır. Bu programdaki çağrılar bir blockchaindeki akıllı bir kontratı temsil eder. Bir kontrol noktası yalnızca işlemleri değil, aynı zamanda işlemlerden önce ve sonra blockchainin durumuna ilişkin taahhütleri de içerir.

Sui, dönem değişikliği üzerine gelen durum taahhüdünü kullanır. Sui, birden fazla validatörden tek bir yanıt ister ve blockchainin durumunu temsil eden hash'i türetmek için bir aksesuar protokolünden yararlanır. Bu protokol çok az bant genişliği tüketir ve işlemlerin alınmasını engellemez. Validatörler her dönem değişiminde kontrol noktaları üretir. Sui, validatörlerin daha da sık kontrol noktaları üretmesini gerektirir. Böylece kullanıcılar bu kontrol noktalarını kullanarak blockchain'i biraz çaba sarf ederek denetleyebilirler.

### Sonuç <a href="#conclusion" id="conclusion"></a>

Özetle, Sui, daha az basit kullanım durumlarında bazı karmaşıklık pahasına birçok performans ve kullanılabilirlik kazanımı sunar. Doğrudan gönderici işlemleri Sui'de mükemmeldir. Ve karmaşık akıllı kontratlar, birden fazla kullanıcının bu nesneleri değiştirebildiği (akıllı kontrata özgü kuralları izleyerek) paylaşılan nesnelerden faydalanabilir. Bu durumda Sui, bir [konsensüs ](https://docs.sui.io/devnet/learn/architecture/consensus)protokolü kullanarak paylaşılan nesneleri içeren tüm işlemleri tamamen düzenler.

Sui, DAG tabanlı bir mempool ve verimli Byzantine Fault Tolerant (BFT) konsensüsü sağlayan [Narwhal ve Bullshark](https://github.com/MystenLabs/narwhal)'a dayanan yeni bir hakemli konsensüs protokolü kullanır. Bu, hem performans hem de sağlamlık açısından son teknoloji ürünüdür.
