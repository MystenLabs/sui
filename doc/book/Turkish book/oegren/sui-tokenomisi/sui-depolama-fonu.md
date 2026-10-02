# Sui Depolama Fonu

Sui, veri depolamayı finanse etmek için verimli ve sürdürülebilir bir ekonomik mekanizma içerir; bu, Sui'nin keyfi olarak büyük miktarlarda zincir içi veri depolayabilme yeteneği göz önüne alındığında önemlidir.

Finansal açıdan, zincir üzerinde veri depolama ciddi bir zamanlararası zorluk ortaya çıkarmaktadır: bugün verileri işleyen ve depolamaya yazan validatörler, gelecekte bu verileri depolaması gereken validatörlerden farklı olabilir. Kullanıcılar hesaplama gücü için yalnızca yazma sırasında ücret ödüyor olsalardı, gelecekteki kullanıcıların geçmişteki kullanıcıları depolama için sübvanse etmeleri ve orantısız derecede yüksek ücretler ödemeleri gerekirdi. Bu olumsuz ağ dışsallığı, ele alınmazsa gelecekte Sui için oldukça vergi verici hale gelebilir.

Sui'nin ekonomik tasarımı, depolama ücretlerini geçmiş işlemlerden gelecekteki validatörlere yeniden dağıtan bir depolama fonu içerir. Kullanıcılar Sui'de işlem yaptıklarında, hem hesaplama hem de depolama için önceden ücret öderler. Depolama ücretleri, SUI delegatörlerine göre validatörlere dağıtılan gelecekteki stake ödüllerinin payını ayarlamak için kullanılan bir depolama fonuna yatırılır. Bu tasarım, gelecekteki Sui validatörlerine uygulanabilir iş modelleri sağlamayı amaçlamaktadır.

### Depolama fonu ödülleri <a href="#storage-fund-rewards" id="storage-fund-rewards"></a>

Sui'nin proof-of-stake mekanizması, toplam stake'i delege edilen hisse artı depolama fonuna yatırılan SUI tokenlerinin toplamı olarak hesaplar. Dolayısıyla, depolama fonu, toplam stake'e göre büyüklüğüne bağlı olarak genel stake ödüllerinden orantılı bir pay alır. Bu stake ödüllerinin çoğunluğu - $\gamma$ payı - depolama maliyetlerini telafi etmek için mevcut doğrulayıcılara ödenirken, kalan $(1-\gamma)$ ödülleri fona yeniden yatırım yapmak için kullanılır. Başka bir deyişle, geçmiş işlemler tarafından sunulan depolama ücretlerine tahakkuk eden stake ödülleri, veri depolama maliyetlerini telafi etmek için mevcut doğrulayıcılara ödenir. Zincir içi depolama gereksinimleri yüksek olduğunda, validatörler depolama maliyetlerini telafi etmek için önemli miktarda ek ödül alırlar. Depolama gereksinimleri düşük olduğunda ise tam tersi olur.

Daha spesifik olarak, depolama fonunun üç temel özelliği vardır:

* Depolama fonu geçmiş işlemlerle finanse edilir ve gas ücretlerinin farklı dönemler arasında kaydırılması için bir araç olarak işlev görür. Bu, gelecekteki validatörlerin depolama maliyetleri için ilk etapta bu depolama gereksinimlerini yaratan geçmiş kullanıcılar tarafından tazmin edilmesini sağlar.
* Depolama fonu yalnızca sermayesinin getirisini öder ve anaparasını dağıtmaz. Yani, pratikte, validatörler sanki depolama fonunun SUI'sini ek stake olarak ödünç alabiliyor ve hisse ödüllerinin çoğunu (bir $\gamma$%) ellerinde tutabiliyorlarmış gibi. Ancak validatörlerin doğrudan depolama fonundan fon almadığını unutmayın. Bu, fonun sermayesini asla kaybetmemesini ve süresiz olarak ayakta kalabilmesini garanti eder. Bu özellik, fona yeniden yatırılan stake ödüllerinin $(1-\gamma)$%'sı ile daha da desteklenmektedir.
* Depolama fonu, kullanıcıların daha önce zincir üzerinde depolanan verileri sildiklerinde depolama ücreti iadesi aldıkları bir silme seçeneği içerir. Bir kullanıcının verileri silmesi halinde, başlangıçta ödenen depolama ücretlerinin bir kısmının iade edileceğini unutmayın. Bu özellik, depolama ücretlerinin verilerin yaşam döngüsü boyunca depolama için ödeme yapmak üzere var olduğu gerçeğiyle gerekçelendirilmiştir. Veriler silindikten sonra depolama için ücret almaya devam etmek için bir neden yoktur ve bu nedenle bu ücretler iade edilir.

> **Önemli:** _Silme seçeneği_ geçmiş işlemlerin silinmesi ile karıştırılmamalıdır. Sui üzerindeki faaliyet her dönem sınırında sonlandırılır ve geçmiş işlemler değişmezdir ve asla tersine çevrilemez. Silinebilecek veri türü, örneğin, bir NFT'nin meta verileri, kullanılmış biletler, sonuçlanmış açık artırmalar gibi artık canlı olmayan nesnelere karşılık gelen verilerdir.

### Depolama fonu mekaniği <a href="#storage-fund-mechanics" id="storage-fund-mechanics"></a>

Depolama fonunun büyüklüğü her dönem boyunca sabittir ve dönem boyunca biriken net girişlere göre dönem sınırında büyüklüğü değişir. Girişler ve çıkışlar şunlara karşılık gelir:

* Cari dönem boyunca gerçekleştirilen işlemler için ödenen saklama ücretlerinden kaynaklanan girişler.
* Fonun getirilerinin yeni anaparaya yeniden yatırılmasından kaynaklanan girişler. Özellikle, depolama fonuna tahakkuk eden ve validatörlere ödenmeyen stake ödüllerinin $(1-\gamma)$ payı.
* Geçmiş işlemlerle ilişkili verileri silen kullanıcılara ödenen depolama ücreti iadelerinden çıkışlar.

İndirim fonksiyonunun temel özelliği, depolama fonu çıkışlarını her zaman orijinal depolama akışından daha az olacak şekilde bireysel işlem seviyesinde sınırlandırmasıdır. Bu mekanizma, depolama fonunun hiçbir zaman tükenmemesini ve büyüklüğünün depoda tutulan veri miktarına paralel olarak hareket etmesini garanti eder.

### Depolama fonu teşvikleri <a href="#storage-fund-incentives" id="storage-fund-incentives"></a>

Depolama fonu Sui ekonomisine arzu edilen çeşitli teşvikler getirmektedir:

* Mekaniği, kullanıcıları verileri silmeye teşvik eder ve bu tür verileri depolamanın maliyeti, bu verileri zincir üzerinde tutmaktan elde edilen değeri aştığında depolama ücretlerinde bir indirim alır. Bu, kullanıcıların verileri saklamaları ekonomik olmadığında depolama alanını ücretsiz hale getirdikleri faydalı bir piyasa tabanlı mekanizma sunar.
* SUI tokenı üzerinde deflasyonist bir baskı yaratır, çünkü artan aktivite daha büyük depolama gereksinimlerine ve dolaşımdan daha fazla SUI çıkarılmasına yol açar.
* Kullanıcıların depolama için dönem başına ödeme modeliyle ödeme yaptığı bir kira modeline ekonomik olarak eşdeğer olması bakımından sermaye açısından verimlidir.
