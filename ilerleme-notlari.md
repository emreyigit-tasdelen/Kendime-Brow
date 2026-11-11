# İlerleme Notları — Kendime-Brow

Karar günlüğü ve oturumlar arası devir notu. En yeni giriş en üstte.
Yeni oturumda önce bu dosyayı ve [ROADMAP.md](ROADMAP.md) dosyasını oku.

## Şu anki durum (özet)

- **Faz:** 0 — mimari tasarım. Kod yazılmıyor.
- **Bekleyen karar:** Teknoloji temeli (Roadmap 0.4).
- **Sonraki adım:** Karar gelince `docs/mimari.md` yazılacak (Roadmap 0.5).

## Kalıcı kurallar

- **Not tutma kuralı:** Roadmap'teki HER görev/adım biter bitmez (ne kadar küçük olursa olsun) aşağıdaki "Görev kayıtları" bölümüne giriş eklenir: hangi görev, ne yapıldı, alınan kararlar, sonraki adım. Hiçbir görev atlanmaz; bu dosya `/clear` veya token bitmesi sonrası devamlılığın tek kaynağıdır. Kayıt eklenince ROADMAP durumu da güncellenir ve commit atılır.

- Repo public. Anahtar, token, kişisel veri, Headscale kimlik bilgisi repoya girmez; her commit öncesi `git diff --cached` kontrol edilir.
- Commit kimliği GitHub noreply adresidir (kişisel e-posta public repoya yazılmaz).
- Lisans Apache 2.0. Eklenecek her kütüphanenin lisans uyumu mimari dokümanda not edilir (GPL/AGPL bileşenler ayrı süreçte izole edilir).
- Dokümanlar Türkçe; bir sonraki (daha ucuz) modelin devam edebileceği netlikte yazılır.

## Kararlar

| Tarih | Karar | Gerekçe |
|---|---|---|
| 2026-11-12 | Repo: `emreyigit-tasdelen/Kendime-Brow`, public, Apache 2.0 | Kullanıcı talebi |
| 2026-11-12 | VPN/Mesh: WireGuard + self-hosted Headscale | Kullanıcı talebi; üçüncü parti VPN'e bağımlılık yok |

## Görev kayıtları

En yeni üstte. Biçim: görev · yapılan · karar · sonraki adım.

### 0.4 Teknoloji temeli kararı — KARAR BEKLİYOR (2026-11-12)
- **Yapılan:** 5 seçenek (Chromium fork, Electron, CEF + native kabuk, Gecko fork, Tauri) karşılaştırıldı, kullanıcıya sunuldu. Tablo aşağıda "Günlük" bölümünde.
- **Karar:** Henüz yok. Öneri: C (CEF + Rust native kabuk). Ayrıca soruldu: hedef platform Windows öncelikli mi, LICENSE'taki ad doğru mu.
- **Sonraki adım:** Kullanıcı yanıtı → bu kaydı kapat, ROADMAP 0.4'ü `[x]` yap, 0.5'e (`docs/mimari.md`) başla.

### 0.3 Roadmap ve ilerleme notları iskeleti — BİTTİ (2026-11-12)
- **Yapılan:** `ROADMAP.md` (Faz 0 ayrıntılı, Faz 1–6 yer tutucu) ve `ilerleme-notlari.md` oluşturuldu.
- **Karar:** Durum işaretleri `[ ] [~] [x] [!] [-]`. Faz 1+ görevleri mimari doküman bitince doldurulacak.
- **Sonraki adım:** 0.4.

### 0.2 LICENSE, .gitignore, README — BİTTİ (2026-11-12)
- **Yapılan:** Apache 2.0 metni apache.org'dan alındı, telif satırı "2026 Emre Yiğit Taşdelen" yapıldı. `.gitignore`: anahtarlar, .env, WireGuard/Headscale dosyaları, pcap/har, build çıktıları, node_modules, Chromium kaynak ağacı, tarayıcı profilleri. `README.md` yazıldı.
- **Karar:** LICENSE'taki ad Windows kullanıcı adından alındı (kullanıcı teyidi bekleniyor).
- **Sonraki adım:** 0.3.

### 0.1 Klasör, git, GitHub repo — BİTTİ (2026-11-12)
- **Yapılan:** `D:\Claude\Kendime-Brow` içinde `git init` (dal: main). `gh repo create` ile public repo `emreyigit-tasdelen/Kendime-Brow` oluşturuldu, ilk commit gönderildi. Commit öncesi hassas veri taraması temiz.
- **Karar:** Commit kimliği yalnız bu repoda ayarlı, GitHub noreply adresi (global git kimliği tanımlı değildi; kişisel e-posta public repoya yazılmasın diye).
- **Sonraki adım:** 0.2.

## Günlük

### 2026-11-12
- Klasör, git, GitHub repo, LICENSE, .gitignore, README, ROADMAP, bu dosya oluşturuldu.
- Teknoloji temeli için seçenekler karşılaştırıldı ve kullanıcıya soruldu (aşağıda).

#### Teknoloji temeli seçenekleri (karar bekliyor)

| | A. Chromium fork | B. Electron | C. CEF + native kabuk | D. Firefox/Gecko fork | E. Tauri/sistem WebView |
|---|---|---|---|---|---|
| Web uyumu | Tam | Tam | Tam | İyi | Platforma göre değişir |
| Gizlilik kontrolü | En yüksek (kaynak seviyesinde yama) | Düşük (ağ yığınına sınırlı erişim) | Orta-yüksek | Yüksek | Düşük |
| RAM/CPU kontrolü | En yüksek | Düşük | Yüksek | Yüksek | Orta |
| Chrome extension desteği | Tam (MV2 dahil tutulabilir) | Kısmi | Kısmi | WebExtensions | Yok |
| Build maliyeti | Çok yüksek (100+ GB, saatler) | Düşük | Düşük (hazır binary) | Yüksek | Düşük |
| Güvenlik güncellemesi yükü | Yüksek (sürekli rebase) | Orta | Orta | Yüksek | Düşük |
| Tek geliştirici için uygunluk | Zor | Kolay ama tarayıcı için zayıf | Uygun | Zor | Tarayıcı için uygun değil |

**Öneri:** C (CEF + Rust native kabuk; modüller ayrı süreçlerde). Ayrıntı sohbette sunuldu.
