# Dinamik Alanlar

Önceki bölümlerde, ilkel verileri ve diğer nesneleri (sarmalama) depolamak için nesne alanlarını kullanmanın çeşitli yollarını inceledik, ancak bu yaklaşımın birkaç sınırlaması vardır:

1. Nesneler, modülü yayınlandığında sabitlenen (yani `struct` bildirimindeki alanlarla sınırlı olan) tanımlayıcılar tarafından anahtarlanan sonlu bir alan kümesine sahiptir.
2. Bir nesne diğer birkaç nesneyi sararsa çok büyük hale gelebilir. Daha büyük nesneler işlemlerde daha yüksek gaz ücretlerine yol açabilir. Buna ek olarak, nesne boyutunda bir üst sınır vardır.
3. Gelecek bölümlerde göreceğimiz gibi, heterojen tipteki nesnelerden oluşan bir koleksiyonu saklamamız gereken kullanım durumları olacaktır. Move vektör tipi tek bir `T` tipi ile örneklenmesi gerektiğinden, bunun için uygun değildir.

Neyse ki Sui, keyfi adlara sahip (sadece tanımlayıcılar değil), anında eklenen ve kaldırılan (yayın sırasında sabitlenmeyen), yalnızca erişildiklerinde gazı etkileyen ve heterojen değerleri depolayabilen _dinamik alanlar_ sağlar. Bu bölümde bu tür alanlarla etkileşim için kütüphaneler tanıtılmaktadır.

#### Mevcut Sınırlamalar <a href="#current-limitations" id="current-limitations"></a>

Dinamik alanların bu ilk sürümde henüz tasarlandığı gibi davranmayan bazı yönleri vardır. Bu alanlar üzerinde aktif olarak çalışıyoruz, ancak şunlara dikkat edin:

* Dinamik alan nesneleriyle ilgili olası dayanıklılık/tutarlılık sorunları: Bir validator dinamik alanlara sahip bir işlemi işlerken çöküp geri geldiğinde, bu nesneleri içeren başka işlemleri işleyemeyebilir.

#### Alanlar vs Nesne Alanları <a href="#fields-vs-object-fields" id="fields-vs-object-fields"></a>

Değerlerinin nasıl depolandığına bağlı olarak farklılık gösteren iki tür dinamik alan vardır - "alanlar" ve "nesne alanları":

* **Alanlar**, `store`'u olan herhangi bir değeri depolayabilir, ancak bu tür bir alanda depolanan bir nesne sarılmış olarak kabul edilecek ve depolamaya erişen harici araçlar (kaşifler, cüzdanlar vb.) tarafından kimliği aracılığıyla erişilemeyecektir.
* **Nesne alanı** değerleri nesne olmalıdır (`key` yeteneğine ve ilk alan olarak `id: UID`'ye sahip olmalıdır), ancak yine de harici araçlar için ID'lerinden erişilebilir olacaktır.

Bu alanlarla etkileşim için modüller sırasıyla [`dynamic_field`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/dynamic\_field.move) ve [`dynamic_object_field`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/dynamic\_object\_field.move) adreslerinde bulunabilir.

#### Alan Adları <a href="#field-names" id="field-names"></a>

Bir nesnenin adları Move tanımlayıcıları olması gereken normal alanlarının aksine, dinamik alan adları `copy`, `drop` ve `store` özelliklerine sahip herhangi bir değer olabilir. Bu, tüm Move ilkellerini (tamsayılar, booleanlar, bayt dizeleri) ve içeriklerinin tümü `copy`, `drop` ve `store` özelliğine sahip olan yapıları içerir.

**Dinamik Alanlar Ekleme**

Dinamik alanlar aşağıdaki API'ler ile eklenir:

```
module sui::dynamic_field {

public fun add<Name: copy + drop + store, Value: store>(
  object: &mut UID,
  name: Name,
  value: Value,
);

}
```

```
module sui::dynamic_object_field {

public fun add<Name: copy + drop + store, Value: key + store>(
  object: &mut UID,
  name: Name,
  value: Value,
);

}
```

Bu fonksiyonlar nesneye adı `name` ve değeri `value` olan bir alan ekler. Bunu çalışırken görmek için şu kod parçacıklarını düşünün:

İlk olarak ebeveyn ve çocuk için iki nesne türü tanımlıyoruz:

```
struct Parent has key {
    id: UID,
}

struct Child has key, store {
    id: UID,
    count: u64,
}
```

Şimdi, bir `Ebeveyn` nesnenin dinamik alanı olarak bir `Child` nesnesi eklemek için bir API tanımlayabiliriz:

```
use sui::dynamic_object_field as ofield;

public entry fun add_child(parent: &mut Parent, child: Child) {
    ofield::add(&mut parent.id, b"child", child);
}
```

Bu fonksiyon `Child` nesnesini değer olarak alır ve onu b "child" (`vector<u8` türünde bir bayt dizesi) adıyla `parent`'in dinamik bir alanı haline getirir. `add_child` çağrısının sonunda, aşağıdaki sahiplik ilişkisine sahip oluruz:

1. Gönderen adresi (hala) `Parent` nesnenin sahibidir.
2. `Parent` nesnesi `Child` nesnesinin sahibidir ve ona b "child" adıyla başvurabilir.&#x20;

> ⚠️Bir alanın üzerine yazmak (zaten tanımlanmış bir alanla aynı Ad türüne ve değere sahip bir alan eklemeye çalışmak) bir hatadır ve bunu yapan bir işlem iptal edilir. Alanlar değiştirilebilir şekilde ödünç alınarak yerinde değiştirilebilir ve önce eski değer kaldırılarak güvenli bir şekilde üzerine yazılabilir (örneğin değer türünü değiştirmek için) (ayrıntılar için aşağıya bakın).

#### Dinamik Alanlara Erişim <a href="#accessing-dynamic-fields" id="accessing-dynamic-fields"></a>

Dinamik alanlara aşağıdaki API'ler kullanılarak referans yoluyla erişilebilir:

```
module sui::dynamic_field {

public fun borrow<Name: copy + drop + store, Value: store>(
    object: &UID,
    name: Name,
): &Value;

public fun borrow_mut<Name: copy + drop + store, Value: store>(
    object: &mut UID,
    name: Name,
): &mut Value;

}
```

Burada `object`, alanın tanımlandığı nesnenin UID'si ve `name` de alanın adıdır.

> 💡`sui::dynamic_object_field` nesne alanları için eşdeğer işlevlere sahiptir, ancak `Value: key + store` kısıtlaması eklenmiştir.

Bu API'lerin daha önce tanımlanan `Parent` ve `Child` tipleri ile nasıl kullanılacağına bakalım:

```
use sui::dynamic_object_field as ofield;

public entry fun mutate_child(child: &mut Child) {
    child.count = child.count + 1;
}

public entry fun mutate_child_via_parent(parent: &mut Parent) {
    mutate_child(ofield::borrow_mut<vector<u8>, Child>(
        &mut parent.id,
        b"child",
    ));
}
```

İlk fonksiyon doğrudan `Child` nesnesine değiştirilebilir bir referans kabul eder ve `Parent` nesnelerine alan olarak eklenmemiş `Child` nesneleriyle çağrılabilir. Gövdesi boştur çünkü burada önem verdiğimiz şey nasıl mutasyona uğratıldığı değil, işlevin çağrılıp çağrılamayacağıdır.

İkinci fonksiyon `Parent` nesnesine mutasyona uğrayabilen bir referans kabul eder ve `mutate_child`'a aktarmak için `borrow_mut` kullanarak dinamik alanına erişir. Bu fonksiyon yalnızca `b "child"` alanı tanımlanmış `Parent` nesneleri üzerinde çağrılabilir. Bir `Parent` nesnesine eklenen bir `Child` nesnesine dinamik alanı aracılığıyla erişilmelidir, bu nedenle ID'si bilinse bile `mutate_child` değil `mutate_child_via_parent` kullanılarak mutasyona uğratılabilir.

> ⚠️Mevcut olmayan bir alanı ödünç almaya çalışan bir işlem iptal edilecektir.

> ⚠️`borrow` ve `borrow_mut` öğelerine aktarılan `Value` türü, depolanan alanın türüyle eşleşmelidir, aksi takdirde işlem iptal edilir.

> ⚠️Dinamik nesne alanı değerlerine bu API'ler aracılığıyla erişilmelidir. Bu nesneleri girdi olarak (değer veya referans olarak) kullanmaya çalışan bir işlem, geçersiz girdilere sahip olduğu için reddedilecektir.

#### Dinamik Alanı Kaldırma <a href="#removing-a-dynamic-field" id="removing-a-dynamic-field"></a>

Normal bir alanda tutulan bir nesneyi "açmaya" benzer şekilde, dinamik bir alan kaldırılarak değeri açığa çıkarılabilir:

```
module sui::dynamic_field {

public fun remove<Name: copy + drop + store, Value: store>(
    object: &mut UID,
    name: Name,
): Value;

}
```

Bu fonksiyon, alanın tanımlandığı `object`'in ID'sine ve alanın `name`'ine değişken bir referans alır. Eğer bir alan `value: Value` değerine sahip bir alan `name` adresindeki `Object`'de, tanımlanmışsa kaldırılır ve `value` değeri döndürülür, aksi takdirde iptal edilir. `Object` üzerinde bu alana erişmeye yönelik gelecekteki girişimler başarısız olur.

> 💡`sui::dynamic_object_field` nesne alanları için eşdeğer bir işleve sahiptir.

Döndürülen değerle tıpkı diğer değerler gibi etkileşime girilebilir (çünkü o herhangi bir değerdir). Örneğin, kaldırılan dinamik nesne alanı değerleri daha sonra `delete` edilebilir veya bir adrese `transfer` edilebilir (örneğin gönderene geri gönderilebilir):

```
use sui::dynamic_object_field as ofield;
use sui::{object, transfer, tx_context};
use sui::tx_context::TxContext;

public entry fun delete_child(parent: &mut Parent) {
    let Child { id, count: _ } = ofield::remove<vector<u8>, Child>(
        &mut parent.id,
        b"child",
    );

    object::delete(id);
}

public entry fun reclaim_child(parent: &mut Parent, ctx: &mut TxContext) {
    let child = ofield::remove<vector<u8>, Child>(
        &mut parent.id,
        b"child",
    );

    transfer::transfer(child, tx_context::sender(ctx));
}
```

> ⚠️Bir alanın ödünç alınmasında olduğu gibi, var olmayan bir alanı veya farklı bir `Value` türüne sahip bir alanı kaldırmaya çalışan bir işlem iptal edilir.

#### Dinamik Alanlara Sahip Bir Nesneyi Silme <a href="#deleting-an-object-with-dynamic-fields" id="deleting-an-object-with-dynamic-fields"></a>

Üzerinde hala tanımlı dinamik alanlar bulunan bir nesneyi silmek mümkündür. Alan değerlerine yalnızca dinamik alanın ilişkili nesnesi ve alan adı aracılığıyla erişilebildiğinden, üzerinde hala tanımlı dinamik alanlar bulunan bir nesnenin silinmesi, bunların tümünü gelecekteki işlemler için erişilemez hale getirir. Bu, alan değerinin `drop` özelliğine sahip olup olmadığına bakılmaksızın geçerlidir.

> ⚠️Üzerinde hala dinamik alanlar tanımlı olan bir nesnenin silinmesine izin verilir, ancak bu işlem tüm alanlarını erişilemez hale getirir.
