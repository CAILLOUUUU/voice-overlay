# Voice Overlay (Rust) - Windows test
# Runs the new .exe next to this script on a separate port with a COPY of your data,
# checks everything it can, asks you a few yes/no questions, then writes a report zip on your Desktop.
# Your normal Voice Overlay (if any) and its files are not modified.

$ErrorActionPreference = 'Continue'
Add-Type -AssemblyName System.Net.Http

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Get-ChildItem -Path $here -Filter 'Voice-Overlay-*.exe' | Select-Object -First 1
$port = 47480
$base = "http://localhost:$port"
$realData = Join-Path $env:APPDATA 'VoiceOverlayCaillou'
$testData = Join-Path $env:TEMP 'VoiceOverlayRustTest'
$report = New-Object System.Collections.Generic.List[string]
$script:fails = 0

function Say($text, $color = 'Gray') { Write-Host $text -ForegroundColor $color }
function Note($text) { $report.Add($text); Say "  $text" }
function Check($name, $ok, $detail = '') {
    if ($ok) { $line = "[OK]   $name" } else { $line = "[FAIL] $name"; $script:fails++ }
    if ($detail) { $line += "  ($detail)" }
    $report.Add($line)
    if ($ok) { Say $line 'Green' } else { Say $line 'Red' }
}
function Ask($question) {
    do { $a = (Read-Host "$question [y/n]").Trim().ToLower() } while ($a -ne 'y' -and $a -ne 'n')
    return $a -eq 'y'
}
# y / n / s (skip): $null when skipped
function AskOpt($question) {
    do { $a = (Read-Host "$question [y/n, s = skip]").Trim().ToLower() } while ('y', 'n', 's' -notcontains $a)
    if ($a -eq 's') { return $null }
    return $a -eq 'y'
}
function ListenerPid {
    $c = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($c) { return [int]$c.OwningProcess }
    return $null
}
function Api($path, $body = $null) {
    try {
        if ($null -eq $body) { return Invoke-RestMethod -Uri "$base$path" -TimeoutSec 10 }
        return Invoke-RestMethod -Uri "$base$path" -Method Post -Body $body -ContentType 'application/json' -TimeoutSec 10
    } catch { return $null }
}
# First full state sent by /events
function Snapshot {
    $client = New-Object System.Net.Http.HttpClient
    try {
        $client.Timeout = [TimeSpan]::FromSeconds(10)
        $resp = $client.GetAsync("$base/events", [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).Result
        $reader = New-Object System.IO.StreamReader($resp.Content.ReadAsStreamAsync().Result)
        for ($i = 0; $i -lt 20; $i++) {
            $line = $reader.ReadLine()
            if ($line -and $line.StartsWith('data: ')) { return ($line.Substring(6) | ConvertFrom-Json) }
        }
    } catch { } finally { $client.Dispose() }
    return $null
}
# Counts live "speaking" messages for some seconds
function CountSpeaking($seconds) {
    $client = New-Object System.Net.Http.HttpClient
    $count = 0
    try {
        $client.Timeout = [TimeSpan]::FromSeconds($seconds + 30)
        $resp = $client.GetAsync("$base/events", [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).Result
        $reader = New-Object System.IO.StreamReader($resp.Content.ReadAsStreamAsync().Result)
        $end = (Get-Date).AddSeconds($seconds)
        $task = $null
        while ((Get-Date) -lt $end) {
            if ($null -eq $task) { $task = $reader.ReadLineAsync() }
            if ($task.Wait(300)) {
                if ($task.Result -eq 'event: speaking') { $count++ }
                $task = $null
            }
        }
    } catch { } finally { $client.Dispose() }
    return $count
}

Say ''
Say '=== Voice Overlay (Rust) - Windows test ===' 'Cyan'
Say ''
if (-not $exe) { Say 'Voice-Overlay-*.exe not found next to this script.' 'Red'; return }
Unblock-File -Path $exe.FullName -ErrorAction SilentlyContinue
$report.Add("Voice Overlay Rust test - $(Get-Date -Format 'yyyy-MM-dd HH:mm')")
$report.Add("exe: $($exe.Name)  ($([math]::Round($exe.Length / 1MB, 1)) MB)")
$report.Add("Windows: $([Environment]::OSVersion.VersionString)")
$report.Add('')

# ---------- Before starting ----------
$discord = Get-Process -Name 'Discord', 'DiscordPTB', 'DiscordCanary' -ErrorAction SilentlyContinue
Check 'Discord desktop app is running' ([bool]$discord)
$leagueOpen = [bool](Get-Process -Name 'LeagueClientUx' -ErrorAction SilentlyContinue)
Note "League client open: $leagueOpen"

# Copy of your data (credentials, token, settings) so your normal setup is never touched
Remove-Item -Recurse -Force $testData -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $testData | Out-Null
$hadData = Test-Path (Join-Path $realData 'config.json')
foreach ($f in 'config.json', 'token.json', 'settings.json', 'polices') {
    $src = Join-Path $realData $f
    if (Test-Path $src) { Copy-Item -Recurse $src $testData }
}
Note "Existing Voice Overlay data copied: $hadData"
$hadToken = Test-Path (Join-Path $testData 'token.json')
$oldSecret = ''
if ($hadData) { try { $oldSecret = [string](Get-Content (Join-Path $testData 'config.json') -Raw | ConvertFrom-Json).clientSecret } catch { } }

# Fake "new version 9.9.9" served on this PC, to test the one-click update for real
$feedPort = 47481
$feedDir = Join-Path $env:TEMP 'VoiceOverlayRustFeed'
Remove-Item -Recurse -Force $feedDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $feedDir | Out-Null
$feedExe = Join-Path $feedDir 'Voice-Overlay-9.9.9.exe'
Copy-Item $exe.FullName $feedExe
$feedSha = (Get-FileHash -Algorithm SHA256 $feedExe).Hash.ToLower()
$feedJson = @{
    tag_name = 'v9.9.9'; html_url = "http://127.0.0.1:$feedPort/page"
    assets = @(@{ name = 'Voice-Overlay-9.9.9.exe'; size = (Get-Item $feedExe).Length; digest = "sha256:$feedSha"
                  browser_download_url = "http://127.0.0.1:$feedPort/Voice-Overlay-9.9.9.exe" })
} | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText((Join-Path $feedDir 'latest'), $feedJson)
$feedJob = Start-Job -ArgumentList $feedDir, $feedPort -ScriptBlock {
    param($dir, $p)
    $l = New-Object System.Net.Sockets.TcpListener([System.Net.IPAddress]::Loopback, $p)
    $l.Start()
    while ($true) {
        # Short waits instead of a blocking accept, so Stop-Job can end this job
        if (-not $l.Pending()) { Start-Sleep -Milliseconds 100; continue }
        $c = $l.AcceptTcpClient()
        try {
            $st = $c.GetStream()
            $rd = New-Object System.IO.StreamReader($st)
            $first = $rd.ReadLine()
            while ($true) { $h = $rd.ReadLine(); if ([string]::IsNullOrEmpty($h)) { break } }
            $name = ($first -split ' ')[1].TrimStart('/')
            $f = Join-Path $dir $name
            if ($name -match '^[A-Za-z0-9.-]+$' -and (Test-Path $f -PathType Leaf)) {
                $body = [IO.File]::ReadAllBytes($f); $code = '200 OK'
            } else { $body = [byte[]]@(); $code = '404 Not Found' }
            $hb = [Text.Encoding]::ASCII.GetBytes("HTTP/1.1 $code`r`nContent-Length: $($body.Length)`r`nConnection: close`r`n`r`n")
            $st.Write($hb, 0, $hb.Length)
            if ($body.Length) { $st.Write($body, 0, $body.Length) }
            $st.Flush()
        } catch { } finally { $c.Close() }
    }
}
$env:VO_UPDATE_FEED = "http://127.0.0.1:$feedPort/latest"

# Start-with-Windows entry: saved now, restored at the end
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$prevRun = (Get-ItemProperty -Path $runKey -Name 'VoiceOverlayCaillou' -ErrorAction SilentlyContinue).VoiceOverlayCaillou

# ---------- Start ----------
$env:VO_DATA_DIR = $testData
$proc = Start-Process -FilePath $exe.FullName -ArgumentList "--port=$port", '--silent' -PassThru
Start-Sleep -Seconds 2
$state = Api '/api/state'
Check 'App starts and answers' ($null -ne $state) "pid $($proc.Id)"
if (-not $state) {
    Note 'Stopped: the app did not answer.'
} else {
    Check 'Version' ($state.version -ne $null) "$($state.version)"

    $pages = @('/', '/settings', '/fonts.css', '/webfonts/inter-500.woff2', '/icon.png')
    $bad = @()
    foreach ($p in $pages) {
        try { $r = Invoke-WebRequest -Uri "$base$p" -UseBasicParsing -TimeoutSec 10; if ($r.StatusCode -ne 200) { $bad += $p } } catch { $bad += $p }
    }
    Check 'Pages load (overlay, settings, fonts, icon)' ($bad.Count -eq 0) ($bad -join ', ')

    # Second copy on the same port must close by itself (single instance)
    $second = Start-Process -FilePath $exe.FullName -ArgumentList "--port=$port", '--silent' -PassThru
    $closed = $second.WaitForExit(5000)
    if (-not $closed) { Stop-Process -Id $second.Id -Force -ErrorAction SilentlyContinue }
    Check 'Second copy closes by itself (single instance)' $closed

    # ---------- Discord ----------
    if (-not $state.hasConfig) {
        Say ''
        Say 'No Discord credentials found. The settings page will open: follow the setup steps there.' 'Yellow'
        Start-Process "$base/settings"
    }
    Say ''
    Say 'Waiting for the Discord connection (if Discord shows an "Authorize" window, click Authorize)...' 'Yellow'
    $snap = $null
    $end = (Get-Date).AddMinutes(3)
    do {
        Start-Sleep -Seconds 2
        $snap = Snapshot
        if ($snap -and $snap.status -eq 'error') { break }
    } while ((Get-Date) -lt $end -and (-not $snap -or $snap.status -ne 'connected'))
    $msg = ''
    if ($snap -and $snap.statusMsg) { $msg = $snap.statusMsg.en }
    Check 'Connected to Discord' ($snap -and $snap.status -eq 'connected') "status=$($snap.status) $msg"

    if ($snap -and $snap.status -eq 'connected') {
        # Secrets moved into Windows Credential Manager
        $vault = (cmdkey /list) -match 'VoiceOverlayCaillou'
        Check 'Secrets stored in Windows Credential Manager' ([bool]$vault) "$(@($vault).Count) entries"
        $cfg = Get-Content (Join-Path $testData 'config.json') -Raw | ConvertFrom-Json
        Check 'Client Secret removed from config.json' ([string]::IsNullOrEmpty($cfg.clientSecret))
        Check 'token.json removed (token is in Credential Manager)' (-not (Test-Path (Join-Path $testData 'token.json')))
        Note "Token reused from your existing setup: $hadToken"

        Say ''
        Say 'Join a voice channel in Discord now (if you are not in one already).' 'Yellow'
        Read-Host 'Press Enter when you are in a voice channel'
        Start-Sleep -Seconds 2
        $snap = Snapshot
        $chan = $null
        if ($snap.channel) { $chan = $snap.channel.name }
        Check 'Follows your voice channel' ([bool]$chan) "$(@($snap.members).Count) people"

        Say ''
        Say 'Now TALK in the voice channel for about 10 seconds (counting starts now)...' 'Yellow'
        $n = CountSpeaking 10
        Check 'Live "who is speaking" updates' ($n -gt 0) "$n updates"

        Say ''
        Say 'Leave the voice channel in Discord now.' 'Yellow'
        Read-Host 'Press Enter once you have left'
        Start-Sleep -Seconds 3
        $snap = Snapshot
        Check 'Notices you left the channel' ($snap -and -not $snap.channel)
        Note "League status seen by the app: $($snap.league.status)"
        if ($leagueOpen) { Check 'League client detected' ($snap.league.status -ne 'off') "$($snap.league.status)" }
    }

    # ---------- Start with Windows (your previous setting is restored afterwards) ----------
    $on = Api '/api/autostart' '{"enabled":true}'
    $val = (Get-ItemProperty -Path $runKey -Name 'VoiceOverlayCaillou' -ErrorAction SilentlyContinue).VoiceOverlayCaillou
    Check 'Start with Windows: turn on' ($on -and $on.autostart -and $val -like "*$($exe.Name)*--silent*")
    $off = Api '/api/autostart' '{"enabled":false}'
    $val = (Get-ItemProperty -Path $runKey -Name 'VoiceOverlayCaillou' -ErrorAction SilentlyContinue).VoiceOverlayCaillou
    Check 'Start with Windows: turn off' ($off -and -not $off.autostart -and -not $val)
    if ($prevRun) { Set-ItemProperty -Path $runKey -Name 'VoiceOverlayCaillou' -Value $prevRun }

    # ---------- Diagnostics report ----------
    $diag = ''
    try { $diag = (Invoke-WebRequest -Uri "$base/api/diagnostics" -UseBasicParsing -TimeoutSec 10).Content } catch { }
    Check 'Diagnostics report' ($diag -like 'Voice Overlay*') "$(($diag -split "`n").Count) lines"
    if ($oldSecret.Length -ge 10) { Check 'Diagnostics contain no Client Secret' (-not $diag.Contains($oldSecret)) }

    # ---------- One-click update (fake version 9.9.9 served on this PC) ----------
    $snap = Snapshot
    Check 'Update found' ($snap -and $snap.update -and $snap.update.canInstall) "$($snap.update.version)"
    if ($snap -and $snap.update -and $snap.update.canInstall) {
        $oldPid = ListenerPid
        Api '/api/update-install' '{}' | Out-Null
        $newPid = $null
        $end = (Get-Date).AddSeconds(40)
        do {
            Start-Sleep -Seconds 1
            $lp = ListenerPid
            if ($lp -and $lp -ne $oldPid -and (Api '/api/state')) { $newPid = $lp }
        } while (-not $newPid -and (Get-Date) -lt $end)
        $why = ''
        $s2 = Snapshot
        if ($s2 -and $s2.updateStatus -and $s2.updateStatus.error) { $why = $s2.updateStatus.error.en }
        Check 'Update installs itself and restarts' ([bool]$newPid) "pid $oldPid -> $newPid $why"
        if ($newPid) {
            $proc = Get-Process -Id $newPid
            Start-Sleep -Seconds 6
            $oldExe = [IO.Path]::ChangeExtension($exe.FullName, 'old.exe')
            Check 'Old version cleaned up after the update' (-not (Test-Path $oldExe))
            Check 'Old process closed' (-not (Get-Process -Id $oldPid -ErrorAction SilentlyContinue))
        }
    }

    # ---------- What only you can see ----------
    Say ''
    Say 'A few things only you can check:' 'Cyan'
    Say '(The tray icon may be hidden behind the ^ arrow next to the clock.)'
    Check 'You see the Voice Overlay icon next to the clock' (Ask 'Do you see the Voice Overlay icon next to the clock?')
    Check 'Only one icon after the update' (Ask 'Move the mouse over the icons next to the clock. Is there exactly ONE Voice Overlay icon?')
    Check 'Left-click on the icon opens the settings' (Ask 'Left-click the icon: does the settings page open in your browser?')
    Check 'Right-click shows the menu' (Ask 'Right-click the icon: do you see "Open settings" and "Quit"?')
    Check 'Settings page looks normal' (Ask 'Does the settings page look like the usual one (no errors, preview visible)?')
    Check 'Ready-made styles' (Ask 'In "Ready-made styles", click a style: does the preview on the right change?')
    $r = AskOpt 'Under "Overlay profiles", click "Add to OBS..." and follow the help text (needs OBS open). Did the source appear in OBS?'
    if ($null -eq $r) { Note 'Add to OBS: skipped' } else { Check 'Add to OBS automatically' $r }
    $mem = [math]::Round((Get-Process -Id $proc.Id -ErrorAction SilentlyContinue).WorkingSet64 / 1MB, 1)
    Note "Memory used by the app: $mem MB"
    Check 'Quit from the tray menu closes the app' (Ask 'Right-click the icon and choose Quit. Did the icon disappear?')
    Start-Sleep -Seconds 1
}

# ---------- Cleanup ----------
if (-not $proc.HasExited) {
    Api '/api/quit' '{}' | Out-Null
    if (-not $proc.WaitForExit(3000)) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    Note 'App closed by the script.'
}
Stop-Job $feedJob -ErrorAction SilentlyContinue; Remove-Job $feedJob -Force -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force $feedDir -ErrorAction SilentlyContinue
foreach ($line in (cmdkey /list)) {
    if ($line -match 'target=(.*VoiceOverlayCaillou)') { cmdkey /delete:$($Matches[1]) | Out-Null }
}

$report.Add('')
$report.Add("Failures: $script:fails")
$out = Join-Path $env:TEMP 'VoiceOverlayRustReport'
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $out | Out-Null
$report | Set-Content -Path (Join-Path $out 'report.txt') -Encoding UTF8
if ($diag) { $diag | Set-Content -Path (Join-Path $out 'diagnostics.txt') -Encoding UTF8 }
foreach ($f in 'voice-overlay.log', 'voice-overlay.log.old') {
    $p = Join-Path $testData $f
    if (Test-Path $p) { Copy-Item $p $out }
}
$zip = Join-Path ([Environment]::GetFolderPath('Desktop')) 'voice-overlay-test-report.zip'
Remove-Item $zip -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $out '*') -DestinationPath $zip
Remove-Item -Recurse -Force $testData -ErrorAction SilentlyContinue

Say ''
if ($script:fails -eq 0) { Say 'All checks passed.' 'Green' } else { Say "$script:fails check(s) failed." 'Red' }
Say "Report saved: $zip" 'Cyan'
Say 'Send that zip back. It contains the results and the app log (Discord names of people in your channel, no passwords or tokens).'
