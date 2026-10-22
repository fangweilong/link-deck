# 重新生成内置“饿死了”音效（assets/low_battery_esi.wav）。
# 依赖：python + edge-tts（pip install edge-tts）、ffmpeg / ffprobe。
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$out = Join-Path $root "assets\low_battery_esi.wav"
$mp3 = Join-Path $env:TEMP "linkdeck_esi_tts.mp3"

python -m edge_tts --voice zh-CN-XiaoyiNeural --rate=+8% --pitch=+12Hz `
    --text "饿死了！饿死了！" --write-media $mp3
if ($LASTEXITCODE -ne 0) { throw "edge-tts 生成失败" }

$durText = & ffprobe -v error -show_entries format=duration -of default=nw=1 $mp3
$durLine = [string]($durText | Select-Object -First 1)
$dur = [double](($durLine -split "=")[-1])

# 找出“最后一段语音”的结束点：若文件尾部是静音区，silence_end 会等于总时长。
# 不能按固定秒数硬切，否则最后一个“了”会被切掉（听感上突然断掉）。
$log = ""
$oldEap = $ErrorActionPreference
$ErrorActionPreference = "Continue"
$log = (& ffmpeg -hide_banner -i $mp3 -af "silencedetect=noise=-45dB:d=0.25" -f null - 2>&1 | Out-String)
$ErrorActionPreference = $oldEap
$starts = [regex]::Matches($log, "silence_start: ([0-9.]+)")
$ends = [regex]::Matches($log, "silence_end: ([0-9.]+)")
$speechEnd = $dur
if ($starts.Count -gt 0 -and $ends.Count -gt 0) {
    $lastStart = [double]$starts[$starts.Count - 1].Groups[1].Value
    $lastEnd = [double]$ends[$ends.Count - 1].Groups[1].Value
    if ([Math]::Abs($lastEnd - $dur) -lt 0.06 -and $lastStart -gt $dur * 0.4) {
        $speechEnd = $lastStart
    }
}

# 尾部多留 0.35s，淡出落在静音里，保留最后一个字的自然收音。
$keep = [Math]::Min($dur, [Math]::Round($speechEnd + 0.35, 3))
$fade = [Math]::Max(0.05, [Math]::Round($keep - 0.15, 3))

& ffmpeg -y -loglevel error -i $mp3 -t $keep `
    -af "highpass=f=70,loudnorm=I=-16:TP=-2.0:LRA=11,afade=t=in:st=0:d=0.02,afade=t=out:st=${fade}:d=0.15" `
    -ac 1 -ar 22050 -c:a pcm_s16le $out
if ($LASTEXITCODE -ne 0) { throw "ffmpeg 转换失败" }

Remove-Item -LiteralPath $mp3 -ErrorAction SilentlyContinue
$sizeKb = [Math]::Round((Get-Item -LiteralPath $out).Length / 1KB, 1)
Write-Host "已生成: $out ($sizeKb KB, ${keep}s, 22050Hz mono 16-bit)" -ForegroundColor Green
