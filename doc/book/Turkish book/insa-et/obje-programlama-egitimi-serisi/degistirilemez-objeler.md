# Değiştirilemez Objeler

Bölüm 1 ve 2'de, bir adresin sahip olduğu nesneleri nasıl oluşturacağımızı ve kullanacağımızı öğrendik. Bu bölümde, değişmez nesnelerin nasıl oluşturulacağını ve kullanılacağını göstereceğiz.&#x20;

Sui'deki nesneler iki geniş kategoride farklı [ownership](https://docs.sui.io/devnet/learn/objects#object-ownership) türlerine sahip olabilir: değişmez nesneler ve değiştirilebilir nesneler. Değişmez bir nesne, asla değiştirilemeyen, aktarılamayan veya silinemeyen bir nesnedir. Bu değişmezlik nedeniyle, nesne kimseye ait değildir ve dolayısıyla herkes tarafından kullanılabilir.

**Değişmez nesne oluşturma**

Bir nesnenin yeni yaratılmış veya zaten bir adres tarafından sahiplenilmiş olmasına bakılmaksızın, bu nesneyi değişmez bir nesneye dönüştürmek için [transfer modülünde](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/transfer.move) aşağıdaki API'yi çağırmamız gerekir:

```
public native fun freeze_object<T: key>(obj: T);
```

Bu çağrıdan sonra, belirtilen nesne kalıcı olarak değişmez hale gelecektir. Bu, geri döndürülemez bir işlemdir; bu nedenle, bir nesneyi yalnızca hiçbir zaman değiştirilmesi gerekmeyeceğinden emin olduğunuzda dondurun.

Mevcut (sahip olunan) bir `ColorObject`'i değişmez bir nesneye dönüştürmek için [color\_object modülüne](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/objects\_tutorial/sources/color\_object.move) bir giriş işlevi ekleyelim:

```
public entry fun freeze_object(object: ColorObject) {
    transfer::freeze_object(object)
}
```

Yukarıdaki fonksiyonda, bir `ColorObject`'in aktarılabilmesi için zaten bir `ColorObject`'e sahip olunması gerekir. Bu çağrının sonunda, bu nesne dondurulur ve asla mutasyona uğratılamaz. Ayrıca artık kimse tarafından sahiplenilmez.

> 💡_`transfer::freeze_object` API'sinin nesneyi değer olarak geçirmeyi gerektirdiğine dikkat edin. Nesneyi değiştirilebilir bir referansla geçirmeye izin verseydik, `freeze_object` çağrısından sonra nesneyi değiştirmeye devam edebilirdik; bu da nesnenin değişmez hale gelmesi gerektiği gerçeğiyle çelişir._

Alternatif olarak, oluşma sırasında değişmez bir nesne oluşturan bir API de sağlayabilirsiniz:

```
public entry fun create_immutable(red: u8, green: u8, blue: u8, ctx: &mut TxContext) {
    let color_object = new(red, green, blue, ctx);
    transfer::freeze_object(color_object)
}
```

Bu işlevde, yeni bir ColorObject oluşturulur ve herhangi biri tarafından sahiplenilmeden önce hemen değişmez bir nesneye dönüştürülür.

**Değişmez nesne kullanma**

Bir nesne değişmez hale geldiğinde, bu nesneyi Move çağrılarında kimlerin kullanabileceğine ilişkin kurallar değişir:

1. Değişmez bir nesne, Move giriş işlevlerine yalnızca \&T olarak salt okunur, değişmez bir referans olarak aktarılabilir.
2. Değişmez nesneleri herkes kullanabilir.

Bir nesnenin değerini diğerine kopyalayan bir fonksiyon tanımladığımızı hatırlayın:

```
public entry fun copy_into(from_object: &ColorObject, into_object: &mut ColorObject);
```

Bu fonksiyonda, herkes `from_object` ilk argümanı olarak değişmez bir nesne iletebilir, ancak ikinci argümanı iletemez.

Değişmez nesneler asla mutasyona uğratılamayacağından, aynı anda birden fazla işlem aynı değişmez nesneyi kullansa bile asla bir veri yarışı olmayacaktır. Dolayısıyla, değişmez nesnelerin varlığı mutabakat için herhangi bir gereklilik oluşturmaz.

**Değişmez nesneyi test edin**

Birim testlerinde değişmez nesnelerle nasıl etkileşim kurduğumuza bir göz atalım. Daha önce, `test_scenario::take_from_sender` API'sini bir birim testinde işlemin göndericisinin sahip olduğu global depolamadan bir nesne almak için kullanmıştık. Ve `take_from_sender` bir nesneyi değer olarak döndürür, bu da onu değiştirmenize, silmenize veya aktarmanıza olanak tanır.

Değişmez bir nesne almak için yeni bir API kullanmamız gerekecek: `test_scenario::take_immutable`. Bu gereklidir çünkü immutable nesnelere yalnızca salt okunur referanslar aracılığıyla erişilebilir. `test_scenario` çalışma zamanı bu immutable nesnenin kullanımını takip edecektir. Nesne bir sonraki işlem başlamadan önce `test_scenario::return_immutable` aracılığıyla döndürülmezse, test iptal edilir.

Şimdi bunun nasıl çalıştığını görelim (`ColorObjectTests::test_immutable`):

```
let sender1 = @0x1;
let scenario_val = test_scenario::begin(sender1);
let scenario = &mut scenario_val;
{
    let ctx = test_scenario::ctx(scenario);
    color_object::create_immutable(255, 0, 255, ctx);
};
test_scenario::next_tx(scenario, sender1);
{
    // take_owned does not work for immutable objects.
    assert!(!test_scenario::has_most_recent_for_sender<ColorObject>(scenario), 0);
};
```

Bu testte, değişmez bir nesne oluşturacak bir işlemi `sender1` olarak gönderiyoruz. Yukarıda gördüğümüz gibi, `can_take_owned` artık `true` döndürmeyecektir, çünkü nesne artık sahipli değildir. Bu nesneyi almak için şunları yapmamız gerekir:

```
// Any sender can work.
let sender2 = @0x2;
test_scenario::next_tx(scenario, sender2);
{
    let object = test_scenario::take_immutable<ColorObject>(scenario);
    let (red, green, blue) = color_object::get_color(object);
    assert!(red == 255 && green == 0 && blue == 255, 0);
    test_scenario::return_immutable(object);
};
```

Bu nesnenin gerçekten kimseye ait olmadığını göstermek için, bir sonraki işlemi `sender2` ile başlatıyoruz. Daha önce açıklandığı gibi, `take_immutable` kullandık ve başarılı oldu! Bu, herhangi bir göndericinin değişmez bir nesne alabileceği anlamına gelir. Sonunda, nesneyi geri döndürmek için yeni bir API çağırmamız gerekiyor: `return_immutable`.

Bu nesnenin gerçekten değişmez olup olmadığını incelemek için, bir `ColorObject`'i değiştirecek bir işlev sunalım ([zincir içi etkileşimleri](https://docs.sui.io/devnet/build/programming-with-objects/ch3-immutable-objects#on-chain-interactions) açıklarken bu işlevi kullanacağız):

```
public entry fun update(
    object: &mut ColorObject,
    red: u8, green: u8, blue: u8,
) {
    object.red = red;
    object.green = green;
    object.blue = blue;
}
```

Özetlemek gerekirse, birim testlerinde değişmez nesnelerle etkileşim kurmak için iki yeni API işlevi sunduk:

* `test_scenario::take_immutable` global depolama alanından değişmez bir nesne wrapper'ı almak için.
* `test_scenario::return_immutable` wrapper global depoya geri döndürmek için.

#### Zincir içi etkileşimler <a href="#on-chain-interactions" id="on-chain-interactions"></a>

Öncelikle, sahip olduğunuz nesnelerin mevcut listesine bir göz atın:

```
$ export ADDR=`sui client active-address`
$ sui client objects $ADDR
```

`ColorObject` kodunu Sui CLI istemcisini kullanarak zincir üzerinde yayınlayalım:

```
$ sui client publish --path $ROOT/sui_programmability/examples/objects_tutorial --gas-budget 10000
```

Önceki bölümlerde yaptığımız gibi paket nesne kimliğini `$PACKAGE` ortam değişkenine ayarlayın.

Ardından yeni bir `ColorObject` oluşturun:

```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "create" --args 0 255 0
```

Yeni oluşturulan nesne kimliğini `$OBJECT` olarak ayarlayın. Geçerli aktif adresteki nesnelerin listesine bakarsak:

```
$ sui client objects $ADDR
```

ID `$OBJECT` ile bir tane daha olmalıdır. Bunu değişmez bir nesneye dönüştürelim:

```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "freeze_object" --args \"$OBJECT\"
```

Şimdi sahip olduğumuz nesnelerin listesine tekrar bakalım:

```
$ sui client objects $ADDR
```

`$OBJECT` artık orada değil. Artık kimse tarafından sahiplenilmiyor. Nesne bilgilerini sorgulayarak artık değişmez olduğunu görebilirsiniz:

```
$ sui client object $OBJECT
Owner: Immutable
...
```

Eğer onu değiştirmeye çalışırsak:

```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "update" --args \"$OBJECT\" 0 0 0
```

Değişmez bir nesnenin değişebilir bir argümana aktarılamayacağından şikayet eder.
