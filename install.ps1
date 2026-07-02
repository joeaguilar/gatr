# gatr installer for Windows (PowerShell)
#
# Usage:
#   .\install.ps1
#   irm https://raw.githubusercontent.com/joeaguilar/gatr/main/install.ps1 | iex
#
# Environment overrides:
#   GATR_VERSION      pin a release tag (default: latest)
#   GATR_INSTALL_DIR  install directory override (default: %USERPROFILE%\.cargo\bin or %LOCALAPPDATA%\gatr\bin)
#   GATR_REPO         GitHub repo (default: joeaguilar/gatr)

$ErrorActionPreference = 'Stop'

$Repo = if ($env:GATR_REPO) { $env:GATR_REPO } else { 'joeaguilar/gatr' }

$arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') {
    'aarch64-pc-windows-msvc'
} else {
    'x86_64-pc-windows-msvc'
}

# Resolve tag: pinned or follow the /releases/latest redirect (no API, no rate limits).
$tag = $env:GATR_VERSION
if (-not $tag) {
    $resp = [System.Net.HttpWebRequest]::Create("https://github.com/$Repo/releases/latest")
    $resp.AllowAutoRedirect = $false
    $location = $resp.GetResponse().Headers['Location']
    if ($location -match '/tag/(.+)$') { $tag = $Matches[1] } else { throw 'could not resolve latest release tag' }
}

$base = "gatr-$tag-$arch"
$url = "https://github.com/$Repo/releases/download/$tag/$base.zip"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $tmp | Out-Null

try {
    Write-Host "downloading $base.zip ..."
    Invoke-WebRequest -Uri $url -OutFile (Join-Path $tmp "$base.zip")

    # Verify checksum when published.
    try {
        Invoke-WebRequest -Uri "$url.sha256" -OutFile (Join-Path $tmp "$base.zip.sha256")
        $expected = (Get-Content (Join-Path $tmp "$base.zip.sha256")).Split(' ')[0].Trim().ToLower()
        $actual = (Get-FileHash -Algorithm SHA256 (Join-Path $tmp "$base.zip")).Hash.ToLower()
        if ($expected -ne $actual) { throw "checksum mismatch: expected $expected, got $actual" }
    } catch [System.Net.WebException] {
        Write-Warning 'no checksum file published; skipping verification'
    }

    Expand-Archive -Path (Join-Path $tmp "$base.zip") -DestinationPath $tmp

    $installDir = $env:GATR_INSTALL_DIR
    if (-not $installDir) {
        $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
        $installDir = if (Test-Path $cargoBin) { $cargoBin } else { Join-Path $env:LOCALAPPDATA 'gatr\bin' }
    }
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    Copy-Item (Join-Path $tmp "$base\gatr.exe") -Destination (Join-Path $installDir 'gatr.exe') -Force

    Write-Host "installed gatr to $installDir\gatr.exe"
    if (($env:Path -split ';') -notcontains $installDir) {
        Write-Warning "$installDir is not on PATH — add it via System Settings or:"
        Write-Warning "  [Environment]::SetEnvironmentVariable('Path', `$env:Path + ';$installDir', 'User')"
    }
    & (Join-Path $installDir 'gatr.exe') --version
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
