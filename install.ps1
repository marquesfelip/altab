<#
.SYNOPSIS
  Installs (or uninstalls) altab, an Alt+Tab replacement for Windows.
  Instala (ou desinstala) o altab, um substituto do Alt+Tab para Windows.

.EXAMPLE
  irm https://raw.githubusercontent.com/marquesfelip/altab/main/install.ps1 | iex

.EXAMPLE
  & ([scriptblock]::Create((irm https://raw.githubusercontent.com/marquesfelip/altab/main/install.ps1))) -Uninstall

.PARAMETER Version
  Release to install, e.g. "0.1.0". Defaults to the latest one.

.PARAMETER Uninstall
  Removes altab (including its start-with-Windows entry).
#>
[CmdletBinding()]
param(
    [string]$Version = 'latest',
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$repo = 'marquesfelip/altab'
$pt = (Get-UICulture).Name -like 'pt*'
function T([string]$en, [string]$ptText) { if ($pt) { $ptText } else { $en } }

$installDir = Join-Path $env:LOCALAPPDATA 'Programs\altab'

if ($Uninstall) {
    $unins = Join-Path $installDir 'unins000.exe'
    if (-not (Test-Path $unins)) {
        Write-Host (T 'altab is not installed.' 'O altab não está instalado.')
        return
    }
    Write-Host (T 'Removing altab...' 'Removendo o altab...')
    # WaitForExit (e não -Wait): -Wait também espera os processos filhos.
    $u = Start-Process -FilePath $unins -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -PassThru
    $u.WaitForExit()
    Write-Host (T 'altab was removed.' 'O altab foi removido.')
    return
}

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64' -and $env:PROCESSOR_ARCHITEW6432 -ne 'AMD64') {
    throw (T 'altab requires 64-bit Windows (x64).' 'O altab requer Windows 64 bits (x64).')
}

$api = if ($Version -eq 'latest') {
    "https://api.github.com/repos/$repo/releases/latest"
} else {
    "https://api.github.com/repos/$repo/releases/tags/v$($Version.TrimStart('v'))"
}

Write-Host (T 'Looking up the release...' 'Procurando a versão...')
$release = Invoke-RestMethod -Uri $api -Headers @{ 'User-Agent' = 'altab-installer' }
$setup = $release.assets | Where-Object { $_.name -eq 'altab-setup.exe' } | Select-Object -First 1
$sums = $release.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
if (-not $setup -or -not $sums) {
    throw (T 'The release does not contain the expected files.' 'A versão não contém os arquivos esperados.')
}

$work = Join-Path ([IO.Path]::GetTempPath()) ("altab-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $exe = Join-Path $work 'altab-setup.exe'
    Write-Host (T "Downloading altab $($release.tag_name)..." "Baixando o altab $($release.tag_name)...")
    Invoke-WebRequest -Uri $setup.browser_download_url -OutFile $exe -UseBasicParsing
    $sumText = (Invoke-WebRequest -Uri $sums.browser_download_url -UseBasicParsing).Content
    if ($sumText -is [byte[]]) { $sumText = [Text.Encoding]::UTF8.GetString($sumText) }

    $expected = $null
    foreach ($line in ($sumText -split "`n")) {
        if ($line -match '^\s*([0-9a-fA-F]{64})\s+\*?altab-setup\.exe\s*$') { $expected = $Matches[1]; break }
    }
    $actual = (Get-FileHash -Algorithm SHA256 -Path $exe).Hash
    if (-not $expected -or $expected -ne $actual) {
        throw (T 'Checksum mismatch: the download is corrupted or was tampered with. Aborting.' `
                 'Checksum não confere: o download está corrompido ou foi adulterado. Abortando.')
    }

    Write-Host (T 'Installing...' 'Instalando...')
    # WaitForExit (e não -Wait): o instalador inicia o altab, e -Wait esperaria por ele também.
    $p = Start-Process -FilePath $exe -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -PassThru
    $p.WaitForExit()
    if ($p.ExitCode -ne 0) {
        throw (T "The installer failed (exit code $($p.ExitCode))." "O instalador falhou (código $($p.ExitCode)).")
    }
    Write-Host (T 'altab is installed and will start with Windows. Press Alt+Tab to try it!' `
                  'O altab foi instalado e iniciará com o Windows. Pressione Alt+Tab para testar!')
}
finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
