param(
    [string]$Tag = ""
)

$ErrorActionPreference = "Stop"
$cargo = Get-Content -LiteralPath (Join-Path $PSScriptRoot "..\Cargo.toml") -Raw
$match = [regex]::Match($cargo, '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"')
if (-not $match.Success) {
    throw "Could not read package.version from Cargo.toml"
}

$version = $match.Groups[1].Value
if ($Tag) {
    $expected = "v$version"
    if ($Tag -ne $expected) {
        throw "Release tag '$Tag' does not match Cargo version '$expected'."
    }
}

Write-Host "SF Commander version $version"
