# Option costs: what each graphics improvement costs, per cell. For each row of the cell list
# the game cycles (--graphics-cycle) through: everything on, everything on but one key (one set
# per key), vanilla, and sun shadows off, logging the mean frame time of each period with vsync
# off (--log-frame-times). The cost of a key is "all on" minus "all on but that key".
#
#   powershell -File tools/profile/option-costs.ps1 -Data "<Data>" [-Cells cells.csv] [-Loops 2]
param(
    # A home whose NVForever.ini has no [Settings]: stored settings win over --size, --msaa,
    # --no-sun-shadows and the rest (docs/design/frame-rate-audit.md §1).
    [string]$NvfHome = $env:NVFOREVER_HOME,
    [string]$Exe = "F:/nvf-target/tracy/release/nvforever.exe",
    [string]$Data = $env:NVF_DATA,
    [string]$Cells = "$PSScriptRoot/cells.csv",
    [string]$Out = "F:/nvf-target/option-costs",
    [int]$Period = 8,                              # seconds per set
    [int]$Loops = 2,                               # times through the sets
    [int]$Warmup = 15,                             # extra seconds for loading (its periods are dropped)
    [string]$Size = "1920x1080"
)
$ErrorActionPreference = "Stop"
if ($NvfHome) { $env:NVFOREVER_HOME = $NvfHome } else { Write-Warning "no -NvfHome: the stored settings of your NVForever.ini win over this script's arguments" }
$keys = "ambient-occlusion", "fine-shadows", "bloom", "sharpen", "anisotropic", "light-shafts", "water-reflections"
$all = $keys -join ","
$sets = [ordered]@{ "all on" = $all }
foreach ($k in $keys) { $sets["without $k"] = ($keys | Where-Object { $_ -ne $k }) -join "," }
$sets["vanilla"] = "vanilla"
$sets["all on, no sun shadows"] = "$all,no-sun-shadows"
$names = @($sets.Keys)
$cycle = ($sets.Values) -join ";"
New-Item -ItemType Directory -Force $Out | Out-Null
Remove-Item Env:NVF_PLAY_TEST_PROFILE -ErrorAction SilentlyContinue
$results = @()
foreach ($row in (Import-Csv $Cells | Where-Object { $_.name -and -not $_.name.StartsWith("#") })) {
    Write-Host "== $($row.name)"
    $n = $names.Count
    $gameArgs = @("--play", "--no-voice", "--no-messages", "--size", $Size, "--cell", $row.cell,
                  "--log-frame-times", $Period, "--graphics-cycle", ('"' + $cycle + '"'),
                  "--run-seconds", ($Warmup + $Period * $n * $Loops + 3))
    if ($row.world) { $gameArgs += @("--world", $row.world) }
    if ($row.pos) { $gameArgs += @("--pos", $row.pos) }
    if ($row.hour) { $gameArgs += @("--hour", $row.hour) }
    if ($row.weather) { $gameArgs += @("--weather", $row.weather) }
    if ($Data) { $gameArgs += @("--data", ('"' + $Data + '"')) }
    $log = Join-Path $Out "$($row.name).log"
    $game = Start-Process $Exe -ArgumentList $gameArgs -PassThru -RedirectStandardError $log
    try { $game.WaitForExit(($Warmup + $Period * $n * $Loops + 90) * 1000) | Out-Null }
    finally { if (-not $game.HasExited) { Stop-Process -Id $game.Id -Force } }
    # Each period's line names the set it ran ("graphics: ..."); the first one covers loading.
    $periods = @(Select-String $log -Pattern 'frame time: ([\d.]+) ms mean.*?graphics: (.*)$' | Select-Object -Skip 1 |
        ForEach-Object { [pscustomobject]@{ ms = [double]$_.Matches[0].Groups[1].Value; what = ($_.Matches[0].Groups[2].Value -replace "; sun at .*$", "").Trim() } })
    $groups = $periods | Group-Object what
    $base = ($groups | Sort-Object { $_.Name.Length } -Descending | Select-Object -First 1).Group.ms | Measure-Object -Average
    foreach ($g in $groups) {
        $mean = ($g.Group.ms | Measure-Object -Average).Average
        $results += [pscustomobject]@{ cell = $row.name; ms = [math]::Round($mean, 2); saves = [math]::Round($base.Average - $mean, 2); runs = $g.Count; set = $g.Name }
    }
}
$results | Export-Csv -NoTypeInformation -Encoding utf8 (Join-Path $Out "option-costs.csv")
$results | Format-Table -AutoSize | Out-String -Width 200
