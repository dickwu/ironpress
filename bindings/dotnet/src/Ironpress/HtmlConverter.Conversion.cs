using System.Diagnostics;
using Ironpress.Interop;

namespace Ironpress;

public sealed partial class HtmlConverter
{
    /// <summary>Convert UTF-16 .NET HTML text to owned PDF bytes.</summary>
    public byte[] ConvertHtml(string html) => Convert(DocumentKind.Html, html, nameof(html));

    /// <summary>Convert UTF-16 .NET Markdown text to owned PDF bytes.</summary>
    public byte[] ConvertMarkdown(string markdown) =>
        Convert(DocumentKind.Markdown, markdown, nameof(markdown));

    private unsafe byte[] Convert(DocumentKind kind, string source, string parameterName)
    {
        EnsureOpen();
        var encoded = Utf8Input.Encode(source, parameterName);
        fixed (byte* data = encoded)
        {
            var input = new NativeBytes((nint)data, (nuint)encoded.Length);
            nint pdf;
            nint error;
            var status = kind switch
            {
                DocumentKind.Html => NativeMethods.ironpress_converter_convert_html(
                    converter, input, out pdf, out error),
                DocumentKind.Markdown => NativeMethods.ironpress_converter_convert_markdown(
                    converter, input, out pdf, out error),
                _ => throw new UnreachableException(),
            };
            return NativeResult.TakePdf(status, pdf, error);
        }
    }

    private enum DocumentKind
    {
        Html,
        Markdown,
    }
}
