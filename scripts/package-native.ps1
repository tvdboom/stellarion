[CmdletBinding()]
param(
    [string]$Target = "",
    [string]$Channel = "",
    [string]$OutputDirectory = "dist",
    [string]$ConfigPath = "",
    [string]$BinaryPath = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$repository = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$outputRoot = [System.IO.Path]::GetFullPath((Join-Path $repository $OutputDirectory))

if (-not $Target) {
    $Target = (rustc -vV | Select-String '^host: ' | ForEach-Object { $_.Line.Substring(6) }).Trim()
}
if (-not $Channel) {
    $Channel = if ($Target -match 'windows') { 'windows' } elseif ($Target -match 'apple-darwin') { 'mac' } else { 'linux' }
}
$stage = Join-Path $outputRoot "stellarion-$Channel"
$archive = Join-Path $outputRoot "stellarion-$Channel.zip"

function Invoke-Checked {
    param([scriptblock]$Command)
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code $LASTEXITCODE"
    }
}

function Set-HeavyProcessLimits {
    if ($env:OS -eq "Windows_NT") {
        $process = Get-Process -Id $PID
        $process.ProcessorAffinity = [IntPtr]0x0FFF
        $process.PriorityClass = "BelowNormal"
    }
}

function Assert-PublishableKey {
    param([Parameter(Mandatory = $true)][string]$Key)

    $normalized = $Key.Trim()
    if ([string]::IsNullOrWhiteSpace($normalized)) {
        throw "Supabase publishable key is empty"
    }
    if ($normalized.StartsWith("sb_secret_", [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to package a Supabase secret key"
    }

    $segments = $normalized.Split('.')
    if ($segments.Count -eq 3) {
        $payload = $segments[1].Replace('-', '+').Replace('_', '/')
        $remainder = $payload.Length % 4
        if ($remainder -ne 1) {
            if ($remainder -gt 0) {
                $payload += "=" * (4 - $remainder)
            }
            $claims = $null
            try {
                $json = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($payload))
                $claims = $json | ConvertFrom-Json -ErrorAction Stop
            } catch {
                $claims = $null
            }
            if ($null -ne $claims -and [string]$claims.role -ieq "service_role") {
                throw "Refusing to package a legacy Supabase service-role key"
            }
        }
    }
}

function Assert-PublicConfigFile {
    param([Parameter(Mandatory = $true)][string]$Path)

    try {
        $config = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        throw "Supabase configuration is not valid JSON: $Path"
    }
    Assert-PublishableKey -Key ([string]$config.publishable_key)
}

$resolvedConfigPath = ""
$repositoryConfigPath = Join-Path $repository "stellarion-config.json"
$exampleConfigPath = Join-Path $repository "stellarion-config.example.json"
if ($ConfigPath) {
    $resolvedConfigPath = (Resolve-Path -LiteralPath $ConfigPath).Path
    Assert-PublicConfigFile -Path $resolvedConfigPath
} elseif ($env:SUPABASE_URL -or $env:SUPABASE_PUBLISHABLE_KEY) {
    if (-not $env:SUPABASE_URL -or -not $env:SUPABASE_PUBLISHABLE_KEY) {
        throw "SUPABASE_URL and SUPABASE_PUBLISHABLE_KEY must be supplied together"
    }
    Assert-PublishableKey -Key $env:SUPABASE_PUBLISHABLE_KEY
} elseif (Test-Path -LiteralPath $repositoryConfigPath) {
    Assert-PublicConfigFile -Path $repositoryConfigPath
} else {
    Assert-PublicConfigFile -Path $exampleConfigPath
}

$expectedRoot = [System.IO.Path]::GetFullPath((Join-Path $repository "dist"))
$expectedPrefix = $expectedRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if ($outputRoot -ne $expectedRoot -and -not $outputRoot.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to clean output outside $expectedRoot"
}
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
if (Test-Path -LiteralPath $stage) {
    Remove-Item -LiteralPath $stage -Recurse -Force
}
New-Item -ItemType Directory -Path $stage -Force | Out-Null

Set-Location $repository
if (-not $SkipBuild) {
    Set-HeavyProcessLimits
    Invoke-Checked { cargo build --release --target $Target --bin stellarion -j12 }
}

if (-not $BinaryPath) {
    $executable = if ($Target -match 'windows') { 'stellarion.exe' } else { 'stellarion' }
    $BinaryPath = Join-Path $repository "target/$Target/release/$executable"
}
Copy-Item -LiteralPath (Resolve-Path $BinaryPath) -Destination $stage
Copy-Item -LiteralPath (Join-Path $repository "assets-runtime") -Destination $stage -Recurse
Copy-Item -LiteralPath (Join-Path $repository "LICENSE") -Destination $stage
Copy-Item -LiteralPath (Join-Path $repository "README.md") -Destination $stage

if ($ConfigPath) {
    Copy-Item -LiteralPath $resolvedConfigPath -Destination (Join-Path $stage "stellarion-config.json")
} elseif ($env:SUPABASE_URL -and $env:SUPABASE_PUBLISHABLE_KEY) {
    @{
        url = $env:SUPABASE_URL
        publishable_key = $env:SUPABASE_PUBLISHABLE_KEY
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $stage "stellarion-config.json") -Encoding utf8
} elseif (Test-Path -LiteralPath $repositoryConfigPath) {
    Copy-Item -LiteralPath $repositoryConfigPath -Destination $stage
} else {
    Copy-Item -LiteralPath $exampleConfigPath -Destination $stage
}

if (Test-Path -LiteralPath $archive) {
    Remove-Item -LiteralPath $archive -Force
}
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $archive -CompressionLevel Optimal
Write-Host "Created $archive"
