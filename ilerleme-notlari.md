# İlerleme Notları — Kendime-Brow

Karar günlüğü ve oturumlar arası devir notu. En yeni giriş en üstte.
Yeni oturumda önce bu dosyayı ve [ROADMAP.md](ROADMAP.md) dosyasını oku.

## Şu anki durum (özet)

- **Faz:** 0 tamamlandı (mimari tasarım). Kod yazılmadı. Bekleyen karar yok.
- **Sonraki adım:** Faz 1, görev 1.1 — CEF binary indir, Rust `cef` crate ile tek pencere aç, V1/V3/V4'ü doğrula; ardından 1.1b aynı denemeyi Ubuntu VM'de tekrarla.
- **Platform:** Çok platformlu, öncelik yok. Doğrulama sırası Windows → Ubuntu (yerel VM; Kali de var) → macOS (donanım bulunursa).
- **Okuma sırası (yeni oturum):** bu dosya → `ROADMAP.md` → `docs/mimari.md` içinden yalnız ilgili bölüm.

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
| 2026-11-12 | Teknoloji temeli: CEF + Rust | Hazır motor binary'si, düşük bakım yükü, modüller motordan bağımsız |
| 2026-11-12 | Mimari kararlar K1–K16 | `docs/mimari.md` §1 |
| 2026-11-12 | Çok platformlu; sıra Windows → Ubuntu → macOS | Kullanıcı kararı; geliştirme ortamı Windows + Ubuntu/Kali VM |
| 2026-11-12 | Ek özellikler: P1 kapsamda, P2 ertelendi, P3 kapsam dışı | Kullanıcı kararı; P1 mevcut altyapıyı yeniden kullanıyor |

## Görev kayıtları

En yeni üstte. Biçim: görev · yapılan · karar · sonraki adım.

### 0.8 Mimari onayı ve teyitler — BİTTİ (2026-11-12)
- **Yapılan:** Kullanıcı teyitleri alındı. Mimariye §3.8 "Çok platformluluk" eklendi (platform soyutlama tablosu, `kb-platform` crate'i, kurallar); K15, §3.2, §13, §14 (V13–V15, iki yeni risk) güncellendi. Roadmap: CI Windows + Ubuntu matrisi, 1.1b / 1.10b / 1.15 Ubuntu görevleri, yeni Faz 7 (macOS); eski 6.7 "Linux desteği" kaldırıldı çünkü Linux artık her fazın parçası.
- **Karar:** Ürün **çok platformlu**, öncelikli platform yok. Doğrulama sırası: Windows → Ubuntu → macOS (en son, donanım bulunursa). Platforma özgü kod yalnız `kb-platform` içinde. Her faz Ubuntu'da da çalışmadan bitmiş sayılmaz. LICENSE'taki ad "Emre Yiğit Taşdelen" doğru.
- **Sonraki adım:** Faz 1.1.

### 0.6 Ek özellik seçimi — BİTTİ (2026-11-12)
- **Yapılan:** Kullanıcı mimari §12'deki önceliklendirmeyi onayladı. `ROADMAP.md` Faz 6 P1'e göre görevlere bölündü (6.1–6.7); mimari §12'ye karar notu eklendi.
- **Karar:** **P1 kapsamda** (komut paleti, kaynak monitörü, yerel terminal, şifre yöneticisi, checksum, arşiv, regex/JSON/hash araçları, kullanıcı scriptleri). **P2 ertelendi** (REST istemcisi, senkronizasyon, port tarayıcı, genel OCR, toplu yeniden adlandırma, ekran kaydı). **P3 kapsam dışı** (Git entegrasyonu, ses düzenleyici).
- **Sonraki adım:** 0.8 teyitleri (engelleyici değil) → Faz 1.1: CEF binary indir, Rust `cef` crate ile tek pencere aç, V1/V3/V4'ü doğrula.

### 0.7 Faz 1+ görevlerinin ayrıntılandırılması — BİTTİ (2026-11-12)
- **Yapılan:** `ROADMAP.md` yeniden yazıldı: Faz 1–6, toplam ~70 görev, her biri mimari bölümüne referanslı. Doğrulama (spike) görevleri V1–V12 ile eşlendi.
- **Karar:** Her faz bağımsız yayınlanabilir sürüm üretir (0.1 … 0.8). Faz 3 üç bağımsız gruba bölündü (PDF / ofis / medya).
- **Sonraki adım:** 0.6 ve 0.8 kullanıcı kararları.

### 0.5 Mimari tasarım dokümanı — BİTTİ (2026-11-12)
- **Yapılan:** `docs/mimari.md` yazıldı (14 bölüm): süreç modeli, sandbox, IPC, gizlilik, performans, PDF/ofis/medya, ağ analizi, uzak bağlantı, mesh + Companion + Headscale sunucu önerileri, ek özellik önceliklendirmesi, repo yapısı, doğrulanacak varsayımlar.
- **Kararlar (K1–K16, ayrıntı dokümanda §1):**
  - Arayüz HTML/TS (SolidJS), `kb://` şeması. Ağır modüller ayrı süreç, isteğe bağlı başlar.
  - Güvenilmeyen dosya ayrıştıran süreçlerin (kb-doc, kb-media) ağ erişimi yok.
  - PDF: PDFium + qpdf + lopdf + Tesseract. Ofis: LibreOfficeKit (isteğe bağlı indirme). Medya: FFmpeg LGPL build, alt süreç.
  - SSH: russh. RDP: IronRDP (yedek FreeRDP). Kasa: ayrı süreç, özel anahtar kasadan çıkmaz.
  - Mesh: `tsnet` (Go) — sürücü/yönetici yetkisi gerektirmez, sistem geneli VPN değildir. Companion: Go, tsnet gömülü, RDP/SSH'ı loopback'ten vekiller.
  - GPL/AGPL bileşenler (Ghostscript, MuPDF, x264, exiv2, OnlyOffice) koda bağlanmaz.
- **Önemli bulgu:** Resmi CEF binary'lerinde H.264/AAC kapalı olabilir (V1). Öyleyse yamasız ama codec argümanlı kendi CEF build'imiz gerekir (V2). Faz 1'in ilk iki görevi bunu doğrular.
- **Sonraki adım:** 0.7.

### 0.4 Teknoloji temeli kararı — BİTTİ (2026-11-12)
- **Yapılan:** 5 seçenek karşılaştırıldı (tablo aşağıda "Günlük" bölümünde), kullanıcıya sunuldu.
- **Karar:** **C — CEF + Rust native kabuk** (kullanıcı onayı). Gerekirse ileride ungoogled-chromium yamalı CEF build'ine geçiş yolu açık (mimari §2.3).
- **Sonraki adım:** 0.5.

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

**Seçilen:** C (CEF + Rust native kabuk).
