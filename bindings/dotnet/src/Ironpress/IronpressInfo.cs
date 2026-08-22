using System.Runtime.InteropServices;
using Ironpress.Interop;

namespace Ironpress;

/// <summary>Version information for the packaged Ironpress native library.</summary>
public static class IronpressInfo
{
    /// <summary>The C ABI generation required by this managed package.</summary>
    public const uint RequiredAbiVersion = 1;

    /// <summary>Gets the C ABI generation implemented by the loaded native library.</summary>
    public static uint AbiVersion => NativeMethods.ironpress_abi_version();

    /// <summary>Gets the Ironpress package version from the loaded native library.</summary>
    public static string Version
    {
        get
        {
            var version = Marshal.PtrToStringUTF8(NativeMethods.ironpress_version());
            return version ?? throw new IronpressException(
                IronpressErrorKind.Internal,
                (int)NativeStatus.Internal,
                "The native library returned no package version.");
        }
    }

    internal static void EnsureCompatibleAbi()
    {
        var actual = AbiVersion;
        if (actual != RequiredAbiVersion)
        {
            throw new PlatformNotSupportedException(
                $"Ironpress ABI {RequiredAbiVersion} is required, but ABI {actual} was loaded.");
        }
    }
}
