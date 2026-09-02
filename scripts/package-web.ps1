[CmdletBinding()]
param(
    [string]$OutputDirectory = "dist",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$repository = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$outputRoot = [System.IO.Path]::GetFullPath((Join-Path $repository $OutputDirectory))
$stage = Join-Path $outputRoot "stellarion-html"
$archive = Join-Path $outputRoot "stellarion-html.zip"
$wasmBindgenVersion = (Get-Content (Join-Path $PSScriptRoot "wasm-bindgen-version.txt") -Raw).Trim()

if (-not (Test-Path -LiteralPath (Join-Path $repository "assets-runtime/.stellarion-assets") -PathType Leaf)) {
    throw "Runtime assets are missing. Install KTX-Software 4.x and run 'just assets' before packaging."
}

. (Join-Path $PSScriptRoot "common.ps1")

function Reset-Stage {
    $expectedRoot = [System.IO.Path]::GetFullPath((Join-Path $repository "dist"))
    $expectedPrefix = $expectedRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    if ($outputRoot -ne $expectedRoot -and -not $outputRoot.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to clean output outside $expectedRoot"
    }
    if (Test-Path -LiteralPath $stage) {
        Remove-Item -LiteralPath $stage -Recurse -Force
    }
    New-Item -ItemType Directory -Path $stage -Force | Out-Null
}

Set-Location $repository
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
Reset-Stage

if (-not $SkipBuild) {
    Set-HeavyProcessLimits
    Invoke-Checked { cargo build --profile wasm-release --target wasm32-unknown-unknown --bin stellarion -j12 }
}

$installedVersion = (& wasm-bindgen --version 2>$null) -replace '^wasm-bindgen ', ''
if ($LASTEXITCODE -ne 0 -or $installedVersion.Trim() -ne $wasmBindgenVersion) {
    throw "wasm-bindgen-cli $wasmBindgenVersion is required. Install it with: cargo install wasm-bindgen-cli --version $wasmBindgenVersion --locked"
}

Invoke-Checked {
    wasm-bindgen "target/wasm32-unknown-unknown/wasm-release/stellarion.wasm" `
        --target web `
        --out-dir $stage `
        --out-name stellarion `
        --no-typescript
}

Copy-Item -LiteralPath (Join-Path $repository "web/index.html") -Destination $stage
Copy-Item -LiteralPath (Join-Path $repository "assets-runtime") -Destination $stage -Recurse
Copy-Item -LiteralPath (Join-Path $repository "LICENSE") -Destination $stage
Copy-Item -LiteralPath (Join-Path $repository "README.md") -Destination $stage

if (Test-Path -LiteralPath $archive) {
    Remove-Item -LiteralPath $archive -Force
}
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $archive -CompressionLevel Optimal
Write-Host "Created $archive"
