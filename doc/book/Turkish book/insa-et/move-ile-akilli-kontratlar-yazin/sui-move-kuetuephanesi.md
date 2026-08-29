# Sui Move Kütüphanesi

Sui, Sui'deki nesneleri manipüle etmemizi sağlayan bir Move kütüphane fonksiyonları listesi sağlar.

### Nesne sahipliği <a href="#object-ownership" id="object-ownership"></a>

Sui'deki nesneler farklı sahiplik türlerine sahip olabilir. Spesifik olarak, bunlar:

* Sadece bir adrese aittir.
* Sadece başka bir nesneye aittir.
* Değişmez.
* Ortak.

#### Bir adrese ait <a href="#owned-by-an-address" id="owned-by-an-address"></a>

``[`Transfer`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/transfer.move) modülü, nesnelerin sahipliğini değiştirmek için gereken tüm API'leri sağlar.

En yaygın durum, bir nesneyi bir adrese aktarmaktır. Örneğin, yeni bir nesne oluşturulduğunda, genellikle bir adrese aktarılır, böylece adres nesneye sahip olur. Bir nesne `obj`'yi bir adres `recipient`'ine aktarmak için:

```
use sui::transfer;

transfer::transfer(obj, recipient);
```

Bu çağrı nesneyi tamamen tüketir ve mevcut işlemde artık erişilemez hale getirir. Bir adres bir nesneye sahip olduğunda, bu nesnenin gelecekteki herhangi bir kullanımı (okuma veya yazma) için, işlemi imzalayan kişi nesnenin sahibi olmalıdır.

#### Başka bir nesneye ait <a href="#owned-by-another-object" id="owned-by-another-object"></a>

Bir nesne, başka bir nesnenin [dinamik nesne alanı](https://docs.sui.io/devnet/build/programming-with-objects/ch5-dynamic-fields) olarak eklendiğinde başka bir nesneye ait olabilir. Harici araçlar dinamik nesne alanı değerini orijinal ID'sinden okuyabilirken, Move'un bakış açısından, `dynamic_object_field` API'leri kullanılarak yalnızca sahibindeki alan aracılığıyla erişilebilir:

```
use sui::dynamic_object_field as ofield;

let a: &mut A = /* ... */;
let b: B = /* ... */;

// Adds `b` as a dynamic object field to `a` with "name" `0: u8`.
ofield::add<u8, B>(&mut a.id, 0, b);

// Get access to `b` at its new position
let b: &B = ofield::borrow<u8, B>(&a.id, 0);
```

Bir dinamik nesne alanının değeri bir işlemdeki giriş işlevine girdi olarak aktarılırsa, bu işlem başarısız olur. Örneğin, bir sahiplik zincirimiz varsa: `Addr1` adresi `a` nesnesine sahipse, `a` nesnesi `b` içeren bir dinamik nesne alanına sahipse ve `b` de `c` içeren bir dinamik nesne alanına sahipse, `c` nesnesini bir Move çağrısında kullanmak için, işlem `Addr1` tarafından imzalanmalı ve `a`'yı bir girdi olarak kabul etmeli ve işlem yürütme sırasında `b` ve `c`'ye dinamik olarak erişilmelidir:

```
use sui::dynamic_object_field as ofield;

// signed of ctx is Addr1
public entry fun entry_function(a: &A, ctx: &mut TxContext) {
  let b: &B = ofield::borrow<u8, B>(&a.id, 0);
  let c: &C = ofield::borrow<u8, C>(&b.id, 0);
}
```

Nesnelerin nasıl aktarılabileceği ve sahiplenilebileceğine ilişkin daha fazla örnek [object\_owner.move](https://github.com/MystenLabs/sui/blob/main/crates/sui-core/src/unit\_tests/data/object\_owner/sources/object\_owner.move) dosyasında bulunabilir.

#### Değiştirilemez <a href="#immutable" id="immutable"></a>

Bir `obj`'i değişmez yapmak için kişi şu çağrıda bulunabilir:

```
transfer::freeze_object(obj);
```

Bu çağrıdan sonra, `obj` değişmez hale gelir, yani asla değiştirilemez veya silinemez. Bu işlem aynı zamanda geri döndürülemez: bir nesne bir kez dondurulduğunda, sonsuza kadar dondurulmuş olarak kalacaktır. Değişmez bir nesne herhangi biri tarafından Move çağrısında referans olarak kullanılabilir.

#### Ortak <a href="#shared" id="shared"></a>

Bir `obj` nesnesini ortak hale getirmek için kişi şu çağrıda bulunabilir:

```
transfer::share_object(obj);
```

Bu çağrıdan sonra, `obj` değiştirilebilir olarak kalır, ancak herkes tarafından paylaşılır hale gelir, yani herkes bu nesneyi değiştirmek için bir işlem gönderebilir. Ancak, böyle bir nesne aktarılamaz veya başka bir nesneye alan olarak gömülemez. Daha fazla ayrıntı için [ortak nesneler](https://docs.sui.io/devnet/learn/objects#shared) belgesine bakın.

### İşlem içeriği <a href="#transaction-context" id="transaction-context"></a>

`TxContext` modülü, geçerli işlem bağlamına dayalı olarak çalışan birkaç önemli API sağlar.

Yeni bir nesneye yeni bir kimlik oluşturmak için:

```
// assmue `ctx` has type `&mut TxContext`.
let info = sui::object::new(ctx);
```

Geçerli işlem göndericisinin adresini almak için:

```
sui::tx_context::sender(ctx)
```

### Sıradaki Adımlar <a href="#next-steps" id="next-steps"></a>

Artık Move diline ve Move kodunun nasıl geliştirilip test edileceğine aşina olduğunuza göre, bazı büyük Move program örneklerine bakmaya ve bunlarla oynamaya hazırsınız. Örnekler arasında tic-tac-toe oyununun uygulanması ve bu eğitim sırasında geliştirmekte olduğumuza benzer bir fantezi oyununun daha gelişmiş bir çeşidi yer almaktadır.
