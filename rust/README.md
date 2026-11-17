# RainbowMiner – Rust port

*(English version below)*

## Magyarul

Ez a mappa a RainbowMiner (PowerShell) profitváltó bányászat-kezelő **Rust nyelvű portja**.
Két program készül belőle:

* `rainbowminer` – a bányászatot vezérlő engine (konzolos program, web-dashboarddal),
* `rainbowminer-gui` – grafikus felület, amely a futó engine-t mutatja és vezérli.

Az engine a meglévő RainbowMiner mappából fut, és annak fájljait használja: `Config/*.txt`,
`Stats/`, `Data/`, `Bin/` (a letöltött bányászprogramok) és a `web/` dashboard. A PowerShell-
és a Rust-verzió ugyanazokat a konfigurációs és statisztika-fájlokat olvassa és írja, így a
kettő között oda-vissza lehet váltani (egyszerre csak az egyiket futtasd).

### Fordítás

```bash
cd rust
cargo build --release                  # csak az engine
cargo build --release --features gui   # az engine és a grafikus felület
```

A programok: `rust/target/release/rainbowminer` és `rust/target/release/rainbowminer-gui`.
Futtathatók innen is (megkeresik a RainbowMiner mappát a szülőkönyvtárak között), vagy
átmásolhatók a RainbowMiner mappába.

Windowsra Windowson ugyanígy (MSVC vagy GNU toolchain), vagy Linuxról keresztfordítással:

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt-get install gcc-mingw-w64-x86-64
cargo build --release --features gui --target x86_64-pc-windows-gnu
```

A grafikus felülethez Linuxon OpenGL (pl. Mesa), X11 vagy Wayland és a `libxkbcommon-x11-0`
csomag kell – asztali rendszereken ezek általában eleve fent vannak.

### Futtatás

```bash
./rainbowminer -Wallet <BTC-cím> -WorkerName rig1 -PoolName Zpool,NiceHash
```

A parancssori paraméterek ugyanazok, mint a `RainbowMiner.ps1`-é (`-Név érték`, `-Név:érték`,
kapcsolók `$true/$false` értékkel is). Első indításkor létrejön a `Config/config.txt` és a többi
konfigurációs fájl; utána ezek szerkesztésével állítható minden. A konfigurációs fájlok
módosítását a futó program a következő körben észreveszi és újratölti. Ahogy az eredetiben is:
ha a `config.txt`-ben egy érték `$Név` helyőrző, akkor a `Data/ConfigDefault.ps1` alapértéke
(pl. `PoolName = Nicehash,Zpool`) erősebb a parancssori értéknél.

Hasznos kapcsolók: `-Help`, `-Version`, `-ListDevices`, `-ListPools`, `-ListMiners`,
`-ConfigFile <út>`, `-BaseDir <út>`.

A web-dashboard az `APIport` porton érhető el (alapértelmezés: 4000), pl. `http://localhost:4000`.
Linuxon minden interfészen figyel (mint az eredeti), ezért távoli eléréshez állítsd be az
`APIauth`/`APIuser`/`APIpassword` vagy az `APIallowIPs` értékét.

Billentyűk futás közben: `X` kilépés, `R` újraindítás, `S` váltás-késleltetés kihagyása,
`N` következő kör, `P` szünet, `L` zárolás, `V` részletes nézet, `B` egyenleg-frissítés,
`W` watchdog-nullázás, `E` gyorsítótár törlése, `D` letöltő indítása.

### Grafikus felület

```bash
./rainbowminer-gui
./rainbowminer-gui --host 192.168.1.10 --port 4000 --user admin --password titok
```

A felület az engine HTTP API-ját használja (a beállításokat – `APIport`, `APIauth`, `APIuser`,
`APIpassword` – a `Config/config.txt`-ből veszi), így ugyanazon a gépen vagy a hálózat egy másik
gépén futó engine-hez is kapcsolódhat. Fülek:

* **Áttekintés:** profit és bevétel naponta (BTC-ben és a beállított pénznemben), fogyasztás,
  futásidő, a futó bányászok (sebesség, profit, share-ek), az eszközök (hőmérséklet,
  ventilátor, fogyasztás, terhelés) és a watchdog-időzítők;
* **Eszközök, Bányászok** (szűrővel, „csak a legjobbak”), **Poolok, Egyenlegek, Benchmarkok**
  (a benchmarkra váró bányászok) és **Konzol** (az engine kimenete és a bányászok naplói);
* **Beállítások:** API-cím, port, felhasználó/jelszó, frissítési időköz, nyelv.

Gombok: szünet/folytatás, bányászok rögzítése, watchdog nullázása, egyenlegek frissítése,
gyorsítótár törlése, az engine leállítása (megerősítéssel) és – ha nem fut – elindítása,
valamint a webes dashboard megnyitása. Magyar és angol felirat (a rendszer nyelve szerint,
vagy a jobb felső sarokban váltható). Kapcsolók: `--host`, `--port`, `--user`, `--password`,
`--basedir`, `--interval <mp>`, `--lang hu|en`, `--tab <fül>`.

### Mi van portolva

* **Mag (Invoke-Core):** konfig-betöltés és -normalizálás (`$Változó` helyettesítés), ütemező
  (scheduler, áramár), pool-lekérdezés, szűrés, watchdog, WhatToMine-árak, összehasonlító árak
  (szinkron-csillapítás, hibahatár-lecsengés, hiszterézis, hashrate-súlyozás), pool-alternatívák,
  bányász-jelöltek, `miners.config.txt` paraméter-összefésülés, profit/költség számítás,
  automatikus benchmark-nullázás verzióváltáskor, legjobb eszköz-kombináció (pontos lefedés),
  post-block mining, CPU/GPU ütközés, profitabilitás-figyelés, kombó mód, exkluzív (bérelt)
  poolok, bányászok indítása/leállítása, összeomlás-számláló és automatikus letiltás,
  elakadás-felismerés, hashrate-statisztikák, `Stats/Totals`, egyenlegek és bevétel-követés.
* **Eszközök:** OpenCL-felismerés (dinamikusan betöltve), nvidia-smi/sysfs tartalékok, CPU
  (CPUID, magok/szálak, topológia), eszközkiválasztás (`Data/devices.json`), kombók,
  szenzoradatok, virtuális gép felismerése.
* **Bányászprogramok kezelése:** letöltés (sha256-ellenőrzés, zip/tar.gz/7z, mentések),
  CUDA-verzió szerinti build-választás, indítás saját folyamatcsoportban, prioritás/niceness,
  CPU-affinitás (Linux), naplófájl, szabályos leállítás, idegen bányászfolyamatok felismerése.
* **Bányász-modulok: mind a 108** a `Miners/*.ps1`-ből, valamint a `customminers.config.txt`
  egyedi bányászai. A parancstáblák az eredeti fájlokból vannak kinyerve
  (`assets/miners/*.json`); a közös logikát (Invoke-MinerFamily, VRAM- és DAG-ellenőrzés,
  CUDA-választás, portok, eszközlisták) a `src/miners/family.rs` valósítja meg.
* **Miner API-k:** Trex, Gminer, lolMiner, Rigel, BzMiner, OneZeroMiner, SrbMinerMulti, XMRig6
  (és XmrigCC), Ccminer, Cast, Claymore, CryptoDredge, Dstm, Dynexsolve, EnemyZ, Ethminer,
  MiniZ, Nanominer, Nheq, RH, SrbMiner, TBMiner, Xgminer és a Wrapper-változatok
  (konzolkimenet-feldolgozás).
* **Pool-modulok: mind a 66** a `Pools/*.ps1`-ből, plusz Userpools (`userpools.config.txt`) és
  WhatToMine (árforrás). A MiningRigRentals modulból a bérléshez szükséges rész van portolva:
  aláírt API, a rigek engedélyezése/letiltása dolgozónként, a bérelt rig exkluzív poolként
  (failover szerverekkel), a bérlőnek küldött üzenetek (indulás, pool offline, hosszabbítás,
  nehézség), automatikus hosszabbítás, a bérlések beállításfájljai (`Config/mrr-<id>.config.txt`)
  és az MRR-szerverek „pingelése”.
* **Egyenleg-modulok (7/46):** 2Miners, HeroMiners, MoneroOcean, Nanopool, NiceHash (aláírt API),
  unMineable, Zpool.
* **HTTP API és web-dashboard:** a `web/` mappa kiszolgálása, a dashboard JSON-végpontjai
  (`/activeminers`, `/runningminers`, `/pools`, `/miners`, `/devices`, `/balances`, `/rates`,
  `/currentprofit`, `/stats`, `/console`, ...), parancsok (`/stop`, `/pause`, `/lockminers`,
  `/updatebalance`, `/watchdogreset`, ...), Basic-auth és IP-szűrés.
* **Hálózat:** gyorsítótárazott REST-kérések (`Cache/`), proxy-beállítás (vagy a rendszer
  `HTTPS_PROXY` változója), az api.rbminer.net kérései az eredetivel azonos `X-RBM-Client` és
  `X-RBM-Auth` fejléccel (enélkül az adat-API 401-et ad), árfolyamok, adatbázis-frissítés.

### Ami (még) nincs portolva

* a MiningRigRentals automatikus rig-létrehozása, ár- és rig-frissítése, rig-csoportjai és
  automatikus benchmarkja (`EnableAutoCreate`, `EnableAutoUpdate`, `EnableAutoPrice`,
  `EnableAutoBenchmark` – bekapcsolva figyelmeztetést ír a napló);
* Server/Client mód, távoli gépek;
* tuningolás: OC-profilok, MSI Afterburner, OC-daemon, EthPill;
* interaktív telepítő (`-Setup`), automatikus frissítés, gépszintű újraindítás;
* a **donation run** (a fejlesztőnek bányászott napi percek) szándékosan nincs portolva;
* fastlane benchmark, benchmark-átvétel új eszközkészletre, pool-latencia preferencia,
  `PowerPriceApi`, szünet aktivitásra/akkumulátorra, MinerStatus-jelentések;
* a többi 39 egyenleg-modul; a BzMinerWrapper konzol-táblás tartaléka;
* Windows alatt a CPU-affinitás nem kerül alkalmazásra.

### Eltérések az eredetitől

Ahol az eredeti kód nyilvánvalóan hibás, a port a szándékolt működést valósítja meg:

* Teamred: a `DevFee = 0-75` elírás helyett 0,75 %; SgminerFancyIX: a navi/nem-navi eszközök
  szétválasztása úgy, ahogy a kód szándéka;
* EpicMine: `stratum+tsl` helyett `stratum+tcp`; MinerRocks (DERO solo): a nem létező
  `$Stat.Difficulty` helyett az átlagos nehézség; ViaBTC: a fix `"ETC-PaymentMode"` szöveg helyett
  a pool azonos nevű mezője dönt a díjról; Hashpool, YadaMiners: a definiálatlan `$Pool_User`
  helyett a tárca; LeafPool: a portlista minden portjára külön pool.

Ahol a viselkedés furcsa, de nem egyértelműen hiba, a port hű marad az eredetihez: WildRig és
SrbMinerMulti nem ad `--worker` paramétert, a TTminer a pool `User` mezőjét használja, a
Teamblack minden parancsnál beállítja a verthash-előfeltételt, a SuprNova SSL-portjainál
a protokoll `ssl`, a SupportXmr az összes lekért blokkot számolja, a 2MinersSolo csak
`AllowZero` mellett ad poolt (ahogy a beállítóprogram javasolja a solo poolokhoz).

### Tesztek

```bash
cargo test                                  # unit tesztek
cargo test --features gui --bin rainbowminer-gui
cargo test miner_dump -- --nocapture        # minden bányász-modul első parancssora egy minta-gépen
cargo test pools_live -- --ignored --nocapture   # minden pool-modul az élő API-kkal
RBM_TEST_POOLS=Zpool,Kryptex cargo test pools_live -- --ignored --nocapture
```

A unit tesztek a számítási részeket fedik (statisztika, verziók, hashrate-konverzió, konfig-
helyettesítés, eszközkiválasztás, kombinációkeresés, paraméter-összefésülés, API-segédek,
bányász-családok, MRR- és rbminer.net-aláírás); egy teszt ellenőrzi, hogy minden
`Pools/*.ps1`-nek van Rust-megfelelője. Végponttól végpontig is ki van próbálva egy kamu
bányászprogrammal (benchmark, profitszámítás, váltás, web/API, szünet/folytatás a grafikus
felületről, szabályos leállítás). Az élő pool-teszt szerint néhány pool megszűnt (pl. Ethwmine,
Hellominer, LeafPool, Molepool) – ezek az eredetiben is hibát jeleznek.

---

## English

This folder holds a **Rust port** of RainbowMiner, the PowerShell profit switching mining
manager. It builds two programs:

* `rainbowminer` – the engine that runs the mining (a console program with the web dashboard),
* `rainbowminer-gui` – a graphical user interface that shows and controls a running engine.

The engine runs from an existing RainbowMiner folder and uses its files: `Config/*.txt`,
`Stats/`, `Data/`, `Bin/` (downloaded miners) and the `web/` dashboard. Both versions read and
write the same config and stats files, so you can switch back and forth (run only one of them at
a time).

### Build and run

```bash
cd rust
cargo build --release                  # the engine only
cargo build --release --features gui   # the engine and the GUI
./target/release/rainbowminer -Wallet <BTC address> -WorkerName rig1 -PoolName Zpool,NiceHash
./target/release/rainbowminer-gui
```

For Windows build natively, or cross-compile from Linux with
`rustup target add x86_64-pc-windows-gnu`, the `gcc-mingw-w64-x86-64` package and
`cargo build --release --features gui --target x86_64-pc-windows-gnu`. On Linux the GUI needs
OpenGL (e.g. Mesa), X11 or Wayland and `libxkbcommon-x11-0`, which desktop systems usually have.

The parameters are those of `RainbowMiner.ps1` (`-Name value`, `-Name:value`, switches). The
first start creates `Config/config.txt` and the other config files; changes to them are picked
up in the next round. Like in the original, a `$Name` placeholder in `config.txt` takes the
value of `Data/ConfigDefault.ps1` (e.g. `PoolName = Nicehash,Zpool`) over the command line.
Extra switches: `-Help`, `-Version`, `-ListDevices`, `-ListPools`, `-ListMiners`,
`-ConfigFile <path>`, `-BaseDir <path>`. The dashboard is served on `APIport` (default 4000);
on Linux it listens on all interfaces like the original, so set `APIauth`/`APIuser`/`APIpassword`
or `APIallowIPs` for remote access.

### Graphical user interface

`rainbowminer-gui` talks to the engine's HTTP API (it reads `APIport`, `APIauth`, `APIuser`,
`APIpassword` from `Config/config.txt`, or takes `--host`, `--port`, `--user`, `--password`),
so it can also watch an engine on another machine. It has a dashboard (profit and earnings per
day in BTC and the configured currency, power, uptime, running miners, devices, watchdog
timers), tabs for devices, miners (with a filter), pools, balances, miners that need a
benchmark and the console, and a settings tab. Buttons pause/resume, lock the miners, reset the
watchdog, update the balances, clear the cache, stop the engine (after a confirmation), start it
when it is not running, and open the web dashboard. English and Hungarian texts (from the system
locale, switchable at the top right). Other switches: `--basedir`, `--interval <s>`,
`--lang hu|en`, `--tab <tab>`.

### What is ported

The core loop (Invoke-Core) with config handling, scheduler, pool selection, WhatToMine prices,
compare prices, pool alternates, miner candidates, profits and costs, device combos, exclusive
(rented) pools, watchdog, crash tracking, stall detection, stats and balances; device detection
(OpenCL, nvidia-smi, sysfs, CPUID); miner downloads and process handling; **all 108 miner
modules** (command tables extracted to `assets/miners/*.json`, shared logic in
`src/miners/family.rs`) and the custom miners; the miner APIs (Trex, Gminer, lolMiner, Rigel,
BzMiner, OneZeroMiner, SrbMinerMulti, XMRig6/XmrigCC, Ccminer, Cast, Claymore, CryptoDredge,
Dstm, Dynexsolve, EnemyZ, Ethminer, MiniZ, Nanominer, Nheq, RH, SrbMiner, TBMiner, Xgminer and
the wrappers); **all 66 pool modules** plus Userpools and WhatToMine; 7 of 46 balance modules;
the HTTP API and web dashboard. Requests to rbminer.net carry the `X-RBM-Client` and
`X-RBM-Auth` headers like the original (the data API answers 401 without them).

MiningRigRentals is ported for renting: the signed API, rig availability per worker, rented
rigs as exclusive pools with failover servers, the renter messages (start, pool offline,
extension, difficulty), auto extend, the rental option files and the MRR pings.

### Not ported (yet)

The automatic rig creation, price/rig updates, rig groups and auto benchmark of
MiningRigRentals (a warning is logged when they are enabled), server/client mode, overclocking
(OC profiles, MSI Afterburner, OC daemon, EthPill), the interactive setup, auto update, the
donation run (left out on purpose), fastlane benchmarks, benchmark seeding for new device sets,
pool latency preference, `PowerPriceApi`, pause on activity/battery, MinerStatus reporting,
39 balance modules, the console fallback of BzMinerWrapper and CPU affinity on Windows.

### Differences to the original

Obvious bugs of the original are fixed: Teamred's `DevFee = 0-75` typo (0.75 %), the
SgminerFancyIX navi/non-navi split, EpicMine's `stratum+tsl` protocol, MinerRocks' solo
difficulty (the non-existing `$Stat.Difficulty`), ViaBTC's payment mode (a literal string
instead of the pool field), the undefined wallet variable of Hashpool and YadaMiners and
LeafPool's port list (one pool per port). Behavior that is
odd but not clearly wrong stays as in the original (WildRig/SrbMinerMulti without `--worker`,
TTminer with the pool user, the Teamblack verthash prerequisite, SuprNova's `ssl` protocol,
SupportXmr counting all blocks, 2MinersSolo needing `AllowZero`).

### Tests

`cargo test` runs the unit tests (stats, versions, hashrate conversion, config substitution,
device selection, combo search, params merge, API helpers, miner families, MRR and rbminer.net
signatures, and a check that every `Pools/*.ps1` has a module); `cargo test --features gui --bin
rainbowminer-gui` the GUI tests; `cargo test miner_dump -- --nocapture` prints the first command
line of every miner module for a sample rig; `cargo test pools_live -- --ignored --nocapture`
runs every pool module against the live APIs (`RBM_TEST_POOLS=Name,Name` limits it). The port
was also run end to end with a fake miner (benchmark, profits, switching, web/API, pause/resume
from the GUI, graceful shutdown). Some pools of the original are gone (e.g. Ethwmine,
Hellominer, LeafPool, Molepool) and fail in the original as well.

### Module map

| PowerShell | Rust |
|---|---|
| `RainbowMiner.ps1` parameters | `src/params.rs` |
| `Modules/Core.psm1` | `src/engine.rs`, `src/combos.rs`, `src/display.rs`, `src/balances.rs` |
| `Modules/ConfigLib.psm1`, `Data/*Default.ps1` | `src/config.rs`, `assets/*.json` |
| `Modules/StatLib.psm1` | `src/stat.rs` |
| `Modules/DeviceLib.psm1` | `src/device.rs`, `src/opencl.rs` |
| `Modules/MinersLib.psm1` | `src/miner.rs`, `src/miners/family.rs` |
| `Modules/MinerAPIs.psm1` | `src/minerapi.rs`, `src/minerapi2.rs` |
| `Modules/PoolsLib.psm1`, `Modules/WhatToMineLib.psm1` | `src/pool.rs`, `src/pools/` |
| `Modules/MiningRigRentals.psm1`, `Modules/PingStratum.psm1` | `src/pools/mrr.rs` |
| `Modules/ProcLib.psm1` | `src/proc.rs` |
| `Scripts/Downloader.ps1` | `src/download.rs` |
| `Modules/WebLib.psm1`, `Modules/Asyncloader.psm1` | `src/web.rs`, `src/rates.rs` |
| `Modules/API.psm1`, `Scripts/API.ps1` | `src/api.rs` |
| `Miners/*.ps1` (108 of 108) | `src/miners/`, `assets/miners/*.json` |
| `Pools/*.ps1` (68 of 68) | `src/pools/` |
| `Balances/*.ps1` (7 of 46) | `src/balances.rs` |
| – (new) | `src/gui/` (`rainbowminer-gui`) |
