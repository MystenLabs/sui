# Objeleri Kullanma

[Bölüm 1](https://docs.sui.io/devnet/build/programming-with-objects/ch1-object-basics)'de Move'da bir Sui nesnesinin nasıl tanımlanacağını, oluşturulacağını ve sahipliğinin nasıl alınacağını ele aldık. Bu bölümde Move çağrılarında sahip olduğunuz nesneleri nasıl kullanacağınıza bakacağız.

Sui kimlik doğrulama mekanizmaları, Move çağrılarında yalnızca sizin sahip olduğunuz nesneleri kullanabilmenizi sağlar. (Sahip olunmayan nesneleri gelecek bölümlerde ele alacağız.) Move çağrılarında bir nesne kullanmak için, onları bir [entry fonksiyonuna](https://docs.sui.io/devnet/build/move#entry-functions) parametre olarak geçirin. Rust'a benzer şekilde, parametreleri geçirmenin birkaç yolu vardır:

#### Nesneleri referans olarak iletme <a href="#pass-objects-by-reference" id="pass-objects-by-reference"></a>

Nesneleri referansla aktarmanın iki yolu vardır: salt okunur referanslar (`&T`) ve değiştirilebilir referanslar (`&mut T`). Salt okunur referanslar nesneden veri okumanıza izin verirken, değiştirilebilir referanslar nesnedeki verileri değiştirmenize izin verir. `ColorObject`'in değerlerinden birini başka bir `ColorObject`'in değeriyle güncellememizi sağlayacak bir fonksiyon eklemeye çalışalım. Bu, hem salt okunur referansları hem de değiştirilebilir referansları kullanarak alıştırma yapacaktır.

Önceki bölümde tanımladığımız `ColorObject` aşağıdaki gibi görünür:

```
struct ColorObject has key {
    id: UID,
    red: u8,
    green: u8,
    blue: u8,
}
```

Şimdi bu fonksiyonu ekleyelim:

```
/// Copies the values of `from_object` into `into_object`.
public entry fun copy_into(from_object: &ColorObject, into_object: &mut ColorObject) {
    into_object.red = from_object.red;
    into_object.green = from_object.green;
    into_object.blue = from_object.blue;
}
```

> 💡_Bu fonksiyonu, işlemlerden bir entry fonksiyonu olarak çağrılabilmesi için `giriş` değiştiricisiyle birlikte bildirdik._

Yukarıdaki fonksiyon imzasında, `from_object` salt okunur bir referans olabilir çünkü yalnızca alanlarını okumamız gerekir; tersine, `into_object` mutasyona uğratmamız gerektiğinden mutasyona uğratılabilir bir referans olmalıdır. Bir transaction'ın `copy_into` fonksiyonuna çağrı yapabilmesi için, transaction'ın göndericisinin hem `from_object` hem de `into_object`'in sahibi olması gerekir.

> 💡_`from_object` bu işlemde salt okunur bir referans olmasına rağmen, Sui depolama alanında hala değiştirilebilir bir nesnedir - aynı anda nesneyi değiştirmek için başka bir işlem gönderilebilir! Bunu önlemek için Sui, salt okunur bir referans olarak aktarılsa bile, işlem girdisi olarak kullanılan herhangi bir değiştirilebilir nesneyi kilitlemelidir. Buna ek olarak, yalnızca nesnenin sahibi nesneyi kilitleyen bir işlem gönderebilir._

Testlerde aynı türden birden fazla nesne ile nasıl etkileşim kurabileceğimizi görmek için bir birim testi yazalım. Önceki bölümde, önceki işlemler tarafından oluşturulan global depodan `T` türünde bir nesne alan `take_from_sender` API'sini tanıttık. Ancak, aynı türde birden fazla nesne varsa ne olur? `take_from_sender` artık hangisini döndüreceğini söyleyemeyecektir. Bu sorunu çözmek için iki yeni, yalnızca test amaçlı API kullanmamız gerekiyor. Birincisi, en son oluşturulan nesnenin kimliğini döndüren `tx_context::last_created_object_id(ctx)`. İkincisi, belirli bir nesne kimliğine sahip `T` türünde bir nesne döndüren `test_scenario::take_from_sender_by_id`'dir. Şimdi teste (`test_copy_into`) bir göz atalım:

```
let owner = @0x1;
let scenario_val = test_scenario::begin(owner);
let scenario = &mut scenario_val;
// Create two ColorObjects owned by `owner`, and obtain their IDs.
let (id1, id2) = {
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 255, 255, ctx);
    let id1 = object::id_from_address(tx_context::last_created_object_id(ctx));
    color_object::create(0, 0, 0, ctx);
    let id2 = object::id_from_address(tx_context::last_created_object_id(ctx));
    (id1, id2)
};
```

Yukarıdaki kod iki nesne oluşturdu. Her çağrıdan hemen sonra, yeni oluşturulan nesnenin kimliğini almak için `tx_context::last_created_object_id` öğesine bir çağrı yaptığımıza dikkat edin. Sonunda iki nesnenin kimliklerini yakalayan `id1` ve `id2`'ye sahibiz. Daha sonra her ikisini de alır ve `copy_into` fonksiyonunu test ederiz:

```
test_scenario::next_tx(scenario, owner);
{
    let obj1 = test_scenario::take_from_sender_by_id<ColorObject>(scenario, id1);
    let obj2 = test_scenario::take_from_sender_by_id<ColorObject>(scenario, id2);
    let (red, green, blue) = color_object::get_color(&obj1);
    assert!(red == 255 && green == 255 && blue == 255, 0);

    let ctx = test_scenario::ctx(scenario);
    color_object::copy_into(&obj2, &mut obj1);
    test_scenario::return_to_sender(scenario, obj1);
    test_scenario::return_to_sender(scenario, obj2);
};
```

Her iki nesneyi farklı ID'ler kullanarak almak için `take_from_sender_by_id` kullandık. Daha sonra `obj1`'in değerini `obj2`'ninkini kullanarak güncellemek için `copy_into` kullandık. Mutasyonun çalıştığını doğrulayabiliriz:

```
test_scenario::next_tx(scenario, owner);
{
    let obj1 = test_scenario::take_from_sender_by_id<ColorObject>(scenario, id1);
    let (red, green, blue) = color_object::get_color(&obj1);
    assert!(red == 0 && green == 0 && blue == 0, 0);
    test_scenario::return_to_sender(scenario, obj1);
};
test_scenario::end(scenario_val);
```

#### Nesneleri değere göre geçirme <a href="#pass-objects-by-value" id="pass-objects-by-value"></a>

Nesneler bir giriş fonksiyonuna değer olarak da aktarılabilir. Bu şekilde, nesne Sui depolama alanının dışına taşınır. Daha sonra bu nesnenin nereye gideceğine karar vermek Move koduna kalmıştır.

> 📚Her [Sui nesnesi struct tipi](https://docs.sui.io/devnet/build/programming-with-objects/ch1-object-basics#define-sui-object) ilk alan olarak `UID`'yi içermesi gerektiğinden ve [UID struct](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/object.move)'ı bırakma özelliğine sahip olmadığından, Sui nesnesi struct tipi de `drop` özelliğine sahip [olamaz](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md#drop). Bu nedenle, herhangi bir Sui nesnesi keyfi olarak droplanamaz ve aşağıda açıklandığı gibi ya tüketilmeli (örneğin, başka bir sahibine aktarılmalı) ya da [paketten çıkarılarak](https://move-book.com/advanced-topics/struct.html#destructing-structures) silinmelidir.

Move'da bir pass-by-value Sui nesnesi ile başa çıkmanın iki yolu vardır:

**Seçenek 1: Nesneyi silin**

Eğer amaç nesneyi gerçekten silmekse, nesneyi paketinden çıkarabiliriz. Bu, Move'un [ayrıcalıklı struct işlemleri kuralları](https://github.com/move-language/move/blob/main/language/documentation/book/src/structs-and-resources.md#privileged-struct-operations) nedeniyle yalnızca struct türünü tanımlayan modülde yapılabilir. Paket açıldıktan sonra, herhangi bir alan da struct türündeyse, özyinelemeli paket açma ve silme gerekecektir.

Ancak, bir Sui nesnesinin id alanı özel işlem gerektirir. Sui'ye bu nesneyi silmek istediğimizi bildirmek için [nesne](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/object.move) modülünde aşağıdaki API'yi çağırmalıyız:

```
public fun delete(id: UID) { ... }
```

`ColorObject` modülünde nesneyi silmemizi sağlayan bir fonksiyon tanımlayalım:

```
    public entry fun delete(object: ColorObject) {
        let ColorObject { id, red: _, green: _, blue: _ } = object;
        object::delete(id);
    }
```

Gördüğümüz gibi, nesne tek tek alanlar oluşturarak paketten çıkarılır. u8 değerleri ilkel tiplerdir ve hepsi bırakılabilir. Ancak `id` (`UID` tipine sahiptir) bırakılamaz ve `object::delete` API'si aracılığıyla açıkça silinmelidir. Bu çağrının sonunda, nesne artık zincir üzerinde saklanmayacaktır.

Bunun için bir birim testi de ekleyebiliriz:

```
let owner = @0x1;
// Create a ColorObject and transfer it to @owner.
let scenario_val = test_scenario::begin(owner);
let scenario = &mut scenario_val;
{
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 0, 255, ctx);
};
// Delete the ColorObject we just created.
test_scenario::next_tx(scenario, owner);
{
    let object = test_scenario::take_from_sender<ColorObject>(scenario);
    color_object::delete(object);
};
// Verify that the object was indeed deleted.
test_scenario::next_tx(scenario, &owner);
{
    assert!(!test_scenario::has_most_recent_for_sender<ColorObject>(scenario), 0);
};
test_scenario::end(scenario_val);
```

İlk kısım [Bölüm 1](https://docs.sui.io/devnet/build/programming-with-objects/ch1-object-basics#writing-unit-tests)'de gördüğümüzle aynıdır, yeni bir `ColorObject` oluşturur ve sahibinin hesabına koyar. İkinci işlem ise test ettiğimiz şeydir: nesneyi depodan almak ve sonra silmek. Nesne silindiği için onu depoya geri döndürmeye gerek yoktur (aslında bu imkansızdır). Testin son kısmı, nesnenin gerçekten de artık global depoda olmadığını ve dolayısıyla oradan alınamayacağını kontrol eder.

**Seçenek 2. Nesneyi aktarın**

Nesnenin sahibi onu başka bir adrese aktarmak isteyebilir. Bunu desteklemek için `ColorObject` modülünün bir `transfer` API'si tanımlaması gerekecektir:

```
public entry fun transfer(object: ColorObject, recipient: address) {
    transfer::transfer(object, recipient)
}
```

> 💡Bir giriş fonksiyonu olmadığı için `transfer::transfer` doğrudan çağrılamaz.

Aktarım için de bir test ekleyelim. Öncelikle, sahibinin hesabında bir nesne oluşturuyoruz ve ardından farklı bir hesap `recipient`'ına aktarıyoruz:

```
let owner = @0x1;
// Create a ColorObject and transfer it to @owner.
let scenario_val = test_scenario::begin(owner);
let scenario = &mut scenario_val;
{
    let ctx = test_scenario::ctx(scenario);
    color_object::create(255, 0, 255, ctx);
};
// Transfer the object to recipient.
let recipient = @0x2;
test_scenario::next_tx(scenario, owner);
{
    let object = test_scenario::take_from_sender<ColorObject>(scenario);
    let ctx = test_scenario::ctx(scenario);
    transfer::transfer(object, recipient, ctx);
};
```

İkinci işlemde, işlemin göndericisinin hala `owner` olması gerektiğine dikkat edin, çünkü yalnızca `owner` olduğu nesneyi aktarabilir. Aktarımdan sonra, `owner`'ın artık nesneye sahip olmadığını, `recipient`'in ise artık nesneye sahip olduğunu doğrulayabiliriz:

```
// Check that owner no longer owns the object.
test_scenario::next_tx(scenario, owner);
{
    assert!(!test_scenario::has_most_recent_for_sender<ColorObject>(scenario), 0);
};
// Check that recipient now owns the object.
test_scenario::next_tx(scenario, recipient);
{
    assert!(test_scenario::has_most_recent_for_sender<ColorObject>(scenario), 0);
};
test_scenario::end(scenario_val);
```

**Zincir içi etkileşimler**

Şimdi bunu zincir üzerinde deneme zamanı. [Bölüm 1](https://docs.sui.io/devnet/build/programming-with-objects/ch1-object-basics#on-chain-interactions)'deki talimatları zaten takip ettiğinizi varsayarsak, paketi yayınlamış ve yeni bir nesne oluşturmuş olmalısınız. Şimdi bunu başka bir adrese aktarmayı deneyebiliriz. Öncelikle başka hangi adreslere sahip olduğunuzu görelim:

```
$ sui client addresses
```

Varsayılan geçerli adres ilk adres olduğundan, alıcı olarak listedeki ikinci adresi seçelim. Benim durumumda `0x1416f3d5af469905b0580b9af843ec82d02efd30` adresi var. Kolaylık olması için kaydedelim:

```
$ export RECIPIENT=0x1416f3d5af469905b0580b9af843ec82d02efd30
```

Şimdi nesneyi bu adrese aktaralım:

```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "transfer" --args \"$OBJECT\" \"$RECIPIENT\"
```

Şimdi `RECIPIENT`' in hangi nesnelere sahip olduğunu görelim:

```
$ sui client objects $RECIPIENT
```

Listedeki nesnelerden birinin yeni `ColorObject` olduğunu görebilmeliyiz! Bu, aktarımın başarılı olduğu anlamına gelir.

Şimdi de bu nesneyi silmeyi deneyelim:

```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "delete" --args \"$OBJECT\"
```

Oops. Hata verecek ve adresin nesneyi kilitleyemediğinden şikayet edecektir, bu geçerli bir hatadır çünkü nesneyi zaten orijinal sahibinden transfer ettik.

Bu nesne üzerinde işlem yapabilmek için istemci adresimizi `$RECIPIENT` olarak değiştirmemiz gerekir:

```
$ sui client switch --address $RECIPIENT
```

Ve tekrar silmeyi deneyin:

```
$ sui client call --gas-budget 1000 --package $PACKAGE --module "color_object" --function "delete" --args \"$OBJECT\"
```

Çıktıda, `İşlem Etkileri` bölümünde silinen nesnelerin bir listesini göreceksiniz. Bu, nesnenin başarıyla silindiğini gösterir. Eğer bunu tekrar çalıştırırsak:

```
$ sui client objects $RECIPIENT
```

Bu nesnenin artık adreste olmadığını göreceğiz.

Artık nesneleri referans ve değer ile nasıl aktaracağınızı ve zincir üzerinde nasıl aktaracağınızı biliyorsunuz.
