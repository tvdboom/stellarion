[CmdletBinding()]
param(
    [string]$Target = "",
    [string]$Channel = "",
    [string]$OutputDirectory = "dist",
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
        try {
            $allowedMask = $process.ProcessorAffinity.ToInt64()
            $limitedMask = [long]0
            $selectedProcessors = 0
            for ($bit = 0; $bit -lt ([IntPtr]::Size * 8) -and $selectedProcessors -lt 12; $bit++) {
                $candidate = [long]1 -shl $bit
                if (($allowedMask -band $candidate) -ne 0) {
                    $limitedMask = $limitedMask -bor $candidate
                    $selectedProcessors++
                }
            }
            if ($limitedMask -ne 0 -and $limitedMask -ne $allowedMask) {
                $process.ProcessorAffinity = [IntPtr]$limitedMask
            }
        } catch {
            Write-Warning "Unable to limit processor affinity: $($_.Exception.Message)"
        }
        $process.PriorityClass = "BelowNormal"
    }
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

if (Test-Path -LiteralPath $archive) {
    Remove-Item -LiteralPath $archive -Force
}
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $archive -CompressionLevel Optimal
Write-Host "Created $archive"
