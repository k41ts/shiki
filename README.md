# Shiki

Asisten suara untuk Windows. Dibuat dengan Rust + Tauri 2, otaknya Gemini (pakai API key kamu sendiri).

## Jalankan
1. Double-click `Shiki.exe`. Tidak perlu install; WebView2 sudah bawaan Windows 11.
2. Window Settings terbuka otomatis. Tempel **Gemini API key** (gratis di https://aistudio.google.com/apikey), lalu klik **Simpan**.
3. Shiki jalan di system tray. Klik ikonnya untuk membuka Settings, To-do, dan Jadwal. Klik kanan untuk keluar.

## Pakai
| Tahan | Fungsi |
|---|---|
| `Ctrl + Win` | **Dikte.** Ngomong (boleh belepotan), lalu lepas. Teks yang sudah rapi langsung ter-paste di kolom yang aktif. |
| `Ctrl + Shift` | **Perintah.** Layar sedikit meredup. Lingkari sesuatu pakai mouse kalau perlu, ngomong, lalu lepas. |

Contoh perintah:
- "Buka Spotify"
- "Cari cara masak spaghetti di YouTube"
- "Tambah to-do telpon mama"
- "Tiap pagi jam 9 buka Unity dan Spotify"
- "Tanggal 10 November jam 11 cari gift wrap di amazon.in"
- (lingkari sesuatu) "Ini apa?" / "Terjemahin ini"
- "Next song", "kecilin volume", "tampilkan desktop"
- "Setel paparazzi dubstep di YouTube", "lompat ke menit 1:30", "maju 30 detik", "pause", "lagu apa ini?"

Shortcut bisa diganti di Settings.

## Kirim file lewat WhatsApp
Bilang (atau ketik) misalnya **"kirim laporan PKL ke Alex di WA"**:
1. Shiki mencari file di Desktop, Documents, Downloads, Pictures dan `D:\Coding`.
2. Di WhatsApp Web (browser, sudah login) Shiki mencari chat yang namanya cocok (toleran salah eja, nama depan cukup), membukanya dan melampirkan file.
3. Shiki bertanya sekali: *"Apakah kamu ingin mengirim laporan.pdf ke Alex Luqman?"*, lalu baru mengirim setelah kamu jawab **ya** (atau klik **Kirim**).

Bisa dimatikan di Loadout › Abilities › File & WhatsApp.

## Telepon lewat WhatsApp
Bilang **"telpon Haidar"** (atau **"video call Haidar"**): Shiki membuka chat yang namanya cocok di WhatsApp Web dan langsung menekan tombol telepon. Kalau ada beberapa chat yang sama cocoknya, Shiki tanya dulu yang mana. Bisa dimatikan di Loadout › Abilities › Telepon WhatsApp.

## Island
Notch kecil di tengah atas layar. **Klik atau scroll ke bawah** di notch untuk membuka panel; Esc atau klik di luar untuk menutup. Notch otomatis sembunyi saat ada aplikasi fullscreen.

| Tab | Isi |
|---|---|
| Chat | Ngobrol atau suruh Shiki dengan mengetik. Pesan suara (Ctrl+Shift) masuk ke obrolan yang sama, dan Shiki ingat konteksnya. |
| Controls | Lagu yang sedang diputar (cover, play/pause/next), volume, aksi cepat. |
| Today | Jam, tanggal, to-do, jadwal. |
| Stats | CPU, RAM, baterai. |
| Clipboard | Riwayat teks yang di-copy (hanya di memori, tidak disimpan ke disk). Klik untuk paste. |
| Apps | Cari dan buka aplikasi, yang paling sering dipakai di atas. |
| Wallpaper | Ganti wallpaper dari folder Pictures dan wallpaper bawaan Windows. |
| Power | Kunci, tidur, keluar akun, restart, matikan. |

## Loadout
Klik ikon Shiki di tray (atau ⚙ di island) untuk membuka Loadout. Empat slot:

| Slot | Isi |
|---|---|
| Soul | Topeng Shiki: Hannya, Oni, atau Kitsune. Berlaku di island dan Loadout. |
| Personality | Gaya bicara: Tenang, Santai, Sarkas, Formal, Samurai, Iblis kontrak. |
| Voice | Shiki membalas dengan suara (Gemini TTS: Charon, Fenrir, Puck, …) atau Mati. Ada tombol ▶ untuk contoh. |
| Abilities | Saklar untuk hal yang boleh dilakukan agent: aplikasi, browser & YouTube, media, mengetik & shortcut, to-do & jadwal. |

## Data
Semua tersimpan di `%APPDATA%\Shiki`: `config.json`, `todos.json`, `tasks.json`, `apps.json` (hitungan pemakaian aplikasi), `thumbs\` (thumbnail wallpaper), `shiki.log`.

## Build dari source
Butuh Rust (rustup) dan Visual Studio Build Tools (workload C++). Jalankan `build.bat`.
Script ini build dengan 2 core supaya beban CPU ringan, lalu menaruh hasilnya di `Shiki.exe`.

## Catatan
- Shiki bisa jalan otomatis saat Windows menyala: Pengaturan › "Jalankan Shiki saat Windows menyala".
- Kalau `Shiki.exe` diganti versi baru di tempat yang sama, Shiki yang sedang jalan otomatis restart ke versi baru saat sedang nganggur.
- Rutinitas hanya jalan selama Shiki berjalan. Supaya ikut nyala saat Windows start, taruh shortcut `Shiki.exe` di folder `shell:startup`.
- Hotkey tidak terdeteksi di jendela yang berjalan sebagai Administrator.
- Pemakaian RAM saat idle sekitar 125 MB (Shiki sekitar 8 MB, sisanya proses WebView2).
