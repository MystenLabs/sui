# Move ile Akıllı Kontratlar Yazın

[Move ](https://github.com/MystenLabs/awesome-move)dili ile akıllı kontratlar oluşturmaya yönelik Sui eğitimine hoş geldiniz. Bu eğitim, Move dilinin kısa bir açıklamasını sunmakta ve Move'un Sui'de nasıl kullanılabileceğini gösteren somut örnekler içermektedir.

### Hızlı Bağlantılar <a href="#quick-links" id="quick-links"></a>

* [Neden Move?](https://docs.sui.io/devnet/learn/why-move) - Harici Move kaynaklarına hızlı bağlantılar ve Solidity ile bir karşılaştırma
* [Sui Move'un Core Move'dan farkı](https://docs.sui.io/devnet/learn/sui-move-diffs) - Core Move dili ile Sui'de kullandığımız Move arasındaki farkları vurgular
* [Nesneleri Programlama Eğitim Serisi](https://docs.sui.io/devnet/build/programming-with-objects) - Sui Move'da nesnelerle etkileşim kurmanın tüm güçlü yollarını gösteren eğitim serisi.

### Move <a href="#move" id="move"></a>

Move, güvenli akıllı kontratlar yazmak için kullanılan açık kaynaklı bir dildir. Başlangıçta Facebook'ta [Diem ](https://github.com/diem/diem)blok zincirini güçlendirmek için geliştirilmiştir. Ancak Move, çok farklı veri ve yürütme modellerine sahip blok zincirleri arasında ortak kütüphaneler, araçlar ve geliştirici toplulukları sağlamak için platformdan bağımsız bir dil olarak tasarlanmıştır. [Sui](https://github.com/MystenLabs/sui/blob/main/README.md), [0L ](https://github.com/OLSF/libra)ve [Starcoin](https://github.com/starcoinorg/starcoin), Move'u kullanmaktadır ve bu dili yakında çıkacak ve mevcut birçok platforma (örneğin [Celo](https://www.businesswire.com/news/home/20210921006104/en/Celo-Sets-Sights-On-Becoming-Fastest-EVM-Chain-Through-Collaboration-With-Mysten-Labs)) entegre etme planları da vardır.

Move dil dokümantasyonu Move GitHub [deposunda ](https://github.com/move-language/move)mevcuttur ve dil özelliklerini ayrıntılı olarak açıklayan bir [öğretici ](https://github.com/move-language/move/blob/main/language/documentation/tutorial/README.md)ve bir [kitap ](https://github.com/move-language/move/blob/main/language/documentation/book/src/SUMMARY.md)içerir. Bunlar Move diline ilişkin anlayışınızı derinleştirmek için paha biçilmez kaynaklardır, ancak kendi kendine yeten bir hale getirmeye çalıştığımız Sui eğitimini takip etmek için kesin önkoşullar değildir. Ayrıca Sui, burada incelediğimiz Move'dan bazı yönlerden farklıdır.

Sui'de Move, kullanıcı düzeyindeki varlıkları temsil eden programlanabilir Sui [nesnelerini ](https://docs.sui.io/devnet/learn/objects)tanımlamak, oluşturmak ve yönetmek için kullanılır. Sui, Move'un bir alt kümesini (diğer adıyla Sui Move) etkin bir şekilde kullanarak Move'da yazılabilecek koda ek kısıtlamalar getirir, bu da orijinal Move belgelerinin belirli bölümlerini Sui'de akıllı kontrat geliştirmeye uygulanamaz hale getirir. Sonuç olarak, bu öğreticiyi ve öğreticide verilen ilgili Move dokümantasyon bağlantılarını takip etmek en iyisidir.

Sui ile birlikte gelen Move koduna bakmadan önce, hem Sui ile birlikte gelen kod hem de geliştiriciler tarafından yazılan özel kod için geçerli olan Move kod organizasyonu hakkında kısaca konuşalım.

### Move Kod Organizasyonu <a href="#move-code-organization" id="move-code-organization"></a>

Move kod organizasyonunun (ve dağıtımının) ana birimi bir pakettir. Bir paket, `.move` uzantılı ayrı dosyalarda tanımlanan bir dizi modülden oluşur. Bu dosyalar Move fonksiyonlarını ve tip tanımlarını içerir. Bir paket, örneğin paket meta verileri veya paket bağımlılıkları gibi paket yapılandırmasını açıklayan `Move.toml` manifest dosyasını içermelidir. Paket bildirim dosyaları hakkında daha fazla bilgi için [Move.toml](https://github.com/move-language/move/blob/main/language/documentation/book/src/packages.md#movetoml) dosyasına bakın.

Minimal paket kaynak dizin yapısı aşağıdaki gibidir ve manifest dosyası ile bir veya daha fazla modül dosyasının bulunduğu `kaynaklar` alt dizinini içerir:

```
my_move_package
├── Move.toml
├── sources
    ├── my_module.move
```

Paket düzeni hakkında daha fazla bilgi için [Paket Düzeni ve Manifest Sözdizimi](https://github.com/move-language/move/blob/main/language/documentation/book/src/packages.md#package-layout-and-manifest-syntax) bölümüne bakın.

Şimdi biraz Move koduna bakmaya hazırız! Ana Move dili yapılarının giriş niteliğindeki açıklaması için okumaya devam edebilir ya da[ basit bir Move paketi yazarak](https://docs.sui.io/devnet/build/move/write-package) ve ek kod [örneklerine ](https://docs.sui.io/devnet/explore/examples)göz atarak doğrudan koda geçebilirsiniz.

### Move kaynak koduna ilk bakış <a href="#first-look-at-move-source-code" id="first-look-at-move-source-code"></a>

Sui platformu, Sui işlemlerini başlatmak için gerekli olan çerçeve Move kodunu içerir. Özellikle Sui, Move dilinde tanımlanan özel varlıklar olan birden fazla kullanıcı tanımlı coin türünü destekler. Sui çerçeve kodu, özel coinlerin oluşturulmasını ve yönetilmesini destekleyen `Coin` modülünü içerir. `Coin` modülü [coin.move](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/coin.move) dosyasında bulunur. Beklediğiniz gibi, `Coin` modülünü içeren paketin nasıl oluşturulacağını açıklayan manifest dosyası ilgili [Move.toml](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/Move.toml) dosyasında bulunur.

`Coin` modül dosyasında modül tanımının nasıl göründüğünü görelim:

```
module sui::coin {
...
}
```

(Şimdilik modül içeriğinin geri kalanı hakkında endişelenmeyelim; daha sonra Move kitabında modüller hakkında daha fazla bilgi edinebilirsiniz).

> **Önemli:** _Sui Move'da, paket adları her zaman CamelCase ile yazılırken, adres takma adları küçük harfle yazılır, örneğin_ `sui`` `_`=`_` ``0x2` _ve `std =`_` ``0x1.` _Yani:_ Sui _= içe aktarılan paketin adı (Sui = sui framework),_ `sui`` `_`=`_` ``0x2` _adres takma adı,_ `sui::sui`` `_`=`_` ``0x2` _adresi altındaki sui modülü ve_ `sui::sui::SUI` _= yukarıdaki modüldeki tip._

Görüldüğü gibi, bir modülü tanımlarken, modül adını (`Coin`) belirtiriz ve öncesinde bu modülün bulunduğu paketin adını (`Sui`) belirtiriz. Paket adı ve modül adının kombinasyonu, Move kaynak kodundaki bir modülü benzersiz bir şekilde tanımlamak için kullanılır (örneğin, diğer modüllerden if kullanabilmek için). Paket adı global olarak benzersizdir, ancak farklı paketler aynı ada sahip modüller içerebilir. Modül adları benzersiz değildir, ancak benzersiz paket adı ile birleştirildiğinde benzersiz bir kombinasyon oluşturur.

Örneğin, yayınlanmış "P" paketiniz varsa, "P" adında başka bir paket yayınlayamazsınız. Aynı anda sistemde "P1::M1", "P2::M1" ve "P1::M2" modüllerine sahip olabilirsiniz ancak "P1::M1" modülüne sahip olamazsınız.

Kaynak kod düzeyinde bir varlığa sahip olmanın yanı sıra, [Move kod organizasyonunda](https://docs.sui.io/devnet/build/move#move-code-organization) tartıştığımız gibi, Sui'deki bir paket aynı zamanda bir Sui nesnesidir ve manifest dosyasında atanan benzersiz bir isme ek olarak benzersiz bir sayısal kimliğe sahip olmalıdır:

```
[addresses]
sui = "0x2"
```

#### Move struct'ları <a href="#move-structs" id="move-structs"></a>

`Coin` modülü, farklı kullanıcı tanımlı madeni para türlerini Sui nesneleri olarak temsil etmek için kullanılabilecek `Coin` struct türünü tanımlar:

```
struct Coin<phantom T> has key, store {
    id: UID,
    value: u64
}
```

Move'un struct türü, C veya C++ gibi diğer programlama dillerinde tanımlanan struct türlerine benzer ve bir ad ve bir dizi tiplendirilmiş alan içerir. Özellikle, struct alanları tamsayı türü gibi ilkel bir türde veya bir struct türünde olabilir.

Move kitabında Move [ilkel türleri](https://github.com/move-language/move/blob/main/language/documentation/book/src/SUMMARY.md#primitive-types) ve [yapıları](https://github.com/move-language/move/blob/main/language/documentation/book/src/structs-and-resources.md) hakkında daha fazla bilgi edinebilirsiniz.

Bir Move struct türünün `Coin` gibi bir Sui nesne türünü tanımlayabilmesi için ilk alanının `id:UID` olmalıdır; bu, [nesne modülünde](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/object.move) tanımlanmış bir struct türüdür. Move struct türü ayrıca, nesnenin Sui'nin global depolama alanında tutulmasını sağlayan `anahtar` yeteneğine de sahip olmalıdır. Bir Move yapısının yetenekleri, yapı tanımındaki `has` anahtar sözcüğünden sonra listelenir ve bunların varlığı (veya yokluğu), bir tanım veya belirli bir yapının örnekleri üzerinde çeşitli özelliklerin uygulanmasına yardımcı olur.

Move kitabında struct [yetenekleri ](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md)hakkında daha fazla bilgi edinebilirsiniz.

Coin struct'ının farklı `coin`türlerini temsil edebilmesinin nedeni, struct tanımının bir tür parametresiyle parametrelendirilmiş olmasıdır. `Coin` struct'ının bir örneği oluşturulduğunda, farklı coin türlerini birbirinden ayırmak için rastgele bir somut Move türü (örneğin başka bir struct türü) geçirilebilir.

Boş zamanlarınızda Generics olarak bilinen Move türü parametreleri (ve isteğe bağlı phantom anahtar sözcüğü) hakkında bilgi edinin.

Özellikle, Sui'de halihazırda tanımlanmış bir özel coin türü, Sui hesaplamaları için ödeme yapmak üzere kullanılan bir tokeni (daha genel olarak gas olarak bilinir) temsil eden Coin'\<SUI dir - bu durumda, Coin yapısını parametrelendirmek için kullanılan somut tür, SUI modülündeki SUI yapısıdır:&#x20;

```
struct SUI has drop {}
```

[Basit bir Move paketinin nasıl yazılacağını ](https://docs.sui.io/devnet/build/move/write-package)anlatan bölümde özel yapıların nasıl tanımlanacağını ve örneklendirileceğini göstereceğiz.

#### Move işlevleri <a href="#move-functions" id="move-functions"></a>

Diğer popüler programlama dillerinde olduğu gibi Move'da da ana hesaplama birimi bir fonksiyondur. [Coin](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/coin.move)[ modülünde](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/coin.move) tanımlanan en basit fonksiyonlardan birine, yani `değer` fonksiyonuna bakalım.

```
public fun value<T>(self: &Coin<T>): u64 {
    self.value
}
```

Bu _genel_ fonksiyon, `Coin` struct'ının belirli bir örneğinde o anda saklanan işaretsiz tamsayı değerini döndürmek için diğer modüllerdeki fonksiyonlar tarafından çağrılabilir. Bir struct'ın alanlarına doğrudan erişime, [Ayrıcalıklı Struct İşlemleri](https://github.com/move-language/move/blob/main/language/documentation/book/src/structs-and-resources.md#privileged-struct-operations) bölümünde açıklandığı gibi yalnızca belirli bir struct'ı tanımlayan modül içinde izin verilir. Fonksiyonun gövdesi, Coin struct örnek parametresinden `değer` alanını alır ve döndürür. `Coin` parametresinin `Coin` struct örneğine salt okunur bir referans olduğuna ve parametre türünün önündeki `&` ile gösterildiğine dikkat edin. Move'un tip sistemi, salt okunur referanslarla (değiştirilebilir referansların aksine) aktarılan struct örneği argümanlarının bir fonksiyonun gövdesinde değiştirilemeyeceğine dair bir değişmezi zorunlu kılar.

Move[ referansları](https://github.com/move-language/move/blob/main/language/documentation/book/src/references.md#references) hakkında daha fazla bilgiyi Move kitabından okuyabilirsiniz.

Move fonksiyonlarının diğer fonksiyonlardan nasıl çağrılacağını ve yenilerinin nasıl tanımlanacağını [basit bir Move paketinin nasıl yazılacağını](https://docs.sui.io/devnet/build/move/write-package) anlatan bölümde göstereceğiz.

Bununla birlikte, diğer fonksiyonlardan çağrılabilen fonksiyonlara ek olarak, Move dilinin Sui çeşidi, doğrudan Sui'den (örneğin, farklı bir dilde yazılabilen bir Sui uygulamasından) çağrılabilen ve belirli bir dizi özelliği karşılaması gereken _entry_ fonksiyonlarını da tanımlar.

**Entry işlevleri**

Sui'deki temel işlemlerden biri, gaz nesnelerinin bireysel kullanıcıları temsil eden adresler arasında aktarılmasıdır. Ve gaz nesnesi transferini gerçekleştirmek için SUI modülünde en basit giriş fonksiyonlarından biri tanımlanmıştır:

```
public entry fun transfer(c: coin::Coin<SUI>, recipient: address, _ctx: &mut TxContext) {
    ...
}
```

(Şimdilik fonksiyon gövdesi hakkında endişelenmeyelim - fonksiyon Sui çerçevesinin bir parçası olduğundan, amaçlanan şeyi yapacağına güvenebilirsiniz).

Genel olarak, bir giriş fonksiyonu aşağıdaki özellikleri karşılamalıdır:

* &#x20;`giriş` değiştiricisine sahiptir.
  * Not: Görünürlük önemli değildir. İşlev `public`, `public(friend)` veya `internal` olabilir.
* dönüş değeri yok
* (isteğe bağlı) son parametre olarak [TxContext](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/tx\_context.move)[ modülünde](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/tx\_context.move) tanımlanan `TxContext` struct'ının bir örneğine değiştirilebilir bir referansa sahiptir

Daha somut olarak, `transfer` fonksiyonu geneldir, geri dönüş değeri yoktur ve üç parametresi vardır:

* `c` - mülkiyeti devredilecek bir gas nesnesini temsil eder
* `recipient` - hedeflenen alıcının [adresi](https://github.com/move-language/move/blob/main/language/documentation/book/src/address.md)
* `_ctx` - `TxContext` struct'ının bir örneğine değiştirilebilir bir referans (bu özel durumda, `_` ile başlayan adıyla belirtildiği gibi bu parametre aslında işlevin gövdesinde kullanılmaz)
  * Kullanılmadığı için parametrenin kaldırılabileceğini unutmayın. `TxContext'in` değiştirilebilir referansı giriş fonksiyonları için isteğe bağlıdır.

Calling Move kodunda `transfer` fonksiyonunun bir Sui CLI istemcisinden nasıl çağrıldığını görebilirsiniz.
