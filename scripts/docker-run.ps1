param(
    [Parameter(
        Position = 0,
        Mandatory = $true
    )]
    [string]$ProjectPath,

    [Parameter(
        Position = 1,
        ValueFromRemainingArguments = $true
    )]
    [string[]]$OpsDeckArguments
)

$ErrorActionPreference = "Stop"

if ($env:OPSDECK_IMAGE) {
    $ImageName = $env:OPSDECK_IMAGE
} else {
    $ImageName = "opsdeck:1.0.0"
}

if ($env:OPSDECK_DATA_VOLUME) {
    $DataVolume = $env:OPSDECK_DATA_VOLUME
} else {
    $DataVolume = "opsdeck-data"
}

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    Write-Error "Docker no está instalado o no está disponible en PATH."
    exit 1
}

try {
    $ResolvedProjectPath = (
        Resolve-Path -LiteralPath $ProjectPath
    ).Path
} catch {
    Write-Error "La carpeta no existe: $ProjectPath"
    exit 1
}

if (
    $null -eq $OpsDeckArguments -or
    $OpsDeckArguments.Count -eq 0
) {
    $OpsDeckArguments = @(
        "status",
        "/workspace"
    )
}

Write-Host ""
Write-Host "Ejecutando OpsDeck con Docker"
Write-Host "Proyecto: $ResolvedProjectPath"
Write-Host "Imagen:   $ImageName"
Write-Host ""

$DockerArguments = @(
    "run",
    "--rm",
    "-it",
    "--add-host",
    "host.docker.internal:host-gateway",
    "--env",
    "HOME=/data",
    "--env",
    "OPSDECK_CONTAINER=1",
    "--mount",
    "type=bind,source=$ResolvedProjectPath,target=/workspace",
    "--mount",
    "type=volume,source=$DataVolume,target=/data/.opsdeck",
    $ImageName
)

$DockerArguments += $OpsDeckArguments

& docker @DockerArguments

if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}