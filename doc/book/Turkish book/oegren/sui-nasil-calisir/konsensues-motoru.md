---
description: Narwhal, Bullshark, ve Tusk, Sui'nin Consensus Motoru
---

# Konsensüs Motoru

Bu, Mysten Labs tarafından sunulan yüksek verimli mempool ve konsensüs olan [Narwhal, Tusk](https://github.com/MystenLabs/narwhal) ve [Bullshark'a ](https://arxiv.org/abs/2201.05677)kısa bir giriş niteliğindedir. Sui, durumunu periyodik olarak kontrol etmek için gerektiğinde konsensüs çalıştırır. Ve toplam sıralama gerektiren işlemler için Narwhal ve Bullshark ya da Tusk, Sui Konsensüs Motorunu oluşturur.

İkili isim, sistemlerin sorumlulukları paylaştığını vurgulamaktadır:

* Konsensüse sunulan verilerin kullanılabilirliğinin sağlanması = [Narwhal](https://arxiv.org/abs/2105.11827)
* Bu verilerin belirli bir sıralaması üzerinde anlaşmak = [Bullshark ](https://arxiv.org/abs/2201.05677)veya [Tusk](https://github.com/MystenLabs/narwhal)

Ağustos 2022'de Bullshark, daha düşük gecikme süresi ve adalet desteği (yavaş validatörlerin bile katkıda bulunabileceği) için varsayılan olarak konsensüs protokolünün Tusk bileşeninin yerini aldı. Protokollerin karşılaştırması için [DAG BFT ile Buluşuyor - Yeni Nesil BFT Konsensüsü](https://decentralizedthoughts.github.io/2022-06-28-DAG-meets-BFT/) bölümüne bakınız.

Yine de, gösterilen değişikliği geri alarak Bullshark yerine Tusk'ı kolayca kullanabilirsiniz: [https://github.com/MystenLabs/narwhal/blob/85c226f2824010ff695d0bc5789a24cad2bce289/node/src/lib.rs#L266](https://github.com/MystenLabs/narwhal/blob/85c226f2824010ff695d0bc5789a24cad2bce289/node/src/lib.rs#L266)

Konsensüs iki katmanlı modülde gerçekleştirilir, bu nedenle Narwhal, HotStuff, İstanbul BFT veya Tendermint gibi harici bir konsensüs algoritması ile birlikte de kullanılabilir. Narwhal, [Celo ](https://www.youtube.com/watch?v=Lwheo3jhAZM)ve [Sommelier ](https://www.prnewswire.com/news-releases/sommelier-partners-with-mysten-labs-to-make-the-cosmos-blockchain-the-fastest-on-the-planet-301381122.html)blockchain'ine entegrasyon aşamasındadır.

Sui Konsensüs Motoru, üretim kriptografisi, kalıcı depolama ve ölçeklendirilmiş bir birincil çalışan mimarisi ile 50 partiden oluşan bir dağıtım için iki saniyelik gecikme süresiyle saniyede 125.000'den fazla işleme ulaşan çoklu-proposer, yüksek verimli konsensüs algoritmaları üzerinde onlarca yıllık çalışmanın en son varyantını temsil etmektedir.

Sui Konsensüs Motoru yaklaşımı aşağıdaki durumlarda önemli ölçeklenebilirlik avantajları sunabilir:

* Giderek daha büyük bloklarla denemeler yapan ve yürütme aşamasından önce kaçak gecikme sürelerini ölçen bir blockchain,
* Hızlı yürütmeye sahip (örneğin işlemlere odaklanmış veya UTXO veri modeline sahip), ancak mempool ve konsensüsün buna ayak uyduramadığı bir blockchain,

### Özellikler <a href="#features" id="features"></a>

Narwhal mempool'un sundukları:

* [Birincil node'da](https://github.com/MystenLabs/narwhal/tree/main/primary)[ ](https://github.com/MystenLabs/narwhal/tree/main/primary)veri kullanılabilirliğinin kriptografik kanıtları ile yüksek verimli bir veri kullanılabilirliği motoru
* bu bilgileri dolaşmak için yapılandırılmış bir grafik veri yapısı
* Disk I/O ve ağ gereksinimlerini birkaç [çalışana ](https://github.com/MystenLabs/narwhal/tree/main/worker)bölen ölçeklendirilmiş bir mimari

[Konsensüs ](https://github.com/MystenLabs/narwhal/tree/main/consensus)bileşeni, grafik çaprazlamalarından yararlanarak sıfır mesaj ek yükü konsensüs algoritması sunar.

### Mimari <a href="#architecture" id="architecture"></a>

Bir Narwhal örneği, bir dizi düğüm arasında bölünmüş bir dizi $3f+1$ hisse biriminden oluşan bir mesaj-geçiş sistemi kurar ve ağı kontrol eden ve en fazla f hisse birimine sahip tarafları bozabilen, hesaplama açısından sınırlı bir düşman varsayar. Validatörler, mempool verilerinin belirtilmemiş bir konsensüs algoritması tarafından kullanıldığı bir bağlamda olduğumuzu vurgulamak için literatürde (DAG tabanlı konsensüs bağlamında) _bloklar_ olarak adlandırılan ve bizim _koleksiyonlar_ olarak etiketlediğimiz işlem gruplarının lidersiz bir grafiğini oluşturmak için işbirliği yapmaktadır.

Grafiğin _köşeleri_ onaylı koleksiyonlardan oluşur. Validatör-yazar tarafından imzalanan her geçerli koleksiyon bir yuvarlak sayı içermeli ve kendisi de validatör hisselerinin bir yeter sayısı (2f+1) tarafından imzalanmalıdır. Bu 2f+1 imzaya _kullanılabilirlik sertifikası_ diyoruz. Ayrıca, bu koleksiyon, bir önceki turdan geçerli sertifikaların (yani, 2f + 1 birim hisseye sahip doğrulayıcılardan gelen sertifikalar) bir çekirdeğine (bkz. Danezis & ark. Şekil 2), grafiğin kenarlarını oluşturan hash işaretçileri içermelidir.

Her koleksiyon şu şekilde oluşturulur: her validatör her tur için _güvenilir_ bir şekilde bir koleksiyon yayınlar. Belirlenen geçerlilik koşullarına tabi olarak, 2f + 1 stake'e sahip validatörler bir koleksiyon alırsa, her biri bir imza ile bunu onaylar. Stake'lerine göre 2f + 1 validatörden gelen imzalar, daha sonra paylaşılan ve potansiyel olarak r + 1 turundaki koleksiyonlara dahil edilen bir kullanılabilirlik sertifikası oluşturur.

Aşağıdaki şekil, A, B, C ve D yetkililerinin katıldığı böyle bir DAG'ın (1'den 5'e kadar) beş tur yapımını temsil etmektedir. Basitlik açısından her bir validatör 1 birim hisseye sahiptir. A'nın A5'teki son turu tarafından geçişli olarak onaylanan koleksiyonlar grafikte tam çizgilerle gösterilmiştir.

flowchart TB subgraph A A5 --> A4 --> A3 --> A2 --> A1 end subgraph B B5 -.-> B4 --> B3 --> B2 --> B1 end subgraph C C5 -.-> C4 --> C3 --> C2 --> C1 end subgraph D D5 -.-> D4 -.-> D3 --> D2 --> D1 end A5 --> B4 & C4 A4 --> C3 & D3 A3 --> B2 & C2 A2 --> C1 & D1 B5 -.-> A4 & C4 B4 --> C3 & D3 B3 --> A2 & C2 B2 --> C1 & D1 C5 -.-> A4 & B4 C4 --> B3 & D3 C3 --> A2 & B2 C2 --> B1 & D1 D5 -.-> A4 & B4 D4 -.-> B3 & C3 D3 --> A2 & B2 D2 --> B1 & C1

### Nasıl Çalışır? <a href="#how-it-works" id="how-it-works"></a>

* Grafik yapısı, her otoritede ve her turda sisteme daha fazla işlem eklenmesine izin verir.
* Sertifikalar, her turda her bir koleksiyonun veya bloğun veri kullanılabilirliğini kanıtlar.
* İçerikleri, her dürüst node'da aynı şekilde dolaşılabilen bir DAG oluşturur.

Bullshark veya Tusk konsensüsü, birkaç a posteriori arasından belirli bir DAG geçişini seçerken, hem onlar hem de harici konsensüs algoritmaları, öncelik kaygılarını yansıtmak için blok / koleksiyon seçimlerine daha fazla karmaşıklık ekleyebilir.

### Bağımlılıklar <a href="#dependencies" id="dependencies"></a>

Narwhal, [Tokio](https://github.com/tokio-rs/tokio), [RocksDB](https://github.com/facebook/rocksdb/) ve genel kriptografi kullanılarak uygulanmıştır. Kriptografi, BLS12-377, BLS12-381 ve Ed25519 kullanılarak node imzalama uygulamalarını içerir.

### Konfigürasyon <a href="#configuration" id="configuration"></a>

Sui Konsensüs Motorunun yeni bir dağıtımını gerçekleştirmek için [Kıyaslamaları Çalıştırma](https://github.com/mystenlabs/narwhal/tree/main/benchmark) bölümündeki talimatları izleyin.

### İleri Okumalar <a href="#further-reading" id="further-reading"></a>

Narwhal ve Tusk (Danezis vd. 2021), yönlendirilmiş asiklik graflardan (DAG) yararlanan bir konsensüs sistemidir. DAG tabanlı konsensüs son 30 yılda geliştirilmiştir ve geçmişin bir kısmı (Wang & al. 2020)'de özetlenmiştir. Narwhal & Tusk'ın en yakın teorik atası (Keidar & al. 2021)'dir.

Narwhal & Tusk [asenkron modelde](https://decentralizedthoughts.github.io/2019-06-01-2019-5-31-models/) geliştirilmiştir. Narwhal ve Tusk'ın Bullshark adı verilen kısmen senkronize bir varyantı (Giridharan 2022)'de sunulmuştur.

Narwhal ve Tusk, Facebook Novi'de [bir araştırma prototipi](https://github.com/facebookresearch/narwhal) olarak başlamıştır.

[Bullshark: DAG BFT Protokolleri Pratik Hale Getirildi](https://arxiv.org/pdf/2201.05677.pdf) - Bullshark daha da yüksek performans için Tusk'ın yerini alıyor.

[DAG BFT ile Buluşuyor - Yeni Nesil BFT Konsensüsü](https://decentralizedthoughts.github.io/2022-06-28-DAG-meets-BFT/) - Sui tarafından kullanılan konsensüs protokolünün gelişimini açıklar.

#### Kaynakça <a href="#bibliography" id="bibliography"></a>

* Danezis, G., Kogias, E. K., Sonnino, A., & Spiegelman, A. (2021). Narwhal and Tusk: A DAG-based Mempool and Efficient BFT Consensus. ArXiv:2105.11827 \[Cs]. [http://arxiv.org/abs/2105.11827](http://arxiv.org/abs/2105.11827)
* Giridharan, N., Kokoris-Kogias, L., Sonnino, A., & Spiegelman, A. (2022). Bullshark: DAG BFT Protocols Made Practical. ArXiv:2201.05677 \[Cs]. [http://arxiv.org/abs/2201.05677](http://arxiv.org/abs/2201.05677)
* Keidar, I., Kokoris-Kogias, E., Naor, O., & Spiegelman, A. (2021). All You Need is DAG. ArXiv:2102.08325 \[Cs]. [http://arxiv.org/abs/2102.08325](http://arxiv.org/abs/2102.08325)
* Wang, Q., Yu, J., Chen, S., & Xiang, Y. (2020). SoK: Diving into DAG-based Blockchain Systems. ArXiv:2012.06128 \[Cs]. [http://arxiv.org/abs/2012.06128](http://arxiv.org/abs/2012.06128)
