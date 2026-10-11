param(
    [switch]$Release,
    [switch]$Demo,
    [switch]$BuildOnly
)

$ErrorActionPreference = "Stop"
if ($env:OS -ne "Windows_NT") {
    throw "Run this script on Windows with Rust MSVC and Visual Studio Build Tools installed."
}

$buildProfile = if ($Release) { "release" } else { "debug" }
$target = "x86_64-pc-windows-msvc"
$oldFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
$oldFxc = $env:GPUI_FXC_PATH
try {
    # Link the MSVC runtime statically so the portable app needs no VC++ install.
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = "$oldFlags -C target-feature=+crt-static".Trim()
    if ($Release -and !$env:GPUI_FXC_PATH) {
        $sdk = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
        $fxc = Get-ChildItem "$sdk\*\x64\fxc.exe" -ErrorAction SilentlyContinue |
            Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
            Select-Object -First 1
        if (!$fxc) {
            throw "Install the Windows SDK with Visual Studio Build Tools (Desktop development with C++), or set GPUI_FXC_PATH to fxc.exe."
        }
        $env:GPUI_FXC_PATH = $fxc.FullName
    }

    $buildArgs = @("build", "--locked", "--manifest-path", "$PSScriptRoot\Cargo.toml", "--target", $target)
    if ($Release) { $buildArgs += "--release" }
    & cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw "Cargo build failed (exit $LASTEXITCODE)." }

    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$PSScriptRoot\target" }
    $binary = Join-Path $targetDir "$target\$buildProfile\speed-native.exe"
    Write-Host "Built $binary"
    if (!$BuildOnly) {
        if ($Demo) { Start-Process $binary -ArgumentList "--demo" }
        else { Start-Process $binary }
    }
} finally {
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $oldFlags
    $env:GPUI_FXC_PATH = $oldFxc
}
