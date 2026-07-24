# Sui Move'un Core Move'dan farkı nedir?

Bu belge Sui programlama modelini açıklamakta ve core (daha önce Diem) Move dili ile Sui'de kullandığımız Move arasındaki farkları vurgulamaktadır. Öncelikle Move'un bir dil, Sui'nin ise bir platform olduğunu unutmayın.

Sui Move'u yaratmanın ardındaki motivasyonlar hakkında daha fazla bilgi edinmek için göz atın: [Sui Move'u Neden Yarattık?](https://medium.com/mysten-labs/why-we-created-sui-move-6a234656c36b)

Genel olarak, diğer sistemler için yazılmış Move kodu bu istisnalar dışında Sui'de çalışacaktır:

* [Global Depolama Operatörleri](https://move-language.github.io/move/global-storage-operators.html) (Global Storage Operators)
* [Anahtar Kabiliyetler](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md#key) (Key Abilities)

İşte temel farklılıkların bir özeti:

1. Sui kendi [nesne-merkezli global depolama](broken-reference)'sını kullanır
2. Adresler [nesne kimliklerini temsil eder](broken-reference)
3. Sui nesnelerinin [global çapta eşsiz kimlikleri](broken-reference) vardır
4. Sui'nin [modül başlatıcıları (init)](https://docs.sui.io/devnet/learn/sui-move-diffs#module-initializers) vardır
5. Sui [giriş noktaları nesne referanslarını girdi olarak alır](https://docs.sui.io/devnet/learn/sui-move-diffs#entry-points-take-object-references-as-input)

Her bir değişikliğin ayrıntılı açıklamasını aşağıda bulabilirsiniz.

### Nesne Merkezli Global Depolama <a href="#object-centric-global-storage" id="object-centric-global-storage"></a>

Core Move'da global depolama, programlama modelinin bir parçasıdır ve _move\_to, move\_from_ ve daha birçok [global depolama operatörü](https://move-language.github.io/move/global-storage-operators.html) gibi özel işlemlerle erişilebilir. Hem kaynaklar hem de modüller core Move global depolama alanında saklanır. Bir modül yayınladığınızda, Move içinde yeni oluşturulan bir modül adresinde saklanır. Yeni bir nesne (diğer adıyla kaynak) oluşturulduğunda, genellikle bir adreste de saklanır.

Ancak zincir üzerinde depolama pahalı ve sınırlıdır (depolama ve indeksleme için optimize edilmemiştir). Mevcut blokzincirleri, pazar yerleri ve sosyal uygulamalar gibi depolama ağırlıklı uygulamaların üstesinden gelecek şekilde ölçeklendirilemez.

Yani Sui Move'da global depolama yoktur. Sui Move'da küresel depolama ile ilgili işlemlerin hiçbirine izin verilmez. (İhlalleri tespit etmek için bunun için bir bayt kodu (bytecode) doğrulayıcımız var.) Bunun yerine, depolama yalnızca Sui içinde gerçekleşir. Bir modül yayınladığımızda, yeni yayınlanan modül Move depolama alanı yerine Sui depolama alanında saklanır. Benzer şekilde, yeni oluşturulan nesneler de Sui depolama alanında saklanır. _Bu aynı zamanda Move'da bir nesneyi okumamız gerektiğinde, global depolama işlemlerine güvenemeyeceğimiz, bunun yerine Sui'nin erişilmesi gereken tüm nesneleri Move'a açıkça aktarması gerektiği anlamına gelir._

### Adresler nesne kimliklerini temsil eder <a href="#addresses-represent-object-ids" id="addresses-represent-object-ids"></a>

Move'da özel bir _adres_ tipi vardır. Bu tip, core Move'daki adresleri temsil etmek için kullanılır. Core Move'un global depolama ile uğraşırken bir hesabın adresini bilmesi gerekir. _Adres_ tipi 16 bayttır ve bu da Core Move güvenlik modeli için yeterlidir.

Sui'de, Move'da global depolamayı desteklemediğimizden, kullanıcı hesaplarını temsil etmek için _adres_ tipine ihtiyacımız yoktur. Bunun yerine, Nesne Kimliğini temsil etmek için _adres_ tipini kullanırız. Adres kullanımını anlamak için Sui framework'ündeki [object.move](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/object.move) dosyasına bakın.

### Global çapta eşsiz kimliklere ve anahtar yeteneğine sahip objeler <a href="#object-with-key-ability-globally-unique-ids" id="object-with-key-ability-globally-unique-ids"></a>

Move'a özel nesneler ile Move-Sui sınırından geçirilebilen nesneler (yani Sui depolama alanında saklanabilen nesneler) arasında ayrım yapabilme ihtiyacımız var. Bu önemlidir çünkü Move-Sui sınırındaki nesneleri serileştirebilmemiz/seri dışı bırakabilmemiz gerekir ve bu işlem nesnelerin şekli hakkında varsayımlarda bulunur.

Bir Sui nesnesine açıklama eklemek için Move'daki [anahtar yeteneğinden](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md#key) yararlanıyoruz. Core Move'da anahtar yeteneği, tipin global depolama için bir anahtar olarak kullanılabileceğini söylemek için kullanılır. Sui Move'da global depolamaya dokunmadığımız için, bu yeteneği yeni bir amaç için kullanabiliyoruz. Anahtar yeteneğine sahip herhangi bir yapının _ID_ tipinde bir _id_ alanıyla başlamasını şart koşuyoruz. ID tipi hem ObjectID'yi hem de sıra numarasını (diğer adıyla sürüm) içerir. ID alanının değişmez olduğundan ve diğer nesnelere aktarılamayacağından (her nesnenin benzersiz bir ID'si olması gerektiğinden) emin olmak için bytecode doğrulayıcılarımız vardır.

### Modül başlatıcıları <a href="#module-initializers" id="module-initializers"></a>

[Nesne merkezli global depolama](https://docs.sui.io/devnet/learn/sui-move-diffs#object-centric-global-storage) bölümünde açıklandığı gibi, Move modülleri Sui depolama alanında yayınlanır. İsteğe bağlı olarak bir modülde tanımlanan özel bir başlatıcı işlevi, modüle özgü verileri önceden başlatmak (örneğin, singleton nesneleri oluşturmak) amacıyla Sui çalışma zamanı tarafından modül yayını sırasında (bir kez) yürütülür. Başlatıcı fonksiyonun yayın sırasında çalıştırılabilmesi için aşağıdaki özelliklere sahip olması gerekir:

* `init ismi`
* `&mut TxContext` tipinin tek parametresi
* Dönüş değerleri yok
* Özel

### Giriş noktaları nesne referanslarını girdi olarak alır <a href="#entry-points-take-object-references-as-input" id="entry-points-take-object-references-as-input"></a>

Sui, diğer fonksiyonlardan çağrılabilen fonksiyonlara ek olarak doğrudan Sui'den çağrılabilen giriş fonksiyonları sunar. [Giriş fonksiyonları](https://docs.sui.io/devnet/build/move#entry-functions) bölümüne bakın.

### Sonuç <a href="#conclusion" id="conclusion"></a>

Özetle Sui, Move'un güvenlik ve esnekliğinden yararlanır ve yukarıda açıklanan özelliklerle onu geliştirerek verimi büyük ölçüde artırır, sonuçlandırmadaki gecikmeleri azaltır ve Move programlamayı kolaylaştırır. Şimdi [Sui'nin nasıl çalıştığını](https://docs.sui.io/devnet/learn/how-sui-works) görün. Tüm ayrıntılar için [Sui Akıllı Kontratlar Platformu white paper'ına](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf) bakın.
