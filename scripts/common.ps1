# Shared checked commands and resource limits for packaging and resolver builds.
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

