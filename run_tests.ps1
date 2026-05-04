# Run all tests, log do souboru, summary + failure extract.
# Pouziti:  powershell -ExecutionPolicy Bypass -File run_tests.ps1
# Vystup:   test_logs/test-<ts>.log, test_logs/failures-<ts>.log

$ErrorActionPreference = "Continue"

$logDir = "test_logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$ts = Get-Date -Format "yyyyMMdd-HHmmss"
$logFile  = Join-Path $logDir "test-$ts.log"
$failFile = Join-Path $logDir "failures-$ts.log"
$buildFile = Join-Path $logDir "build-$ts.log"

# Hlavicka logu
$header = @"
============================================================
RustWebEngine - test runner
Cas:    $(Get-Date)
Vystup: $logFile
============================================================
"@
$header | Tee-Object -FilePath $logFile

# === BUILD ===
Write-Host ""
Write-Host "=== Build ===" -ForegroundColor Cyan
cargo build --color=never 2>&1 | Tee-Object -FilePath $buildFile
$buildExit = $LASTEXITCODE
Get-Content $buildFile | Add-Content $logFile

if ($buildExit -ne 0) {
    Write-Host ""
    Write-Host "BUILD FAILED (exit $buildExit) - viz $buildFile" -ForegroundColor Red
    Copy-Item $buildFile $failFile
    exit 1
}

# === TESTY ===
Write-Host ""
Write-Host "=== cargo test --no-fail-fast ===" -ForegroundColor Cyan
$testTmp = Join-Path $logDir "_test_tmp.log"
cargo test --no-fail-fast --color=never 2>&1 | Tee-Object -FilePath $testTmp
$testExit = $LASTEXITCODE
Get-Content $testTmp | Add-Content $logFile
Remove-Item $testTmp -ErrorAction SilentlyContinue

# Extract failure regions: radky s "FAILED", "panicked at", a "---- name stdout ----" do failures.log
$failures = @()
$content = Get-Content $logFile
$inFailureBlock = $false
foreach ($line in $content) {
    if ($line -match "^---- .+ stdout ----") {
        $inFailureBlock = $true
        $failures += $line
        continue
    }
    if ($inFailureBlock) {
        if ($line -match "^test result:") {
            $inFailureBlock = $false
            continue
        }
        if ($line.Trim() -eq "" -and $failures.Count -gt 0 -and $failures[-1].Trim() -eq "") {
            $inFailureBlock = $false
            continue
        }
        $failures += $line
    }
    if ($line -match "FAILED|panicked at|assertion") {
        if (-not $inFailureBlock) { $failures += $line }
    }
}
$failures | Out-File -FilePath $failFile -Encoding utf8

# === SUMMARY ===
Write-Host ""
Write-Host "=== Summary ===" -ForegroundColor Cyan
$summaryLines = $content | Select-String -Pattern "^test result:"
foreach ($s in $summaryLines) { Write-Host $s.Line }

# Aggregate failed count
$totalFailed = 0
$totalPassed = 0
foreach ($s in $summaryLines) {
    if ($s.Line -match "(\d+) passed.*?(\d+) failed") {
        $totalPassed += [int]$matches[1]
        $totalFailed += [int]$matches[2]
    }
}

Write-Host ""
Write-Host "Passed celkem: $totalPassed"
Write-Host "Failed celkem: $totalFailed"
Write-Host "Log:           $logFile"
Write-Host "Failures log:  $failFile"

if ($totalFailed -gt 0 -or $testExit -ne 0) {
    Write-Host ""
    Write-Host "FAIL ($totalFailed selhanych testu)" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "OK ($totalPassed testu prosly)" -ForegroundColor Green
exit 0
