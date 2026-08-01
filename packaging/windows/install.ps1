param(
    [switch]$NoDesktopShortcut
)

$ErrorActionPreference = "Stop"

$SourceDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path

$CliSource = Join-Path $SourceDirectory "opsdeck.exe"
$DesktopSource = Join-Path $SourceDirectory "opsdeck-desktop.exe"
$UninstallSource = Join-Path $SourceDirectory "uninstall.ps1"

$InstallDirectory = Join-Path $env:LOCALAPPDATA "Programs\OpsDeck"

$CliDestination = Join-Path $InstallDirectory "opsdeck.exe"
$DesktopDestination = Join-Path $InstallDirectory "opsdeck-desktop.exe"
$UninstallDestination = Join-Path $InstallDirectory "uninstall.ps1"

if (-not (Test-Path -LiteralPath $CliSource)) {
    throw "No se encontró $CliSource"
}

if (-not (Test-Path -LiteralPath $DesktopSource)) {
    throw "No se encontró $DesktopSource"
}

New-Item `
    -ItemType Directory `
    -Path $InstallDirectory `
    -Force `
    | Out-Null

Copy-Item `
    -LiteralPath $CliSource `
    -Destination $CliDestination `
    -Force

Copy-Item `
    -LiteralPath $DesktopSource `
    -Destination $DesktopDestination `
    -Force

Copy-Item `
    -LiteralPath $UninstallSource `
    -Destination $UninstallDestination `
    -Force

$Shell = New-Object -ComObject WScript.Shell

$ProgramsDirectory = [Environment]::GetFolderPath(
    [Environment+SpecialFolder]::Programs
)

$StartMenuDirectory = Join-Path $ProgramsDirectory "OpsDeck"

New-Item `
    -ItemType Directory `
    -Path $StartMenuDirectory `
    -Force `
    | Out-Null

$StartMenuShortcutPath = Join-Path $StartMenuDirectory "OpsDeck.lnk"
$StartMenuShortcut = $Shell.CreateShortcut($StartMenuShortcutPath)

$StartMenuShortcut.TargetPath = $DesktopDestination
$StartMenuShortcut.WorkingDirectory = $InstallDirectory
$StartMenuShortcut.Description = "Centro de control local para proyectos"
$StartMenuShortcut.IconLocation = "$DesktopDestination,0"
$StartMenuShortcut.Save()

if (-not $NoDesktopShortcut) {
    $DesktopDirectory = [Environment]::GetFolderPath(
        [Environment+SpecialFolder]::DesktopDirectory
    )

    $DesktopShortcutPath = Join-Path $DesktopDirectory "OpsDeck.lnk"
    $DesktopShortcut = $Shell.CreateShortcut($DesktopShortcutPath)

    $DesktopShortcut.TargetPath = $DesktopDestination
    $DesktopShortcut.WorkingDirectory = $InstallDirectory
    $DesktopShortcut.Description = "Centro de control local para proyectos"
    $DesktopShortcut.IconLocation = "$DesktopDestination,0"
    $DesktopShortcut.Save()
}

$CurrentUserPath = [Environment]::GetEnvironmentVariable(
    "Path",
    "User"
)

$PathEntries = @(
    $CurrentUserPath `
        -split ";" `
        | Where-Object {
            -not [string]::IsNullOrWhiteSpace($_)
        }
)

$AlreadyInPath = $false

foreach ($Entry in $PathEntries) {
    if (
        [string]::Equals(
            $Entry.TrimEnd("\"),
            $InstallDirectory.TrimEnd("\"),
            [StringComparison]::OrdinalIgnoreCase
        )
    ) {
        $AlreadyInPath = $true
        break
    }
}

if (-not $AlreadyInPath) {
    $UpdatedPath = @(
        $PathEntries
        $InstallDirectory
    ) -join ";"

    [Environment]::SetEnvironmentVariable(
        "Path",
        $UpdatedPath,
        "User"
    )
}

if (
    -not (
        $env:Path `
            -split ";" `
            | Where-Object {
                [string]::Equals(
                    $_.TrimEnd("\"),
                    $InstallDirectory.TrimEnd("\"),
                    [StringComparison]::OrdinalIgnoreCase
                )
            }
    )
) {
    $env:Path = "$env:Path;$InstallDirectory"
}

Write-Host ""
Write-Host "OpsDeck fue instalado correctamente."
Write-Host ""
Write-Host "Aplicación:"
Write-Host "  $DesktopDestination"
Write-Host ""
Write-Host "CLI:"
Write-Host "  $CliDestination"
Write-Host ""
Write-Host "Puedes abrir OpsDeck desde el menú Inicio."
Write-Host ""
Write-Host "Abre una nueva terminal antes de usar:"
Write-Host ""
Write-Host "  opsdeck --version"
Write-Host ""