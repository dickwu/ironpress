param(
    [Parameter(Mandatory = $true)]
    [string]$VcpkgRoot,
    [Parameter(Mandatory = $true)]
    [string]$Triplet
)

$ErrorActionPreference = "Stop"
$TestDirectory = $PSScriptRoot
$RepositoryDirectory = (Resolve-Path "$TestDirectory/../../..").Path
$VcpkgDirectory = (Resolve-Path $VcpkgRoot).Path
$WorkDirectory = Join-Path ([IO.Path]::GetTempPath()) "ironpress-vcpkg-$([guid]::NewGuid())"
$InstalledDirectory = Join-Path $WorkDirectory "installed"
$BuildDirectory = Join-Path $WorkDirectory "build"

New-Item -ItemType Directory -Path $WorkDirectory | Out-Null
try {
    & "$VcpkgDirectory/vcpkg.exe" install `
        "--x-manifest-root=$TestDirectory" `
        "--x-install-root=$InstalledDirectory" `
        "--overlay-ports=$RepositoryDirectory/packaging/vcpkg/ports" `
        "--triplet=$Triplet"
    if ($LASTEXITCODE -ne 0) {
        throw "vcpkg install failed"
    }

    & cmake `
        -S $TestDirectory `
        -B $BuildDirectory `
        -DCMAKE_BUILD_TYPE=Release `
        "-DCMAKE_TOOLCHAIN_FILE=$VcpkgDirectory/scripts/buildsystems/vcpkg.cmake" `
        "-DVCPKG_INSTALLED_DIR=$InstalledDirectory" `
        -DVCPKG_MANIFEST_MODE=OFF `
        "-DVCPKG_TARGET_TRIPLET=$Triplet"
    if ($LASTEXITCODE -ne 0) {
        throw "CMake configure failed"
    }

    & cmake --build $BuildDirectory --config Release
    if ($LASTEXITCODE -ne 0) {
        throw "CMake build failed"
    }

    $env:PATH = "$InstalledDirectory/$Triplet/bin;$env:PATH"
    $ExecutableDirectory = Join-Path $BuildDirectory "Release"
    & "$ExecutableDirectory/ironpress_c_consumer.exe"
    if ($LASTEXITCODE -ne 0) {
        throw "C consumer failed"
    }
    & "$ExecutableDirectory/ironpress_cpp_consumer.exe"
    if ($LASTEXITCODE -ne 0) {
        throw "C++ consumer failed"
    }
} finally {
    Remove-Item -Recurse -Force $WorkDirectory
}
