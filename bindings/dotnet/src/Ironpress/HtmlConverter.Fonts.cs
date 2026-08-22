using Ironpress.Interop;

namespace Ironpress;

public sealed partial class HtmlConverter
{
    /// <summary>Add or replace one custom TrueType font family.</summary>
    /// <param name="family">The CSS font-family name.</param>
    /// <param name="fontData">Raw TrueType font bytes.</param>
    public unsafe HtmlConverter AddFont(string family, ReadOnlySpan<byte> fontData)
    {
        ArgumentException.ThrowIfNullOrEmpty(family);
        if (fontData.IsEmpty)
        {
            throw new ArgumentException("Font data must not be empty.", nameof(fontData));
        }
        EnsureOpen();

        var encodedFamily = Utf8Input.Encode(family, nameof(family));
        fixed (byte* familyData = encodedFamily)
        fixed (byte* fontBytes = fontData)
        {
            var status = NativeMethods.ironpress_converter_add_font(
                converter,
                new NativeBytes((nint)familyData, (nuint)encodedFamily.Length),
                new NativeBytes((nint)fontBytes, (nuint)fontData.Length),
                out var error);
            return Complete(status, error);
        }
    }

    /// <summary>Parse and install one optional CJK or emoji fallback pack.</summary>
    public unsafe HtmlConverter AddFontPack(FontPackKind kind, ReadOnlySpan<byte> fontData)
    {
        EnsureOpen();
        fixed (byte* fontBytes = fontData)
        {
            var status = NativeMethods.ironpress_converter_add_font_pack(
                converter,
                (uint)kind,
                new NativeBytes((nint)fontBytes, (nuint)fontData.Length),
                out var error);
            return Complete(status, error);
        }
    }
}
