$ErrorActionPreference = 'Stop'

$crateRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path (Join-Path $crateRoot '..\..')
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$bindgenCommand = Get-Command 'wasm-bindgen.exe' -ErrorAction SilentlyContinue
$bindgen = if ($bindgenCommand) {
    $bindgenCommand.Source
} else {
    Join-Path $env:USERPROFILE '.local\tools\wasm-bindgen-0.2.127\wasm-bindgen-0.2.127-x86_64-pc-windows-msvc\wasm-bindgen.exe'
}
$output = Join-Path $repoRoot 'js\solver\wasm\pkg'

if (-not (Test-Path -LiteralPath $cargo)) {
    throw "cargo.exe was not found at $cargo"
}
if (-not (Test-Path -LiteralPath $bindgen)) {
    throw 'wasm-bindgen-cli 0.2.127 is required. Install it or add wasm-bindgen.exe to PATH.'
}

& $cargo +stable-x86_64-pc-windows-gnu build --manifest-path (Join-Path $crateRoot 'Cargo.toml') --release --target wasm32-unknown-unknown --lib
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& $bindgen (Join-Path $crateRoot 'target\wasm32-unknown-unknown\release\sp_kernel.wasm') --target web --out-dir $output --out-name sp_kernel --no-typescript
exit $LASTEXITCODE
