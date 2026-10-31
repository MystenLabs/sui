# Koleksiyonlar

Son bölümde, mevcut nesneleri dinamik alanlarla genişletmenin bir yolu tanıtıldı, ancak hala (potansiyel olarak `drop`'lanmayan) dinamik alanlara sahip bir nesneyi silmenin mümkün olduğu belirtildi. Bu, bir nesneye az sayıda statik olarak bilinen ek alan eklerken bir sorun olmayabilir, ancak dinamik alanlar olarak sınırsız sayıda anahtar-değer çifti tutabilen zincir üzerindeki koleksiyon türleri için özellikle istenmeyen bir durumdur.

Bu bölüm, dinamik alanlar kullanılarak oluşturulmuş, ancak içerdikleri girdilerin sayısını saymak ve boş olmadığında yanlışlıkla silinmeye karşı korumak için ek desteğe sahip bu tür iki koleksiyonu ( `Table` ve `Bag` ) kapsamaktadır.

Aşağıda tartışılan tipler ve fonksiyonlar, `table` ve `bag` modüllerinde Sui çerçevesine yerleştirilmiştir. Dinamik alanlarda olduğu gibi, her ikisinin de `object_` varyantı vardır: `object_table` içinde `ObjectTable` ve `object_bag` içinde `ObjectBag`. Table ile `ObjectTable` ve `Bag` ile `ObjectBag` arasındaki ilişki, bir alan ile bir nesne alanı arasındaki ilişkiyle aynıdır: İlki herhangi bir `store` türünü değer olarak tutabilir, ancak değer olarak depolanan nesneler harici depolamadan görüntülendiğinde gizlenecektir. İkincisi nesneleri yalnızca değer olarak saklayabilir, ancak bu nesneleri harici depolama alanındaki kimliklerinde görünür tutar.

#### Mevcut Sınırlamalar <a href="#current-limitations" id="current-limitations"></a>

Koleksiyonlar dinamik alanların üzerine inşa edilmiştir ve bu nedenle sınırlamalarına tabidir. Ek olarak, aşağıdaki işlevsellik planlanmaktadır, ancak şu anda desteklenmemektedir:

* `sui::bag::contains<K: copy + drop + store>(bag: &Bag, k: K): bool` bool, anahtar-değer çiftinin `bag` içinde anahtar `k: K` ve herhangi bir türde bir değer (benzer bir kontrol gerçekleştiren `contain_with_type`'a ek olarak, ancak belirli bir değer türünün geçilmesini gerektirir).

#### Table'lar <a href="#tables" id="tables"></a>

```
module sui::table {

struct Table<K: copy + drop + store, V: store> has key, store { /* ... */ }

public fun new<K: copy + drop + store, V: store>(
    ctx: &mut TxContext,
): Table<K, V>;

}
```

`Table<K, V>` homojen bir haritadır, yani tüm anahtarları birbiriyle aynı tiptedir (`K`) ve tüm değerleri de birbiriyle aynı tiptedir (`V`). Bir `&mut TxContext'e` erişim gerektiren `sui::table::new` ile oluşturulur, çünkü `Table`'lar, diğer nesneler gibi aktarılabilen, paylaşılabilen, sarılabilen veya açılabilen nesnelerdir.

> 💡`Table`'ın nesne koruyan sürümü için `sui::bag::ObjectTable` sayfasına bakın.

#### Bag'ler <a href="#bags" id="bags"></a>

```
module sui::bag {

struct Bag has key, store { /* ... */ }

public fun new(ctx: &mut TxContext): Bag;

}
```

`Bag` heterojen bir haritadır, bu nedenle rastgele türlerdeki anahtar-değer çiftlerini tutabilir (birbirleriyle eşleşmeleri gerekmez). Bu nedenle `Bag` türünün herhangi bir tür parametresine sahip olmadığını unutmayın. `Table` gibi, `Bag` de bir nesnedir, bu nedenle `sui::bag::new` ile bir nesne oluşturmak, bir kimlik oluşturmak için bir `&mut TxContext` sağlamayı gerektirir.

> 💡`Bag`'in nesne koruyan sürümü için `sui::bag::ObjectBag`'e bakın.

***

Aşağıdaki bölümler koleksiyon API'lerini açıklamaktadır. `sui::table` kod örnekleri için temel olarak kullanılacak ve diğer modüllerin farklı olduğu yerlerde açıklamalar yapılacaktır.

**Koleksiyonlarla Etkileşim**

Tüm koleksiyon türleri, ilgili modüllerinde tanımlanan aşağıdaki işlevlerle birlikte gelir:

```
module sui::table {

public fun add<K: copy + drop + store, V: store>(
    table: &mut Table<K, V>,
    k: K,
    v: V,
);

public fun borrow<K: copy + drop + store, V: store>(
    table: &Table<K, V>,
    k: K
): &V;

public fun borrow_mut<K: copy + drop + store, V: store>(
    table: &mut Table<K, V>,
    k: K
): &mut V;

public fun remove<K: copy + drop + store, V: store>(
    table: &mut Table<K, V>,
    k: K,
): V;

}
```

Bu fonksiyonlar sırasıyla koleksiyona girdi ekler, okur, yazar ve koleksiyondan girdi çıkarır ve hepsi değer olarak anahtar kabul eder. `Table`, `K` ve `V` için tip parametrelerine sahiptir, bu nedenle bu fonksiyonları aynı `Table` örneği üzerinde `K` ve `V`'nin farklı örnekleriyle çağırmak mümkün değildir, ancak `Bag` bu tip parametrelerine sahip değildir ve bu nedenle aynı örnek üzerinde farklı örneklerle çağrılara izin verir.

> ⚠️Dinamik alanlarda olduğu gibi, mevcut bir anahtarın üzerine yazmaya veya mevcut olmayan bir anahtara erişmeye veya kaldırmaya çalışmak bir hatadır.

> ⚠️`Bag`'in heterojenliğinin ekstra esnekliği, tür sisteminin bir türle değer ekleme ve ardından başka bir türde ödünç alma veya kaldırma girişimlerini statik olarak engellemeyeceği anlamına gelir. Bu model, dinamik alanların davranışına benzer şekilde çalışma zamanında bir iptal ile başarısız olacaktır.

#### Sorgulama Uzunluğu <a href="#querying-length" id="querying-length"></a>

Aşağıdaki fonksiyon ailesini kullanarak tüm koleksiyon türlerinin uzunluklarını sorgulamak ve boş olup olmadıklarını kontrol etmek mümkündür:

```
module sui::table {

public fun length<K: copy + drop + store, V: store>(
    table: &Table<K, V>,
): u64;

public fun is_empty<K: copy + drop + store, V: store>(
    table: &Table<K, V>
): bool;

}
```

> 💡`Bag` bu API'lere sahiptir, ancak `Bag` bu tür parametrelerine sahip olmadığı için `K` ve `V` üzerinde genel değildir.

#### Kapsama için Sorgulama <a href="#querying-for-containment" id="querying-for-containment"></a>

Tüm koleksiyonlar anahtar içerme açısından sorgulanabilir:

```
module sui::table {

public fun contains<K: copy + drop + store, V: store>(
    table: &Table<K, V>
    k: K
): bool;

}
```

`Bag` için eşdeğer fonksiyon şöyledir,

```
module sui::bag {

public fun contains_with_type<K: copy + drop + store, V: store>(
    bag: &Bag,
    k: K
): bool;

}
```

`bag` 'in `k: K` ve `V` türünde bir değer içeren bir anahtar-değer çifti içerip içermediğini test eder.

#### Temizlik <a href="#clean-up" id="clean-up"></a>

Giriş bölümünde belirtildiği gibi, koleksiyon türleri boş olmadıklarında yanlışlıkla silinmeye karşı koruma sağlar. Bu koruma, `drop`'a sahip olmadıkları gerçeğinden gelir, bu nedenle bu API kullanılarak açıkça silinmeleri gerekir:

```
module sui::table {

public fun destroy_empty<K: copy + drop + store, V: store>(
    table: Table<K, V>,
);

}
```

Bu fonksiyon koleksiyonu değer olarak alır. Hiç girdi içermiyorsa silinir, aksi takdirde çağrı iptal edilir. `sui::table::Table` ayrıca bir kolaylık fonksiyonuna sahiptir,

```
module sui::table {

public fun drop<K: copy + drop + store, V: drop + store>(
    table: Table<K, V>,
);

}
```

yalnızca değer türünün de drop olduğu tablolar için çağrılabilir, bu da tabloların boş olup olmadıklarına bakılmaksızın silinmesine olanak tanır.

> 💡_`Drop`'un kapsam dışına çıkmadan önce uygun tablolarda örtük olarak çağrılmayacağını unutmayın. Açıkça çağrılması gerekir, ancak çalışma zamanında başarılı olması garanti edilir._

> _💡`Bag` ve `ObjectBag` drop özelliğini destekleyemez çünkü bazıları `drop` özelliğine sahip olan bazıları da olmayan çeşitli türleri tutuyor olabilirler._

> _💡`ObjectTable` drop'u desteklemez çünkü değerleri drop edilemeyen nesneler olmalıdır (çünkü bir `id: UID` alanı içermelidirler ve `UID`'de drop yoktur)._

#### ⚠️Eşitlik <a href="#equality" id="equality"></a>

Koleksiyonlar üzerindeki eşitlik özdeşliğe dayanır, yani bir koleksiyon türünün bir örneği, aynı girdileri tutan tüm koleksiyonlara değil, yalnızca kendisine eşit kabul edilir:

```
let t1 = sui::table::new<u64, u64>(ctx);
let t2 = sui::table::new<u64, u64>(ctx);

assert!(&t1 == &t1, 0);
assert!(&t1 != &t2, 1);
```

İstediğiniz eşitlik tanımının bu olması pek olası değildir, kullanmayın!
