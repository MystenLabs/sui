# Sui Nasıl Çalışır?

Bu belge mühendisler, geliştiriciler ve blokzincir hakkında bilgi sahibi teknik okuyucular için yazılmıştır. Derin programlama dili veya dağıtık sistem uzmanlığı varsaymamaktadır. Sui'nin nasıl çalıştığına dair çok daha derin bir açıklama için [Sui white paper'ına](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf) bakın. Sui ve diğer blokzincir sistemleri arasındaki farklara ilişkin üst düzey bir genel bakış için [Sui'nin Diğer Blokzincirlerden Farkları](https://docs.sui.io/devnet/learn/sui-compared) bölümüne bakınız.

### Özet <a href="#tldr" id="tldr"></a>

Sui blockchain'i, daha önce ulaşılamayacağı düşünülen bir hız ve ölçekte çalışır. Sui, çoğu blokzincir işleminin çakışmayan durumlara dokunduğunu varsayar, yani işlemler paralel olarak çalışabilir. Sui, tek yazarlı nesneler için optimizasyon yaparak basit işlemler için consensus'tan vazgeçen bir tasarıma izin verir.

Sui'nin tasarımı, geleneksel blokzincirin ateşle ve unut yayını yerine, bir talep sahibinin veya bir vekilin bir işlemi kesinliğe kavuşturmak için validatörlerle proaktif olarak konuşmasını sağlar. Bu da basit işlemlerin neredeyse anında sonuçlanmasını sağlar.

Bu düşük gecikme süresiyle Sui, işlemlerin oyunlara ve gerçek zamanlı olarak tamamlanması gereken diğer ortamlara dahil edilmesini kolaylaştırır. Sui ayrıca, güçlü doğal güvenlik ve daha anlaşılır bir programlama modeline sahip blokzincirleri için tasarlanmış bir dil olan Move'da yazılmış akıllı kontratları da desteklemektedir.

Bant genişliği maliyetinin giderek azaldığı bir dünyada, kullanıcılar adına işlem oylamasını sağlamayı kolay, eğlenceli ve belki de karlı bulacak bir hizmet ekosistemi yaratıyoruz.

### Bileşenler <a href="#components" id="components"></a>

Bu temel Sui kavramlarına aşina olun:

* [Nesneler ](https://docs.sui.io/devnet/learn/objects)- Sui, Move paketleri (diğer adıyla akıllı sözleşmeler) tarafından oluşturulan ve yönetilen programlanabilir nesnelere sahiptir. Move paketlerinin kendileri de nesnedir. Bu nedenle, Sui nesneleri iki kategoriye ayrılabilir: değiştirilebilir veri değerleri ve değiştirilemez paketler.
* [İşlemler ](https://docs.sui.io/devnet/learn/transactions)- Sui defterindeki tüm güncellemeler bir işlem aracılığıyla gerçekleşir. Bu bölümde Sui tarafından desteklenen işlem türleri ve bunların yürütülmesinin defteri nasıl değiştirdiği açıklanmaktadır.
* [Validatörler ](https://docs.sui.io/devnet/learn/architecture/validators)- Sui ağı, her biri Sui yazılımının kendi örneğini ayrı bir makinede (veya aynı varlık tarafından işletilen parçalanmış bir makine kümesinde) çalıştıran bir dizi bağımsız validatör tarafından işletilir.

### Mimari <a href="#architecture" id="architecture"></a>

Sui, her biri küresel olarak benzersiz bir kimliğe sahip programlanabilir [nesneler ](https://docs.sui.io/devnet/learn/objects)koleksiyonunu depolayan dağıtılmış bir defterdir (ledger). Her nesne _tek bir adrese_ aittir ve her adres keyfi sayıda nesneye sahip olabilir.

Defter, belirli bir adres tarafından gönderilen bir [işlem ](https://docs.sui.io/devnet/learn/transactions)aracılığıyla güncellenir. Bir işlem nesneleri oluşturabilir, yok edebilir ve yazabilir, ayrıca bunları başka adreslere aktarabilir.

Yapısal olarak, bir işlem bir dizi girdi nesnesi referansı ve defterde zaten var olan bir Hareket kodu nesnesine bir işaretçi içerir. Bir işlemin yürütülmesi, girdi nesnelerinde ve (varsa) sahipleriyle birlikte yeni oluşturulan bir dizi nesnede güncellemeler üretir. Göndericisi A adresi olan bir işlem, _A_'nın sahip olduğu nesneleri, paylaşılan nesneleri ve ilk iki gruptaki diğer nesnelerin sahip olduğu nesneleri girdi olarak kabul edebilir.

<figure><img src="../../.gitbook/assets/image (6).png" alt=""><figcaption><p>Akış Şeması</p></figcaption></figure>

Sui validatörleri, [Bizans Tutarlı Yayın](https://en.wikipedia.org/wiki/Byzantine\_fault) (Byzantine Consistent Broadcast) kullanarak işlemleri yüksek verimle paralel olarak kabul eder ve yürütür.

### Sisteme genel bakış <a href="#system-overview" id="system-overview"></a>

Bu bölüm, Sui'nin ana performans ve güvenlik hedeflerine nasıl ulaştığı hakkında daha fazla bilgi edinmek isteyen teknik bir kitle için yazılmıştır.

Sui, tipik blockchain işleminin kullanıcıdan kullanıcıya bir transfer veya varlık manipülasyonu olduğunu varsayar. Sui, bu senaryo için optimize edilmiştir. Sonuç olarak Sui, iki tür varlık arasında ayrım yapar (i) yalnızca belirli sahipleri tarafından değiştirilebilen sahip olunan nesneler ve (ii) belirli sahipleri olmayan ve birden fazla kullanıcı tarafından değiştirilebilen paylaşılan nesneler. Bu ayrım, yalnızca sahip olunan nesneleri içeren basit işlemler için consensus'dan vazgeçerek çok düşük gecikme süresi elde eden bir tasarıma olanak tanır.

Sui, blockchain'in büyümesinin önündeki en büyük engellerden biri olan [head-of line blocking](https://docs.sui.io/devnet/learn/how-sui-works)'i ortadan kaldırıyor. Blockchain node'ları, en son onaylı işlemler gibi tüm blockchain'in durumunu temsil eden bir akümülatör tutar. Node'lar, işlemin bloklarda yaptığı değişiklikleri (ekleme, çıkarma, değiştirme) yansıtan bu duruma bir güncelleme eklemek için bir konsensüs protokolüne katılır. Bu konsensüs protokolü, artıştan önce blockchain'in durumu, durum güncellemesinin kendisinin geçerliliği ve uygunluğu ve artıştan sonra blockchain'in durumu üzerinde bir anlaşmaya varılmasını sağlar. Bu artışlar periyodik olarak akümülatörde toplanır.

Sui'de bu [konsensüs](https://docs.sui.io/devnet/learn/how-sui-works) protokolü yalnızca işlem paylaşılan nesneleri içerdiğinde gereklidir. Bunun için Sui, [Narwhal ve Bullshark](https://docs.sui.io/devnet/learn/how-sui-works) DAG-bazlı mempool ve verimli Byzantine Fault Tolerant (BFT, Bizans Hata Toleranslı) konsensüsü sunar. Paylaşılan nesneler söz konusu olduğunda, Sui validatörleri, paylaşılan nesnelere erişen diğer işlemlere göre işlemi tamamen düzenlemek için diğer blockchain'lerdeki daha aktif validatörlerin rolünü oynar.

Sui, tek bir durum toplamı yerine belirli nesneleri yönetmeye odaklandığından, bunları benzersiz bir şekilde raporlar: (i) Sui'deki her nesnenin benzersiz bir sürüm numarası vardır ve (ii) her yeni sürüm, kendileri de sürümlü nesneler olan çeşitli bağımlılıkları içerebilen bir işlemden oluşturulur.

Sonuç olarak, bir Sui validatörü - ya da durumun bir kopyasına sahip başka herhangi bir varlık - bir nesnenin oluşumundan bu yana geçmişini gösteren nedensel bir geçmişini sergileyebilir. Sui, birçok durumda, bu nedensel geçmişin başka bir nesnenin nedensel geçmişiyle sıralanmasının önemsiz olduğu iddiasını açıkça ortaya koyar; ve bu bilginin önemli olduğu birkaç durumda, Sui bu ilişkiyi verilerde açık hale getirir.

Sui, işlem işlemenin [klasik anlamda](https://hal.inria.fr/inria-00609399/document) [nihai tutarlılığa](https://en.wikipedia.org/wiki/Eventual\_consistency) uymasını garanti eder. Bu iki kısma ayrılır:

* Eventual delivery (Nihai teslimat) - bir dürüst validatör bir işlemi işlerse, diğer tüm dürüst validatörler de eninde sonunda aynısını yapacaktır.
* Convergence (yakınsama) - aynı işlem kümesini gören iki doğrulayıcı sistemin aynı görünümünü paylaşır (aynı duruma ulaşır).

Ancak bir blockchainin aksine Sui, yakınsamaya tanıklık etmek için işlem akışını durdurmaz.

### Basit İşlemler <a href="#simple-transactions" id="simple-transactions"></a>

[Birçok işlemin](https://eprint.iacr.org/2019/611.pdf) blockchain durumunun diğer keyfi kısımlarıyla karmaşık karşılıklı bağımlılıkları yoktur. Genellikle finansal kullanıcılar sadece bir alıcıya bir varlık göndermek ister ve bu basit işlemin kabul edilebilir olup olmadığını ölçmek için gereken tek veri, gönderenin adresinin yeni bir görünümüdür. Bu gözlem Sui'nin [konsensüsten](https://app.gitbook.com/o/HSlCsQNxGspN5PR0puQa/s/pQWj2eKJ2etESc81Yy6H/) vazgeçmesine ve bunun yerine [Byzantine Consistent Broadcast](https://link.springer.com/book/10.1007/978-3-642-15260-3)'e dayalı daha basit algoritmalar kullanmasına olanak tanır. Gerçek dünyadaki basit işlem örnekleri için potansiyel single-writer uygulamalar listemize bakın.

Bu protokoller, peer-reviewed güvenlik garantileriyle birlikte gelen [FastPay](https://arxiv.org/abs/2003.11506) tasarımına dayanmaktadır. Özetle Sui, tüm zincir yerine yalnızca ilgili veri parçası için kilit alma (veya "dünyayı durdurma") yaklaşımını benimsemektedir. Bu durumda, ihtiyaç duyulan tek bilgi gönderici adresidir ve bu adres bir seferde yalnızca bir işlem gönderebilir.

Sui bu yaklaşımı, Move'un nesne modelini kullanarak ve Move'un güçlü sahiplik modelinden yararlanarak, gönderenin kontrolü altındaki birden fazla öğeye açıkça bağlı olabilecek daha kapsamlı işlemlere doğru genişletir. Bağımlılıkların açık olmasını gerektiren Sui, işlem validasyonuna çok şeritli bir yaklaşım uygulayarak bu bağımsız işlem akışlarının diğerlerinden etkilenmeden ilerleyebildiğinden emin olur.

Sui, işlemleri geleneksel bloklar halinde gruplamak yerine tek tek valide eder. Bu yaklaşımın en önemli avantajı düşük gecikme süresidir; her başarılı işlem, işlemin Sui ağı tarafından işleneceğini herkese kanıtlayan bir kesinlik sertifikasını hızlı bir şekilde alır.

Bir Sui işlemi gönderme süreci bu nedenle geleneksel blockchainlere göre biraz daha karmaşıktır. Normal bir blockchain, aynı yazardan gelen bir grup işlemi fire-and forget modunda kabul edebilirken, Sui işlem gönderimi aşağıdaki adımları izler:

1. Gönderici bir işlemi tüm Sui validatörlerine yayınlar.
2. Her Sui validatörü bu işlem için bireysel bir oyla yanıt verir. Her oy, validatörün sahip olduğu hisseye bağlı olarak belirli bir ağırlığa sahiptir.
3. Gönderici bu oyların Byzantine-resistant-majority'sini bir sertifikada toplar ve bunu tüm Sui validatörlerine geri yayınlar. Bu, işlemin başarısız olamayacağına (iptal edilmeyeceğine) dair kesinliği sağlayarak işlemi çözer.
4. İsteğe bağlı olarak, gönderen işlemin etkilerini detaylandıran bir sertifika alır.

Bu adımlar göndericiden daha fazlasını talep etse de, bunları verimli bir şekilde gerçekleştirmek yine de minimum gecikmeyle kriptografik bir kesinlik kanıtı sağlayabilir. Orijinal işlemin kendisinin hazırlanmasının yanı sıra, bir işlem için oturum yönetimi herhangi bir özel anahtara erişim gerektirmez ve üçüncü bir tarafa devredilebilir. Sui, [Sui Gateway hizmetlerini](https://docs.sui.io/devnet/learn/how-sui-works#sui-gateway-services) sağlamak için bu gözlemden yararlanır.

### Karmaşık Kontratlar <a href="#complex-contracts" id="complex-contracts"></a>

Karmaşık akıllı kontratlar, birden fazla kullanıcının bu nesneleri mutasyona uğratabileceği (akıllı kontrata özgü kuralları izleyerek) paylaşılan nesnelerden faydalanabilir. Bu durumda Sui, bir [konsensüs](https://docs.sui.io/devnet/learn/architecture/consensus) protokolü kullanarak paylaşılan nesneleri içeren tüm işlemleri tamamen düzenler. Sui, [Narwhal](https://github.com/MystenLabs/narwhal) tabanlı yeni bir peer-reviewed konsensüs protokolü kullanmaktadır. Bu, hem performans hem de sağlamlık açısından son teknoloji ürünüdür.

Narwhal mempool, disk I/O'sunu ve ağ gereksinimlerini birkaç çalışana bölen yüksek verimli bir veri kullanılabilirliği motoru ve ölçeklendirilmiş bir mimari sunar. Bullshark ise grafik geçişlerinden yararlanarak zero-message overhead konsensüs algoritması sunuyor.

Paylaşılan nesneleri içeren işlemler ayrıca gas ücretlerini ödemek için en az bir sahip olunan nesne içerir. Bu nedenle, Sui'nin güvenlik özelliklerini garanti altına almak için sahip olunan nesnelerle ilgili protokolü, işlemi sıralayan protokolle dikkatli bir şekilde oluşturmak çok önemlidir. Paylaşılan nesneler söz konusu olduğunda, işlem gönderimi şu adımları izler:

1. Gönderici bir işlemi tüm Sui validatörlerine yayınlar.
2. Her Sui validatörü bu işlem için bireysel bir oyla yanıt verir. Her oy, validatörün sahip olduğu hisseye bağlı olarak belirli bir ağırlığa sahiptir.
3. Gönderici bu oyların Byzantine-resistant-majority'sini bir sertifikada toplar ve bunu tüm Sui validatörlerine geri yayınlar. _Ancak bu kez sertifika Byzantine Agreement yoluyla sıralanır._
4. İşlem başarılı bir şekilde sıralandıktan sonra, kullanıcı işlemi gerçekleştirmek için sertifikayı validatörlere tekrar yayınlar.

### Ölçeklenebilirlik <a href="#scalability" id="scalability"></a>

Belirtildiği gibi, Sui sadece sahip olunan nesneleri içeren işlemlere toplam bir düzen getirmez. Bunun yerine, işlemler [nedensel olarak sıralanır](https://docs.sui.io/devnet/learn/sui-compared#causal-order-vs-total-order). Bir `T1` işlemi, bir `T2` işleminde girdi nesneleri olarak kullanılan bir `O1` çıktı nesnesi üretiyorsa, bir validatör `T2`'yi yürütmeden önce `T1`'i yürütmelidir. Nedensel bir ilişkinin var olması için `T2`'nin bu nesneleri doğrudan kullanması gerekmediğini unutmayın; örneğin, `T1` daha sonra `T3` tarafından kullanılan çıktı nesneleri üretebilir ve `T2` de `T3`'ün çıktı nesnelerini kullanabilir. Ancak, nedensel ilişkisi olmayan işlemler Sui doğrulayıcıları tarafından herhangi bir sırada işlenebilir. Bu içgörü, Sui'nin yürütmeyi büyük ölçüde paralelleştirmesine ve birden fazla makinede parçalamasına olanak tanır.

Sui, paylaşılan nesneleri içeren işlemleri tamamen sıralamak için [son teknoloji Narwhal konsensüs protokolünü](https://arxiv.org/abs/2105.11827) kullanır. Konsensüs alt sistemi, validatör başına daha fazla makine ekleyerek daha fazla işlem sıralayabilmesi açısından da ölçeklendirilebilir.

### Akıllı Kontrat Programlama <a href="#smart-contract-programming" id="smart-contract-programming"></a>

Sui akıllı kontratları [Move dilinde](https://github.com/MystenLabs/awesome-move/blob/main/README.md) yazılmıştır. Move güvenli ve açıklayıcıdır ve Move'un tip sistemi ve veri modeli, Sui'yi ölçeklenebilir kılan paralel anlaşma / yürütme stratejilerini doğal olarak destekler. Move, başlangıçta [Diem blockchain](https://www.diem.com/)'i için [Meta](http://meta.com/)'da geliştirilen akıllı kontratlar oluşturmaya yönelik açık kaynaklı bir programlama dilidir. Bu dil platformdan bağımsızdır ve Sui tarafından benimsenmesinin yanı sıra diğer platformlarda da (örn. [0L](https://0l.network/), [StarCoin](https://starcoin.org/en/)) popülerlik kazanmaktadır.

Move'un özellikleri hakkında daha kapsamlı bir açıklamayı şurada bulabilirsiniz:

* [Move Programlama dili kitabı](https://github.com/move-language/move/blob/main/language/documentation/book/src/introduction.md)
* Sui'ye özgü Move talimatları ve bu sitedeki farklılıklar
* Sui [white paper](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf)'ı ve Sui bağlamında Move'un resmi tanımı
