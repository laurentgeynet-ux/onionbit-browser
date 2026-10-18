# Assemble une archive autonome dillorust (Windows).
# Usage : powershell -File packaging\package.ps1   (depuis la racine du dépôt)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

$ver  = (Select-String -Path crates/dillo/Cargo.toml -Pattern '^version = "(.*)"' |
         Select-Object -First 1).Matches.Groups[1].Value
$name = "onionbit-browser-$ver-windows-x86_64"
$out  = "dist/$name"

cargo build --release --workspace

Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$out/bin", "$out/dpi", "$out/etc/dillo" | Out-Null

Copy-Item target/release/onionbit-browser.exe, target/release/dilloc.exe "$out/bin/"
Copy-Item target/release/dillo-dpid.exe  "$out/bin/dpid.exe"
Copy-Item target/release/dillo-dpidc.exe "$out/bin/dpidc.exe"

# Layout dpid : dpi/<service>/<service>.dpi.exe (install-dpi-local amont)
foreach ($svc in 'bookmarks','cookies','downloads','file','ftp','datauri','vsource','hello') {
    New-Item -ItemType Directory -Force "$out/dpi/$svc" | Out-Null
    $leaf = if ($svc -in 'ftp','datauri','vsource','hello') { "$svc.filter.dpi.exe" } else { "$svc.dpi.exe" }
    Copy-Item "target/release/dpi-$svc.exe" "$out/dpi/$svc/$leaf"
}

Copy-Item packaging/dillorc "$out/etc/dillo/dillorc"
Set-Content "$out/etc/dillo/dpidrc" ("# dpidrc - dpi_dir systeme (adapter au chemin d''installation)`n# dpi_dir=C:\Program Files\{0}\dpi" -f $name)
Copy-Item packaging/README.md "$out/README.txt"

New-Item -ItemType Directory -Force dist | Out-Null
Compress-Archive -Path $out -DestinationPath "dist/$name.zip" -Force
Write-Output "dist/$name.zip"
