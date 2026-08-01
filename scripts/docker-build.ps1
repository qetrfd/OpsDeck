$ErrorActionPreference = "Stop"

$RootDirectory = Split-Path -Parent $PSScriptRoot

if ($env:OPSDECK_IMAGE) {
    $ImageName = $env:OPSDECK_IMAGE
} else {
    $ImageName = "opsdeck:1.0.0"
}

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    Write-Error "Docker no está instalado o no está disponible en PATH."
    exit 1
}

Write-Host ""
Write-Host "Construyendo la imagen Docker de OpsDeck..."
Write-Host "Imagen: $ImageName"
Write-Host ""

& docker build `
    --pull `
    --tag $ImageName `
    $RootDirectory

if ($LASTEXITCODE -ne 0) {
    Write-Error "No se pudo construir la imagen Docker."
    exit $LASTEXITCODE
}

Write-Host ""
Write-Host "Imagen creada correctamente:"
Write-Host "  $ImageName"
Write-Host ""