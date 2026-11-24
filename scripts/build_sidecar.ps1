$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$PythonBin = if ($env:PYTHON_BIN) { $env:PYTHON_BIN } else { Join-Path $RootDir "..\.offshoredev\Scripts\python.exe" }
$OutDir = Join-Path $RootDir "src-tauri\resources\sidecar"
$BuildTmp = Join-Path $RootDir "sidecar\.build_tmp"

if (-not (Test-Path $PythonBin)) {
    Write-Error "Python interpreter not found at: $PythonBin. Set PYTHON_BIN to a Python with offshore-podcast and nuitka installed."
    exit 1
}

Write-Host "Building offshore sidecar with Nuitka using $PythonBin ..."
if (Test-Path $BuildTmp) { Remove-Item -Recurse -Force $BuildTmp }
if (Test-Path $OutDir) { Remove-Item -Recurse -Force $OutDir }
New-Item -ItemType Directory -Force -Path $BuildTmp | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutDir) | Out-Null

& $PythonBin -m nuitka `
    --standalone `
    --assume-yes-for-downloads `
    --include-package=offshore `
    --include-package-data=offshore `
    --output-dir="$BuildTmp" `
    --output-filename=offshore-sidecar.exe `
    (Join-Path $RootDir "sidecar\offshore_sidecar.py")

Move-Item (Join-Path $BuildTmp "offshore_sidecar.dist") $OutDir
New-Item -ItemType File -Force -Path (Join-Path $OutDir ".keep") | Out-Null
Remove-Item -Recurse -Force $BuildTmp

Write-Host "Sidecar built at: $OutDir\offshore-sidecar.exe"
