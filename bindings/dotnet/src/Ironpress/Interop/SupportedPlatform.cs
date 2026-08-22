using System.Runtime.InteropServices;

namespace Ironpress.Interop;

internal static class SupportedPlatform
{
    internal static void EnsureCurrent()
    {
        var architecture = RuntimeInformation.ProcessArchitecture;
        var supported =
            (OperatingSystem.IsLinux() && architecture is Architecture.X64 or Architecture.Arm64)
            || (OperatingSystem.IsMacOS() && architecture is Architecture.X64 or Architecture.Arm64)
            || (OperatingSystem.IsWindows() && architecture == Architecture.X64);

        if (!supported)
        {
            throw new PlatformNotSupportedException(
                $"Ironpress does not ship native assets for {RuntimeInformation.RuntimeIdentifier}.");
        }
    }
}
