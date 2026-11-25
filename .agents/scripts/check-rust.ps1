$ProgressPreference = 'SilentlyContinue'
$WarningPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$rawInput = ""
try {
    $rawInput = [Console]::In.ReadToEnd()
} catch {
    $rawInput = ""
}

$executionNum = 0
if ($rawInput) {
    try {
        $json = $rawInput | ConvertFrom-Json
        if ($json.executionNum) {
            $executionNum = [int]$json.executionNum
        }
    } catch {}
}

if (-not (Test-Path "Cargo.toml") -and (Test-Path "../Cargo.toml")) {
    Set-Location ..
}

if ($executionNum -gt 3 -or -not (Test-Path "Cargo.toml")) {
    [Console]::Out.Write('{"decision":"allow"}')
    exit 0
}

$fmtRaw = & cargo fmt --all --check 2>&1
if ($LASTEXITCODE -ne 0) {
    $response = @{
        decision = "continue"
        reason   = "cargo fmt --check falló: corre cargo fmt."
    } | ConvertTo-Json -Compress
    [Console]::Out.Write($response)
    exit 0
}

$clippyRaw = & cargo clippy --all-targets -- -D warnings 2>&1
$clippyCode = $LASTEXITCODE

if ($clippyCode -ne 0) {
    $output = ($clippyRaw | ForEach-Object { $_.ToString() }) -join "`r`n"
    if ($output.Length -gt 3000) {
        $output = $output.Substring(0, 3000)
    }
    $reason = "cargo clippy falló. Corrige esto: " + $output
    $response = @{
        decision = "continue"
        reason   = $reason
    } | ConvertTo-Json -Compress
    [Console]::Out.Write($response)
    exit 0
}

[Console]::Out.Write('{"decision":"allow"}')
exit 0
