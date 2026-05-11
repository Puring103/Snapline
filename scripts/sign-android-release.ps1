param(
  [string]$UnsignedApk = "apps/client/src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk",
  [string]$OutputApk = "release-artifacts/Snapline_0.1.0_universal-release-signed.apk",
  [string]$PropertiesFile = "android-signing/snapline-release.properties"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

function Read-Properties($path) {
  $props = @{}
  Get-Content -LiteralPath $path | ForEach-Object {
    if ($_ -match "^\s*([^#][^=]*)=(.*)$") {
      $props[$matches[1].Trim()] = $matches[2].Trim()
    }
  }
  return $props
}

function Resolve-BuildTools {
  $sdkRoot = $env:ANDROID_HOME
  if (-not $sdkRoot) { $sdkRoot = $env:ANDROID_SDK_ROOT }
  if (-not $sdkRoot) { $sdkRoot = Join-Path $env:LOCALAPPDATA "Android\Sdk" }
  $buildToolsRoot = Join-Path $sdkRoot "build-tools"
  if (-not (Test-Path -LiteralPath $buildToolsRoot)) {
    throw "Android SDK build-tools not found under $buildToolsRoot"
  }
  Get-ChildItem -LiteralPath $buildToolsRoot -Directory |
    Sort-Object Name -Descending |
    Select-Object -First 1
}

$props = Read-Properties $PropertiesFile
$buildTools = Resolve-BuildTools
$zipalign = Join-Path $buildTools.FullName "zipalign.exe"
$apksigner = Join-Path $buildTools.FullName "apksigner.bat"

if (-not (Test-Path -LiteralPath $zipalign)) { throw "zipalign.exe not found in $($buildTools.FullName)" }
if (-not (Test-Path -LiteralPath $apksigner)) { throw "apksigner.bat not found in $($buildTools.FullName)" }
if (-not (Test-Path -LiteralPath $UnsignedApk)) { throw "Unsigned APK not found: $UnsignedApk" }

$storeFile = $props.storeFile
if (-not [IO.Path]::IsPathRooted($storeFile)) {
  $storeFile = Join-Path $repoRoot $storeFile
}
if (-not (Test-Path -LiteralPath $storeFile)) { throw "Keystore not found: $storeFile" }

$outputDir = Split-Path -Parent $OutputApk
if ($outputDir) {
  New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
}

$alignedApk = Join-Path ([IO.Path]::GetTempPath()) ("snapline-aligned-" + [guid]::NewGuid() + ".apk")
try {
  & $zipalign -p -f 4 $UnsignedApk $alignedApk
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  Remove-Item -LiteralPath $OutputApk, "$OutputApk.idsig" -Force -ErrorAction SilentlyContinue
  & $apksigner sign `
    --ks $storeFile `
    --ks-key-alias $props.keyAlias `
    --ks-pass "pass:$($props.storePassword)" `
    --key-pass "pass:$($props.keyPassword)" `
    --out $OutputApk `
    $alignedApk
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  & $apksigner verify --verbose --print-certs $OutputApk
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
  Remove-Item -LiteralPath $alignedApk -Force -ErrorAction SilentlyContinue
}
