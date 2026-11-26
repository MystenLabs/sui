# Proof-of-Stake

Sui platformu, işlemleri gerçekleştiren validatör setini belirlemek için delege edilmiş proof-of-stake'e dayanır.

### SUI token delegasyonu <a href="#sui-token-delegation" id="sui-token-delegation"></a>

Her bir dönem içinde işlemler, her biri SUI token sahiplerinden devredilen belirli miktarda hisseye sahip sabit bir validatör grubu tarafından işlenir. Bir validatörün toplam hissedeki payı, her bir validatörün işlemlerin gerçekleştirilmesi için oylama gücündeki payını belirlemesi açısından önemlidir. SUI'nin devredilmesi, SUI jetonlarının tüm dönem boyunca kilitli olduğu anlamına gelir. SUI token sahipleri, dönem değiştiğinde SUI paylarını geri almakta veya temsilci validatörlerini değiştirmekte özgürdür.

### Ekonomik model <a href="#economic-model" id="economic-model"></a>

Şimdi Sui ekonomisinin farklı bileşenlerinin birbirleriyle nasıl etkileşime girdiğini tartışarak Sui'nin delege edilmiş proof-of-stake sistemini tanıtacağız. Tamamlayıcı bir referans olarak, [Sui Tokenomics](https://docs.sui.io/devnet/build) genel bakışındaki staking ve tokenomics diyagramına bakın.

Sui ekonomik modeli aşağıdaki gibi çalışır:

Her dönem (epoch) başında: Üç önemli şey olur:

* SUI sahipleri tokenlerinin (bir kısmını) validatörlere devreder ve yeni bir [komite ](https://docs.sui.io/devnet/learn/architecture/validators#committees)oluşturulur.
* Referans gas fiyatları Sui'nin [gas fiyat mekanizmasında](https://docs.sui.io/devnet/learn/tokenomics/gas-pricing) açıklandığı şekilde belirlenir
* [Depolama fonunun](https://docs.sui.io/devnet/learn/tokenomics/storage-fund) büyüklüğü bir önceki dönemin net girişi kullanılarak ayarlanır.

Bu işlemlerin ardından protokol, toplam pay miktarını temsil edilen pay artı depolama fonu toplamı olarak hesaplar. Temsilci hissesi payını $\alpha$ olarak adlandırın.

Her dönem boyunca: Kullanıcılar Sui platformuna işlem gönderir ve validatörler bunları işler. Her işlem için kullanıcılar ilgili hesaplama ve depolama gas'i ücretlerini öderler. Kullanıcıların önceki işlem verilerini sildiği durumlarda, kullanıcılar depolama ücretlerinde kısmi bir indirim alırlar. Validatörler diğer validatörlerin davranışlarını gözlemler ve birbirlerinin performansını değerlendirir.

Her dönemin sonunda: Protokol, proof-of-stake mekanizmasının katılımcılarına hisse ödüllerini dağıtır. Bu iki ana adımda gerçekleşir:

* Toplam stake ödülü miktarı, dönem boyunca tahakkuk eden hesaplama ücretleri artı dönemin stake ödülü sübvansiyonlarının toplamı olarak hesaplanır. İkinci bileşen, dolaşımdaki SUI miktarı toplam arzına ulaştığında uzun vadede ortadan kalkacağı için isteğe bağlıdır.
* Stake ödüllerinin toplam miktarı çeşitli kuruluşlar arasında dağıtılır. Daha da önemlisi, dönemin toplam hissesinin hesaplanmasında depolama fonunun dikkate alındığını unutmayın. Bununla birlikte, depolama fonu, devredilen SUI'nin sahip olduğu şekilde herhangi bir kuruluşa ait değildir. Bunun yerine, Sui'nin ekonomik modeli depolama fonuna tahakkuk eden stake ödüllerini - toplam stake ödüllerinin $(1-\alpha)$'lık bir payı - depolama maliyetlerini telafi etmek için validatörlere dağıtır. Bu ödüllerden $\gamma$ payı doğrulayıcılara ödenirken, kalan $(1-\gamma)$ fonun sermayesine yeniden yatırım yapmak için kullanılır. Son olarak, validatörlerin SUI token sahiplerinden delegasyon ücreti olarak $\delta%$ komisyon aldığını varsayalım. Stake ödüllerinin katılımcılar arasında paylaşımı şu şekilde verilir:

\$$ DelegatorRewards \ = \ ( 1 - \delta ) \ \times \ \alpha \ \times \ StakeRewards \$$

\$$ ValidatorRewards \ = \ ( \ \delta\alpha \ + \ \gamma (1 - \alpha) \ ) \ \times \ StakeRewards \$$

\$$ Reinvestment \ = \ ( 1 - \gamma ) \ \times \ ( 1 - \alpha ) \ \times \ StakeRewards \$$

### Stake ödülleri dağıtımı <a href="#stake-reward-distribution" id="stake-reward-distribution"></a>

Sui'nin gas fiyatlandırma mekanizması, yetkilendirilmiş proof-of-stake mekanizması ile birlikte, validatörlerin düşük ancak sürdürülebilir gas ücretleri ile sorunsuz bir şekilde çalışmaya teşvik edildiği verimli bir ekonomik model sunar. Belirli bir validatör $v$ şu kadar hisse ödülü alır:

\$$ ValidatorRewards(v) \ = \ RewardShare(v) \ \times \ ValidatorRewards \$$

Burada $RewardShare(v)$ [gas fiyat mekanizmasında](https://docs.sui.io/devnet/learn/tokenomics/gas-pricing) belirlenir. SUI token sahiplerinin, temsilci validatörleriyle aynı hisse ödül payını aldıklarını unutmayın. Spesifik olarak, $v$ değerindeki bir validatörde delege olan SUI token sahipleri aşağıdakilere eşit ödüller alırlar:

\$$ DelegatorRewards(v) \ = \ RewardShare(v) \ \times \ DelegatorRewards \$$

Net olarak, bu tasarım validatörleri düşük gas fiyat teklifleri ile çalışmaya teşvik eder - ancak çok düşük değil, aksi takdirde kesilmiş stake ödülleri alırlar. Sonuç olarak, Sui'nin gas fiyat mekanizması ve yetkilendirilmiş proof-of-stake sistemi, validatörlerin uygulanabilir iş modelleriyle çalışırken düşük gas ücretleri belirlediği adil fiyatlar için sağlıklı bir rekabeti teşvik eder.

### Sui teşvikleri <a href="#sui-incentives" id="sui-incentives"></a>

Sui'nin ekonomik modeli, Sui kullanıcılarına önemli bir izleme rolü vermektedir. Bir yandan, kullanıcılar işlemlerinin mümkün olduğunca hızlı ve verimli bir şekilde işlenmesini isterler. Cüzdanlar gibi kullanıcı istemcileri, en duyarlı validatörlerle iletişime öncelik vererek bunu teşvik eder. Bu tür verimli işlemler, daha az yanıt veren doğrulayıcılara göre artırılmış ödüllerle telafi edilir. Öte yandan, SUI token delegatörleri, temsilci validatörleriyle aynı artırılmış veya cezalandırılmış ödülleri alırlar. Dolayısıyla, yanıt vermeyen bir validatör Sui'nin teşviklerine iki kat daha fazla maruz kalır: ödüllerin azaltılması yoluyla doğrudan ve stakerlar tokenlarını daha duyarlı validatörlere taşıdıkça gelecek dönemlerdeki azalan delege hissesi yoluyla dolaylı olarak kaybederler.
