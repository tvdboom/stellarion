# Shared checked commands and resource limits for packaging and resolver builds.

# Validate the exact destination before any package cleanup, including existing linked parents.
function Assert-PackagePath {
    param([string]$Repository, [string]$Path)
    $expectedRoot = [System.IO.Path]::GetFullPath((Join-Path $Repository "dist"))
    $expectedPrefix = $expectedRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    $candidate = [System.IO.Path]::GetFullPath($Path)
    if ($candidate -ne $expectedRoot -and -not $candidate.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to clean output outside $expectedRoot"
    }
    $ancestor = $candidate
    while ($ancestor -and $ancestor -ne $Repository) {
        if (Test-Path -LiteralPath $ancestor) {
            $item = Get-Item -LiteralPath $ancestor -Force
            if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                throw "Refusing to package through linked path $ancestor"
            }
        }
        $ancestor = [System.IO.Path]::GetDirectoryName($ancestor)
    }
}

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

