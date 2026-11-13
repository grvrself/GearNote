param(
    [string]$NodeExe,
    [string]$ToolDirectory
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$staging = Join-Path $projectRoot '.release-tools'
New-Item -ItemType Directory -Force -Path $staging | Out-Null

if ($NodeExe) {
    Copy-Item -LiteralPath $NodeExe -Destination (Join-Path $staging 'node.exe') -Force
}
if ($ToolDirectory) {
    foreach ($name in @('ffmpeg.exe', 'ffprobe.exe', 'yt-dlp.exe', 'FFmpeg-LICENSE.txt', 'FFmpeg-README.txt', 'yt-dlp-LICENSE.txt')) {
        Copy-Item -LiteralPath (Join-Path $ToolDirectory $name) -Destination (Join-Path $staging $name) -Force
    }
}

foreach ($name in @('node.exe', 'ffmpeg.exe', 'ffprobe.exe', 'yt-dlp.exe', 'FFmpeg-LICENSE.txt', 'FFmpeg-README.txt', 'yt-dlp-LICENSE.txt')) {
    if (-not (Test-Path -LiteralPath (Join-Path $staging $name) -PathType Leaf)) {
        throw "Missing .release-tools/$name. Pass -NodeExe and -ToolDirectory first."
    }
}

$expectedHashes = @{
    'node.exe' = '3331E1FFE19874215472217C5E94F5A0C6D8E18C4AC7111D3937AA0AD5E9B4A5'
    'ffmpeg.exe' = '3256173F3F8BFFD7DF12227C68ADF68025EDB1832273A9530688A7BB1ED8EDEC'
    'ffprobe.exe' = 'F0D36ECBBDD3BCFAC3EFA078C96C7271C2E68B3810595552AC3B7F17E9A65C52'
    'yt-dlp.exe' = 'F5389D3698ECDE4EF06F4E6B1CE0294FE9AC1396228C8990B8D3416D6D177A7D'
}
foreach ($name in $expectedHashes.Keys) {
    $actual = (Get-FileHash -LiteralPath (Join-Path $staging $name) -Algorithm SHA256).Hash
    if ($actual -ne $expectedHashes[$name]) { throw "Unexpected SHA-256 for $name. Review this tool and its license before distributing." }
}

Push-Location $projectRoot
try {
    & (Join-Path $staging 'node.exe') --version
    & (Join-Path $staging 'ffmpeg.exe') -version | Select-Object -First 1
    & (Join-Path $staging 'ffprobe.exe') -version | Select-Object -First 1
    & (Join-Path $staging 'yt-dlp.exe') --version
    npm run tauri -- build -b nsis -c src-tauri/tauri.distribution.conf.json --no-sign --ci
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}
