# Neden Move?

Sui'de [Akıllı Kontratları](https://docs.sui.io/devnet/build/move) Move Programlama dili ile yazarsınız. Bu sayfa önemli [Move](https://golden.com/wiki/Move\_\(programming\_language\)-MNA4DZ6) kaynaklarına bağlantı verir ve [Move](https://github.com/move-language/move/tree/main/language/documentation) ile Solidity programlama dillerini karşılaştırır. Geleneksel akıllı kontrat dilleriyle ilgili sorunların tam bir açıklaması için [Move Problem Bildirimi](https://github.com/MystenLabs/awesome-move/blob/main/docs/problem\_statement.md)'ne bakın.

### Sui Move <a href="#sui-move" id="sui-move"></a>

İlk olarak, Move'un iyi seviyede desteklenen [Rust](https://www.rust-lang.org/) programlama diline dayandığını unutmayın. S[ui Move, core Move'dan ince ama belirgin şekillerde ayrılır](https://docs.sui.io/devnet/learn/sui-move-diffs). İşte Sui Move'u geliştirmek için kaynaklar:

* [Sui Move duyurusu](https://sui.io/resources-move/why-we-created-sui-move/)
* [Sui kaynak kodu](https://github.com/MystenLabs/sui)
* `rustdoc` [çıktısı](https://docs.sui.io/devnet/build/install#rustdoc)
* [Örneklerle Sui Move](https://examples.sui.io/)

### Move kaynakları <a href="#move-resources" id="move-resources"></a>

Bu bölüm Move programlama dili ile ilgili harici kaynaklara bağlantıları bir araya getirir. Bu sitedeki core Move kaynakları için Move ile Akıllı Kontratlar sayfamıza ve Nesnelerle Move Programlama eğitim serimize de bakınız.

* Programlanabilir nesnelerin ayrıntılı olarak anlatıldığı [Move & Sui podcast](https://zeroknowledge.fm/228-2/)'i Zero Knowledge'da.
* Sui ekibinin bir üyesi tarafından yazılan orijinal [Move Kitabı](https://move-book.com/index.html)
* [Core Move](https://github.com/move-language/move/tree/main/language/documentation) dokümantasyonu:
  * [Başlangıç Eğitimi](https://github.com/move-language/move/blob/main/language/documentation/tutorial/README.md) - Bir Move modülü yazmak için adım adım ilerleyen bir kılavuz.
  * [Kitap](https://github.com/move-language/move/blob/main/language/documentation/book/src/introduction.md) - [Çeşitli konuları](https://github.com/move-language/move/tree/main/language/documentation/book/src) içeren bir özet.
  * [Örnekler](https://github.com/move-language/move/tree/main/language/documentation/examples/experimental) - Bir [madeni parayı tanımlamak](https://github.com/move-language/move/tree/main/language/documentation/examples/experimental/basic-coin) ve [takas etmek](https://github.com/move-language/move/tree/main/language/documentation/examples/experimental/coin-swap) gibi bir dizi örnek.
* [Awesome Move](https://github.com/MystenLabs/awesome-move/blob/main/README.md) - Blok incirlerinden kod örneklerine kadar Move ile ilgili kaynakların bir özeti.

### Move vs Solidity <a href="#move-vs-solidity" id="move-vs-solidity"></a>

Şu anda, blockchain dilleri sahnesindeki ana oyuncu Solidity'dir. İlk blockchain dillerinden biri olan Solidity, iyi bilinen veri türlerini (örn. byte, array, string) ve veri yapılarını (hashmaps gibi) kullanarak temel programlama dili kavramlarını uygulamak ve iyi bilinen bir tabanı kullanarak özel soyutlamalar oluşturabilmek için tasarlanmıştır.

Ancak, blockchain teknolojisi geliştikçe, blockchain dillerinin temel amacının dijital varlıklarla yapılan işlemler olduğu ve bu tür dillerin temel niteliğinin güvenlik ve doğrulanabilirlik (ek bir güvenlik katmanı olarak) olduğu anlaşılmıştır.

Move her iki sorunu da ele almak için özel olarak tasarlanmıştır: dijital varlıkların temsili ve bunlar üzerinde güvenli işlemler. Ek koruma sağlamak için [Move Prover](https://arxiv.org/abs/2110.08362) doğrulama aracı ile birlikte geliştirilmiştir. Bu, Move geliştiricilerinin uygulamalarının temel doğruluk özellikleri için resmi spesifikasyonlar yazmalarına ve ardından bu özelliklerin tüm olası işlemler ve girdiler için geçerli olup olmadığını kontrol etmek için prover'ı kullanmalarına olanak tanır.

EVM ve Move arasındaki temel farklardan biri de varlıklar için kullanılan veri modelidir:

* https://app.gitbook.com/o/vNbegyLt3evXUJ8IDXQi/s/pQWj2eKJ2etESc81Yy6H/oegren/neden-move/\~/permissions

Sui, performans için Move veri modelinden büyük ölçüde yararlanır. Sui'nin kalıcı durumu, işlemler tarafından güncellenebilen, oluşturulabilen ve yok edilebilen bir dizi programlanabilir Move nesnesidir. Her nesne, Sui validatörlerinin nesneyi kullanan işlemleri nedensel olarak ilgisiz işlemlerle paralel olarak yürütmesine ve işlemesine olanak tanıyan sahiplik meta verilerine sahiptir. Move'un tip sistemi, bu sahiplik meta verilerinin yürütmeler arasında bütünlüğünü sağlar. Sonuç, geliştiricilerin sıradan Move akıllı kontratları yazdığı, ancak validatörlerin işlemleri olabildiğince verimli bir şekilde yürütmek ve işlemek için veri modelinden yararlandığı bir sistemdir.

EVM veri modelinde bu mümkün değildir. Varlıklar dinamik olarak indekslenebilir haritalarda saklandığından, bir validatör işlemlerin aynı varlığa ne zaman dokunabileceğini belirleyemez. Sui'nin paralel yürütme ve taahhüt şeması, kontratlar arasında serbestçe akabilen yapılandırılmış varlıkları tanımlamak için kelime dağarcığına sahip Move gibi bir dile ihtiyaç duyar. Açık konuşmak gerekirse: **EVM/Solidity'yi Move'a tercih etsek bile, Sui'yi benzersiz kılan performans atılımlarından ödün vermeden bunları Sui'de kullanamazdık.**

Move'un ana avantajlarından biri veri birleştirilebilirliğidir. İlk varlık X'i içinde tutacak yeni bir yapı (varlık) Y oluşturmak her zaman mümkündür. Daha da fazlası - jeneriklerin eklenmesiyle, herhangi bir varlığı sarabilecek, sarılmış bir varlığa ek özellikler sağlayabilecek veya onu başkalarıyla birleştirebilecek jenerik sarıcı (generic wrapper) Z(T) tanımlamak mümkündür. [Sandviç örneğimizde](https://github.com/MystenLabs/sui/tree/main/sui\_programmability/examples/basics/sources/sandwich.move) birleştirilebilirliğin nasıl çalıştığını görün.
