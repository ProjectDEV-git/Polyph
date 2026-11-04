# Installs Polyph from a GitHub release for the current Windows user.
#
#   irm https://raw.githubusercontent.com/ProjectDEV-git/Polyph/Main/scripts/install.ps1 | iex
#
# Settings (environment): POLYPH_VERSION (a tag, default the latest release),
# POLYPH_PREFIX (default %LOCALAPPDATA%\Programs\Polyph), POLYPH_REPO,
# POLYPH_INSTALL_BASE (a URL holding the archives and SHA256SUMS).
$ErrorActionPreference = 'Stop'

$Repo = if ($env:POLYPH_REPO) { $env:POLYPH_REPO } else { 'ProjectDEV-git/Polyph' }
$Version = if ($env:POLYPH_VERSION) { $env:POLYPH_VERSION } else { 'latest' }
$Prefix = if ($env:POLYPH_PREFIX) { $env:POLYPH_PREFIX } else { Join-Path $env:LOCALAPPDATA 'Programs\Polyph' }

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
  'AMD64' { 'x86_64' }
  default { throw "Polyph has no Windows build for $($env:PROCESSOR_ARCHITECTURE); build from source instead." }
}
$target = "$arch-pc-windows-msvc"

if ($env:POLYPH_INSTALL_BASE) {
  $base = $env:POLYPH_INSTALL_BASE
} else {
  if ($Version -eq 'latest') {
    $rel = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ 'User-Agent' = 'polyph-install' }
    $Version = $rel.tag_name
  }
  $base = "https://github.com/$Repo/releases/download/$Version"
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("polyph-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  $sums = Join-Path $tmp 'SHA256SUMS'
  Invoke-WebRequest "$base/SHA256SUMS" -OutFile $sums -UseBasicParsing
  $line = Get-Content $sums | Where-Object { $_ -match "\s(polyph-.*-$([regex]::Escape($target))\.zip)$" } | Select-Object -First 1
  if (-not $line) { throw "This release has no build for $target." }
  $want, $archive = ($line -split '\s+', 2)
  Write-Host "Downloading $archive"
  $zip = Join-Path $tmp $archive
  Invoke-WebRequest "$base/$archive" -OutFile $zip -UseBasicParsing
  $got = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
  if ($got -ne $want.ToLower()) { throw "Checksum mismatch for $archive (expected $want, got $got)." }

  Expand-Archive $zip -DestinationPath $tmp
  $src = Join-Path $tmp ($archive -replace '\.zip$', '')
  New-Item -ItemType Directory -Force -Path (Join-Path $Prefix 'bin'), (Join-Path $Prefix 'share') | Out-Null
  Copy-Item (Join-Path $src 'bin\*') (Join-Path $Prefix 'bin') -Force
  $share = Join-Path $Prefix 'share\polyph'
  if (Test-Path $share) { Remove-Item $share -Recurse -Force }
  Copy-Item (Join-Path $src 'share\polyph') $share -Recurse

  $bin = Join-Path $Prefix 'bin'
  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (-not (($userPath -split ';') -contains $bin)) {
    [Environment]::SetEnvironmentVariable('Path', ($userPath.TrimEnd(';') + ";$bin"), 'User')
    Write-Host "Added $bin to your PATH; open a new terminal to use it."
  }
  Write-Host ("Installed " + (& (Join-Path $bin 'polyph.exe') --version) + " to $bin")
} finally {
  Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
