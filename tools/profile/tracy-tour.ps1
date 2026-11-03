# Tracy tour: profile a list of cells one launch at a time with a Tracy build of nvforever.
# For each row of the cell list it starts the game there, waits for the cell to load, records
# a capture with tracy-capture and exports CPU and GPU zone CSVs (tracy-csvexport, -g = GPU).
# The captures come from game data: keep the output folder private, never commit it.
#
# Build first (own target folder, it rebuilds Bevy):
#   cd engine; cargo build --release -p nvforever --features nvforever/tracy --target-dir F:/nvf-target/tracy
# Tracy 0.14.x tools (tracy-capture.exe, tracy-csvexport.exe) must match the client Bevy pins
# (docs/design/frame-rate-audit.md §1).
#
#   powershell -File tools/profile/tracy-tour.ps1 -Tracy C:\tracy -Data "D:\Steam\...\Fallout New Vegas\Data"
param(
    # A home whose NVForever.ini has no [Settings]: stored settings win over --size, --msaa,
    # --no-sun-shadows and the rest (docs/design/frame-rate-audit.md §1).
    [string]$NvfHome = $env:NVFOREVER_HOME,
    [string]$Exe = "F:/nvf-target/tracy/release/nvforever.exe",
    [Parameter(Mandatory)][string]$Tracy,          # folder with tracy-capture.exe and tracy-csvexport.exe
    [string]$Data = $env:NVF_DATA,
    [string]$Cells = "$PSScriptRoot/cells.csv",
    [string]$Out = "F:/nvf-target/tracy-tour",
    [int]$Warmup = 20,                             # seconds after launch before recording
    [int]$Seconds = 30,                            # capture length
    [int]$Repeat = 1,                              # runs per cell
    [string]$Size = "1920x1080",
    [string]$Graphics = "enhanced",                # test runs default to vanilla (--graphics)
    [switch]$Vsync,                                # keep vsync on (default: --log-frame-times 5, vsync off, so waits do not hide costs)
    [switch]$NoCollector                           # leave NVF_PLAY_TEST_PROFILE off (no profile: log lines, less overhead)
)
$ErrorActionPreference = "Stop"
if ($NvfHome) { $env:NVFOREVER_HOME = $NvfHome } else { Write-Warning "no -NvfHome: the stored settings of your NVForever.ini win over this script's arguments" }
$capture = Join-Path $Tracy "tracy-capture.exe"
$export = Join-Path $Tracy "tracy-csvexport.exe"
foreach ($t in $Exe, $capture, $export) { if (-not (Test-Path $t)) { throw "not found: $t" } }
New-Item -ItemType Directory -Force $Out | Out-Null
if (-not $NoCollector) { $env:NVF_PLAY_TEST_PROFILE = "5" } else { Remove-Item Env:NVF_PLAY_TEST_PROFILE -ErrorAction SilentlyContinue }

$rows = Import-Csv $Cells | Where-Object { $_.name -and -not $_.name.StartsWith("#") }
foreach ($row in $rows) {
    for ($run = 1; $run -le $Repeat; $run++) {
        $tag = if ($Repeat -gt 1) { "$($row.name)-$run" } else { $row.name }
        Write-Host "== $tag"
        $gameArgs = @("--play", "--no-voice", "--no-messages", "--size", $Size,
                      "--run-seconds", ($Warmup + $Seconds + 10), "--cell", $row.cell, "--graphics", $Graphics)
        if (-not $Vsync) { $gameArgs += @("--log-frame-times", "5") }
        if ($row.world) { $gameArgs += @("--world", $row.world) }
        if ($row.pos) { $gameArgs += @("--pos", $row.pos) }
        if ($row.hour) { $gameArgs += @("--hour", $row.hour) }
        if ($row.weather) { $gameArgs += @("--weather", $row.weather) }
        if ($Data) { $gameArgs += @("--data", $Data) }
        $log = Join-Path $Out "$tag.log"
        $quoted = $gameArgs | ForEach-Object { if ("$_" -match "\s") { "`"$_`"" } else { "$_" } }
        $game = Start-Process $Exe -ArgumentList $quoted -PassThru -RedirectStandardError $log
        try {
            Start-Sleep $Warmup
            if ($game.HasExited) { Write-Warning "$tag : the game exited early, see $log"; continue }
            # On-demand client: recording starts when this connects, so the load is not in it.
            $trace = Join-Path $Out "$tag.tracy"
            & $capture -o $trace -s $Seconds -f | Out-Null
            if (Test-Path $trace) {
                & $export $trace | Set-Content -Encoding utf8 (Join-Path $Out "$tag-cpu.csv")
                & $export -g $trace | Set-Content -Encoding utf8 (Join-Path $Out "$tag-gpu.csv")
            } else { Write-Warning "$tag : no capture (did the game start?)" }
            $game.WaitForExit(30000) | Out-Null
        } finally {
            # Stop only the game this script started.
            if (-not $game.HasExited) { Stop-Process -Id $game.Id -Force }
        }
    }
}
Write-Host "done: $Out"
