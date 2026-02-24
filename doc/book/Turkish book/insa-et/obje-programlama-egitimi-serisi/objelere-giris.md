# Objelere Giriş

#### Sui Nesnesini Tanımlama <a href="#define-sui-object" id="define-sui-object"></a>

Move'da ilkel veri tiplerinin yanı sıra struct kullanarak organize veri struct'ları tanımlayabiliriz. Örneğin:

```
struct Color {
    red: u8,
    green: u8,
    blue: u8,
}
```

Yukarıdaki `struct`, RGB rengini temsil edebilen bir veri struct'ını tanımlar. Bunun gibi `struct`'lar, karmaşık semantiğe sahip verileri düzenlemek için kullanılabilir. Ancak, `Color`gibi struct'ların örnekleri henüz Sui nesneleri değildir. Bir Sui nesne türünü temsil eden bir `struct` tanımlamak için, tanıma bir `key`(anahtar) özelliği eklemeliyiz ve `struct`'ın ilk alanı, [nesne modülünden](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/object.move) `UID` türüne sahip nesnenin `id`'si (kimliği) olmalıdır:

```
use sui::object::UID;

struct ColorObject has key {
    id: UID,
    red: u8,
    green: u8,
    blue: u8,
}
```

Artık `ColorObject`bir Sui nesne türünü temsil eder ve sonunda Sui zincirinde depolanabilecek Sui nesneleri oluşturmak için kullanılabilir.

> 📚_Hem core Move hem de Sui Move'da,_ [_anahtar yeteneği_](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md#key) _global depolamada anahtar olarak görünebilecek bir türü belirtir. Bununla birlikte, global depolamanın yapısı biraz farklıdır: core Move (type, `adress`) indeksli bir harita kullanırken, Sui Move nesne kimlikleriyle anahtarlanan bir harita kullanır._

> 💡_`UID` türü Sui için dahili bir türdür ve büyük olasılıkla doğrudan onunla uğraşmanız gerekmeyecektir. Meraklı okuyucular için, bir nesneyi tanımlayan "benzersiz kimliği (unique ID)" içerir. `UID` türündeki iki değerin hiçbir zaman aynı temel bayt kümesine sahip olmayacağı anlamında benzersizdir._

#### Sui Nesnesini Oluşturmak <a href="#create-sui-object" id="create-sui-object"></a>

Bir Sui nesne türünü nasıl tanımlayacağımızı öğrendiğimize göre, bir Sui nesnesini nasıl oluşturacağız/tanımlayacağız? Kendi türünden yeni bir Sui nesnesi oluşturmak için, `id` dahil olmak üzere her bir alana bir başlangıç değeri atamalıyız. Bir Sui nesnesi için yeni bir `UID` oluşturmanın tek yolu `object::new` fonksiyonunu çağırmaktır. `new` işlevi, benzersiz `id`'ler oluşturmak için geçerli işlem bağlamını bir argüman olarak alır. İşlem bağlamı `&mut TxContext` tipindedir ve bir [entry fonksiyonundan](https://docs.sui.io/devnet/build/move#entry-functions) (bir işlemden doğrudan çağrılabilen bir fonksiyon) aktarılmalıdır. `ColorObject` için bir yapıcıyı nasıl tanımlayabileceğimize bakalım:

```
// object creates an alias to the object module, which allows us call
// functions in the module, such as the `new` function, without fully
// qualifying, e.g. `sui::object::new`.
use sui::object;
// tx_context::TxContext creates an alias to the the TxContext struct in tx_context module.
use sui::tx_context::TxContext;


fun new(red: u8, green: u8, blue: u8, ctx: &mut TxContext): ColorObject {
    ColorObject {
        id: object::new(ctx),
        red,
        green,
        blue,
    }
}
```

> 💡_Move, alan adının bağlı olduğu değer değişkeninin adıyla aynı olması durumunda alan değerlerini atlamamıza olanak tanıyan alan punning'ini destekler. Yukarıdaki kod, "`red: red,`" ifadesinin kısaltması olarak "`red,`" yazmak için bundan yararlanır._

#### Sui Nesnesini Saklamak <a href="#store-sui-object" id="store-sui-object"></a>

`ColorObject` için bir kurucu tanımladık. Bu kurucunun çağrılması, değeri geçerli işlevden döndürülebileceği, diğer işlevlere aktarılabileceği veya başka bir struct'ın içinde saklanabileceği yerel bir değişkene koyacaktır. Ve tabii ki, nesne kalıcı global depolama alanına yerleştirilebilir, böylece dış dünya tarafından okunabilir ve sonraki işlemlerde erişilebilir.

Kalıcı depolama alanına nesne eklemeye yönelik tüm API'ler [`transfer`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/transfer.move) modülünde bulunur. Anahtar API'lerden biri şudur:&#x20;

```
public fun transfer<T: key>(obj: T, recipient: address)
```

Bu işlem `obj`'yi, `recipient`'i objenin sahibi olarak kaydeden meta verilerle birlikte global depolama alanına yerleştirir. Sui'de her nesnenin bir sahibi olmalıdır; bu sahip bir adres, başka bir nesne veya "ortak" olabilir -- daha fazla ayrıntı için [nesne sahipliği](https://docs.sui.io/devnet/learn/objects#object-ownership) bölümüne bakın.

> 💡_Move çekirdeğinde, (a, T) -> t girişini global depoya eklemek için move\_to(a: adres, t: T) çağırırız. Ancak (yukarıda açıklandığı gibi) Sui Move'un global depolama şeması farklı olduğundan, move\_to veya core Move'daki diğer global depolama operatörleri yerine Transfer API'lerini kullanırız. Bu operatörler Sui Move'da kullanılamaz._

Bu API'nin yaygın bir kullanımı, nesneyi geçerli işlemin göndericisine/imzalayıcısına aktarmaktır (örneğin, size ait bir NFT'ye nane basmak). Geçerli işlemin göndericisini elde etmenin tek yolu, bir giriş fonksiyonundan aktarılan işlem bağlamına güvenmektir. Bir giriş fonksiyonunun son argümanı, `ctx: &mut TxContext` olarak tanımlanan geçerli işlem bağlamı olmalıdır. Geçerli imzalayanın adresini elde etmek için `tx_context::sender(ctx)` çağrılabilir.

Aşağıda yeni bir `ColorObject` oluşturan ve bunu işlemin göndericisine ait kılan kod yer almaktadır:

```
use sui::transfer;

// This is an entry function that can be called directly by a Transaction.
public entry fun create(red: u8, green: u8, blue: u8, ctx: &mut TxContext) {
    let color_object = new(red, green, blue, ctx);
    transfer::transfer(color_object, tx_context::sender(ctx))
}
```

> 💡_Adlandırma kuralı: Kurucular tipik olarak `new` olarak adlandırılır ve struct türünün bir örneğini döndürür. `create` işlevi tipik olarak struct'ı oluşturan ve istenen sahibine (çoğunlukla göndericiye) aktaran bir entry işlevi olarak tanımlanır._

`ColorObject'e` renk değerlerini döndüren bir getter da ekleyebiliriz, böylece `ColorObject` dışındaki modüller bu değerleri okuyabilir:

```
public fun get_color(self: &ColorObject): (u8, u8, u8) {
    (self.red, self.green, self.blue)
}
```

Kodun tamamını [color\_object.move](https://app.gitbook.com/s/rmN1QQp5gHQReuPAxVTk/) adresinde bulabilirsiniz.

Kodu derlemek için, [Sui'yi yüklediğinizden](https://docs.sui.io/devnet/build/install) emin olun, böylece sui PATH'de olur. Kod kök dizininde (`Move.toml'nin` olduğu yer), çalıştırın:

```
sui move build
```

#### Birim testlerinin yazımı <a href="#writing-unit-tests" id="writing-unit-tests"></a>

`Create` fonksiyonunu tanımladıktan sonra, Sui işlemlerini göndermeye gerek kalmadan, birim testlerini kullanarak Move'da bu fonksiyonu test etmek istiyoruz. [Sui, global depolamayı Move dışında ayrı olarak yönettiği için](https://docs.sui.io/devnet/learn/sui-move-diffs#object-centric-global-storage), Move içinde global depolamadan nesneleri almanın doğrudan bir yolu yoktur. Bu da bir soru ortaya çıkarıyor: `create` fonksiyonunu çağırdıktan sonra, nesnenin düzgün bir şekilde aktarıldığını nasıl kontrol edeceğiz?

Move'da kolay test yapmaya yardımcı olmak için, [test\_scenario](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/test\_scenario.move) modülünde global depolama alanına yerleştirilen nesnelerle etkileşime girmemizi sağlayan kapsamlı bir test çerçevesi sağlıyoruz. Bu, herhangi bir işlevin davranışını doğrudan Move birim testlerinde test etmemizi sağlar. Bunların çoğu [Move test dokümanımızda](https://docs.sui.io/devnet/build/move/build-test#sui-specific-testing) da ele alınmaktadır.

`test_scenario`'nun amacı, her biri belirli bir adresten gönderilen bir dizi Sui işlemini taklit etmektir. Test yazan bir geliştirici, bu işlemi gönderen kullanıcının adresini argüman olarak alan ve bir test senaryosunu temsil eden `Scenario` struct'ının bir örneğini döndüren `test_scenario::begin` işlevini kullanarak ilk işlemi başlatır.

Scenario struct'ının bir örneği, Sui'nin nesne depolamasını taklit eden adres başına bir nesne havuzu içerir ve havuzdaki nesneleri işlemek için yardımcı işlevler sağlanır. İlk işlem tamamlandıktan sonra, mevcut senaryoyu temsil eden Scenario struct'ının bir örneğini ve (yeni) bir kullanıcının adresini argüman olarak alan test\_scenario::next\_tx işlevi kullanılarak sonraki işlemler başlatılabilir.

Şimdi `create` fonksiyonu için bir test yazmayı deneyelim. `test_scenario` kullanması gereken testler ayrı bir modülde, ya bir `tests` dizini altında ya da aynı dosyada ancak `#[test_only]` ile açıklanmış bir modülde olmalıdır. Bunun nedeni, `test_scenario`'nun kendisinin yalnızca test amaçlı bir modül olması ve yalnızca test amaçlı modüller tarafından kullanılabilmesidir.

Öncelikle, teste sabit kodlanmış bir test adresiyle başlıyoruz, bu da bize `test_scenario::begin` ile başlatılan işlemi bu adresten gönderiyormuşuz gibi bir işlem bağlamı verecektir. Ardından, bir `ColorObject` oluşturması ve bunu test adresine aktarması gereken `create` fonksiyonunu çağırıyoruz:

```
let owner = @0x1;
// Create a ColorObject and transfer it to @owner.
let scenario_val = test_scenario::begin(owner);
let scenario = &mut scenario_val;
{
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 0, 255, ctx);
};
```

> 📚_"}" den sonra bir ";" olduğuna dikkat edin. ; bir dizi ifadeyi sıralamak için gereklidir ve { ... bloğu bile bir ifadedir. } bloğu bile bir ifadedir! Ayrıntılı açıklama için_ [_Move kitabına_](https://move-book.com/syntax-basics/expression-and-scope.html) _bakın._

Şimdi, ilk işlem tamamlandıktan sonra (**ve yalnızca ilk işlem tamamlandıktan sonra**), `@0x1` adresi nesneye sahip olmalıdır. Önce nesnenin başkasına ait olmadığından emin olalım:

```
let not_owner = @0x2;
// Check that not_owner does not own the just-created ColorObject.
test_scenario::next_tx(scenario, not_owner);
{
    assert!(!test_scenario::has_most_recent_for_sender<ColorObject>(scenario), 0);
};
```

`test_scenario::next_tx`, işlem göndericisini bir öncekinden farklı yeni bir adres olan `@0x2`'ye geçirir. `test_scenario::has_most_recent_for_sender`, işlemin mevcut göndericisinin sahip olduğu global depolama alanında verilen türde bir nesnenin gerçekten var olup olmadığını kontrol eder. Bu kodda, `@0x2` herhangi bir nesneye sahip olmadığı için böyle bir nesneyi kaldıramayacağımızı iddia ediyoruz.

> 💡_`assert!`'in ikinci parametresi hata kodudur. Test dışı kodlarda, genellikle üretimde meydana gelebilecek her hata türü için özel hata kodu sabitlerinin bir listesini tanımlarız. Ancak birim testleri için bu genellikle gereksizdir çünkü çok fazla varlık olacaktır ve hata üzerine stacktrace hatanın nerede olduğunu söylemek için yeterlidir. Bu nedenle, assertion'lar için birim testlerinde sadece `0` koymanızı öneririz._

Son olarak `@0x1`'in nesneye sahip olduğunu ve nesne değerinin tutarlı olduğunu kontrol ederiz:

```
test_scenario::next_tx(scenario, owner);
{
    let object = test_scenario::take_from_sender<ColorObject>(scenario);
    let (red, green, blue) = color_object::get_color(&object);
    assert!(red == 255 && green == 0 && blue == 255, 0);
    test_scenario::return_to_sender(scenario, object);
};
test_scenario::end(scenario_val);
```

`test_scenario::take_from_sender`, geçerli işlem göndericisinin sahip olduğu verilen türdeki nesneyi global depodan kaldırır (ayrıca `has_most_recent_for_sender` öğesini de örtük olarak kontrol eder). Bu kod satırı başarılı olursa, `sahibinin` gerçekten `ColorObject` türünde bir nesneye sahip olduğu anlamına gelir. Ayrıca nesnenin alan değerlerinin oluşturma sırasında ayarladıklarımızla eşleşip eşleşmediğini de kontrol ederiz. Sonunda, `test_scenario::return_to_sender` öğesini çağırarak nesneyi global depoya geri döndürmeliyiz, böylece nesne global depoya geri döner. Bu aynı zamanda, test sırasında nesnede herhangi bir mutasyon meydana gelirse, küresel deponun değişikliklerden haberdar olmasını sağlar.

Yine, kodun tamamını [color\_object.move](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/objects\_tutorial/sources/color\_object.move) dosyasında bulabilirsiniz.

Testi çalıştırmak için kod kök dizininde aşağıdakileri çalıştırmanız yeterlidir:

```
sui move test
```

#### Zincir İçi Etkileşimler <a href="#on-chain-interactions" id="on-chain-interactions"></a>

Şimdi gerçek işlemlerde `create`'i çağırmayı deneyelim ve ne olacağını görelim. Bunu yapmak için Sui'yi ve CLI istemcisini başlatmamız gerekiyor. Sui ağını başlatmak ve istemciyi kurmak için [Sui CLI istemci kılavuzunu](https://docs.sui.io/devnet/build/cli-client) takip edin.

Başlamadan önce, varsayılan istemci adresine bir göz atalım (bu, daha sonra nesneye sahip olacak adrestir):

```
$ sui client active-address
```

Bu size mevcut müşteri adresini söyleyecektir.

İlk olarak, kodu zincir üzerinde yayınlamamız gerekir. Sui kaynak kodunu içeren deponun kök dizinine giden yolun $ROOT olduğunu varsayalım:

```
$ sui client publish --path $ROOT/sui_programmability/examples/objects_tutorial --gas-budget 10000
```

Yayınlanan paket nesne ID'sini **Yayınlama Sonuçları** çıktısında bulabilirsiniz:

```
----- Publish Results ----
The newly published package object: (0x57258f32746fd1443f2a077c0c6ec03282087c19, SequenceNumber(1), o#b3a8e284dea7482891768e166e4cd16f9749e0fa90eeb0834189016c42327401)
```

Göreceğiniz tam verilerin farklı olacağını unutmayın. Bu üçlüdeki ilk onaltılık dize paket nesne kimliğidir (bu durumda `0x57258f32746fd1443f2a077c0c6ec03282087c19`). Kolaylık sağlamak için bunu bir ortam değişkenine kaydedelim:

```
$ export PACKAGE=0x57258f32746fd1443f2a077c0c6ec03282087c19
```

Daha sonra bir renk nesnesi oluşturmak için fonksiyonu çağırabiliriz:

```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "create" --args 0 255 0
```

Çıktının **İşlem Etkileri** bölümünde, **Oluşturulan Nesneler** listesinde aşağıdaki gibi bir nesnenin gösterildiğini göreceksiniz:

```
Created Objects:
0x5eb2c3e55693282faa7f5b07ce1c4803e6fdc1bb SequenceNumber(1) o#691b417670979c6c192bdfd643630a125961c71c841a6c7d973cf9429c792efa
```

Yine kolaylık olması için nesne kimliğini kaydedelim:

```
$ export OBJECT=0x5eb2c3e55693282faa7f5b07ce1c4803e6fdc1bb
```

Bu nesneyi inceleyebilir ve ne tür bir nesne olduğunu görebiliriz:

```
$ sui client object $OBJECT
```

Bu size nesnenin meta verilerini türüyle birlikte gösterecektir:

```
Owner: AddressOwner(k#5db53ebb05fd3ea5f1d163d9d487ee8cd7b591ee)
Version: 1
ID: 0x5eb2c3e55693282faa7f5b07ce1c4803e6fdc1bb
Readonly: false
Type: 0x57258f32746fd1443f2a077c0c6ec03282087c19::color_object::ColorObject
```

Gördüğümüz gibi, daha önce gördüğümüz mevcut varsayılan istemci adresine ait. Ve bu nesnenin türü `ColorObject`!

Ayrıca `--json` parametresini ekleyerek nesnenin veri içeriğine de bakabilirsiniz:

```
$ sui client object $OBJECT --json
```

Bu, Move nesnesindeki `red`, `green` ve `blue` değerleri gibi tüm alanların değerlerini yazdıracaktır.

Tebrikler! Nesnelerin nasıl tanımlanacağını, oluşturulacağını ve aktarılacağını öğrendiniz. Ayrıca işlemleri taklit etmek ve nesnelerle etkileşim kurmak için nasıl birim testleri yazacağınızı da biliyor olmalısınız. Bir sonraki bölümde, sahip olduğumuz nesneleri nasıl kullanacağımızı öğreneceğiz.
