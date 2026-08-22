using System.Runtime.InteropServices;

namespace Ironpress.Interop;

[StructLayout(LayoutKind.Sequential)]
internal readonly struct NativeBytes
{
    internal NativeBytes(nint data, nuint length)
    {
        Data = data;
        Length = length;
    }

    internal readonly nint Data;

    internal readonly nuint Length;
}
