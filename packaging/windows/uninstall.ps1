param(
    [switch]$PurgeData
)

$ErrorActionPreference = "Stop"

$InstallDirectory = Join-Path $env:LOCALAPPDATA "Programs\OpsDeck"
$DataDirectory = Join-Path $env:LOCALAPPDATA "OpsDeck"

$ProgramsDirectory = [Environment]::GetFolderPath(
    [Environment+SpecialFolder]::Programs
)

$StartMenuDirectory = Join-Path $ProgramsDirectory "OpsDeck"

$DesktopDirectory = [Environment]::GetFolderPath(
    [Environment+SpecialFolder]::DesktopDirectory
)

$DesktopShortcut = Join-Path $DesktopDirectory "OpsDeck.lnk"

Remove-Item `
    -LiteralPath $DesktopShortcut `
    -Force `
    -ErrorAction SilentlyContinue

Remove-Item `
    -LiteralPath $StartMenuDirectory `
    -Recurse `
    -Force `
    -ErrorAction SilentlyContinue

$CurrentUserPath = [Environment]::GetEnvironmentVariable(
    "Path",
    "User"
)

$UpdatedEntries = @(
    $CurrentUserPath `
        -split ";" `
        | Where-Object {
            -not [string]::IsNullOrWhiteSpace($_) `
            -and -not [string]::Equals(
                $_.TrimEnd("\"),
                $InstallDirectory.TrimEnd("\"),
                [StringComparison]::OrdinalIgnoreCase
            )
        }
)

[Environment]::SetEnvironmentVariable(
    "Path",
    ($UpdatedEntries -join ";"),
    "User"
)

if ($PurgeData) {
    Remove-Item `
        -LiteralPath $DataDirectory `
        -Recurse `
        -Force `
        -ErrorAction SilentlyContinue
}

Set-Location $env:TEMP

Remove-Item `
    -LiteralPath $InstallDirectory `
    -Recurse `
    -Force `
    -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "OpsDeck fue desinstalado."
Write-Host ""

if ($PurgeData) {
    Write-Host "También se eliminaron los datos:"
    Write-Host "  $DataDirectory"
} else {
    Write-Host "Los proyectos, políticas e historial se conservaron:"
    Write-Host "  $DataDirectory"
}

Write-Host ""