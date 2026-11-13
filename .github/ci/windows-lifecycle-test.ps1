# Disposable Windows CI runner only. Verify manual start with login autostart off.
$ErrorActionPreference = 'Stop'
& cargo build -p localweb-cli -p localweb-daemon
if ($LASTEXITCODE -ne 0) { throw 'Could not build lifecycle fixtures.' }
$cli = (Resolve-Path target/debug/localweb.exe).Path
$daemon = (Resolve-Path target/debug/localwebd.exe).Path
$root = Join-Path $env:RUNNER_TEMP ('localweb-lifecycle-' + [guid]::NewGuid())
$previousData = $env:LOCALAPPDATA
$previousRuntime = $env:LOCALWEB_RUNTIME_DIR
$unrelated = $null
if (Get-ScheduledTask -TaskName 'LocalWeb Daemon' -ErrorAction SilentlyContinue) {
    throw 'Lifecycle fixture requires a fresh runner without a LocalWeb task.'
}
New-Item -ItemType Directory -Path $root | Out-Null
try {
    $env:LOCALAPPDATA = Join-Path $root 'data'
    $env:LOCALWEB_RUNTIME_DIR = Join-Path $root 'runtime'
    & $cli autostart disable
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the disabled login task.' }
    if ((Get-ScheduledTask -TaskName 'LocalWeb Daemon').Settings.Enabled) { throw 'Autostart was not disabled.' }
    $dummy = Join-Path $root 'localwebd.exe'
    Copy-Item (Join-Path ([Environment]::SystemDirectory) 'ping.exe') $dummy
    $unrelated = Start-Process $dummy -ArgumentList @('-t', '127.0.0.1') -WindowStyle Hidden -PassThru
    foreach ($attempt in 1..2) {
        & $cli start
        if ($LASTEXITCODE -ne 0) { throw 'Manual start failed with login autostart off.' }
        $status = (& $cli status --json | ConvertFrom-Json)
        if (-not $status.data.ready) { throw 'The manually started daemon was not ready.' }
        $unrelated.Refresh()
        if ($unrelated.HasExited) { throw 'Restart killed an unrelated same-name process.' }
        if ((Get-ScheduledTask -TaskName 'LocalWeb Daemon').Settings.Enabled) { throw 'Manual start changed the login preference.' }
    }
    & $cli autostart enable
    if ($LASTEXITCODE -ne 0) { throw 'Could not enable login autostart.' }
    & $cli start
    if ($LASTEXITCODE -ne 0) { throw 'Restart failed with autostart enabled.' }
    if (-not (Get-ScheduledTask -TaskName 'LocalWeb Daemon').Settings.Enabled) { throw 'Restart disabled login autostart.' }
    & $cli shutdown
    if ($LASTEXITCODE -ne 0) { throw 'Graceful shutdown failed.' }
    $unrelated.Refresh()
    if ($unrelated.HasExited) { throw 'Shutdown killed an unrelated process.' }
} finally {
    & $cli shutdown 2>$null
    if ($unrelated -and -not $unrelated.HasExited) { Stop-Process -Id $unrelated.Id -Force }
    Unregister-ScheduledTask -TaskName 'LocalWeb Daemon' -Confirm:$false -ErrorAction SilentlyContinue
    $env:LOCALAPPDATA = $previousData
    $env:LOCALWEB_RUNTIME_DIR = $previousRuntime
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
