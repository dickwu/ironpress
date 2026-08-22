using System.Runtime.InteropServices;

namespace Ironpress.Interop;

internal abstract class OwnedHandle : SafeHandle
{
    protected OwnedHandle(nint ownedHandle)
        : base(nint.Zero, true)
    {
        SetHandle(ownedHandle);
    }

    public sealed override bool IsInvalid => handle == nint.Zero;
}

internal sealed class ConverterHandle : OwnedHandle
{
    private ConverterHandle(nint ownedHandle)
        : base(ownedHandle)
    {
    }

    internal static ConverterHandle Take(nint ownedHandle) => new(ownedHandle);

    protected override bool ReleaseHandle()
    {
        var ownedHandle = handle;
        var status = NativeMethods.ironpress_converter_free(ref ownedHandle);
        handle = ownedHandle;
        return status == NativeStatus.Ok && IsInvalid;
    }
}

internal sealed class BufferHandle : OwnedHandle
{
    private BufferHandle(nint ownedHandle)
        : base(ownedHandle)
    {
    }

    internal static BufferHandle Take(nint ownedHandle) => new(ownedHandle);

    protected override bool ReleaseHandle()
    {
        var ownedHandle = handle;
        var status = NativeMethods.ironpress_buffer_free(ref ownedHandle);
        handle = ownedHandle;
        return status == NativeStatus.Ok && IsInvalid;
    }
}

internal sealed class ErrorHandle : OwnedHandle
{
    private ErrorHandle(nint ownedHandle)
        : base(ownedHandle)
    {
    }

    internal static ErrorHandle Take(nint ownedHandle) => new(ownedHandle);

    protected override bool ReleaseHandle()
    {
        var ownedHandle = handle;
        var status = NativeMethods.ironpress_error_free(ref ownedHandle);
        handle = ownedHandle;
        return status == NativeStatus.Ok && IsInvalid;
    }
}
