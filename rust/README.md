# RainbowMiner – Rust port

*(English version below)*

## Magyarul

Ez a mappa a RainbowMiner (PowerShell) profitváltó bányászat-kezelő **Rust nyelvű portja**.
Egyetlen önálló bináris (`rainbowminer`), amely a meglévő RainbowMiner mappából fut, és annak
fájljait használja: `Config/*.txt`, `Stats/`, `Data/`, `Bin/` (a letöltött bányászprogramok) és
a `web/` dashboard. A PowerShell-verzió és a Rust-verzió ugyanazokat a konfigurációs és
statisztika-fájlokat olvassa és írja, így a kettő között oda-vissza lehet váltani (egyszerre
csak az egyiket futtasd).

### Fordítás

```bash
cd rust
cargo build --release
```

A bináris: `rust/target/release/rainbowminer`. Futtatható innen is (megkeresi a RainbowMiner
mappát a bináris szülőkönyvtárai között), vagy átmásolható a RainbowMiner mappába.

Windowsra Windowson ugyanígy (`cargo build --release`, MSVC vagy GNU toolchain), vagy Linuxról
keresztfordítással:

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt-get install gcc-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu
```

### Futtatás

```bash
./rainbowminer -Wallet <BTC-cím> -WorkerName rig1 -PoolName Zpool,NiceHash
```

A parancssori paraméterek ugyanazok, mint a `RainbowMiner.ps1`-é (`-Név érték`, `-Név:érték`,
kapcsolók `$true/$false` értékkel is). Első indításkor létrejön a `Config/config.txt` és a többi
konfigurációs fájl; utána ezek szerkesztésével állítható minden. A konfigurációs fájlok
módosítását a futó program a következő körben észreveszi és újratölti.

Hasznos kapcsolók: `-Help`, `-Version`, `-ListDevices`, `-ListPools`, `-ListMiners`,
`-ConfigFile <út>`, `-BaseDir <út>`.

A web-dashboard az `APIport` porton érhető el (alapértelmezés: 4000), pl. `http://localhost:4000`.
Linuxon minden interfészen figyel (mint az eredeti), ezért távoli eléréshez állítsd be az
`APIauth`/`APIuser`/`APIpassword` vagy az `APIallowIPs` értékét.

Billentyűk futás közben: `X` kilépés, `R` újraindítás, `S` váltás-késleltetés kihagyása,
`N` következő kör, `P` szünet, `L` zárolás, `V` részletes nézet, `B` egyenleg-frissítés,
`W` watchdog-nullázás, `E` gyorsítótár törlése, `D` letöltő indítása.

### Mi van portolva

* **Mag (Invoke-Core):** konfig-betöltés és -normalizálás (`$Változó` helyettesítés), ütemező
  (scheduler, áramár), pool-lekérdezés, szűrés, watchdog, WhatToMine-árak, összehasonlító árak
  (szinkron-csillapítás, hibahatár-lecsengés, hiszterézis, hashrate-súlyozás), pool-alternatívák,
  bányász-jelöltek, `miners.config.txt` paraméter-összefésülés, profit/költség számítás,
  automatikus benchmark-nullázás verzióváltáskor, legjobb eszköz-kombináció (pontos lefedés),
  post-block mining, CPU/GPU ütközés, profitabilitás-figyelés, kombó mód, bányászok
  indítása/leállítása, összeomlás-számláló és automatikus letiltás, elakadás-felismerés
  (stall detection), hashrate-statisztikák (mozgóátlagok, hibafelismerés), `Stats/Totals`,
  egyenlegek és bevétel-követés.
* **Eszközök:** OpenCL-felismerés (dinamikusan betöltve), nvidia-smi/sysfs tartalékok, CPU
  (CPUID, magok/szálak, topológia), eszközkiválasztás (`Data/devices.json`), kombók,
  szenzoradatok (hőmérséklet, fogyasztás).
* **Bányászprogramok kezelése:** letöltés (sha256-ellenőrzés, zip/tar.gz/7z, mentések),
  indítás saját folyamatcsoportban, prioritás/niceness, CPU-affinitás (Linux), naplófájl,
  szabályos leállítás.
* **Miner API-k:** Trex, Gminer, lolMiner, Rigel, BzMiner, OneZeroMiner, SrbMinerMulti
  (autotune-intenzitások mentésével), XMRig6 (konfigfájlok, threads-előfuttatás), Ccminer
  (TCP), Wrapper és CustomWrapper (konzolkimenet-feldolgozás).
* **Pool-modulok (10/68):** 2Miners, HeroMiners, MiningDutch, MoneroOcean, Nanopool, NiceHash,
  unMineable, Zpool, Userpools (`userpools.config.txt`), WhatToMine (árforrás).
* **Bányász-modulok (9/108 + egyedi):** BzMiner, CpuminerJayddee (cpuminer-opt), Gminer,
  lolMiner, OneZeroMiner, Rigel, SrbMinerMulti, Trex, Xmrig, valamint a
  `customminers.config.txt` egyedi bányászai. A parancstáblák az eredeti `Miners/*.ps1`
  fájlokból vannak kinyerve (`assets/miners/*.json`).
* **Egyenleg-modulok (7/46):** 2Miners, HeroMiners, MoneroOcean, Nanopool, NiceHash (aláírt API),
  unMineable, Zpool.
* **HTTP API és web-dashboard:** a `web/` mappa kiszolgálása (`#include` feldolgozással), a
  dashboard által használt fő JSON-végpontok (`/activeminers`, `/runningminers`, `/pools`,
  `/miners`, `/devices`, `/balances`, `/rates`, `/currentprofit`, `/stats`, `/console`, ...),
  parancsok (`/stop`, `/pause`, `/lockminers`, `/updatebalance`, `/watchdogreset`, ...),
  Basic-auth és IP-szűrés, benchmark-nullázó szkriptek natívan.
* **Hálózat:** gyorsítótárazott REST-kérések (`Cache/`), proxy-beállítás (vagy a rendszer
  `HTTPS_PROXY` változója), rendszer-tanúsítványtár, árfolyamok (Coinbase + tartalék),
  adatbázis-frissítés (unprofitable-listák, DAG-méretek).

### Ami (még) nincs portolva

* MiningRigRentals (bérlés), Server/Client mód, távoli gépek;
* tuningolás: OC-profilok, MSI Afterburner, OC-daemon, EthPill;
* interaktív telepítő (`-Setup`), automatikus frissítés, újraindítás/reboot gépszinten;
* a **donation run** (a fejlesztőnek bányászott napi percek) szándékosan nincs portolva;
* fastlane benchmark, benchmark-átvétel új eszközkészletre, pool-latencia preferencia,
  `PowerPriceApi`, szünet aktivitásra/akkumulátorra, monitorozó szerver (MinerStatus) jelentések;
* a többi pool-, bányász- és egyenleg-modul (a fenti listákon kívül);
* a BzMinerWrapper konzol-táblás tartaléka (csak a HTTP API van portolva);
* Windows alatt a CPU-affinitás nem kerül alkalmazásra.

A hiányzó modulok a meglévők mintájára adhatók hozzá: pool → `src/pools/` + `pools::registry()`,
bányász → `src/miners/` + `miners::registry()` (a parancstábla kinyerhető a PowerShell-fájlból),
egyenleg → `balances::modules()`.

### Tesztek

```bash
cargo test
```

A unit tesztek a számítási részeket fedik (statisztika-átlagolás és hibafelismerés, verziók,
hashrate-konverzió, konfig-helyettesítés, eszközkiválasztás, kombinációkeresés, paraméter-összefésülés,
API-segédek). Végponttól végpontig tesztelve: pool-lekérdezés (Zpool), egy kamu egyedi bányász
benchmarkolása, profitszámítás, váltás a jobb algoritmusra, web/API és szabályos leállítás.

---

## English

This folder holds a **Rust port** of RainbowMiner, the PowerShell profit switching mining
manager. It is a single binary (`rainbowminer`) that runs from an existing RainbowMiner folder
and uses its files: `Config/*.txt`, `Stats/`, `Data/`, `Bin/` (downloaded miners) and the `web/`
dashboard. Both versions read and write the same config and stats files, so you can switch back
and forth (run only one of them at a time).

### Build and run

```bash
cd rust
cargo build --release
```

```bash
./target/release/rainbowminer -Wallet <BTC address> -WorkerName rig1 -PoolName Zpool,NiceHash
```

For Windows build natively with `cargo build --release`, or cross-compile from Linux with
`rustup target add x86_64-pc-windows-gnu`, the `gcc-mingw-w64-x86-64` package and
`cargo build --release --target x86_64-pc-windows-gnu`.

The parameters are those of `RainbowMiner.ps1` (`-Name value`, `-Name:value`, switches). The
first start creates `Config/config.txt` and the other config files; changes to them are picked
up in the next round. Extra switches: `-Help`, `-Version`, `-ListDevices`, `-ListPools`,
`-ListMiners`, `-ConfigFile <path>`, `-BaseDir <path>`. The dashboard is served on `APIport`
(default 4000); on Linux it listens on all interfaces like the original, so set
`APIauth`/`APIuser`/`APIpassword` or `APIallowIPs` for remote access.

### Module map

| PowerShell | Rust |
|---|---|
| `RainbowMiner.ps1` parameters | `src/params.rs` |
| `Modules/Core.psm1` (Start/Invoke/Stop-Core, Set-MinerStats, Get-BestMinerDeviceCombos, Get-Balance) | `src/engine.rs`, `src/combos.rs`, `src/display.rs`, `src/balances.rs` |
| `Modules/ConfigLib.psm1`, `Data/*Default.ps1` | `src/config.rs`, `assets/*.json` |
| `Modules/StatLib.psm1` (Set-Stat, Get-Stat) | `src/stat.rs` |
| `Modules/DeviceLib.psm1` | `src/device.rs`, `src/opencl.rs` |
| `Modules/MinersLib.psm1` | `src/miner.rs` |
| `Modules/MinerAPIs.psm1` | `src/minerapi.rs` |
| `Modules/PoolsLib.psm1`, `Modules/WhatToMineLib.psm1` | `src/pool.rs`, `src/pools/` |
| `Modules/ProcLib.psm1` | `src/proc.rs` |
| `Scripts/Downloader.ps1` | `src/download.rs` |
| `Modules/WebLib.psm1`, `Modules/Asyncloader.psm1` | `src/web.rs`, `src/rates.rs` |
| `Modules/API.psm1`, `Scripts/API.ps1` | `src/api.rs` |
| `Miners/*.ps1` (9 of 108) | `src/miners/`, `assets/miners/*.json` |
| `Pools/*.ps1` (10 of 68) | `src/pools/` |
| `Balances/*.ps1` (7 of 46) | `src/balances.rs` |

### Not ported (yet)

MiningRigRentals, server/client mode, overclocking (OC profiles, MSI Afterburner, OC daemon,
EthPill), the interactive setup, auto update, the donation run (left out on purpose), fastlane
benchmarks, benchmark seeding for new device sets, pool latency preference, `PowerPriceApi`,
pause on activity/battery, MinerStatus reporting, the console fallback of BzMinerWrapper, CPU
affinity on Windows, and the remaining pool, miner and balance modules. New modules follow the
existing ones: add a pool to `src/pools/` and `pools::registry()`, a miner to `src/miners/` and
`miners::registry()` (its command table can be extracted from the PowerShell module), a balance
module to `balances::modules()`.

### Tests

`cargo test` runs the unit tests (stat averages and fault detection, versions, hashrate
conversion, config substitution, device selection, combo search, params merge, API helpers).
The port was also run end to end: pools from Zpool, a fake custom miner benchmarked, profits
calculated, switch to the more profitable algorithm, web/API, graceful shutdown.
