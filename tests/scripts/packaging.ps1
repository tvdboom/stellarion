# Run with: powershell -NoProfile -ExecutionPolicy Bypass -File tests/scripts/packaging.ps1
$ErrorActionPreference = 'Stop'
$repository = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
. (Join-Path $repository 'scripts/common.ps1')

Assert-PackagePath -Repository $repository -Path (Join-Path $repository 'dist/nested/stellarion-windows')
foreach ($channel in @('../../outside', '..\outside', 'channel/name')) {
    $rejected = $false
    try { & (Join-Path $repository 'scripts/package-native.ps1') -Channel $channel -Target 'test' -SkipBuild }
    catch { $rejected = $_.Exception.Message -like 'Package channel must*' }
    if (-not $rejected) { throw "Accepted unsafe package channel: $channel" }
}
foreach ($relative in @('dist/../../outside', 'dist-neighbor/stellarion', 'dist/stellarion-a/../../outside')) {
    $rejected = $false
    try { Assert-PackagePath -Repository $repository -Path (Join-Path $repository $relative) }
    catch { $rejected = $true }
    if (-not $rejected) { throw "Accepted unsafe package path: $relative" }
}

# A junction beneath dist must not redirect cleanup to another directory.
$testRoot = Join-Path $repository ('target/packaging-verification-' + [guid]::NewGuid().ToString('N'))
$fixture = Join-Path $testRoot 'repository'
$outside = Join-Path $testRoot 'outside'
New-Item -ItemType Directory -Path (Join-Path $fixture 'dist'), $outside -Force | Out-Null
$link = Join-Path $fixture 'dist/linked'
try {
    New-Item -ItemType Junction -Path $link -Target $outside | Out-Null
    $rejected = $false
    try { Assert-PackagePath -Repository $fixture -Path (Join-Path $link 'package') }
    catch { $rejected = $true }
    if (-not $rejected) { throw 'Accepted a package destination through a junction' }
} finally {
    # Remove the junction itself before recursively removing our verified disposable fixture.
    if (Test-Path -LiteralPath $link) { [System.IO.Directory]::Delete($link) }
    $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
    $expectedPrefix = [System.IO.Path]::GetFullPath((Join-Path $repository 'target')) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $resolvedTestRoot.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing to delete a fixture outside target'
    }
    Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
}
Write-Output 'PowerShell packaging path checks passed.'
