$ErrorActionPreference = 'Stop'
$buildScript = Join-Path $PSScriptRoot '../../scripts/build-windows.ps1'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('nid-build-test-' + [Guid]::NewGuid().ToString('N'))
$flagVariables = @('CARGO_ENCODED_RUSTFLAGS', 'RUSTFLAGS', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS')
$previousEnvironment = @{}
foreach ($name in $flagVariables) { $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name) }
$global:NidBuildTestState = @{
    root = $testRoot
    flags = @{}
    arguments = @()
    configuration = ''
    architecture = '8664'
    dependencies = @('KERNEL32.dll', 'api-ms-win-crt-runtime-l1-1-0.dll')
    failBuild = $false
}

function Get-Command {
    param($Name, $ErrorAction)
    if ($Name -eq 'dumpbin.exe') { return @{ Source = 'Invoke-TestDumpbin' } }
    throw "Unexpected command lookup: $Name"
}
function cargo {
    if ($args[0] -eq 'metadata') {
        $global:LASTEXITCODE = 0
        return @{ target_directory = (Join-Path $global:NidBuildTestState.root 'custom target') } | ConvertTo-Json
    }
    $global:NidBuildTestState.arguments = $args
    $configIndex = [Array]::IndexOf($args, '--config')
    $global:NidBuildTestState.configuration = if ($configIndex -ge 0) { Get-Content -LiteralPath $args[$configIndex + 1] -Raw } else { '' }
    foreach ($name in @('CARGO_ENCODED_RUSTFLAGS', 'RUSTFLAGS', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS')) {
        $global:NidBuildTestState.flags[$name] = [Environment]::GetEnvironmentVariable($name)
    }
    $global:LASTEXITCODE = [int]$global:NidBuildTestState.failBuild
}
function Invoke-TestDumpbin {
    Require ((Get-Content -LiteralPath $args[1] -Raw) -like 'x64 target *') 'Packaged executables must come from the explicit target directory'
    $global:LASTEXITCODE = 0
    if ($args[0] -eq '/HEADERS') { return "            $($global:NidBuildTestState.architecture) machine (x64)" }
    return @('  Image has the following dependencies:') + $global:NidBuildTestState.dependencies
}
function Require($Condition, $Message) { if (-not $Condition) { throw $Message } }
function ExpectFailure($Pattern, $Action) {
    $failure = $null
    try { & $Action } catch { $failure = $_.Exception.Message }
    Require ($failure -like $Pattern) "Expected '$Pattern', got '$failure'"
}
function ClearFlags {
    foreach ($name in @('CARGO_ENCODED_RUSTFLAGS', 'RUSTFLAGS', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS')) {
        Remove-Item "Env:$name" -ErrorAction SilentlyContinue
    }
}

try {
    foreach ($directory in @('scripts', 'windows', 'custom target/x86_64-pc-windows-msvc/release', 'target/release')) {
        New-Item -ItemType Directory -Path (Join-Path $testRoot $directory) -Force | Out-Null
    }
    Copy-Item -LiteralPath $buildScript -Destination (Join-Path $testRoot 'scripts/build-windows.ps1')
    foreach ($file in @('LICENSE', 'windows/README.md', 'scripts/install-windows.ps1')) {
        [IO.File]::WriteAllText((Join-Path $testRoot $file), 'fixture')
    }
    $built = Join-Path $testRoot 'custom target/x86_64-pc-windows-msvc/release'
    [IO.File]::WriteAllText((Join-Path $built 'nid.exe'), 'x64 target engine')
    [IO.File]::WriteAllText((Join-Path $built 'nid-app.exe'), 'x64 target app')
    [IO.File]::WriteAllText((Join-Path $testRoot 'target/release/nid.exe'), 'stale host engine')
    [IO.File]::WriteAllText((Join-Path $testRoot 'target/release/nid-app.exe'), 'stale host app')
    $build = Join-Path $testRoot 'scripts/build-windows.ps1'
    $dist = Join-Path $testRoot 'windows/dist'

    ClearFlags
    New-Item -ItemType Directory -Path (Join-Path $dist 'Assets') -Force | Out-Null
    & $build
    $arguments = $global:NidBuildTestState.arguments
    Require ($arguments[[Array]::IndexOf($arguments, '--target') + 1] -eq 'x86_64-pc-windows-msvc') 'Cargo must build the packaged architecture explicitly'
    $command = " $($arguments -join ' ') "
    foreach ($part in @(' build ', ' --locked ', ' --release ', ' -p nid-kvm ', ' -p nid-app ', ' --bin nid ', ' --bin nid-app ')) {
        Require ($command.Contains($part)) "Cargo must build the engine and the app with '$($part.Trim())'"
    }
    Require ($global:NidBuildTestState.configuration -eq 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]') 'Without environment overrides, Cargo must merge the target flags'
    $expected = @('Nid.App.exe', 'nid.exe', 'install-windows.ps1', 'README.md', 'LICENSE')
    $files = @(Get-ChildItem -LiteralPath $dist -Name)
    Require ($files.Count -eq $expected.Count -and -not ($expected | Where-Object { $files -cnotcontains $_ })) "The distributable must hold exactly $($expected -join ', '), not $($files -join ', ')"
    Require ((Get-Content -LiteralPath (Join-Path $dist 'Nid.App.exe') -Raw) -eq 'x64 target app') 'Nid.App.exe must be the target build of nid-app'
    Require ((Get-Content -LiteralPath (Join-Path $dist 'nid.exe') -Raw) -eq 'x64 target engine') 'nid.exe must be the target build of the engine'

    foreach ($name in $flagVariables) {
        ClearFlags
        $separator = if ($name -eq 'CARGO_ENCODED_RUSTFLAGS') { [string][char]31 } else { ' ' }
        $original = '-C' + $separator + 'target-feature=-crt-static' + $separator + '--cfg=existing_flag'
        [Environment]::SetEnvironmentVariable($name, $original)
        & $build
        Require ($global:NidBuildTestState.flags[$name] -eq "$original${separator}-C${separator}target-feature=+crt-static") 'Existing Rust flags must remain intact, with static CRT taking precedence'
        Require ([Environment]::GetEnvironmentVariable($name) -eq $original) 'Build flags must be restored afterwards'
    }

    $env:CARGO_ENCODED_RUSTFLAGS = '--cfg=highest_priority'
    $env:RUSTFLAGS = '--cfg=lower_priority'
    & $build
    Require ($global:NidBuildTestState.flags.CARGO_ENCODED_RUSTFLAGS.EndsWith('target-feature=+crt-static')) 'The highest-priority Cargo flag source must get the static CRT flag'
    Require ($global:NidBuildTestState.flags.RUSTFLAGS -eq '--cfg=lower_priority') 'Lower-priority flags must remain unchanged'
    $global:NidBuildTestState.failBuild = $true
    ExpectFailure '*Rust build failed*' { & $build }
    Require ($env:CARGO_ENCODED_RUSTFLAGS -eq '--cfg=highest_priority') 'Flags must also be restored after a failed build'
    $global:NidBuildTestState.failBuild = $false

    ClearFlags
    $global:NidBuildTestState.architecture = 'AA64'
    ExpectFailure '*Nid.App.exe must be Windows x64*' { & $build }
    $global:NidBuildTestState.architecture = '8664'
    $global:NidBuildTestState.dependencies += '    VCRUNTIME140.dll'
    ExpectFailure '*Nid.App.exe requires a Visual C++ runtime DLL*' { & $build }
    $global:NidBuildTestState.dependencies = @('KERNEL32.dll')

    Remove-Item -LiteralPath (Join-Path $built 'nid-app.exe')
    ExpectFailure '*Build output is missing*nid-app.exe' { & $build }
    Write-Output 'Windows build checks passed.'
} finally {
    foreach ($name in $previousEnvironment.Keys) {
        if ($null -eq $previousEnvironment[$name]) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name]) }
    }
    Remove-Variable NidBuildTestState -Scope Global
    Remove-Item -LiteralPath $testRoot -Recurse -Force
}
