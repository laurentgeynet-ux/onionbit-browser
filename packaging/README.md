# Packaging DilloRust

`package.sh` (Linux/Unix) et `package.ps1` (Windows) construisent le
workspace en `--release` puis assemblent une archive autonome :

```
dillorust-<ver>-<os>-<arch>/
  bin/
    dillo          navigateur (dillo.cc)
    dilloc         client de contrôle distant (dilloc.c)
    dpid           démon de plugins (dpid/main.c)
    dpidc          contrôle du démon (dpid/dpidc.c)
  dpi/
    bookmarks/bookmarks.dpi(.exe)
    cookies/cookies.dpi(.exe)
    downloads/downloads.dpi(.exe)
    file/file.dpi(.exe)
    ftp/ftp.filter.dpi(.exe)
    datauri/datauri.filter.dpi(.exe)
    vsource/vsource.filter.dpi(.exe)
    hello/hello.filter.dpi(.exe)
  etc/dillo/
    dillorc        gabarit amont (verbatim `dillorc` du checkout)
    dpidrc         gabarit — `dpi_dir=` à adapter à l'installation
  README.txt       rappel d'installation
```

Les noms et l'arborescence `dpi/` sont **contraintes par le protocole**,
pas cosmétiques :

- `dillo` lance le démon sous le nom nu `dpid` (`~/.dillo/dpid`,
  `<bindir>/dpid`, `PATH` — `Dpi_start_dpid`) : le binaire
  `dillo-dpid` est renommé `dpid` dans le package.
- `dpid` découvre les plugins sous `<dpi_dir>/<service>/*.dpi`
  (`.dpi.exe` accepté sous Windows) ; le nom du service = nom du
  sous-répertoire (≙ `install-dpi-local` amont : `ftp/ftp.filter.dpi`).

## Installation (≙ `install-dpi-local` amont)

```sh
# 1. Exécutables quelque part dans le PATH (ou ~/.dillo/)
cp bin/* ~/.dillo/            # dpid est aussi cherché dans ~/.dillo

# 2. Plugins
cp -r dpi/* ~/.dillo/dpi/

# 3. Config — les défauts sont compilés, dillorc est optionnel
cp etc/dillo/dillorc ~/.dillo/dillorc
#   ou : export DILLO_SYSCONF=<pkg>/etc/dillo
```

Alternative système : `dpi_dir=/chemin/absolu/dpi` dans
`~/.dillo/dpidrc` (ou `etc/dillo/dpidrc` du package — résolu via
`<exe>/../etc/dillo/dpidrc` sous Windows, `/etc/dillo/dpidrc` POSIX).

## Linux

Le build Linux se fait **sur une machine Linux**
(`x86_64-unknown-linux-gnu` est vérifié par `cargo check` en CI
croisée, mais le link exige une toolchain GNU). Sur la cible :

```sh
cargo build --release --workspace
./packaging/package.sh
# → dist/dillorust-<ver>-linux-x86_64.tar.gz
```

Dépendances runtime Linux : X11/Wayland via `winit` + fontes
`fontdb` (fontconfig non requis — `fontdb` scanne les répertoires
standard). Aucune bibliothèque C n'est liée par nos crates.
