$ErrorActionPreference = "Stop"

$RootDirectory = Split-Path -Parent $PSScriptRoot

Set-Location $RootDirectory

$Metadata = cargo metadata `
    --no-deps `
    --format-version 1 `
    | ConvertFrom-Json

$Package = $Metadata.packages `
    | Where-Object {
        $_.name -eq "opsdeck"
    } `
    | Select-Object -First 1

if ($null -eq $Package) {
    throw "No se encontró el paquete opsdeck en Cargo.toml."
}

$Version = $Package.version

$Architecture = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" {
        "x64"
    }

    "ARM64" {
        "arm64"
    }

    default {
        throw "Arquitectura no compatible: $env:PROCESSOR_ARCHITECTURE"
    }
}

if ($env:CARGO_TARGET_DIR) {
    if ([System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        $TargetDirectory = $env:CARGO_TARGET_DIR
    } else {
        $TargetDirectory = Join-Path `
            $RootDirectory `
            $env:CARGO_TARGET_DIR
    }
} else {
    $TargetDirectory = Join-Path $RootDirectory "target"
}

$ReleaseDirectory = Join-Path $TargetDirectory "release"

$CliBinary = Join-Path $ReleaseDirectory "opsdeck.exe"
$DesktopBinary = Join-Path $ReleaseDirectory "opsdeck-desktop.exe"

$DistDirectory = Join-Path $RootDirectory "dist-native"

$PackageName = "OpsDeck-$Version-windows-$Architecture"
$PackageDirectory = Join-Path $DistDirectory $PackageName
$ZipFile = Join-Path $DistDirectory "$PackageName.zip"

Write-Host ""
Write-Host "Compilando OpsDeck $Version para Windows..."
Write-Host ""

cargo build `
    --locked `
    --release `
    --bins

if (-not (Test-Path -LiteralPath $CliBinary)) {
    throw "No se encontró $CliBinary"
}

if (-not (Test-Path -LiteralPath $DesktopBinary)) {
    throw "No se encontró $DesktopBinary"
}

Remove-Item `
    -LiteralPath $PackageDirectory `
    -Recurse `
    -Force `
    -ErrorAction SilentlyContinue

Remove-Item `
    -LiteralPath $ZipFile `
    -Force `
    -ErrorAction SilentlyContinue

New-Item `
    -ItemType Directory `
    -Path $PackageDirectory `
    -Force `
    | Out-Null

Copy-Item `
    -LiteralPath $CliBinary `
    -Destination (
        Join-Path $PackageDirectory "opsdeck.exe"
    )

Copy-Item `
    -LiteralPath $DesktopBinary `
    -Destination (
        Join-Path $PackageDirectory "opsdeck-desktop.exe"
    )

Copy-Item `
    -LiteralPath (
        Join-Path $RootDirectory "packaging\windows\install.ps1"
    ) `
    -Destination (
        Join-Path $PackageDirectory "install.ps1"
    )

Copy-Item `
    -LiteralPath (
        Join-Path $RootDirectory "packaging\windows\uninstall.ps1"
    ) `
    -Destination (
        Join-Path $PackageDirectory "uninstall.ps1"
    )

$ReadmeSource = Join-Path $RootDirectory "README.md"

if (Test-Path -LiteralPath $ReadmeSource) {
    Copy-Item `
        -LiteralPath $ReadmeSource `
        -Destination (
            Join-Path $PackageDirectory "README.md"
        )
}

$Instructions = @"
OPSDECK PARA WINDOWS

INSTALACIÓN

1. Abre PowerShell dentro de esta carpeta.
2. Ejecuta:

   Set-ExecutionPolicy -Scope Process Bypass
   .\install.ps1

La aplicación aparecerá en el menú Inicio y en el escritorio.

CLI:

   opsdeck --version
   opsdeck list

DESINSTALACIÓN

   Set-ExecutionPolicy -Scope Process Bypass
   .\uninstall.ps1

DESINSTALACIÓN COMPLETA, INCLUYENDO DATOS

   .\uninstall.ps1 -PurgeData
"@

Set-Content `
    -LiteralPath (
        Join-Path $PackageDirectory "INSTALACION.txt"
    ) `
    -Value $Instructions `
    -Encoding UTF8

Compress-Archive `
    -Path (
        Join-Path $PackageDirectory "*"
    ) `
    -DestinationPath $ZipFile `
    -CompressionLevel Optimal

Remove-Item `
    -LiteralPath $PackageDirectory `
    -Recurse `
    -Force

Write-Host ""
Write-Host "Paquete Windows generado:"
Write-Host ""
Write-Host "  $ZipFile"
Write-Host ""