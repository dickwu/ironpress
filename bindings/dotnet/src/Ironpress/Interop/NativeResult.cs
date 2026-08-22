using System.Runtime.InteropServices;

namespace Ironpress.Interop;

internal static class NativeResult
{
    internal static void EnsureSuccess(NativeStatus status, nint error)
    {
        using var errorOwner = ErrorHandle.Take(error);

        if (status == NativeStatus.Ok && errorOwner.IsInvalid)
        {
            return;
        }

        if (status == NativeStatus.Ok)
        {
            throw ContractViolation("A successful native call returned an error owner.");
        }

        var message = errorOwner.IsInvalid
            ? $"Ironpress failed with native status {(int)status}."
            : ReadErrorMessage(errorOwner);
        throw new IronpressException(ToPublicKind(status), (int)status, message);
    }

    internal static ConverterHandle TakeConverter(
        NativeStatus status,
        nint converter,
        nint error)
    {
        var converterOwner = ConverterHandle.Take(converter);
        try
        {
            EnsureSuccess(status, error);
            if (converterOwner.IsInvalid)
            {
                throw ContractViolation("Native converter allocation returned no owner.");
            }

            return converterOwner;
        }
        catch
        {
            converterOwner.Dispose();
            throw;
        }
    }

    internal static byte[] TakePdf(NativeStatus status, nint pdf, nint error)
    {
        using var pdfOwner = BufferHandle.Take(pdf);
        EnsureSuccess(status, error);

        if (pdfOwner.IsInvalid)
        {
            throw ContractViolation("Native conversion returned no PDF owner.");
        }

        return CopyBytes(pdfOwner);
    }

    private static byte[] CopyBytes(BufferHandle buffer)
    {
        var length = NativeMethods.ironpress_buffer_len(buffer);
        if (length > int.MaxValue)
        {
            throw ContractViolation("Native PDF exceeds the managed array limit.");
        }

        var byteCount = (int)length;
        var data = NativeMethods.ironpress_buffer_data(buffer);
        if (byteCount > 0 && data == nint.Zero)
        {
            throw ContractViolation("Native PDF bytes are absent.");
        }

        var bytes = new byte[byteCount];
        if (byteCount > 0)
        {
            Marshal.Copy(data, bytes, 0, byteCount);
        }

        return bytes;
    }

    private static string ReadErrorMessage(ErrorHandle error)
    {
        var length = NativeMethods.ironpress_error_message_len(error);
        if (length > int.MaxValue)
        {
            return "Ironpress returned an oversized native diagnostic.";
        }

        var byteCount = (int)length;
        var data = NativeMethods.ironpress_error_message_data(error);
        if (byteCount == 0 || data == nint.Zero)
        {
            return "Ironpress returned an empty native diagnostic.";
        }

        return Marshal.PtrToStringUTF8(data, byteCount)
            ?? "Ironpress returned an invalid native diagnostic.";
    }

    private static IronpressErrorKind ToPublicKind(NativeStatus status) => status switch
    {
        NativeStatus.InvalidArgument => IronpressErrorKind.InvalidArgument,
        NativeStatus.InvalidUtf8 => IronpressErrorKind.InvalidUtf8,
        NativeStatus.InvalidEnum => IronpressErrorKind.InvalidEnum,
        NativeStatus.InvalidHandle => IronpressErrorKind.InvalidHandle,
        NativeStatus.OutputNotEmpty => IronpressErrorKind.OutputNotEmpty,
        NativeStatus.Parse => IronpressErrorKind.Parse,
        NativeStatus.Css => IronpressErrorKind.Css,
        NativeStatus.Layout => IronpressErrorKind.Layout,
        NativeStatus.Render => IronpressErrorKind.Render,
        NativeStatus.Font => IronpressErrorKind.Font,
        NativeStatus.Io => IronpressErrorKind.Io,
        NativeStatus.Security => IronpressErrorKind.Security,
        NativeStatus.Internal => IronpressErrorKind.Internal,
        _ => IronpressErrorKind.Unknown,
    };

    private static IronpressException ContractViolation(string message) =>
        new(IronpressErrorKind.Internal, (int)NativeStatus.Internal, message);
}
