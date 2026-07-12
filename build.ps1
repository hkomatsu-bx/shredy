<#
.SYNOPSIS
  Shredy 一括ビルドスクリプト（PowerShell 版）。
.DESCRIPTION
  ワークスペース全体（shredy-sys / shredy-core / shredy-cli / shredy-gui）を一括ビルドする。
  CPU 最適化（target-cpu=x86-64-v3）は .cargo/config.toml に従う。
.PARAMETER Target
  ビルドプロファイル: debug（既定）または release。
.PARAMETER Clean
  指定した場合、ビルド前に target フォルダをクリーンアップする（cargo clean）。
.EXAMPLE
  .\build.ps1
  デバッグビルド（既定）。
.EXAMPLE
  .\build.ps1 release
  リリースビルド。
.EXAMPLE
  .\build.ps1 release -Clean
  target をクリーンアップしてからリリースビルド。
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet('debug', 'release')]
    [string]$Target = 'debug',

    [switch]$Clean
)

$ErrorActionPreference = 'Stop'

# どこから実行してもリポジトリルート（このスクリプトの場所）で動かす。
Set-Location -LiteralPath $PSScriptRoot

if ($Target -eq 'release') {
    $flags = @('--release')
    $outDir = 'target/release'
} else {
    $flags = @()
    $outDir = 'target/debug'
}

if ($Clean) {
    Write-Host "==> target フォルダをクリーンアップします (cargo clean)"
    cargo clean
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host "==> Shredy をビルドします (profile=$Target)"
cargo build --workspace @flags
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> ビルド完了。成果物:"
foreach ($exe in @('shredy.exe', 'shredy-gui.exe')) {
    $path = Join-Path $outDir $exe
    if (Test-Path -LiteralPath $path) {
        Write-Host "    $path"
    }
}
