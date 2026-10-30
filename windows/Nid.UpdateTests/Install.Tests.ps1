$ErrorActionPreference = 'Stop'
$installer = Join-Path $PSScriptRoot '../../scripts/install-windows.ps1'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('nid-installer-test-' + [Guid]::NewGuid().ToString('N'))
$previousLocalAppData = $env:LOCALAPPDATA
$env:LOCALAPPDATA = Join-Path $testRoot 'profile'
$global:NidInstallerTestState = @{
    assetName = 'nid-windows-x86_64-Setup.exe'
    processCalls = @()
    badChecksum = $false
    missingAsset = $false
    requestedLatest = $false
}

function Invoke-RestMethod {
    param($Uri, $Headers)
    if ($Uri -ne 'https://api.github.com/repos/autistic-computer/nidus/releases/latest') { throw "Unexpected release URL: $Uri" }
    $global:NidInstallerTestState.requestedLatest = $true
    $assets = @(@{ name = 'SHA256SUMS'; browser_download_url = 'https://example.invalid/v1/SHA256SUMS' })
    if (-not $global:NidInstallerTestState.missingAsset) { $assets += @{ name = $global:NidInstallerTestState.assetName; browser_download_url = 'https://example.invalid/v1/setup' } }
    return @{ assets = $assets }
}
function Invoke-WebRequest {
    param($Uri, $OutFile, [switch]$UseBasicParsing)
    if ($Uri -eq 'https://example.invalid/v1/setup') { [IO.File]::WriteAllText($OutFile, 'test installer'); return }
    if ($Uri -ne 'https://example.invalid/v1/SHA256SUMS') { throw "Unexpected asset URL: $Uri" }
    $setupPath = Join-Path (Split-Path -Parent $OutFile) $global:NidInstallerTestState.assetName
    $hash = if ($global:NidInstallerTestState.badChecksum) { '0' * 64 } else { (Get-FileHash $setupPath -Algorithm SHA256).Hash.ToLowerInvariant() }
    [IO.File]::WriteAllText($OutFile, "$hash  $($global:NidInstallerTestState.assetName)`n")
}
function Start-Process {
    param($FilePath, $ArgumentList, [switch]$Wait, [switch]$PassThru)
    $global:NidInstallerTestState.processCalls += @{ file = $FilePath; args = $ArgumentList }
    if ([IO.Path]::GetFileName($FilePath) -eq $global:NidInstallerTestState.assetName) {
        $destination = Join-Path $env:LOCALAPPDATA 'Nid.App'
        New-Item -ItemType Directory -Path $destination -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $destination 'Nid.App.exe'), 'test launcher')
        return @{ ExitCode = 0 }
    }
}
function Get-CimInstance { param($ClassName, $Filter) return @() }
function Require($Condition, $Message) { if (-not $Condition) { throw $Message } }
function ExpectFailure($Pattern, $Action) {
    $failure = $null
    try { & $Action } catch { $failure = $_.Exception.Message }
    Require ($failure -like $Pattern) "Expected '$Pattern', got '$failure'"
}

try {
    New-Item -ItemType Directory -Path $env:LOCALAPPDATA -Force | Out-Null
    & $installer -NoLaunch
    Require $global:NidInstallerTestState.requestedLatest 'The installer must resolve the latest release'
    Require ($global:NidInstallerTestState.processCalls.Count -eq 1) '-NoLaunch must only run the setup program'
    Require ($global:NidInstallerTestState.processCalls[0].args -eq '--silent') 'Setup must return before the script decides whether to launch'

    $global:NidInstallerTestState.processCalls = @(); $global:NidInstallerTestState.badChecksum = $true
    ExpectFailure '*checksum does not match*' { & $installer -NoLaunch }
    Require ($global:NidInstallerTestState.processCalls.Count -eq 0) 'A checksum mismatch must not close the app or execute the download'

    $global:NidInstallerTestState.badChecksum = $false; $global:NidInstallerTestState.missingAsset = $true
    ExpectFailure '*no complete Windows installer*' { & $installer -NoLaunch }
    Require ($global:NidInstallerTestState.processCalls.Count -eq 0) 'An incomplete release must not execute anything'

    $source = Join-Path $testRoot 'local release'
    New-Item -ItemType Directory -Path $source -Force | Out-Null
    ExpectFailure '*installer is missing*' { & $installer -SourceDirectory $source -NoLaunch }
    [IO.File]::WriteAllText((Join-Path $source $global:NidInstallerTestState.assetName), 'local installer')
    $global:NidInstallerTestState.requestedLatest = $false
    & $installer -SourceDirectory $source -NoLaunch
    Require (-not $global:NidInstallerTestState.requestedLatest) 'A local release must not use the network'
    Require ($global:NidInstallerTestState.processCalls[0].args -eq '--quit') 'The installed app must be asked to stop before setup'
    Require ($global:NidInstallerTestState.processCalls[-1].args -eq '--silent') 'Setup must follow app shutdown'
    Write-Output 'Windows installer checks passed.'
} finally {
    Remove-Variable NidInstallerTestState -Scope Global
    $env:LOCALAPPDATA = $previousLocalAppData
    Remove-Item -LiteralPath $testRoot -Recurse -Force
}
