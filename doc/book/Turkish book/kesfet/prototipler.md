# Prototipler

Karşınızda etkileyici NFT'lerin mümkün kıldığı hızı, ölçeklenebilirliği ve zengin etkileşimleri gösteren, değişken iki kısa oyun alfa öncesi prototipi.

### Sui ve Gaming <a href="#sui-and-gaming" id="sui-and-gaming"></a>

Web3'ün hızla benimsendiği ilk sektörlerden biri olan oyun, popüler bir tartışma konusudur. Ancak, mevcut web3 oyunları, oyundan çok yatırım olarak görülmekte ve kullanıcıların elde tutulması oyunların kendisinden ziyade piyasa koşullarından etkilenmektedir.

Peki mevcut web3 oyunlarında eksik olan nedir? Öncelikle, başarılı bir web3 oyunu web1 veya web2 oyunlarından tamamen farklı bir deneyim sunmalıdır. Gerçekten parlamak için, web3 oyunları, anlamlı bir şekilde, doğrulanabilir sahiplik ile tamamen zincir üzerinde, dinamik ve birleştirilebilir dijital varlıkların avantajlarından yararlanmalıdır. Bu özellikler inanılmaz ve yaratıcı oyun ve ekosistemleri güçlendirerek muazzam bir değer ve etkileşim yaratabilir.

İkinci olarak, harika oyunlar, oyun kurmayı ve eğlenceli, kullanıcı merkezli deneyimler yaratmayı bilen deneyimli oyun geliştiricileri ve kurucuları gerektirir. Web3'te oyun geliştirmeye hevesli çok sayıda yetenek var, ancak yaratıcılıkları platform sınırlamaları ve yeni bir programlama dili öğrenmenin sancıları nedeniyle engelleniyor.

Sui ile, oyun geliştiricilerin platform performansı veya ücretleriyle sınırlandırılmaması ve hayal ettikleri her türlü deneyimi yaratabilmeleri gerektiğine inanıyoruz. Daha da önemlisi, harika oyunlar geliştirmek için oyun geliştiricilerin akıllı kontratlar yazma konusunda da uzman olmaları gerekmemelidir. Bunun yerine, iyi oldukları alana, yani oyuncular için harika oyunlar geliştirmeye odaklanmalılar.

**İsteğe bağlı akıllı kontratlar**

[Move](https://github.com/MystenLabs/awesome-move/blob/main/README.md) tek kelimeyle harika: güvenli, etkileyici ve reentrancy'den muaf; ancak Sui üzerinde anlamlı deneyimler oluşturmak için Move uzmanlığı gerekli değil. Geliştiricilerin ve içerik oluşturucuların Sui'yi oyun için kullanmaya başlamalarını kolaylaştırmak için, yaygın kullanım durumlarını ve oyun varlıklarıyla ilgili özellikleri ele alan oyun SDK'larını yayınlayacağız.

**Bunu nasıl yaptık**

Oyun geliştirme stüdyosu GenITeam tarafından oluşturulan bu prototipler hem Unity SDK hem de Sui [API'lerini](https://docs.sui.io/sui-jsonrpc) kullanıyor.

GenITeam'in bu işbirliği üzerinde çalışan geliştiricileri ne akıllı kontrat ne de Move geliştiricisidir. Onların girdilerine dayanarak bir veri modeli oluşturduk ve basit API'leri paylaştık. Bu API'ler sayesinde GenITeam, değiştirilebilir, zincir üzerindeki diğer varlıklara sahip olan ve diğer uygulamalara serbestçe aktarılabilen tamamen zincir üzerinde NFT'ler oluşturabildi.

Bu kavram kanıtı yapısı, Sui aracılığıyla oyun geliştiricileri için açılan yetenekleri göstermeyi amaçlamaktadır. Önümüzdeki aylarda ek yetenekleri açıklarken oyun topluluğundaki yaratıcı beyinlerin neler ortaya çıkardığını görmeyi dört gözle bekliyoruz. Düzeltilen her hata ile oyun geliştiricilerin bir SDK'da ne aradıklarına dair bilgiler edindik. Sui, farklı derecelerde akıllı kontratlar uzmanlığına sahip tüm geliştirici seviyeleri için erişilebilir SDK'lar oluşturmaya kararlıdır.

İşte paylaştığımız API örnekleri ve canavar (prototipte Monstars olarak adlandırılmıştır) oluşturmak ve güncellemek için hazırlanan akıllı kontratlar :

#### API Move call - Canavar Yarat <a href="#api-move-call---create-monster" id="api-move-call---create-monster"></a>

POST `/call` with body:

```
    {
       "sender": "{{owner}}",
       "packageObjectId": "0x2",
       "module": "geniteam",
       "function": "create_monster",
       "args": [
           "0x{{player_id}}",
           "0x{{farm_id}}",
           "0x{{pet_monsters}}",
           {{monster_name}},
           {{monster_img_index}},
           {{breed}},
           {{monster_affinity}},
           {{monster_description}}
       ],
       "gasObjectId": "{{gas_object_id}}",
       "gasBudget": 2000
```

#### API Move call - Canavarı Güncelle <a href="#api-move-call---update-monster" id="api-move-call---update-monster"></a>

POST `/call` with body:

```
    {
       "sender": "{{owner}}",
       "packageObjectId": "0x2",
       "module": "geniteam",
       "function": "update_monster_stats",
       "args": [
           "0x{{player_id}}",
           "0x{{farm_id}}",
           "0x{{pet_monsters}}",
           "0x{{monster_id}}",
           {{monster_level}},
           {{hunger_level}},
           {{affection_level}},
           {{buddy_level}}
       ],
       "gasObjectId": "{{gas_object_id}}",
       "gasBudget": 2000
```

#### API Move call - Canavar Bilgisini Oku <a href="#api-move-call---read-monster-data" id="api-move-call---read-monster-data"></a>

```
GET /object_info?objectId={{monster_id}}
```

#### Akıllı Kontrat: Canavar Yarat <a href="#smart-contract-create-monster" id="smart-contract-create-monster"></a>

```
   struct Monster has key, store {
        info: Info,
        monster_name: String,
        monster_img_index: u64,
        breed: u8,
        monster_affinity: u8,
        monster_description: String,
        monster_level: u64,
        monster_xp: u64,
        hunger_level: u64,
        affection_level: u64,
        buddy_level: u8,

        // ID of the applied cosmetic at this slot
        applied_monster_cosmetic_0_id: Option<ID>,
        // ID of the applied cosmetic at this slot
        applied_monster_cosmetic_1_id: Option<ID>,
    }

    // Create a Monster and add it to the Farm's collection of Monsters
    public entry fun create_monster(_player: &mut Player,
                              farm: &mut Farm,
                              pet_monsters_c: &mut collection::Collection,
                              monster_name: vector<u8>,
                              monster_img_index: u64,
                              breed: u8,
                              monster_affinity: u8,
                              monster_description: vector<u8>,
                              ctx: &mut TxContext
    ) {

        let monster = create_monster_(
            monster_name,
            monster_img_index,
            breed,
            monster_affinity,
            monster_description,
            ctx
        );

        // Check if this is the right collection
        assert!(*&farm.pet_monsters_id == *ID::id(pet_monsters_c), EMONSTER_COLLECTION_NOT_OWNED_BY_FARM);


        // Add it to the collection
        collection::add(pet_monsters_c, monster);
    }

    // Creates a basic Monster object
    public fun create_monster_(
        monster_name: vector<u8>,
        monster_img_index: u64,
        breed: u8,
        monster_affinity: u8,
        monster_description: vector<u8>,
        ctx: &mut TxContext
    ): Monster {

        Monster {
            info: object::new(ctx),
            monster_name: ASCII::string(monster_name),
            monster_img_index,
            breed,
            monster_affinity,
            monster_description: ASCII::string(monster_description),
            monster_level: 0,
            monster_xp: 0,
            hunger_level: 0,
            affection_level: 0,
            buddy_level: 0,
            applied_monster_cosmetic_0_id: Option::none(),
            applied_monster_cosmetic_1_id: Option::none(),
        }
    }
```

#### Akıllı Kontrat:Canavarı Güncelle <a href="#smart-contract-update-monster" id="smart-contract-update-monster"></a>

```
    // Update the attributes of a monster
    public entry fun update_monster_stats(
        _player: &mut Player,
        _farm: &mut Farm,
        _pet_monsters: &mut collection::Collection,
        self: &mut Monster,
        monster_level: u64,
        hunger_level: u64,
        affection_level: u64,
        buddy_level: u8,
    ) {
        self.monster_level = monster_level;
        self.hunger_level = hunger_level;
        self.affection_level = affection_level;
        self.buddy_level = buddy_level;
    }
```

**Sui Monstar prototipi**

Sui Monstar bir evcil hayvan simülasyon oyunu örneğidir.

Oynanışı:

* Köpek ve kedi dostlarınızla oynayın, onları besleyin ve giydirin.
* Evcil hayvanlarınızı afinite rünleri ile geliştirin!
* Çiftliğinizi süsleyin.
* Oyun ve etkileşimler yoluyla çiftlik ve evcil hayvan seviyelerinizi yükseltin.

Sui Monstars'da sevimli monstarları yakalayın ve siz onları besleyip etkileşime girdikçe size yaklaşmalarını izleyin. Bu monstarlar, çiftliğiniz ve aksesuarlarınızın hepsi zincir üzerindeki NFT'lerdir. Oyun boyunca oynadıkça sağlık, dostluk ve aksesuarlar gibi özelliklerin tümü canlı olarak güncellenir.

{% embed url="https://www.youtube.com/watch?v=sAMT5x8W3B8" %}

__

__<img src="../.gitbook/assets/image (4).png" alt="" data-size="original">__

&#x20;_Monstar'ınıza element rünleri takın ve NFT'nizin güncellenmiş özelliklerle gelişmesini izleyin_

Hepsi bu kadar değil! Monstarlarınız güçlendikçe, Sui Battler'da savaşmanıza yardımcı olmaları için onları kullanabilirsiniz.

**Sui Battler prototipi**

Sui Battler, sevimli monstarlarınızın savaşçılara dönüştüğü örnek bir oyundur!

Oynanışı:

* Düşman dalgalarıyla savaşın ve deneyim ve güçlendirmeler kazanın.
* Sui Monstars'tan kendi evcil hayvanınızdan yardım alın.
* Sui Monstars'ta evcil hayvanınızı geliştirin ve özel savaş yeteneklerinin kilidini açın.
* Monstarlarınız zincir üzerindeki savaşınızın tarihini kaydeder!

![](<../.gitbook/assets/image (9).png>)

_Özel yeteneklerin kilidini açmak için Monstarlarınızı geliştirin_

**Bu neden önemli?**

* Değiştirilebilir NFT'ler daha zengin ve daha yaratıcı oyun anlamına gelir. Artık NFT'leri "değiştirmek" için karmaşık geçici çözümlere veya NFT'leri yakmanıza, tüm verilerinizi ve geçmişinizi kaybetmenize gerek yok.
* Kullanılabilirlik odaklı API'ler Sui üzerinde geliştirme yapmayı kolaylaştırır.
* Benzersiz ölçeklenebilirlik ve anında yerleşim, değişikliklerin, varlık durumunun, bakiyenin ve sahipliklerin oyunla birlikte anında canlı olarak gerçekleşebileceği anlamına gelir. Artık gecikme veya geçici çözümler yok.
* Sınır yaratıcılıktır. İçerik oluşturucular varlıklarını çeşitli uygulama ve oyunlarda özgürce kullanabilirler.
* Zengin geçmişe sahip, tamamen zincir üzerinde, birleştirilebilir NFT'ler yeni nesil oyun ekonomilerini mümkün kılmaktadır.

### Daha fazla <a href="#further-reading" id="further-reading"></a>

* Sui [API](https://docs.sui.io/sui-jsonrpc)'lerine göz atın
* Sui nesneleri hakkında bilgi edinin.
