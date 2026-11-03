# Builds the fake executable, then runs every test program against copies of it.
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
cargo run -q -p lib_test --bin setup
foreach ($t in "write","append","read","delete","clear","limit","run","invalid_file") {
  cargo run -q -p lib_test --bin "test_$t"
  if ($LASTEXITCODE -ne 0) { exit 1 }
}
Write-Host "ALL OK"
