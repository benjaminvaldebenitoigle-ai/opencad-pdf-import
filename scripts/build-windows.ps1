$ErrorActionPreference = "Stop"

$requiredVersion = "rustc 1.98.1 (48a229cea 2026-09-01)"
$toolchain = "1.98.1-x86_64-pc-windows-msvc"
$actualVersion = (& rustc "+$toolchain" --version).Trim()

if ($actualVersion -ne $requiredVersion) {
    throw "Rust incompatible. Se requiere '$requiredVersion' y se encontro '$actualVersion'."
}

cargo "+$toolchain" build --release --locked
if ($LASTEXITCODE -ne 0) {
    throw "La compilacion fallo."
}

$projectRoot = Split-Path -Parent $PSScriptRoot
$packageDir = Join-Path $projectRoot "dist\opencad.pdf_import"
New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot "plugin.toml") -Destination $packageDir -Force
Copy-Item -LiteralPath (Join-Path $projectRoot "target\release\opencad_pdf_import.dll") `
    -Destination (Join-Path $packageDir "opencad.pdf_import-windows-x86_64.dll") -Force

$zipPath = Join-Path $projectRoot "dist\opencad.pdf_import-windows-x86_64.zip"
if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath
}
Compress-Archive -Path "$packageDir\*" -DestinationPath $zipPath
Write-Output "Paquete creado: $zipPath"
